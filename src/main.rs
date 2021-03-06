#![feature(test, generators, generator_trait)]

#![cfg_attr(all(not(target_arch = "x86"), not(target_arch = "x86_64")), feature(core_intrinsics))]

#[cfg(test)]
#[macro_use] extern crate quickcheck;

use std::ops::{Generator, GeneratorState};
use std::str::FromStr;
use std::cmp::Ordering::{Less, Equal, Greater};
use std::pin::Pin;

#[cfg(target_arch = "x86_64")]
fn prefetch<T>(reference: &T) {
    use std::arch::x86_64::{_mm_prefetch, _MM_HINT_NTA};
    let pointer: *const _ = &*reference;
    unsafe { _mm_prefetch(pointer as _, _MM_HINT_NTA) }
}

#[cfg(target_arch = "x86")]
fn prefetch<T>(reference: &T) {
    use std::arch::x86::{_mm_prefetch, _MM_HINT_NTA};
    let pointer: *const _ = &*reference;
    unsafe { _mm_prefetch(pointer as _, _MM_HINT_NTA) }
}

#[cfg(all(not(target_arch = "x86"), not(target_arch = "x86_64")))]
fn prefetch<T>(reference: &T) {
    use std::intrinsics::prefetch_read_data;
    let pointer: *const _ = &*reference;
    let locality = 0;
    unsafe { prefetch_read_data(pointer as _, locality) }
}

fn binary_search_gen(s: &[i32], value: i32) -> impl Generator<Yield=(), Return=Result<usize, usize>> + '_ {
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
        if cmp == Equal { Ok(base) } else { Err(base + (cmp == Less) as usize) }
    }
}

fn main() {
    let vec: Vec<_> = (0..10_000_000).collect();
    let value = std::env::args().nth(1).and_then(|s| i32::from_str(&s).ok()).unwrap_or(10_000);

    let bsa = binary_search_gen(vec.as_slice(), value);
    let bsb = binary_search_gen(vec.as_slice(), value);
    let bss = vec![bsa, bsb];

    for mut bs in bss {
        let res = loop {
            match Pin::new(&mut bs).resume() {
                GeneratorState::Yielded(_) => (),
                GeneratorState::Complete(result) => break result,
            }
        };
        println!("{:?}", res);
    }
}

#[cfg(test)]
mod tests {
    extern crate test;

    use rand::{rngs::StdRng, SeedableRng, Rng};
    use super::*;

    quickcheck! {
        fn qc_easy(xs: Vec<i32>, x: i32) -> bool {
            let mut xs = xs;

            xs.sort_unstable();
            xs.dedup();

            let a = xs.binary_search(&x);
            let mut bs = binary_search_gen(&xs, x);
            let b = loop {
                match Pin::new(&mut bs).resume() {
                    GeneratorState::Yielded(_) => (),
                    GeneratorState::Complete(result) => break result,
                }
            };

            a == b
        }
    }

    fn gen_values(rng: &mut impl Rng, size: usize) -> Vec<i32> {
        let mut vec = vec![0i32; size]; // 256MB

        rng.fill(vec.as_mut_slice());
        vec.sort_unstable();
        vec.dedup();

        vec
    }

    #[bench]
    fn basic_one_256mb(b: &mut test::Bencher) {
        let mut rng = StdRng::seed_from_u64(42);

        let value = rng.gen();
        let vec = gen_values(&mut rng, 256*1024*1024); // 256MB

        b.iter(|| {
            let res = vec.binary_search(&value);
            test::black_box(res)
        })
    }

    #[bench]
    fn gen_one_256mb(b: &mut test::Bencher) {
        let mut rng = StdRng::seed_from_u64(42);

        let value = rng.gen();
        let vec = gen_values(&mut rng, 256*1024*1024); // 256MB

        b.iter(|| {
            let mut bs = binary_search_gen(&vec, value);
            let res = loop {
                match Pin::new(&mut bs).resume() {
                    GeneratorState::Yielded(_) => (),
                    GeneratorState::Complete(result) => break result,
                }
            };
            test::black_box(res)
        })
    }

    #[bench]
    fn basic_100_256mb(b: &mut test::Bencher) {
        let mut rng = StdRng::seed_from_u64(42);

        let values = gen_values(&mut rng, 100);
        let vec = gen_values(&mut rng, 256*1024*1024); // 256MB

        b.iter(|| {
            for value in &values {
                let res = vec.binary_search(&value);
                let _ = test::black_box(res);
            }
        })
    }

    #[bench]
    fn gen_100_256mb(b: &mut test::Bencher) {
        let mut rng = StdRng::seed_from_u64(42);

        let values = gen_values(&mut rng, 100);
        let vec = gen_values(&mut rng, 256*1024*1024); // 256MB

        b.iter(|| {
            let mut bss: Vec<_> = values.iter().map(|v| binary_search_gen(&vec, *v)).collect();

            while !bss.is_empty() {
                for i in 0..bss.len() {
                    loop {
                        let mut bs = match bss.get_mut(i) {
                            Some(bs) => bs,
                            None => break,
                        };

                        match Pin::new(&mut bs).resume() {
                            GeneratorState::Yielded(_) => break,
                            GeneratorState::Complete(_) => {
                                bss.swap_remove(i);
                            },
                        }
                    }
                }
            }
        })
    }
}
