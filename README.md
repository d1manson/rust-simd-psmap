# SimdPerfectScanMap (simd-psmap)

A fast Map implementation in Rust designed for **immutable** maps with about **100 keys** or less. Requires **nightly Rust** for
access to the `portable_simd` feature (which is critical here). In some cases it does beat 
[`FxHashMap`](https://doc.rust-lang.org/stable/nightly-rustc/rustc_data_structures/fx/type.FxHashMap.html), so possibly there is
some value in this approach, especially if it could be optimised even further (which might be doable).

### How it works

It is a bit like a [Perfect hash map](https://en.wikipedia.org/wiki/Perfect_hash_function) in that it preforms a chunk of work upfront to ensure efficient lookups later.

But it's not doing a hash at all, rather it's doing a series of Bitmap Index Scans followed by bitwise ANDs, 
[like a database would](https://www.postgresql.org/docs/current/indexes-bitmap-scans.html).  
 
Let's explain using a really simple example (we only show the keys as the values aren't interesting):

```rust
[
    "hello",
    "world",
    "its",
    "me"
]
```

In this case, the very fist character is unique across all the keys, so when presented with a query string we only need to
check the first character to decide which (if any) of they keys it matches. To make that fast, we prepare a
[SIMD](https://en.wikipedia.org/wiki/Single_instruction,_multiple_data)-compatible
array with the bytes `[b'h', b'w', b'i', b'm', 0, 0, ...]`, so that we can do the comparison really quickly. If we find a
match, we then check the associated key to make sure it really does match the query string exactly.  

A slightly more complex example would be this:

```rust
[
    "hello",
    "help",
    "bello",
]
```

Here, there is no single position which is unqiue across all the keys, but we could scan the first character and the fourth
character to disambiguate between keys. Thus we prepare two SIMD-compatible arrays: `[b'h', b'h', b'b', 0, 0, ...]` and
`[b'l', b'p', b'l', 0, 0, ...]`. Now, when presented with a query string, we check the first character and the fourth character,
ANDing the two results together to see where we have a match for both. Again, once we've found a match (if any) we validate
the associated key to make sure it really does match the query string exactly.

Note that we pad shorter keys with the sequence `0, 1, 2, 3, ...` within the logic, effectively allowing the key
length itself to be matched against (indirectly).

I refer to the map as "Perfect" because after performing the Index scans and doing the ANDing, we only get a single
`true` value in the vector, i.e. a unique index, rather like a Perfect hash.

Automatically working out which character positions to use is a bit fiddly. Currently it uses a greedy algorithm that roughly 
speaking seeks to minimise entropy at each step. It's not implemented very efficiently right now, because the aim was only to
optimise the `.get(key)` function. 


### A microbenchmark

Here we take a random sample from a list of 59 keys from a [real world schema](https://data.crunchbase.com/reference/get_data-entities-organizations-entity-id), where
we vary the sample size and see how the performance varies.

```rust
// keys (which we sample from)
[ "acquirer_identifier","aliases","categories","category_groups","closed_on","company_type","contact_email","created_at","delisted_on","demo_days","description","entity_def_id","exited_on","facebook","facet_ids","founded_on","founder_identifiers","hub_tags","identifier","image_id","image_url","layout_id","legal_name","linkedin","listed_stock_symbol","location_group_identifiers","location_identifiers","name","num_alumni","num_current_advisor_positions","num_current_positions","num_employees_enum","num_enrollments","num_event_appearances","num_portfolio_organizations","num_sub_organizations","operating_status","owner_identifier","permalink","permalink_aliases","phone_number","program_application_deadline","program_duration","program_type","rank","rank_org","school_method","school_program","school_type","short_description","status","stock_exchange_symbol","stock_symbol","twitter","updated_at","uuid","website","website_url","went_public_on"];
```

Note that performance is not directly a function of the number of keys, but how many scans are required to
dismabiguate the keys, and how long the keys are (for the final string comparison, aka the validation step).

![Alt text](docs/lines.svg)

This benchmark was done on an M4 Mac which only has 128 bit SIMD, i.e. **16 bytes per lane**. You can see that while the number of 
keys
is 16 or below it manages to beat `FxHashMap` (for the record the default `std::HashMap` is about 2x slower than `FxHashMap` for 
this kind of benchmark).

**A note on scaling** - while the number of keys fits within a single SIMD lane, there may be no scaling factor at all because as 
we add keys, we simply replace the zero padding in the SIMD lanes with actual data, and thus we end up doing more useful work, but
paying the same computational price. Of course, the more keys you have, the more likely it is that you will eventually need to add
in a new scan to dissambiguate, so in reality there is some kind of scaling (which is presumably what is shown in the chart, though
I haven't double checked that).

As such, I believe that if the arch has 256bit or 512bit SIMD registers, it will potentially beat `FxHashMap` up to `32`/`64` 
keys respectively.

### Faster?

The original version of this map used SIMD even more widely, in particular the `query` string was always zero padded to make using 
SIMD easier in the verify step, which seems to actually be a particularly slow part of the logic. 

When the number of keys is longer than a single SIMD lane it ought to be better to partition the keys so that even if some of the 
scans need to be applied to all lanes, other scans can be applied within a single lane (to disambiguate keys within the lane once 
it's established that the query doesn't belong to any of the keys in other lanes), thus reducing the total number of operations.
Implementing that properly would certianly be a bit fiddly!

In any case, if this were to be taken seriously it would be worth optimising the logic that "solves" the perfect problem. It is 
currently done deterministically, but not very efficiently. That said, for the relevant size of map (<100keys) it's probably not a 
big deal.

### Variant use case

That original version of this concept didn't require you to pre-slice the query string to the key length, you just needed to 
provide an "open ended" slice, starting at the start of the key, this allowed for even more optimisation opportunities upstream
(if finding the end of the key takes a few cycles). A standard HashMap (or FxHashMap) can't do this becuase they need to hash a
whole key in order to find it.


### Is this novel?

Dunno. You tell me ;). I went with a permissive licence, but do let me know if you find this useful!

