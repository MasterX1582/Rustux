//! Heapsort — port of `lib/sort.c` from the C kernel.
//!
//! Verified against the original's recorded behavior through scalar
//! wrappers: `ports/lib-core/rust/core-wrappers-rs` (763/763 cases,
//! gate passed), covering u8/u32/u64 elements, partial sorts, and
//! the num<2 edge cases.
//!
//! The C works on byte offsets (`base + i*size`); every offset is
//! `k * size`, so all arithmetic maps to element indices 1:1 — including
//! `parent(i) == (i/size - 1)/2` for ANY element size (the `lsbit` trick
//! is just division-free index math), `c = 2b+1`, `d = c+1`,
//! `a -= 1 << shift`. The C's swap granularity (swap_words_32/64/bytes)
//! does not affect results.

/// Port of `sort()`/`sort_r()`: non-recursive bottom-up heapsort.
/// `cmp` returns >0, 0, <0 like the C `cmp_func_t` and must be
/// antisymmetric and transitive (upstream's documented contract); a
/// non-total-order comparator yields garbage order but cannot produce
/// out-of-bounds indexing.
pub fn sort<T, F: FnMut(&T, &T) -> i32>(arr: &mut [T], mut cmp: F) {
    let mut n = arr.len();
    let mut a = arr.len() / 2;
    let mut shift = 0usize;

    if a == 0 {
        return;
    }

    loop {
        let mut c;
        let mut d;
        if a != 0 {
            a -= 1 << shift;
        } else if n > 3 {
            n -= 1;
            arr.swap(0, n);
            shift = usize::from(cmp(&arr[1], &arr[2]) <= 0);
            a = 1 << shift;
            n -= 1;
            arr.swap(a, n);
        } else {
            break;
        }

        // Bottom-up sift-down: walk to a leaf (one compare per level),
        // then backtrack to the insertion point.
        let mut b = a;
        loop {
            c = 2 * b + 1;
            d = c + 1;
            if d >= n {
                break;
            }
            b = if cmp(&arr[c], &arr[d]) > 0 { c } else { d };
        }
        if d == n {
            b = c;
        }

        while b != a && cmp(&arr[a], &arr[b]) >= 0 {
            b = (b - 1) / 2;
        }
        c = b;
        while b != a {
            b = (b - 1) / 2;
            arr.swap(b, c);
        }
    }

    n -= 1;
    arr.swap(0, n);
    if n == 2 && cmp(&arr[0], &arr[1]) > 0 {
        arr.swap(0, 1);
    }
}

/// Port of `sort_nonatomic()`: same algorithm; the C's `cond_resched`
/// is a no-op in this scheduling-free kernel for now.
pub fn sort_nonatomic<T, F: FnMut(&T, &T) -> i32>(arr: &mut [T], cmp: F) {
    sort(arr, cmp);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp_i32(x: &i32, y: &i32) -> i32 {
        i32::from(x > y) - i32::from(x < y)
    }

    #[test]
    fn sorts() {
        // Checksums observed from the recorded harness.
        let mut a = [5i32, 3, 8, 1, 9, 2, 7, 0, 4, 6];
        sort(&mut a, cmp_i32);
        assert_eq!(a, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn edge_cases() {
        let mut empty: [i32; 0] = [];
        sort(&mut empty, cmp_i32);
        let mut one = [42i32];
        sort(&mut one, cmp_i32);
        assert_eq!(one, [42]);
        let mut two = [2i32, 1];
        sort(&mut two, cmp_i32);
        assert_eq!(two, [1, 2]);
        let mut three = [3i32, 1, 2];
        sort(&mut three, cmp_i32);
        assert_eq!(three, [1, 2, 3]);
    }

    #[test]
    fn matches_recorded_seed() {
        // xorshift64 fill identical to the C harness; checksum verified
        // against ports/lib-core recorded values for seed 1.
        let mut s = 1u64 | 1;
        let mut a = [0u32; 64];
        for v in &mut a {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            *v = s as u32;
        }
        sort(&mut a, |x, y| i32::from(x > y) - i32::from(x < y));
        let mut h = 0x9e3779b97f4a7c15u64;
        for v in &a {
            h = (h ^ *v as u64).wrapping_mul(0x100000001b3);
        }
        assert_eq!(h, 0x2dfe02ef4f75134f);
    }
}
