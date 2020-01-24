#![feature(test, generators, generator_trait)]

#[cfg(test)]
#[macro_use] extern crate quickcheck;

use std::ops::{Generator, GeneratorState};
use std::str::FromStr;
use std::cmp::Ordering::{Less, Equal, Greater};
use std::arch::x86_64::{_mm_prefetch, _MM_HINT_NTA};
use std::pin::Pin;

use std::future::Future;
use std::task::{Context, Poll};

struct FuturePrefetch<'a> {
    fetched: bool,
    reference: &'a i32,
}

impl FuturePrefetch<'_> {
    fn new(reference: &i32) -> FuturePrefetch {
        FuturePrefetch { fetched: false, reference }
    }
}

impl Future for FuturePrefetch<'_> {
    type Output = i32;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        if self.fetched {
            Poll::Ready(*self.reference)
        } else {
            let pointer: *const _ = &*self.reference;
            unsafe { _mm_prefetch(pointer as _, _MM_HINT_NTA) };
            unsafe { *self.map_unchecked_mut(|this| &mut this.fetched) = true };
            ctx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

async fn binary_search(s: &[i32], value: i32) -> Result<usize, usize> {
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
        let cmp = FuturePrefetch::new(reference).await.cmp(&value);
        base = if cmp == Greater { base } else { mid };
        size -= half;
    }
    // base is always in [0, size) because base <= mid.
    let reference = unsafe { s.get_unchecked(base) };
    let cmp = FuturePrefetch::new(reference).await.cmp(&value);
    if cmp == Equal { Ok(base) } else { Err(base + (cmp == Less) as usize) }
}

use std::sync::{Arc, Condvar, Mutex};
use std::task::{RawWaker, RawWakerVTable, Waker};

#[derive(Default)]
struct Park(Mutex<bool>, Condvar);

fn unpark(park: &Park) {
    *park.0.lock().unwrap() = true;
    park.1.notify_one();
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(
    |clone_me| unsafe {
        let arc = Arc::from_raw(clone_me as *const Park);
        std::mem::forget(arc.clone());
        RawWaker::new(Arc::into_raw(arc) as *const (), &VTABLE)
    },
    |wake_me| unsafe { unpark(&Arc::from_raw(wake_me as *const Park)) },
    |wake_by_ref_me| unsafe { unpark(&*(wake_by_ref_me as *const Park)) },
    |drop_me| unsafe { drop(Arc::from_raw(drop_me as *const Park)) },
);

fn run_future<F: Future>(mut fut: F) -> F::Output {
    let mut fut = unsafe { Pin::new_unchecked(&mut fut) };
    let park = Arc::new(Park::default());
    let sender = Arc::into_raw(park.clone());
    let raw_waker = RawWaker::new(sender as *const _, &VTABLE);
    let waker = unsafe { Waker::from_raw(raw_waker) };
    let mut ctx = Context::from_waker(&waker);

    loop {
        match fut.as_mut().poll(&mut ctx) {
            Poll::Pending => {
                let mut runnable = park.0.lock().unwrap();
                while !*runnable {
                    runnable = park.1.wait(runnable).unwrap();
                }
                *runnable = false;
            },
            Poll::Ready(value) => return value,
        }
    }
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
            let pointer: *const _ = &*reference;
            yield unsafe { _mm_prefetch(pointer as _, _MM_HINT_NTA) };
            let cmp = (*reference).cmp(&value);
            base = if cmp == Greater { base } else { mid };
            size -= half;
        }
        // base is always in [0, size) because base <= mid.
        let reference = unsafe { s.get_unchecked(base) };
        let pointer: *const _ = &*reference;
        yield unsafe { _mm_prefetch(pointer as _, _MM_HINT_NTA) };
        let cmp = (*reference).cmp(&value);
        if cmp == Equal { Ok(base) } else { Err(base + (cmp == Less) as usize) }
    }
}

fn main() {
    let vec: Vec<_> = (0..10_000_000).collect();
    let value = std::env::args().nth(1).and_then(|s| i32::from_str(&s).ok()).unwrap_or(10_000);
    let res = run_future(binary_search(&vec, value));
    println!("{:?}", res);

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
            let b = run_future(binary_search(&xs, x));

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
    fn async_one_256mb(b: &mut test::Bencher) {
        let mut rng = StdRng::seed_from_u64(42);

        let value = rng.gen();
        let vec = gen_values(&mut rng, 256*1024*1024); // 256MB

        b.iter(|| {
            let res = run_future(binary_search(&vec, value));
            test::black_box(res)
        })
    }

    #[bench]
    fn basic_multi_256mb(b: &mut test::Bencher) {
        let mut rng = StdRng::seed_from_u64(42);

        let values = gen_values(&mut rng, 16);
        let vec = gen_values(&mut rng, 256*1024*1024); // 256MB

        b.iter(|| {
            for value in &values {
                let res = vec.binary_search(&value);
                let _ = test::black_box(res);
            }
        })
    }

    #[bench]
    fn async_multi_256mb(b: &mut test::Bencher) {
        let mut rng = StdRng::seed_from_u64(42);

        let values = gen_values(&mut rng, 16);
        let vec = gen_values(&mut rng, 256*1024*1024); // 256MB

        b.iter(|| {
            for value in &values {
                let res = run_future(binary_search(&vec, *value));
                let _ = test::black_box(res);
            }
        })
    }
}
