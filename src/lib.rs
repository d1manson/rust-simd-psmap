#![feature(portable_simd)]

use std::convert::TryFrom;
use std::simd::num::SimdInt;
use std::{array, hint, usize};
use std::simd::cmp::SimdPartialEq;
use std::simd::{Simd, SupportedLaneCount, LaneCount};

const MAX_KEY_SEARCH_LEN: usize = 32;

fn roughly_log_2(x: usize) -> usize {
    (usize::BITS - x.leading_zeros()) as usize
}

/// Use `::try_from()` to construct an instance. It is immutable after construction; query it with `.get()`, or `.iter()`.
/// 
/// Set `LANE_SIZE` to the maxium available SIMD width for the target architecture, in bytes; ideally 64, but 16 is still not too bad.
/// Note portable_simd will work with any value via emulation, but it's not a good idea to do extra work if it's expensive.
/// 
/// To pick a value for `MAX_LANES`, you really need to benchmark against alternative map implementations to see how many lanes of
/// work can be executed here before it becomes slower than another map implementation.
/// 
/// It is best suited to <100 keys, but you can stretch things further with a large enough value for `MAX_LANES`. 
#[derive(Debug)]
pub struct SimdPerfectScanMap<T, const MAX_LANES: usize, const LANE_SIZE: usize> 
where LaneCount<LANE_SIZE>: SupportedLaneCount
{
    key_vals: Vec<(String, T)>,
    n_lanes_of_entities: usize,
    n_chars: usize,
    // we allocate the below as inline arrays, but we only need n_lanes_of_entities * n_chars elements (always <= MAX_LANES)
    char_positions: [usize; MAX_LANES],
    indexes: [Simd<u8, LANE_SIZE>; MAX_LANES],
    n_valid: [usize; MAX_LANES]
}


impl<T, const MAX_LANES: usize,  const LANE_SIZE: usize> TryFrom<Vec<(String, T)>>
for SimdPerfectScanMap<T, MAX_LANES, LANE_SIZE>  
where LaneCount<LANE_SIZE>: SupportedLaneCount
{
    type Error = (&'static str, Vec<(String, T)>);

    /// If there's an error, it returns back ownership of the `key_vals` in the second element of the Err tuple in case you want
    /// to support using an alternative fallback map of some other kind.
    fn try_from(key_vals: Vec<(String, T)>) -> Result<Self, Self::Error> {
        if key_vals.len() == 0 {
            return Err(("Empty map not supported", key_vals));
        }

        if key_vals.len() > MAX_LANES * LANE_SIZE {
            return Err(("Too many keys to perform even a single scan", key_vals));
        }

        let max_len = key_vals.iter().map(|(k, _)| k.as_bytes().len()).max().unwrap().min(MAX_KEY_SEARCH_LEN);

        let n_lanes_of_entities = key_vals.len().div_ceil(LANE_SIZE);
        let mut solved = false;
        let mut positions = vec![0; 0];
        
        // Yes, there are a lot of nested loops here, but MAX_LANES and max_key_len are capped fairly low.
        // If needed there are definitely some straightforward ways to reduce the complexity here, such as by storing the
        // selected characters themselves (as we end up doing in the `indexes` later) and sort after each new char so that
        // duplicates appear next to one another. Then when adding a new char you just need to loop over existing block of 
        // duplicates rather than all other keys, and count how many are still dups as you go. But in reality this is taking
        // less than 1ms at startup so it's not worth over complicating.
        for _ in 1..=(MAX_LANES/n_lanes_of_entities) {
            let mut position_score = vec![0; max_len];
            for new_char_idx in 0..max_len {
                if positions.contains(&new_char_idx){
                    position_score[new_char_idx] = usize::MAX;
                    continue;
                }
                positions.push(new_char_idx); // temporarily add it to calculate a score
            
                for (k_self, _) in &key_vals {
                    // each key contributes to the score for new_char_idx...
                    let k_self = k_self.as_bytes();
                    let mut tests_matches_keys = vec![true; key_vals.len()];
                    for &char_idx_sub in positions[..].iter() {
                        // we could pad with zero beyond the end of a key, but instead we pad with 0, 1, 2, 3, ... as that's more valuable when scanning
                        let char_self =  *k_self.get(char_idx_sub).unwrap_or(&((char_idx_sub.wrapping_sub(k_self.len()) as u8)));
                        for (idx, (k_other, _)) in key_vals.iter().enumerate() {
                            let k_other = k_other.as_bytes();
                            let char_other = *k_other.get(char_idx_sub).unwrap_or(&((char_idx_sub.wrapping_sub(k_other.len())) as u8)) ;
                            tests_matches_keys[idx] &= char_self == char_other;
                        }
                    }
                    let tests_scan_n_other_keys: usize = tests_matches_keys.iter().map(|&b| b as usize).sum::<usize>() - 1;
                    position_score[new_char_idx] += roughly_log_2(tests_scan_n_other_keys); 
                }

                positions.pop(); // as promised, adding the new_char was only temporary
            }
            let best_idx = position_score.iter().enumerate().min_by_key(|(_, s)| *s).unwrap().0;
            positions.push(best_idx);
            if position_score[best_idx] == 0 {
                solved = true;
                break;
            }
        }

        if !solved {
            return Err(("Unable to 'solve' with a sufficiently small number of scans", key_vals));
        }

        let n_chars = positions.len();   

        let mut indexes = [Simd::<u8, LANE_SIZE>::splat(0); MAX_LANES];
        let mut n_valid = [0; MAX_LANES];
        let mut char_positions = [0; MAX_LANES];
        for lane_idx in 0..n_lanes_of_entities {
            for scan_idx in 0..n_chars {
                let test_idx = lane_idx * n_chars + scan_idx;
                char_positions[test_idx] = positions[scan_idx];

                let mut v = Simd::<u8, LANE_SIZE>::splat(0);
                let start_idx = lane_idx * LANE_SIZE;
                let end_idx = if start_idx + LANE_SIZE > key_vals.len() { key_vals.len() } else { start_idx + LANE_SIZE };
                for (idx, (k, _)) in key_vals[start_idx..end_idx].iter().enumerate() {
                    let k = k.as_bytes();
                    v[idx] = *k.get(char_positions[scan_idx]).unwrap_or(&((char_positions[scan_idx].wrapping_sub(k.len())) as u8));
                }
                indexes[test_idx] = v;
                n_valid[test_idx] = end_idx - start_idx;
            }
        }

        return Ok(SimdPerfectScanMap::<T, MAX_LANES, LANE_SIZE>{
            n_lanes_of_entities,
            n_chars,
            char_positions,
            indexes,
            n_valid,
            key_vals
        });
    }
}


impl<T, const MAX_LANES: usize,  const LANE_SIZE: usize>
SimdPerfectScanMap<T, MAX_LANES, LANE_SIZE>
where LaneCount<LANE_SIZE>: SupportedLaneCount
{
    /// This is branchless when compiled, except for the loops and the final validaiton check. The loops always make the same number 
    /// of iterations for a given instance, with no early-exit conditions. This should keep the branch predictor happy.
    pub fn get(&self, query: &String) -> Option<&T>{
        let query = query.as_bytes();
        unsafe {
            // SAFETY: designed that way in `try_from` method, which is the only way to construct this struct
            hint::assert_unchecked(self.n_lanes_of_entities >= 1);
            hint::assert_unchecked(self.n_chars >= 1);
        }

        let mut matched_idx = 0;
        let mut test_idx = 0;
        for lane_idx in 0..self.n_lanes_of_entities {
            let matched: [i8; LANE_SIZE] = array::from_fn(|i| (LANE_SIZE - i) as i8); 
            let mut matched = Simd::<i8, LANE_SIZE>::from(matched);
            for _scan_idx in 0..self.n_chars {                
                unsafe {
                    // SAFETY: designed that way in `try_from` method, which is the only way to construct this struct
                    hint::assert_unchecked(test_idx < MAX_LANES);
                }
                let char_idx = self.char_positions[test_idx];
                
                // it would be nice to use .unwrap_or(&alt), but the compiler isn't able to optimise that because zero might
                // be a legitimate value in query which shouldn't be replaced (and it relies on being able to do that).
                // We do opt to treat zero as special, basically we assume no actual query String contains a zero byte.
                let alt = char_idx.wrapping_sub(query.len()) as u8;
                let query_c = *query.get(char_idx).unwrap_or(&0);
                let query_c = if query_c == 0 { alt } else { query_c };

                let index = self.indexes[test_idx];
                matched &= index.simd_eq(Simd::<u8, LANE_SIZE>::splat(query_c)).to_int();
                test_idx += 1; // = lane_idx * n_chars + scan_idx  (but implemented as a counter)
            }
            unsafe {
                // SAFETY: designed that way in `try_from` method, which is the only way to construct this struct
                hint::assert_unchecked(test_idx -1 < MAX_LANES);
            }

            // there can only ever be one match given how we construct the indexes (even if the query is not a valid key, the index design still ensures uniqueness)
            // thus we can use += instead of an if statement here, which is faster.
            let matched = LANE_SIZE - matched.reduce_max() as usize; // amazingly, using reduce_max(), having started with [16, 15, ..., 1, 0] is faster than using a mask and .first_set()
            matched_idx += if matched < self.n_valid[test_idx -1] && matched != 0 { matched as usize + lane_idx * LANE_SIZE } else { 0 };
        }
        
        unsafe {
            // SAFETY:  see the line above with self.n_valid, and the comment above that line
            hint::assert_unchecked(matched_idx < self.key_vals.len());
        }

        let found = &self.key_vals[matched_idx];
        return if found.0.as_bytes() == query { Some(&found.1) } else { None };
    }

    pub fn iter(&self) -> impl Iterator<Item=&(String, T)> {
        self.key_vals.iter()
    }

    pub fn len(&self) -> usize {
        self.key_vals.len()
    }
}


    