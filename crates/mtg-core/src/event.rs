//! Game events. Every change to the game is an event; the log is the replay
//! and the LLM-facing narration (design §4.5). Replacement and prevention
//! effects run before an event happens (`engine::replace`); trigger
//! detection runs over every event as it is emitted (`GameState::emit`).

use serde::{Deserialize, Serialize};

use crate::card::{AbilityId, CardRef, CounterKind};
use crate::ids::{ObjId, PlayerId};
use crate::state::{GameResult, LossReason, Target, Zone};
use crate::turn::Step;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    GameStart {
        starting_player: PlayerId,
    },
    MulliganTaken {
        player: PlayerId,
        count: u8,
    },
    HandKept {
        player: PlayerId,
        bottomed: u8,
    },
    TurnBegin {
        turn: u32,
        player: PlayerId,
    },
    StepBegin {
        step: Step,
    },
    Draw {
        player: PlayerId,
        card: ObjId,
    },
    DrawFromEmptyLibrary {
        player: PlayerId,
    },
    ZoneChange {
        old: ObjId,
        new: ObjId,
        def: CardRef,
        from: Zone,
        to: Zone,
    },
    LandPlayed {
        player: PlayerId,
        land: ObjId,
    },
    Cast {
        player: PlayerId,
        spell: ObjId,
        def: CardRef,
        targets: Vec<Target>,
    },
    Tapped {
        obj: ObjId,
    },
    Untapped {
        obj: ObjId,
    },
    Pass {
        player: PlayerId,
    },
    Resolve {
        spell: ObjId,
        def: CardRef,
    },
    Fizzle {
        spell: ObjId,
        def: CardRef,
    },
    Attack {
        attacker: ObjId,
        target: Target,
    },
    Block {
        blocker: ObjId,
        attacker: ObjId,
    },
    Damage {
        source: ObjId,
        target: Target,
        amount: u32,
        combat: bool,
    },
    LifeChange {
        player: PlayerId,
        delta: i32,
        life: i32,
    },
    Destroyed {
        obj: ObjId,
        def: CardRef,
    },
    Discard {
        player: PlayerId,
        card: ObjId,
    },
    EffectEnded {
        affected: ObjId,
    },
    CountersAdded {
        obj: ObjId,
        kind: CounterKind,
        n: u32,
    },
    CountersRemoved {
        obj: ObjId,
        kind: CounterKind,
        n: u32,
    },
    Attached {
        obj: ObjId,
        to: ObjId,
    },
    /// A triggered ability triggered (CR 603.2); it waits for the next time
    /// a player would receive priority.
    Triggered {
        source: ObjId,
        ability: AbilityId,
        controller: PlayerId,
    },
    /// A triggered ability was put on the stack as object `obj` (CR 603.3).
    TriggerOnStack {
        obj: ObjId,
        source: ObjId,
        def: CardRef,
    },
    /// A triggered ability was removed without going on the stack or
    /// without resolving: no legal targets (CR 603.3d) or its intervening
    /// "if" no longer holds (CR 603.4).
    TriggerRemoved {
        source: ObjId,
        def: CardRef,
    },
    /// A prevention effect of `by` prevented `amount` damage from `source`
    /// to `target` (CR 615.6).
    Prevented {
        source: ObjId,
        target: Target,
        amount: u32,
        by: CardRef,
    },
    /// A replacement effect of `by` modified an event (CR 614.6).
    Replaced {
        by: CardRef,
    },
    /// An effect made `player` win the game (CR 104.2b).
    PlayerWon {
        player: PlayerId,
    },
    PlayerLost {
        player: PlayerId,
        reason: LossReason,
    },
    GameOver {
        result: GameResult,
    },
}
