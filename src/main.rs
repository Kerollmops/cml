#![feature(coroutines, coroutine_trait)]

use coroutines_mem_lookups::binary_search_cor;

use std::ops::{Coroutine, CoroutineState};
use std::pin::Pin;

fn main() {
    let vec: Vec<_> = (0..10_000_000).collect();
    let value = std::env::args()
        .nth(1)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(10_000);

    let bsa = binary_search_cor(vec.as_slice(), value);
    let bsb = binary_search_cor(vec.as_slice(), value);
    let bss = vec![bsa, bsb];

    for mut bs in bss {
        let res = loop {
            match Pin::new(&mut bs).resume(()) {
                CoroutineState::Yielded(_) => (),
                CoroutineState::Complete(result) => break result,
            }
        };
        println!("{:?}", res);
    }
}
