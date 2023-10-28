use std::cell::RefCell;

use rand::seq::SliceRandom;
use rand::SeedableRng;

use rand_xoshiro::SplitMix64 as TheRng;

fn init_rng() -> TheRng {
    let mut buf = [0; 8];
    getrandom::getrandom(&mut buf).unwrap();
    TheRng::from_seed(buf)
}

pub(crate) fn get_shuffled_indices(size: usize) -> Vec<usize> {
    thread_local!(static RNG: RefCell<TheRng> = RefCell::new(init_rng()));
    RNG.with_borrow_mut(|rng| {
        let mut seq = (0..size).collect::<Vec<_>>();
        seq.shuffle(rng);
        seq
    })
}
