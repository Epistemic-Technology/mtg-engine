//! Recording MTG Arena matches for engine testing.
//!
//! Arena's client log, with detailed logging on, carries the whole
//! conversation between the client and the rules engine: every game state,
//! every decision offered to the player, and every answer. This crate
//!
//! 1. follows the log and archives each match's raw messages
//!    ([`recorder`], [`archive`]);
//! 2. rebuilds the full game state at every step from Arena's state diffs
//!    ([`state`]);
//! 3. translates a recorded game into a [`trace`]: named cards, normalized
//!    snapshots, game events, and the decisions taken, in a form meant to
//!    be replayed against `mtg-core`.

pub mod archive;
pub mod cards;
pub mod log;
pub mod recorder;
pub mod render;
pub mod service;
pub mod state;
pub mod trace;
