//! Identifiers for players, game objects, and effects.

use std::fmt;

use serde::{Deserialize, Serialize};

slotmap::new_key_type! {
    /// Key of a game object in the state arena. An object that changes zones
    /// becomes a new object with a new key (CR 400.7); the old key is kept in
    /// the last-known-information store.
    pub struct ObjId;
}

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
