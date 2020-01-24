#![feature(test, generators, generator_trait)]

#[cfg(test)]
#[macro_use] extern crate quickcheck;

use std::mem;
use std::cmp::Ordering::{Less, Equal, Greater};
use std::ops::{Generator, GeneratorState};
use std::arch::x86_64::{_mm_prefetch, _MM_HINT_NTA};
use std::pin::Pin;

enum BinarySearch<'a> {
    Start {
        slice: &'a [i32],
        value: i32,
    },
    Explore {
        slice: &'a [i32],
        size: usize,
        value: i32,
        base: usize,
        half: usize,
        mid: usize,
    },
    Final {
        slice: &'a [i32],
        value: i32,
        base: usize,
    },
    Done,
}

impl<'a> BinarySearch<'a> {
    fn new(slice: &'a [i32], value: i32) -> BinarySearch<'a> {
        BinarySearch::Start { slice, value }
    }
}

impl Generator for BinarySearch<'_> {
    type Yield = ();
    type Return = Result<usize, usize>;

    fn resume(mut self: Pin<&mut Self>) -> GeneratorState<(), Self::Return> {
        match mem::replace(&mut *self, BinarySearch::Done) {
            BinarySearch::Start { slice, value } => {
                let size = slice.len();

                if size == 0 {
                    *self = BinarySearch::Done;
                    return GeneratorState::Complete(Err(0));
                }

                let base = 0usize;
                if size > 1 {
                    let half = size / 2;
                    let mid = base + half;

                    // mid is always in [0, size), that means mid is >= 0 and < size.
                    // mid >= 0: by definition
                    // mid < size: mid = size / 2 + size / 4 + size / 8 ...
                    unsafe { _mm_prefetch(mid as _, _MM_HINT_NTA) }

                    *self = BinarySearch::Explore { slice, size, value, base, half, mid };
                    GeneratorState::Yielded(())
                } else {

                    // base is always in [0, size) because base <= mid.
                    unsafe { _mm_prefetch(base as _, _MM_HINT_NTA) };

                    *self = BinarySearch::Final { slice, value, base };
                    GeneratorState::Yielded(())
                }
            }

            // This state is always called after we triggered a memory prefetch
            // therefore reading the value at `mid` should be faster
            BinarySearch::Explore { slice, size, value, base, half, mid } => {

                let cmp = unsafe { slice.get_unchecked(mid).cmp(&value) };
                let base = if cmp == Greater { base } else { mid };
                let size = size - half;

                if size > 1 {
                    let half = size / 2;
                    let mid = base + half;

                    // mid is always in [0, size), that means mid is >= 0 and < size.
                    // mid >= 0: by definition
                    // mid < size: mid = size / 2 + size / 4 + size / 8 ...
                    unsafe { _mm_prefetch(mid as _, _MM_HINT_NTA) }

                    *self = BinarySearch::Explore { slice, size, value, base, half, mid };
                    GeneratorState::Yielded(())
                } else {

                    *self = BinarySearch::Final { slice, value, base };
                    GeneratorState::Yielded(())
                }
            }

            // This state is always called after a prefetch, this is the final state.
            // It indicates that we are in a slice that contains only one value.
            BinarySearch::Final { slice, value, base } => {

                // base is always in [0, size) because base <= mid.
                let cmp = unsafe { slice.get_unchecked(base).cmp(&value) };
                let result = if cmp == Equal { Ok(base) } else { Err(base + (cmp == Less) as usize) };

                *self = BinarySearch::Done;
                GeneratorState::Complete(result)
            }

            BinarySearch::Done => {
                panic!("generator resumed after completion")
            }
        }
    }
}

// _MM_HINT_NTA
fn main() {
    let slice: Vec<_> = (0..10_000_000).collect();
    let value = 10_000;

    let mut generator = BinarySearch::new(&slice, value);

    loop {
        match Pin::new(&mut generator).resume() {
            GeneratorState::Yielded(_) => println!("yield"),
            GeneratorState::Complete(result) => {
                println!("complete {:?}", result);
                break;
            },
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate test;
    use super::*;

    quickcheck! {
        fn qc_easy(xs: Vec<i32>, x: i32) -> bool {
            let mut xs = xs;

            xs.sort_unstable();
            xs.dedup();

            let a = xs.binary_search(&x);

            let mut generator = BinarySearch::new(&xs, x);
            let b = loop {
                match Pin::new(&mut generator).resume() {
                    GeneratorState::Yielded(_) => (),
                    GeneratorState::Complete(result) => break result,
                }
            };

            a == b
        }
    }

    #[bench]
    fn name(b: &mut test::Bencher) {
        b.iter(|| {
            // ...
        })
    }
}
