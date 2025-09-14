#![allow(internal_features)]
#![feature(core_intrinsics, coroutines, coroutine_trait)]

use std::cmp::Ordering::{Equal, Greater, Less};
use std::io;
use std::ops::{Coroutine, CoroutineState};
use std::pin::Pin;

#[cfg(not(target_os = "linux"))]
use memmap2::Mmap;

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
    s: &[i64],
    value: i64,
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
/// (size of i64).
#[cfg(not(target_os = "linux"))]
pub fn load_pages_at_offsets(mmap: &Mmap, offsets: &[usize]) -> io::Result<()> {
    use memmap2::Advice;
    offsets
        .iter()
        .copied()
        .try_for_each(|offset| mmap.advise_range(Advice::WillNeed, offset, size_of::<i64>()))
}

#[cfg(target_os = "linux")]
pub fn load_pages_at_offsets(file: &std::fs::File, offsets: &[usize]) -> io::Result<()> {
    use std::os::fd::AsRawFd as _;
    use std::ptr;

    use io_uring::{IoUring, opcode, types};

    let entries = offsets.len().next_power_of_two().try_into().unwrap();
    let mut ring = IoUring::new(entries)?;

    for &offset in offsets {
        let fd = file.as_raw_fd();
        // We don't care about the data, we just want to make sure it's in the page cache.
        let entry = opcode::Read::new(types::Fd(fd), ptr::null_mut(), 0)
            .offset(offset as u64)
            .build();
        unsafe {
            ring.submission()
                .push(&entry)
                .expect("submission queue is full");
        }
    }

    ring.submit_and_wait(offsets.len())?;

    Ok(())
}

/// A coroutine version of a binary_search algorithm that yields offsets
/// it plans to look at just before it looks at them.
///
/// The goal is to run multiple coroutines concurrently and fetch the pages
/// corresponding to the offsets. Resuming the coroutine after the pages are
/// known to be in page/CPU cache.
pub fn binary_search_yield_offsets_cor(
    s: &[i64],
    value: i64,
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
