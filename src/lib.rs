#![allow(internal_features)]
#![feature(coroutines, coroutine_trait)]
#![cfg_attr(
    all(not(target_arch = "x86"), not(target_arch = "x86_64")),
    feature(core_intrinsics)
)]

use std::cmp::Ordering::{Equal, Greater, Less};
use std::ops::Coroutine;

fn prefetch<T>(reference: &T) {
    use std::intrinsics::prefetch_read_data;
    let pointer: *const _ = &*reference;
    prefetch_read_data::<_, 0>(pointer as _);
}

pub fn binary_search_gen(
    s: &[i32],
    value: i32,
) -> impl Coroutine<Yield = (), Return = Result<usize, usize>> + '_ {
    #[coroutine]
    move || {
        let mut size = s.len();
        if size == 0 {
            return Err(0);
        }
        let mut base = 0usize;
        while size > 1 {
            let half = size / 2;
            let mid = base + half;
            // mid is always in [0, size), that means mid is >= 0 and < size.
            // mid >= 0: by definition
            // mid < size: mid = size / 2 + size / 4 + size / 8 ...
            let reference = unsafe { s.get_unchecked(mid) };
            yield prefetch(reference);
            let cmp = (*reference).cmp(&value);
            base = if cmp == Greater { base } else { mid };
            size -= half;
        }
        // base is always in [0, size) because base <= mid.
        let reference = unsafe { s.get_unchecked(base) };
        yield prefetch(reference);
        let cmp = (*reference).cmp(&value);
        if cmp == Equal {
            Ok(base)
        } else {
            Err(base + (cmp == Less) as usize)
        }
    }
}
