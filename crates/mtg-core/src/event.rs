//! Game events. Every change to the game is an event; the log is the replay
//! and the LLM-facing narration (design §4.5). M1 records events but does
//! not yet run replacement effects or trigger detection over them.

use serde::{Deserialize, Serialize};

use crate::card::{CardRef, CounterKind};
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
    Attached {
        obj: ObjId,
        to: ObjId,
    },
    PlayerLost {
        player: PlayerId,
        reason: LossReason,
    },
    GameOver {
        result: GameResult,
    },
}
