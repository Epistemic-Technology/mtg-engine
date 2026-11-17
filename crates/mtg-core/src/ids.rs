//! Identifiers for players, game objects, and effects.

use std::fmt;

use serde::{Deserialize, Serialize};

slotmap::new_key_type! {
    /// Key of a game object in the state arena. An object that changes zones
    /// becomes a new object with a new key (CR 400.7); the old key is kept in
    /// the last-known-information store.
    pub struct ObjId;
}

/// Hasher for maps keyed by `ObjId` (two `u32` writes: index and version).
/// The default `SipHash` cost 12–21% of effect-heavy games; keys here are
/// small, dense, and never adversarial, so one multiply suffices.
#[derive(Default, Clone, Copy)]
pub struct IdHasher(u64);

impl std::hash::Hasher for IdHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.write_u64(u64::from(*b));
        }
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.write_u64(u64::from(i));
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.0 = (self.0.rotate_left(32) ^ i).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

/// A hash map keyed by `ObjId` using [`IdHasher`].
pub type IdMap<V> = std::collections::HashMap<ObjId, V, std::hash::BuildHasherDefault<IdHasher>>;

/// Seat index. Two players in M1; the type is sized for more.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct PlayerId(pub u8);

impl PlayerId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "P{}", self.0 + 1)
    }
}

/// Monotonic timestamp assigned when an object enters the battlefield or an
/// effect begins (CR 613.7).
pub type Timestamp = u64;
