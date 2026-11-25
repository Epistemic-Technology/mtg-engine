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
    /// An attacking creature wasn't blocked as blockers were declared (CR
    /// 509.1h): "attacks and isn't blocked".
    Unblocked {
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
    /// A creature explored (CR 701.44), after its reveal and its result.
    Explored {
        obj: ObjId,
    },
    /// `crew` was tapped to pay the cost of `vehicle`'s crew or saddle
    /// ability: it "crews a Vehicle" or "saddles a Mount" (CR 702.122b,
    /// 702.171c).
    Crewed {
        crew: ObjId,
        vehicle: ObjId,
    },
    /// A Mount became saddled (CR 702.171b).
    BecameSaddled {
        obj: ObjId,
    },
    /// A spell or ability `player` controls tapped an untapped permanent
    /// (just after its `Tapped` event): "Whenever you tap an untapped
    /// creature an opponent controls".
    TappedBy {
        obj: ObjId,
        player: PlayerId,
    },
    /// A Room's door was unlocked (CR 709.5c-i), as it entered or later:
    /// `door` 0 is the left half; `fully`, both are now unlocked.
    Unlocked {
        obj: ObjId,
        door: u8,
        fully: bool,
    },
    /// A Class gained a level or a Case became solved (`Effect::BecomeStage`).
    StageChanged {
        obj: ObjId,
        stage: u8,
    },
    /// A player collected evidence (CR 701.59a).
    /// A player discovered (CR 701.57), for this value.
    Discovered {
        player: PlayerId,
        value: u32,
    },
    /// A player gave a gift (CR 702.174: as the spell resolves, or as the
    /// permanent's gift trigger resolves).
    GaveGift {
        player: PlayerId,
    },
    /// A player manifested dread (CR 701.62), with the card they put into
    /// their graveyard this way, if any ("Whenever you manifest dread", it
    /// triggers even if nothing could be manifested).
    ManifestedDread {
        player: PlayerId,
        milled: Option<ObjId>,
    },
    CollectedEvidence {
        player: PlayerId,
    },
    /// A keyword action performed (`TriggerSpec::Acts`): by `player`, of
    /// `obj` for an object's action, `n` the number involved (energy
    /// gotten, a monstrosity's N).
    Acted {
        player: PlayerId,
        obj: Option<ObjId>,
        action: crate::card::KeywordAction,
        n: u32,
    },
    /// A player foraged (CR 701.61a).
    Foraged {
        player: PlayerId,
    },
    /// A player searched their own library (CR 701.19).
    Searched {
        player: PlayerId,
    },
    /// A player committed a crime (CR 700.13): they cast a spell,
    /// activated an ability, or put a triggered ability on the stack that
    /// targets an opponent, something an opponent controls, or a card in
    /// an opponent's graveyard. Once per spell or ability.
    CommittedCrime {
        player: PlayerId,
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
        /// The zone it was played from (CR 305.1: the hand, or a zone a
        /// permission names).
        from: Zone,
    },
    Cast {
        player: PlayerId,
        spell: ObjId,
        def: CardRef,
        targets: Vec<Target>,
    },
    /// A copy of a spell put onto the stack (CR 707.10): not cast, but
    /// "whenever you cast or copy" (magecraft) sees it.
    SpellCopied {
        player: PlayerId,
        spell: ObjId,
        def: CardRef,
    },
    Tapped {
        obj: ObjId,
    },
    /// A creature was tapped to pay a teamwork cost, just after its
    /// `Tapped` event (Agent Maria Hill).
    TappedForTeamwork {
        obj: ObjId,
    },
    /// A permanent was tapped for mana by a mana ability (CR 106.11a),
    /// just after its mana was added. Emitted only while a permanent with
    /// a tapped-for-mana trigger is on the battlefield
    /// (`GameState::mana_trigger_sources`).
    TappedForMana {
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
        /// Surveil rather than scry.
        #[serde(default)]
        surveil: bool,
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
    /// A player flipped a coin (CR 705) and won or lost the flip.
    FlippedCoin {
        player: PlayerId,
        won: bool,
    },
    /// A player rolled a die (CR 706): its number of sides and the result.
    RolledDie {
        player: PlayerId,
        sides: u8,
        result: u8,
    },
    Destroyed {
        obj: ObjId,
        def: CardRef,
    },
    Discard {
        player: PlayerId,
        card: ObjId,
    },
    /// `player` cycled a card (CR 702.29): discarded it to pay a cycling
    /// cost. `card` is the card in the zone it was discarded to; `x` the
    /// X paid for the cycling cost ("cycling {X}{G}{G}").
    Cycled {
        player: PlayerId,
        card: ObjId,
        def: CardRef,
        x: u32,
    },
    EffectEnded {
        affected: ObjId,
    },
    CountersAdded {
        obj: ObjId,
        kind: CounterKind,
        n: u32,
        /// Who put them (CR 122.6): the controller of the effect, or of
        /// the permanent for counters it enters with or puts as a cost.
        by: PlayerId,
        /// How many of that kind it had before (a Saga's chapters, CR
        /// 714.2c).
        before: u32,
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
    /// A player became the target of spell or ability `by` (CR 115),
    /// reported only while a permanent with
    /// `TriggerSpec::TargetedByYourAbility` is on the battlefield.
    PlayerTargeted {
        player: PlayerId,
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
    /// It became day or night (CR 731); `changed` if it was the other
    /// (for "whenever day becomes night or night becomes day").
    DayNight {
        day: bool,
        changed: bool,
    },
    /// A player became the monarch (CR 725).
    BecameMonarch {
        player: PlayerId,
    },
    /// A player took the initiative (CR 726).
    TookInitiative {
        player: PlayerId,
    },
    /// The Ring tempted a player (CR 701.54), who chose `bearer`.
    RingTempted {
        player: PlayerId,
        bearer: Option<ObjId>,
    },
    /// A player's venture marker moved into a room of a dungeon (CR 309.5).
    Ventured {
        player: PlayerId,
        dungeon: CardRef,
        room: u8,
    },
    /// A player completed a dungeon (CR 309.7).
    CompletedDungeon {
        player: PlayerId,
        dungeon: CardRef,
    },
    /// A player's speed changed (CR 702.179).
    Speed {
        player: PlayerId,
        speed: u8,
    },
    /// A player got the city's blessing (CR 702.131).
    CityBlessing {
        player: PlayerId,
    },
    /// A player got an enduring story (CR 702.195).
    EnduringStory {
        player: PlayerId,
    },
    GameOver {
        result: GameResult,
    },
}

impl Event {
    /// Which `TriggerSpec::watch_mask` bit this event sets; 0 for events no
    /// trigger watches.
    #[inline]
    pub fn watch_bit(&self) -> u32 {
        use crate::card::{
            WATCH_ACT, WATCH_ATTACH, WATCH_ATTACK, WATCH_BLOCK, WATCH_CAST, WATCH_COUNTERS,
            WATCH_CYCLE, WATCH_DAMAGE, WATCH_DESIG, WATCH_DISCARD, WATCH_DRAW, WATCH_FACE_UP,
            WATCH_LIFE_CHANGE, WATCH_ROLL, WATCH_SACRIFICE, WATCH_STEP_BEGIN, WATCH_TAP,
            WATCH_TARGETED, WATCH_ZONE_CHANGE,
        };
        match self {
            Event::ZoneChange { .. } => WATCH_ZONE_CHANGE,
            Event::StepBegin { .. } => WATCH_STEP_BEGIN,
            Event::Attack { .. } => WATCH_ATTACK,
            Event::Block { .. } | Event::Blocked { .. } | Event::Unblocked { .. } => WATCH_BLOCK,
            Event::Damage { .. } => WATCH_DAMAGE,
            Event::LifeChange { .. } => WATCH_LIFE_CHANGE,
            Event::Cast { .. } | Event::SpellCopied { .. } | Event::LandPlayed { .. } => WATCH_CAST,
            Event::Targeted { .. }
            | Event::PlayerTargeted { .. }
            | Event::CommittedCrime { .. } => WATCH_TARGETED,
            Event::Tapped { .. }
            | Event::Untapped { .. }
            | Event::TappedForMana { .. }
            | Event::TappedForTeamwork { .. } => WATCH_TAP,
            Event::TurnedFaceUp { .. } => WATCH_FACE_UP,
            Event::Draw { .. } => WATCH_DRAW,
            Event::Cycled { .. } => WATCH_CYCLE,
            Event::Discard { .. } => WATCH_DISCARD,
            Event::Attached { .. } => WATCH_ATTACH,
            Event::RolledDie { .. } | Event::FlippedCoin { .. } => WATCH_ROLL,
            Event::DayNight { .. }
            | Event::Unlocked { .. }
            | Event::StageChanged { .. }
            | Event::CollectedEvidence { .. }
            | Event::ManifestedDread { .. }
            | Event::GaveGift { .. }
            | Event::Discovered { .. }
            | Event::Foraged { .. }
            | Event::Searched { .. }
            | Event::ControlChanged { .. } => WATCH_DESIG,
            Event::Sacrificed { .. } => WATCH_SACRIFICE,
            Event::CountersAdded { .. } | Event::CountersRemoved { .. } => WATCH_COUNTERS,
            Event::Scried { .. }
            | Event::Explored { .. }
            | Event::Crewed { .. }
            | Event::BecameSaddled { .. }
            | Event::Activated { .. }
            | Event::Transformed { .. }
            | Event::Acted { .. }
            | Event::TappedBy { .. } => WATCH_ACT,
            _ => 0,
        }
    }
}
