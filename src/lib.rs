#![allow(internal_features)]
#![feature(coroutines, coroutine_trait)]
#![cfg_attr(
    all(not(target_arch = "x86"), not(target_arch = "x86_64")),
    feature(core_intrinsics)
)]

use std::cmp::Ordering::{Equal, Greater, Less};
#[cfg(target_os = "macos")]
use std::io;
use std::ops::{Coroutine, CoroutineState};
use std::pin::Pin;

use memmap2::{Advice, Mmap};

pub fn prefetch<T>(reference: &T) {
    use std::intrinsics::prefetch_read_data;
    let pointer: *const _ = &*reference;
    prefetch_read_data::<_, 0>(pointer as _);
}

/// A coroutine version of a binary_search algorithm that yields
/// after having prefetched the data.
///
/// The goal is to run multiple coroutines concurrently and fetch
/// the pages corresponding to the offsets. Resuming the coroutine
/// after the pages are known to be in CPU cache.
pub fn binary_search_cor(
    s: &[i32],
    value: i32,
) -> impl Coroutine<Yield = (), Return = Result<usize, usize>> + '_ {
    let mut inner = binary_search_yield_offsets_cor(s, value);
    #[coroutine]
    move || loop {
        match Pin::new(&mut inner).resume(()) {
            CoroutineState::Yielded(offset) => yield prefetch(&s[offset]),
            CoroutineState::Complete(res) => return res,
        }
    }
}

/// Ask the kernel to load the pages corresponding to the offsets.
///
/// Note that this function takes offsets in bytes. Offsets must be
/// converted accordingly. It also asks the kernel to load only 4 bytes
/// (size of i32).
pub fn load_pages_at_offsets(mmap: &Mmap, offsets: &[usize]) -> io::Result<()> {
    offsets
        .iter()
        .copied()
        .try_for_each(|offset| mmap.advise_range(Advice::WillNeed, offset, size_of::<i32>()))
}

/// A coroutine version of a binary_search algorithm that yields offsets
/// it plans to look at just before it looks at them.
///
/// The goal is to run multiple coroutines concurrently and fetch the pages
/// corresponding to the offsets. Resuming the coroutine after the pages are
/// known to be in page/CPU cache.
pub fn binary_search_yield_offsets_cor(
    s: &[i32],
    value: i32,
) -> impl Coroutine<Yield = usize, Return = Result<usize, usize>> + '_ {
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
            yield mid;
            let cmp = (*reference).cmp(&value);
            base = if cmp == Greater { base } else { mid };
            size -= half;
        }
        // base is always in [0, size) because base <= mid.
        let reference = unsafe { s.get_unchecked(base) };
        yield base;
        let cmp = (*reference).cmp(&value);
        if cmp == Equal {
            Ok(base)
        } else {
            Err(base + (cmp == Less) as usize)
        }
    }
}
