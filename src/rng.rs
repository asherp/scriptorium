// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Deterministic RNG, seeded from a hex string (a block hash, a txid, any
// stable identifier the host has).
//
// splitmix32: small, fast, good enough scatter for branch/leaf choice — not
// cryptographic, and doesn't need to be. The same seed always yields the same
// stream, which is the whole point: two readers looking at the same source
// see the same illumination.

/// Anything the turtle can draw a number in `[0, 1)` from.
///
/// The walk is written against this rather than against [`Rng`] directly so a
/// test can pin the stream flat (see [`Fixed`]) and ask about the *geometry*
/// of a collision response rather than about a particular random draw.
pub trait Rand {
    fn next_f64(&mut self) -> f64;
}

/// A constant stream — `Fixed(0.5)` puts every jitter and deflection exactly
/// on the unturned heading.
#[derive(Clone, Copy, Debug)]
pub struct Fixed(pub f64);

impl Rand for Fixed {
    fn next_f64(&mut self) -> f64 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub struct Rng {
    state: u32,
}

impl Rng {
    /// Seeds from the hex digits of `seed`, ignoring everything else — so a
    /// composite key (`"<hash>:<stage>:<x>,<y>"`) can be handed straight in.
    pub fn from_hex(seed: &str) -> Self {
        let mut state: u32 = 0x9e37_79b9;
        let mut any = false;
        for ch in seed.chars() {
            if !ch.is_ascii_hexdigit() {
                continue;
            }
            any = true;
            state = (state ^ (ch as u32)).wrapping_mul(0x0100_0193);
        }
        if !any {
            // The empty-after-cleaning case: the reference implementation
            // substitutes the single character '0' rather than seeding from
            // nothing at all.
            state = (0x9e37_79b9u32 ^ ('0' as u32)).wrapping_mul(0x0100_0193);
        }
        Rng { state }
    }
}

impl Rand for Rng {
    fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6D2B_79F5);
        let mut t = self.state;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_yields_the_same_stream() {
        let mut a = Rng::from_hex("deadbeef");
        let mut b = Rng::from_hex("deadbeef");
        for _ in 0..32 {
            assert_eq!(a.next_f64(), b.next_f64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::from_hex("1111");
        let mut b = Rng::from_hex("2222");
        assert_ne!(a.next_f64(), b.next_f64());
    }

    #[test]
    fn draws_stay_in_the_unit_interval() {
        let mut r = Rng::from_hex("00000000000000000009a5b2b9c4de6c");
        for _ in 0..10_000 {
            let v = r.next_f64();
            assert!((0.0..1.0).contains(&v), "draw {v} left [0,1)");
        }
    }

    #[test]
    fn non_hex_characters_are_ignored_not_mixed_in() {
        // The composite per-anchor key is hashed through this: only its hex
        // digits count, so the separators may change shape freely.
        let mut a = Rng::from_hex("abc:123");
        let mut b = Rng::from_hex("abc123");
        assert_eq!(a.next_f64(), b.next_f64());
    }
}
