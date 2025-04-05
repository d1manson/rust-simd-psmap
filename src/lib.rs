#![feature(portable_simd)]

use std::convert::TryFrom;
use std::{hint, usize};
use std::simd::cmp::SimdPartialEq;
use std::simd::{Simd, Mask, SupportedLaneCount, LaneCount};

const MAX_KEY_SEARCH_LEN: usize = 32;

fn roughly_log_2(x: usize) -> usize {
    (usize::BITS - x.leading_zeros()) as usize
}

/// Use `::try_from()` to construct an instance. It is immutable after construction, only query it with `.get()`, or `.iter()`.
/// It is best suited to <100 keys, but you can stretch things further with a lareg enough value for `N_TEST_LANES`. 
#[derive(Debug)]
pub struct SimdPerfectScanMap<T, const N_TEST_LANES: usize, const LANE_SIZE_TEST: usize> 
where LaneCount<LANE_SIZE_TEST>: SupportedLaneCount
{
    key_vals: Vec<(String, T)>,
    n_lanes_of_entities: usize,
    n_chars: usize,
    // we allocate the below as inline arrays, but we only need n_lanes_of_entities * n_chars elements (always <= N_TEST_LANES)
    char_positions: [usize; N_TEST_LANES],
    indexes: [Simd<u8, LANE_SIZE_TEST>; N_TEST_LANES],
    masks: [u64; N_TEST_LANES]
}


impl<T, const N_TEST_LANES: usize,  const LANE_SIZE_TEST: usize> TryFrom<Vec<(String, T)>>
for SimdPerfectScanMap<T, N_TEST_LANES, LANE_SIZE_TEST>  
where LaneCount<LANE_SIZE_TEST>: SupportedLaneCount
{
    type Error = (&'static str, Vec<(String, T)>);

    /// If there's an error, it returns back ownership of the `key_vals` in the second element of the Err tuple in case you want
    /// to support using an alternative fallback map of some other kind.
    fn try_from(key_vals: Vec<(String, T)>) -> Result<Self, Self::Error> {
        if key_vals.len() == 0 {
            return Err(("Empty map not supported", key_vals));
        }

        if key_vals.len() > N_TEST_LANES * LANE_SIZE_TEST {
            return Err(("Too many keys to perform even a single scan", key_vals));
        }

        let max_len = key_vals.iter().map(|(k, _)| k.as_bytes().len()).max().unwrap().min(MAX_KEY_SEARCH_LEN);

        let n_lanes_of_entities = key_vals.len().div_ceil(LANE_SIZE_TEST);
        let mut solved = false;
        let mut positions = vec![0; 0];
        
        // Yes, there are a lot of nested loops here, but N_TEST_LANES and max_key_len are capped fairly low.
        // If needed there are definitely some straightforward ways to reduce the complexity here, such as by storing the
        // selected characters themselves (as we end up doing in the `indexes` later) and sort after each new char so that
        // duplicates appear next to one another. Then when adding a new char you just need to loop over existing block of 
        // duplicates rather than all other keys, and count how many are still dups as you go. But in reality this is taking
        // less than 1ms at startup so it's not worth over complicating.
        for _ in 1..=(N_TEST_LANES/n_lanes_of_entities) {
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

        let mut indexes = [Simd::<u8, LANE_SIZE_TEST>::splat(0); N_TEST_LANES];
        let mut masks = [0; N_TEST_LANES];
        let mut char_positions = [0; N_TEST_LANES];
        for lane_idx in 0..n_lanes_of_entities {
            for scan_idx in 0..n_chars {
                let test_idx = lane_idx * n_chars + scan_idx;
                char_positions[test_idx] = positions[scan_idx];

                let mut v = Simd::<u8, LANE_SIZE_TEST>::splat(0);
                let start_idx = lane_idx * LANE_SIZE_TEST;
                let end_idx = if start_idx + LANE_SIZE_TEST > key_vals.len() { key_vals.len() } else { start_idx + LANE_SIZE_TEST };
                for (idx, (k, _)) in key_vals[start_idx..end_idx].iter().enumerate() {
                    let k = k.as_bytes();
                    v[idx] = *k.get(char_positions[scan_idx]).unwrap_or(&((char_positions[scan_idx].wrapping_sub(k.len())) as u8));
                }
                indexes[test_idx] = v;
                masks[test_idx] = if end_idx - start_idx == 64 as usize { !0 } else { (1 << (end_idx - start_idx)) - 1};
            }
        }

        return Ok(SimdPerfectScanMap::<T, N_TEST_LANES, LANE_SIZE_TEST>{
            n_lanes_of_entities,
            n_chars,
            char_positions,
            indexes,
            masks,
            key_vals
        });
    }
}


impl<T, const N_TEST_LANES: usize,  const LANE_SIZE_TEST: usize>
SimdPerfectScanMap<T, N_TEST_LANES, LANE_SIZE_TEST>
where LaneCount<LANE_SIZE_TEST>: SupportedLaneCount
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
            let mut matched = Mask::<i8, LANE_SIZE_TEST>::splat(true);
            for _scan_idx in 0..self.n_chars {                
                unsafe {
                    // SAFETY: designed that way in `try_from` method, which is the only way to construct this struct
                    hint::assert_unchecked(test_idx < N_TEST_LANES);
                }
                let char_idx = self.char_positions[test_idx];
                
                let alt = char_idx.wrapping_sub(query.len()) as u8;
                let query_c = *query.get(char_idx).unwrap_or(&0);
                let query_c = if query_c == 0 { alt } else { query_c };

                let index = self.indexes[test_idx];
                matched &= index.simd_eq(Simd::<u8, LANE_SIZE_TEST>::splat(query_c));
                test_idx += 1;
            }
            unsafe {
                // SAFETY: designed that way in `try_from` method, which is the only way to construct this struct
                hint::assert_unchecked(test_idx -1 < N_TEST_LANES);
            }
            let matched = matched.to_bitmask() & self.masks[test_idx-1];
            matched_idx += if matched == 0 { 0 } else { matched.trailing_zeros() as usize + lane_idx * LANE_SIZE_TEST };
        }
        
        unsafe {
            // SAFETY:  the `& self.masks[test_idx]` ensures that the only bits that can be 1 are those actually corresponding to a key (given how masks are constructed)
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


    