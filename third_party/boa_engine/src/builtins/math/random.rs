//! A per-thread xoshiro256++ generator, seeded from the standard library's random hasher keys, for `Math.random`.

use std::{
    cell::Cell,
    hash::{BuildHasher, RandomState},
};

thread_local! {
    static STATE: Cell<[u64; 4]> = Cell::new(seed());
}

fn seed() -> [u64; 4] {
    let keys = RandomState::new();
    let mut state = [0u64; 4];
    for (index, word) in state.iter_mut().enumerate() {
        *word = splitmix64(keys.hash_one(index));
    }
    if state == [0; 4] {
        state[0] = 1;
    }
    state
}

const fn splitmix64(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn next_u64() -> u64 {
    STATE.with(|cell| {
        let mut s = cell.get();
        let result = s[0].wrapping_add(s[3]).rotate_left(23).wrapping_add(s[0]);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        cell.set(s);
        result
    })
}

/// Returns a uniformly distributed `f64` in `[0, 1)`.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn random_f64() -> f64 {
    (next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}
