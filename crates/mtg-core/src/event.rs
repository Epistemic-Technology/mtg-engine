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
    /// An attacking creature became blocked (CR 509.1h), once per combat.
    Blocked {
        attacker: ObjId,
    },
    /// A player revealed their hand (or another player looked at it).
    RevealedHand {
        player: PlayerId,
    },
    /// A face-down permanent was turned face up (CR 708.8).
    TurnedFaceUp {
        obj: ObjId,
    },
    /// A card was revealed (the top of a library, as a creature explores).
    Revealed {
        obj: ObjId,
        def: CardRef,
    },
    /// A double-faced permanent turned to its other face (CR 701.27).
    Transformed {
        obj: ObjId,
    },
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
    /// A spell was countered (CR 701.5).
    Countered {
        spell: ObjId,
        def: CardRef,
    },
    /// Scry or surveil: how many of the looked-at cards went down (to the
    /// bottom or the graveyard).
    Scried {
        player: PlayerId,
        looked: u32,
        down: u32,
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
    /// An activated ability of `source` was put on the stack as `obj`, its
    /// costs paid (CR 602.2).
    Activated {
        obj: ObjId,
        source: ObjId,
        def: CardRef,
    },
    /// A permanent was sacrificed (CR 701.21); the zone change follows.
    Sacrificed {
        obj: ObjId,
        def: CardRef,
    },
    /// A token was created (CR 111.2) under `controller`'s control.
    TokenCreated {
        obj: ObjId,
        def: CardRef,
        controller: PlayerId,
    },
    /// Control of a permanent changed (CR 613.1b).
    ControlChanged {
        obj: ObjId,
        from: PlayerId,
        to: PlayerId,
    },
    /// An Equipment became unattached (CR 704.5n).
    Unattached {
        obj: ObjId,
    },
    /// A legendary permanent was put into its owner's graveyard by the
    /// legend rule (CR 704.5j).
    LegendRule {
        obj: ObjId,
        def: CardRef,
    },
    /// A permanent became the target of spell or ability `by` (CR 115).
    Targeted {
        target: ObjId,
        by: ObjId,
    },
    /// A delayed triggered ability was created (CR 603.7) about `about`.
    DelayedTriggerCreated {
        source: ObjId,
        def: CardRef,
        about: ObjId,
    },
    /// An object became a copy of a permanent with definition `of`
    /// (CR 707).
    BecameCopy {
        obj: ObjId,
        of: CardRef,
    },
    PlayerLost {
        player: PlayerId,
        reason: LossReason,
    },
    /// An emblem was created in the command zone (CR 114).
    EmblemCreated {
        obj: ObjId,
        def: CardRef,
        controller: PlayerId,
    },
    /// A regeneration shield replaced the destruction of `obj` (CR 701.15a).
    Regenerated {
        obj: ObjId,
        by: CardRef,
    },
    /// Damage was redirected from `from` to `to` (Pariah).
    Redirected {
        from: Target,
        to: Target,
        amount: u32,
        by: CardRef,
    },
    GameOver {
        result: GameResult,
    },
}

impl Event {
    /// Which `TriggerSpec::watch_mask` bit this event sets; 0 for events no
    /// trigger watches.
    #[inline]
    pub fn watch_bit(&self) -> u16 {
        use crate::card::{
            WATCH_ATTACK, WATCH_BLOCK, WATCH_CAST, WATCH_DAMAGE, WATCH_DRAW, WATCH_FACE_UP,
            WATCH_LIFE_CHANGE, WATCH_STEP_BEGIN, WATCH_TAP, WATCH_TARGETED, WATCH_ZONE_CHANGE,
        };
        match self {
            Event::ZoneChange { .. } => WATCH_ZONE_CHANGE,
            Event::StepBegin { .. } => WATCH_STEP_BEGIN,
            Event::Attack { .. } => WATCH_ATTACK,
            Event::Block { .. } | Event::Blocked { .. } => WATCH_BLOCK,
            Event::Damage { .. } => WATCH_DAMAGE,
            Event::LifeChange { .. } => WATCH_LIFE_CHANGE,
            Event::Cast { .. } => WATCH_CAST,
            Event::Targeted { .. } => WATCH_TARGETED,
            Event::Tapped { .. } | Event::Untapped { .. } => WATCH_TAP,
            Event::TurnedFaceUp { .. } => WATCH_FACE_UP,
            Event::Draw { .. } => WATCH_DRAW,
            _ => 0,
        }
    }
}
