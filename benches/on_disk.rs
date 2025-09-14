#![feature(coroutines, coroutine_trait)]

use bytemuck::checked::cast_slice;
use coroutines_mem_lookups::{binary_search_yield_offsets_cor, load_pages_at_offsets};

use divan::Bencher;
use memmap2::Mmap;
use rand::{Rng, SeedableRng, rngs::StdRng};

use std::fs::File;
use std::io::{self, Write as _};
use std::ops::{Coroutine, CoroutineState};
use std::pin::Pin;

const SEED: u64 = 42;

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
    let mut rng = StdRng::seed_from_u64(SEED);
    let (_playground_file, playground_mmap, niddles) =
        gen_playground_and_niddles::<SIZE>(&mut rng, lookups).unwrap();
    let playground = cast_slice(&playground_mmap[..]);

    bencher.bench(|| {
        for niddle in &niddles {
            let index = divan::black_box(playground.binary_search(niddle).unwrap_or_else(|e| e));
            assert!(index <= playground.len());
        }
    });
}

#[divan::bench(consts = SIZES, args = [1, 100, 500, 1000, 10_000])]
fn coroutine<const SIZE: usize>(bencher: Bencher, lookups: usize) {
    let mut rng = StdRng::seed_from_u64(SEED);
    let (playground_file, playground_mmap, niddles) =
        gen_playground_and_niddles::<SIZE>(&mut rng, lookups).unwrap();
    let playground = cast_slice(&playground_mmap[..]);

    bencher.bench(|| {
        let mut bss: Vec<_> = niddles
            .iter()
            .map(|v| binary_search_yield_offsets_cor(&playground, *v))
            .collect();
        let mut offsets_to_load = Vec::with_capacity(bss.len());

        while !bss.is_empty() {
            offsets_to_load.clear();
            for i in 0..bss.len() {
                loop {
                    let mut bs = match bss.get_mut(i) {
                        Some(bs) => bs,
                        None => break,
                    };

                    match Pin::new(&mut bs).resume(()) {
                        CoroutineState::Yielded(i32_offset) => {
                            // convert offset
                            offsets_to_load.push(i32_offset * size_of::<i32>());
                            break;
                        }
                        CoroutineState::Complete(res) => {
                            let index = divan::black_box(res.unwrap_or_else(|e| e));
                            assert!(index <= playground.len());
                            let done = bss.swap_remove(i);
                            drop(done);
                        }
                    }
                }
            }

            // Once we fetched all the offsets to load, load them
            #[cfg(not(target_os = "linux"))]
            load_pages_at_offsets(&playground_mmap, &offsets_to_load).unwrap();
            #[cfg(target_os = "linux")]
            load_pages_at_offsets(&playground_file, &offsets_to_load).unwrap();
        }
    });
}

fn gen_playground_and_niddles<const SIZE: usize>(
    rng: &mut impl Rng,
    lookups: usize,
) -> io::Result<(File, Mmap, Vec<i64>)> {
    let (file, playground_mmap) = gen_playground(rng, SIZE)?;
    let playground = cast_slice(&playground_mmap[..]);
    let min = playground.iter().next().unwrap();
    let max = playground.iter().last().unwrap();
    let niddles = gen_niddles(min, max, lookups);
    Ok((file, playground_mmap, niddles))
}

fn gen_niddles(min: &i64, max: &i64, lookups: usize) -> Vec<i64> {
    let mut rng = StdRng::seed_from_u64(42);
    let mut niddles = Vec::with_capacity(lookups as usize);
    for _ in 0..lookups {
        niddles.push(rng.gen_range(min, max));
    }
    niddles
}

fn gen_playground(rng: &mut impl Rng, mut size: usize) -> io::Result<(File, Mmap)> {
    const BLOCK_SIZE: usize = 4 * 1024 * 1024; // 4GiB

    let mut file = tempfile::tempfile()?;
    file.set_len((size * size_of::<i64>()) as u64)?;

    let mut vec = vec![0; BLOCK_SIZE];
    let mut prev = i64::MIN;
    while size > 0 {
        let block_size = size.min(BLOCK_SIZE);

        for v in &mut vec[..block_size] {
            *v = prev.checked_add(rng.gen_range(1, 10)).unwrap();
            prev = *v;
        }

        let bytes = bytemuck::cast_slice(&vec[..block_size]);
        file.write_all(bytes)?;
        size = size.checked_sub(block_size).unwrap();
    }

    let mmap = unsafe { Mmap::map(&file) }?;
    Ok((file, mmap))
}
