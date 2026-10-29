//! Rules engine: game state, events, continuous effects, turn structure, and
//! the decision interface. See `design.md` §4.
//!
//! M0 scaffold: nothing is implemented yet. M1 adds the core loop.

/// Engine version, reported in game records so a replay can name the engine
/// that produced it.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
