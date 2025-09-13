#![feature(coroutines, coroutine_trait)]

use coroutines_mem_lookups::binary_search_gen;

use divan::Bencher;
use rand::{rngs::StdRng, Rng, SeedableRng};

use std::ops::{Coroutine, CoroutineState};
use std::pin::Pin;

fn main() {
    divan::main();
}

const SIZES: [usize; 5] = [
    256 * 1024 * 1024,       // 256MiB
    1024 * 1024 * 1024,      // 1GiB
    2 * 1024 * 1024 * 1024,  // 2GiB
    8 * 1024 * 1024 * 1024,  // 8GiB
    20 * 1024 * 1024 * 1024, // 20GiB
];

#[divan::bench(consts = SIZES, args = [1, 100, 500, 1000, 10_000])]
fn basic<const SIZE: usize>(bencher: Bencher, lookups: usize) {
    let mut rng = StdRng::seed_from_u64(42);
    let (vec, niddles) = gen_playground_and_niddles::<SIZE>(&mut rng, lookups);

    bencher.bench(|| {
        for niddle in &niddles {
            let index = divan::black_box(vec.binary_search(niddle).unwrap_or_else(|e| e));
            assert!(index <= vec.len());
        }
    });
}

#[divan::bench(consts = SIZES, args = [1, 100, 500, 1000, 10_000])]
fn coroutine<const SIZE: usize>(bencher: Bencher, lookups: usize) {
    let mut rng = StdRng::seed_from_u64(42);
    let (vec, niddles) = gen_playground_and_niddles::<SIZE>(&mut rng, lookups);

    bencher.bench(|| {
        let mut bss: Vec<_> = niddles
            .iter()
            .map(|v| binary_search_gen(&vec, *v))
            .collect();

        while !bss.is_empty() {
            for i in 0..bss.len() {
                loop {
                    let mut bs = match bss.get_mut(i) {
                        Some(bs) => bs,
                        None => break,
                    };

                    match Pin::new(&mut bs).resume(()) {
                        CoroutineState::Yielded(_) => break,
                        CoroutineState::Complete(res) => {
                            let index = divan::black_box(res.unwrap_or_else(|e| e));
                            assert!(index <= vec.len());
                            let done = bss.swap_remove(i);
                            drop(done);
                        }
                    }
                }
            }
        }
    });
}

fn gen_playground_and_niddles<const SIZE: usize>(
    rng: &mut impl Rng,
    lookups: usize,
) -> (Vec<i32>, Vec<i32>) {
    let playground = gen_playground(rng, SIZE);
    let min = playground.iter().next().unwrap();
    let max = playground.iter().last().unwrap();
    let niddles = gen_niddles(min, max, lookups);
    (playground, niddles)
}

fn gen_niddles(min: &i32, max: &i32, lookups: usize) -> Vec<i32> {
    let mut rng = StdRng::seed_from_u64(42);
    let mut niddles = Vec::with_capacity(lookups as usize);
    for _ in 0..lookups {
        niddles.push(rng.gen_range(min, max));
    }
    niddles
}

fn gen_playground(rng: &mut impl Rng, size: usize) -> Vec<i32> {
    let mut vec = vec![0i32; size / size_of::<i32>()];

    let mut prev = i32::MIN;
    for v in &mut vec {
        *v = prev + rng.gen_range(1, 10);
        prev = *v;
    }

    vec
}
