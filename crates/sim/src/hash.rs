//! State hashing for desync detection.
//!
//! FNV-1a over a canonical byte serialisation of whatever implements
//! [`HashState`]. Not cryptographic and not meant to be: it exists so two
//! machines (or two runs) can cheaply agree that they are still in the same
//! world, and so a replay test can name the exact tick where they stopped.

use crate::fx::Fx;
use crate::vec2::Vec2Fx;

/// Accumulates a 64-bit FNV-1a hash.
#[derive(Clone, Debug)]
pub struct StateHasher {
    h: u64,
}

const OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01B3;

impl Default for StateHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl StateHasher {
    /// A fresh hasher.
    pub const fn new() -> StateHasher {
        StateHasher { h: OFFSET }
    }

    /// Feeds raw bytes.
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.h ^= b as u64;
            self.h = self.h.wrapping_mul(PRIME);
        }
    }

    /// Feeds a `u8`.
    pub fn write_u8(&mut self, v: u8) {
        self.write_bytes(&[v]);
    }
    /// Feeds a `u16` (little-endian).
    pub fn write_u16(&mut self, v: u16) {
        self.write_bytes(&v.to_le_bytes());
    }
    /// Feeds a `u32` (little-endian).
    pub fn write_u32(&mut self, v: u32) {
        self.write_bytes(&v.to_le_bytes());
    }
    /// Feeds a `u64` (little-endian).
    pub fn write_u64(&mut self, v: u64) {
        self.write_bytes(&v.to_le_bytes());
    }
    /// Feeds an `i32` (little-endian).
    pub fn write_i32(&mut self, v: i32) {
        self.write_bytes(&v.to_le_bytes());
    }
    /// Feeds an `i64` (little-endian).
    pub fn write_i64(&mut self, v: i64) {
        self.write_bytes(&v.to_le_bytes());
    }
    /// Feeds a `bool` as one byte.
    pub fn write_bool(&mut self, v: bool) {
        self.write_u8(v as u8);
    }
    /// Feeds anything hashable.
    pub fn write<T: HashState + ?Sized>(&mut self, v: &T) {
        v.hash_state(self);
    }

    /// The hash so far.
    pub const fn finish(&self) -> u64 {
        self.h
    }
}

/// Types whose simulation-relevant state can be fed to a [`StateHasher`].
///
/// Implementations must be **canonical**: equal state, equal bytes, on every
/// platform. Never hash pointers, capacities, or anything derived from
/// allocation.
pub trait HashState {
    /// Feeds this value's state into `h`.
    fn hash_state(&self, h: &mut StateHasher);
}

impl HashState for Fx {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_i32(self.raw());
    }
}
impl HashState for Vec2Fx {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_i32(self.x.raw());
        h.write_i32(self.y.raw());
    }
}
impl HashState for bool {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_bool(*self);
    }
}
impl HashState for u8 {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(*self);
    }
}
impl HashState for u16 {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(*self);
    }
}
impl HashState for u32 {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(*self);
    }
}
impl HashState for u64 {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(*self);
    }
}
impl HashState for i32 {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_i32(*self);
    }
}
impl HashState for i64 {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_i64(*self);
    }
}
impl<T: HashState> HashState for Option<T> {
    fn hash_state(&self, h: &mut StateHasher) {
        match self {
            None => h.write_u8(0),
            Some(v) => {
                h.write_u8(1);
                v.hash_state(h);
            }
        }
    }
}
impl<T: HashState> HashState for [T] {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.len() as u64);
        for v in self {
            v.hash_state(h);
        }
    }
}
impl<T: HashState> HashState for Vec<T> {
    fn hash_state(&self, h: &mut StateHasher) {
        self.as_slice().hash_state(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_known_answers() {
        // Standard FNV-1a 64 test vectors.
        assert_eq!(StateHasher::new().finish(), 0xCBF2_9CE4_8422_2325);
        let mut h = StateHasher::new();
        h.write_bytes(b"a");
        assert_eq!(h.finish(), 0xAF63_DC4C_8601_EC8C);
        let mut h = StateHasher::new();
        h.write_bytes(b"foobar");
        assert_eq!(h.finish(), 0x85944171F73967E8);
    }

    #[test]
    fn different_inputs_different_hashes() {
        let mut a = StateHasher::new();
        a.write(&Fx::ONE);
        let mut b = StateHasher::new();
        b.write(&Fx::TWO);
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn options_and_slices_are_length_prefixed() {
        let mut a = StateHasher::new();
        a.write(&vec![Fx::ONE, Fx::ZERO]);
        let mut b = StateHasher::new();
        b.write(&vec![Fx::ONE]);
        b.write(&Fx::ZERO);
        assert_ne!(a.finish(), b.finish());

        let mut c = StateHasher::new();
        c.write(&Some(Fx::ZERO));
        let mut d = StateHasher::new();
        d.write(&None::<Fx>);
        assert_ne!(c.finish(), d.finish());
    }
}
