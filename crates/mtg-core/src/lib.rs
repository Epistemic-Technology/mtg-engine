//! Rules engine: game state, events, continuous effects, turn structure, and
//! the decision interface. See `design.md` §4.
//!
//! M1 (in progress): core loop with vanilla cards.

pub mod card;
pub mod decision;
pub mod engine;
pub mod event;
pub mod event_text;
pub mod ids;
pub mod layers;
pub mod mana;
pub mod record;
pub mod state;
pub mod turn;
pub mod view;

/// Engine version, reported in game records so a replay can name the engine
/// that produced it.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
