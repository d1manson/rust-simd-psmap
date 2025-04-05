# SimdPerfectScanMap (simd-psmap)

A fast Map implementation in Rust designed for **immutable** maps with about **100 keys** or less. Requires **nightly Rust** for
access to the `portable_simd` feature (which is critical here). It claims to be about 2x faster than `std::HashMap` in the
sweetspot usecase.

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

If your architecture supports 512 bit SIMD, you should be able to handle 64 keys just as fast as 6. In real world usage my
sense is that 2-3 scans is probably enough in most cases.

### Faster?

The original version of this map used SIMD even more widely, in particular the `query` string was always zero padded to make using SIMD easier in the verify step, which seems to actually be a particularly slow part of the logic. That original version
also didn't require you to pre-slice the query string to the key length, you just needed to provide an "open endeded" slice,
starting at the start of the key, this allowed for even more optimisation upstream.

The logic within the constructor has not been at all optimised - it has some very deeply nested loops which "solve" the perfect
problem deterministically, but not very efficiently. That said, for the relevant size of map (<100keys) it's probably not a big deal.


### Is this novel?

Dunno. You tell me ;). I went with a permissive licence, but do let me know if you find this useful!

