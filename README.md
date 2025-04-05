# SimdPerfectScanMap (simd-psmap)

A fast Map implementation in Rust designed for **immutable** maps with about **100 keys** or less. Requires **nightly Rust** for
access to the `portable_simd` feature (which is critical here). It claims to be about 2x faster than `std::HashMap` in the
sweetspot usecase.  

**Update:** it's hard to beat `FxHashMap`, but it can do so by a small margin in the best cases. So this is likely not that useful as is, 
though maybe there is some potential here with a bit more work. That said, one thing this kind of approach can potentially work for
is matching against an "open-ended" slice, where you don't know where the end of the key is in your query string (which is sort of
the original use case that inspired this to be created). For a regular hash map you can only query on an exact key (in order to hash it).


### How it works

It is a bit like a [Perfect hash map](https://en.wikipedia.org/wiki/Perfect_hash_function) in that it preforms a chunk of work upfront to ensure efficient lookups later.

But it's not doing a hash at all, rather it's doing a series of Bitmap Index Scans followed by bitwise ANDs, 
[like a database would](https://www.postgresql.org/docs/current/indexes-bitmap-scans.html).  
 
For the ith character in a query string, it tests every key to see if the ith character in the key matches the query string at
position i, producing a bool vector with the length of the number of keys. This test is done using pre-prepared
[SIMD](https://en.wikipedia.org/wiki/Single_instruction,_multiple_data) lanes, 
which will be really fast if the number of keys is less than or equal to the architecture's max lane size (e.g. 64 keys if
512bit SIMD is available; but it will still work ok if there are more keys than the lane size).

This test is repeated for characters i_0, i_1, i_2, ... ANDing together the results, so that the bool vector ends up with just a
single element set to true (or no true at all if there is no match). Ending up with a single true in the vector for any valid key is what makes it "Perfect".
 
The dumb way to do this would be simply to run the test for every single char (up to the length of the longest key). But it's
possible to do a lot better than that if you choose i_0, i_1, ... carefully. We need the choice to ultimately result in a single
true value for each key when we encounter it in the wild, but have as few i's as possible.  This is where the upfront work comes
in - we do some nasty nested loops to come up with the selection (and prepare the associated SIMD lanes for efficient 
querying).  Currently it uses a greedy algorithm that roughly speaking seeks to minimise entropy at each step; 
I haven't explored alternatives to that because it seems to work well enough.
 
Note that after doing this "bitmap scan" logic, it then validates the query string exactly to make sure it really does match. 

### Some microbenchmarks

You could do all kinds of different benchmarks. See the `benches` dir for full details, but here we walk through some examples 
in a bit of detail:

**Example 1**

```rust
// (key, val)
("key1", DummyVal(1001)),
("now4", DummyVal(1002)),
("something", DummyVal(1003)),
("another", DummyVal(1004)),
("interesting", DummyVal(1005)),
("thanks", DummyVal(1005)),
```

In this example, simply scanning the first character is enough to distinguish all the keys, i.e. we fill a SIMD register with: 

* scan 1: `[b'k', b'n', b's', b'a', b'i', b't', /*zero padding to end of SIMD lane */]`

and then compare the first character of the query string against that.

It's more than 2x as fast as the default `std::HashMap`, though only slightly faster than `FxHashMap`:

![Alt text](docs/violin1.svg)

**Example 2**

```rust
// (key, val)
("key1".into(), DummyVal(1001)),
("key1longer".into(), DummyVal(1002)),
("key".into(), DummyVal(1003)),
("now4".into(), DummyVal(1004)),
("something".into(), DummyVal(1005)),
("something_b".into(), DummyVal(1006))
```

In this example you need to make several comparisons to uniquely identify each key. We prepare three SIMD registers' worth:

* scan 1: `[b'1', b'1', TERMINAL_BYTE, b'4', b'e', b'e', /*zero padding to end of SIMD lane */]`
* scan 2: `[0, b'r', 0, 0, TERMINAL_BYTE, b'_', /*zero padding to end of SIMD lane */]`
* scan 3: `[TERMINAL_BYTE, b'l', 0, TERMINAL_BYTE, b't', b't', /*zero padding to end of SIMD lane */]`

(The `TERMINAL_BYTE` marks one byte beyond the end of each key.)

Each new scan adds a few hundred picoseconds, making this slightly less than 2x as fast as `std::HashMap`:

![Alt text](docs/violin2.svg)


**Example 3**

Here we take a random sample from a list of 59 keys from a [real world schema](https://data.crunchbase.com/reference/get_data-entities-organizations-entity-id), where
we vary the sample size and see how the performance varies.

```rust
// keys (which we sample from)
[ "acquirer_identifier","aliases","categories","category_groups","closed_on","company_type","contact_email","created_at","delisted_on","demo_days","description","entity_def_id","exited_on","facebook","facet_ids","founded_on","founder_identifiers","hub_tags","identifier","image_id","image_url","layout_id","legal_name","linkedin","listed_stock_symbol","location_group_identifiers","location_identifiers","name","num_alumni","num_current_advisor_positions","num_current_positions","num_employees_enum","num_enrollments","num_event_appearances","num_portfolio_organizations","num_sub_organizations","operating_status","owner_identifier","permalink","permalink_aliases","phone_number","program_application_deadline","program_duration","program_type","rank","rank_org","school_method","school_program","school_type","short_description","status","stock_exchange_symbol","stock_symbol","twitter","updated_at","uuid","website","website_url","went_public_on"];
```

 Note that performance is not just a function of the number of keys, but how many scans are required to
dismabiguate the keys, and how long the keys are (for the final string comparison, aka the validation step).

Whereaas the above benchmarks were done on an M4 Mac, this was done in GCP using an `e2-standard-2 (2 vCPUs, 8 GB Memory) / Intel Broadwell` instance, with `+avx2`,
i.e. 32 bytes per SIMD lane. For comparison, the `std::HashMap` takes ~30ns, and the `FxHashMap` is ~15ns.

![Alt text](docs/lines3.svg)

As you can see, the performance is relatively stable (and comparable to `FxHashMap) while the number of keys is less than the width of a SIMD lane, and then it jumps up.
If your architecture supports 512 bit SIMD, you should be able to get up to 64 without a huge increase, assuming the number of scans doesn't go much above 3.

### Faster?

The original version of this map used SIMD even more widely, in particular the `query` string was always zero padded to make using SIMD easier in the verify step, which seems to actually be a particularly slow part of the logic. That original version
also didn't require you to pre-slice the query string to the key length, you just needed to provide an "open endeded" slice,
starting at the start of the key, this allowed for even more optimisation upstream. 

When the number of keys is longer than a single SIMD lane it ought to be better to partition the keys so that even if the some of the scans need to be applied to all lanes,
other scans can be applied within a single lane (to disambiguate keys within the lane once it's established that the query doesn't belong to any of the keys in other lanes),
thus reducing the total number of operations.

The logic within the constructor has not been at all optimised - it has some very deeply nested loops which "solve" the perfect
problem deterministically, but not very efficiently. That said, for the relevant size of map (<100keys) it's probably not a big deal.


### Is this novel?

Dunno. You tell me ;). I went with a permissive licence, but do let me know if you find this useful!

