//! The decision interface: what the engine hands to a player and what it
//! accepts back (design §4.7).

use serde::{Deserialize, Serialize};

use crate::card::{AbilityDef, AbilityId, CardRef};
use crate::ids::{ObjId, PlayerId};
use crate::state::{GameResult, GameState, Target};

/// Index into `Decision::options`.
pub type ChoiceId = u32;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum DecisionKind {
    /// Keep or mulligan the opening hand (CR 103.5).
    Mulligan,
    /// Put a card from hand on the bottom of the library after mulliganing.
    Bottom,
    /// Take an action with priority or pass (CR 117).
    PriorityAction,
    /// Choose the `index`th target of `spell` (CR 601.2c).
    ChooseTarget { spell: ObjId, index: usize },
    /// Declare attackers (CR 508.1).
    DeclareAttackers,
    /// Declare blockers (CR 509.1).
    DeclareBlockers,
    /// Divide `attacker`'s combat damage among its blockers (CR 510.1c),
    /// or a blocker's among the attackers it blocks (`attacker` is then
    /// the blocker, CR 510.1d).
    AssignCombatDamage { attacker: ObjId },
    /// Discard to maximum hand size (CR 514.1).
    Discard,
    /// Choose which of several triggered abilities to put on the stack next
    /// (CR 603.3b). The one chosen first resolves last.
    OrderTriggers,
    /// Several replacement or prevention effects apply to one event: the
    /// affected player chooses which applies first (CR 616.1).
    ChooseReplacement,
    /// A player controls two or more legendary permanents with the same
    /// name and chooses which to keep (CR 704.5j).
    LegendRule,
    /// Choose the mode of a modal spell being cast (CR 601.2b, 700.2).
    ChooseMode { spell: ObjId },
    /// A "you may" clause of `def` as a spell or ability resolves.
    YouMay { def: CardRef },
    /// What a permanent enters as a copy of (CR 707; Clone), or nothing.
    ChooseCopy,
    /// X for a spell with {X} in its mana cost (CR 601.2b); the options are
    /// the values the player can pay for.
    ChooseX { spell: ObjId },
    /// A permanent to sacrifice as an additional cost of casting the spell
    /// (CR 601.2h).
    ChooseSacrifice { spell: ObjId },
    /// A card to discard as an additional cost of casting the spell.
    ChooseDiscardCost { spell: ObjId },
    /// The player or planeswalker `attacker` attacks (CR 508.1a).
    ChooseAttackTarget { attacker: ObjId },
    /// Scry: which of the top cards go to the bottom (CR 701.18).
    Scry { count: u32 },
    /// Surveil: which of the top cards go to the graveyard (CR 701.42).
    Surveil { count: u32 },
    /// A color: of one mana of any color an ability adds, or a color
    /// chosen as a permanent enters or an effect resolves ("the color of
    /// your choice").
    ChooseColor,
    /// A creature type or basic land type, chosen as a permanent enters or
    /// an effect resolves ("the creature type of your choice"):
    /// `Subtype` options.
    ChooseType,
    /// A permanent to sacrifice for an edict ("sacrifices a creature of
    /// their choice"): `Sacrifice` options.
    Edict,
    /// A card to find in a library search (CR 701.19): `Target` of a
    /// library card, or `Decline` to find nothing.
    SearchLibrary,
    /// "Counter target spell unless its controller pays ...": pay or not.
    PayOrCounter { def: CardRef },
    /// Whether the owner puts `def` on top of their library (or Nth from
    /// the top): `Accept`; or on the bottom: `Decline`.
    TopOrBottom { def: CardRef },
    /// Which of `spell`'s two additional costs to pay (CR 601.2b):
    /// `Mode(0)` the first printed, `Mode(1)` the other; only those that
    /// can be paid.
    ChooseAdditionalCost { spell: ObjId },
    /// Whether to pay `spell`'s kicker (CR 702.33): `Accept` kicks.
    Kicker { spell: ObjId },
    /// Whether to pay a spell's buyback cost (CR 702.27).
    Buyback { spell: ObjId },
    /// Whether to pay a spell's offspring cost (CR 702.175a).
    Offspring { spell: ObjId },
    /// Whether to bargain a spell (CR 702.166a): `Sacrifice` options for
    /// the artifact, enchantment, or token, or `Decline`.
    Bargain { spell: ObjId },
    /// Which card from the hand to put onto the battlefield (offered as
    /// `Choice::Discard` options), or `Done` for none.
    PutFromHand,
    /// Whether to cast an exiled card now (madness, suspend, rebound,
    /// cascade): `Accept` casts it.
    CastFromExile { card: ObjId },
    /// How `spell` divides its damage or counters among its targets as it
    /// is cast (CR 601.2d): `Divide` options.
    Divide { spell: ObjId },
    /// An object for an effect to act on as it resolves ("tap another
    /// untapped creature you control", "return a creature card from your
    /// graveyard to your hand"): `Sacrifice` options for permanents,
    /// `Discard` options for cards, and `Done` to stop where the effect
    /// says "up to".
    ChooseCard,
    /// Which of `def`'s alternatives to do as it resolves ("sacrifice an
    /// artifact or discard a card", "your choice of lifelink or
    /// indestructible"): `Mode` options.
    ChooseOption { def: CardRef },
    /// A card in hand with splice onto Arcane to splice onto `spell` as it
    /// is cast (CR 702.47a): `Discard` options naming the cards, or `Done`.
    Splice { spell: ObjId },
    /// Whether to conspire `spell` (CR 702.78a): `Accept` taps two
    /// untapped creatures the caster controls that share a color with it
    /// (the engine picks them), `Decline` doesn't.
    Conspire { spell: ObjId },
    /// The `index`th target of `spell`, a copy of a spell (CR 707.10c):
    /// `Target` options, the copied target first (keep it).
    CopyTarget { spell: ObjId, index: usize },
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Choice {
    Keep,
    Mulligan,
    Bottom(ObjId),
    Pass,
    PlayLand(ObjId),
    Cast(ObjId),
    Target(Target),
    /// A complete attacker declaration.
    Attackers(Vec<ObjId>),
    /// Incremental declaration: add one attacker.
    AddAttacker(ObjId),
    /// A complete blocker declaration as (blocker, attacker) pairs.
    Blocks(Vec<(ObjId, ObjId)>),
    /// Incremental declaration: one creature blocks one attacker.
    AddBlock {
        blocker: ObjId,
        attacker: ObjId,
    },
    /// Finish an incremental declaration.
    Done,
    /// Combat damage division as (recipient, amount) pairs.
    Division(Vec<(Target, u32)>),
    /// Discard this card; `def` names it when the state shown predates
    /// the question (a card drawn by the same resolution, see
    /// `KeepLegend`).
    Discard {
        card: ObjId,
        def: CardRef,
    },
    /// Put the pending triggered ability at this index of
    /// `GameState::pending_triggers` on the stack.
    Trigger(usize),
    /// Apply the `index`th applicable replacement effect, from card `def`.
    Replacement {
        index: usize,
        def: CardRef,
    },
    /// Activate an ability of a permanent (CR 602); `sacrifice` is the
    /// permanent chosen for a "sacrifice a Goblin" cost.
    Activate {
        source: ObjId,
        ability: AbilityId,
        sacrifice: Option<ObjId>,
        /// The card chosen for a "discard a card" cost.
        discard: Option<ObjId>,
    },
    /// Keep this legendary permanent; the others with its name go to the
    /// graveyard (CR 704.5j). Asked in the middle of an action and so
    /// possibly shown against the state before it: the card and the
    /// permanent's rank by age (0 = oldest) describe it without the object.
    KeepLegend {
        obj: ObjId,
        def: CardRef,
        age: usize,
    },
    /// Cast a modal spell with this mode (an index into its modes).
    Mode(usize),
    /// Do the optional thing.
    Accept,
    /// Do not.
    Decline,
    /// Enter as a copy of this permanent; `def` names it when the state
    /// shown predates the question (see `KeepLegend`).
    CopyOf {
        obj: ObjId,
        def: CardRef,
    },
    /// Enter as itself.
    NoCopy,
    /// The value of X.
    X(u32),
    /// Sacrifice this permanent as an additional cost.
    Sacrifice(ObjId),
    /// Attack this player or planeswalker.
    AttackTarget(Target),
    /// Scry: put these cards on the bottom, in this order; the rest stay
    /// on top.
    ScryBottom(Vec<ObjId>),
    /// Surveil: put these cards into the graveyard.
    SurveilGraveyard(Vec<ObjId>),
    /// A color of mana to add (CR 106.4), or a chosen color.
    Color(crate::mana::Color),
    /// A chosen creature type or basic land type.
    Subtype(crate::card::Subtype),
    /// Cast one of a card's other faces: an adventurer's spell (CR 715.3)
    /// or a split card's half (CR 709.3), `face` indexing `CardDef::faces`.
    CastFace {
        card: ObjId,
        face: u8,
    },
    /// Cast a card for its evoke cost (CR 702.74a).
    CastEvoke(ObjId),
    /// Cast a card for its dash cost (CR 702.109a).
    CastDash(ObjId),
    /// Cast a card for the alternative cost its text offers (CR 118.9):
    /// "You may sacrifice a Mountain rather than pay this spell's mana
    /// cost."
    CastAlt(ObjId),
    /// Cast a card for its bestow cost, as an Aura spell (CR 702.103a).
    CastBestow(ObjId),
    /// Cast a card for its overload cost (CR 702.96a).
    CastOverload(ObjId),
    /// Suspend a card from hand (CR 702.62a), a special action.
    Suspend(ObjId),
    /// Foretell a card from hand (CR 702.143a), a special action.
    Foretell(ObjId),
    /// Cast a card with morph or disguise face down as a 2/2 for {3}
    /// (CR 702.37c, 702.168).
    CastFaceDown(ObjId),
    /// Turn a face-down permanent face up for its morph (or disguise, or
    /// manifested creature card's mana) cost: a special action (CR 116.2b).
    TurnFaceUp(ObjId),
    /// A spell's division of damage or counters (CR 601.2d): the amount
    /// for each target of its variable group, in the order chosen; each at
    /// least 1.
    Divide(Vec<u32>),
}

impl Choice {
    /// Human-readable form using card names from `state`.
    #[allow(clippy::too_many_lines)] // one arm per choice kind
    pub fn describe(&self, state: &GameState) -> String {
        let name = |id: &ObjId| state.name_of(*id).to_string();
        let names = |ids: &[ObjId]| -> String {
            if ids.is_empty() {
                "nothing".to_string()
            } else {
                ids.iter().map(name).collect::<Vec<_>>().join(", ")
            }
        };
        let target = |t: &Target| match t {
            Target::Player(p) => format!("player {}", p.0 + 1),
            Target::Object(id) => name(id),
        };
        match self {
            Choice::Keep => "keep".into(),
            Choice::Mulligan => "mulligan".into(),
            Choice::Bottom(id) => format!("put {} on the bottom", name(id)),
            Choice::Pass => "pass".into(),
            Choice::PlayLand(id) => format!("play {}", name(id)),
            Choice::Cast(id) => format!("cast {}", name(id)),
            Choice::Target(t) => format!("target {}", target(t)),
            Choice::Attackers(set) => format!("attack with {}", names(set)),
            Choice::AddAttacker(id) => format!("add attacker {}", name(id)),
            Choice::Blocks(set) => {
                if set.is_empty() {
                    "no blocks".into()
                } else {
                    set.iter()
                        .map(|(b, a)| format!("{} blocks {}", name(b), name(a)))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            }
            Choice::AddBlock { blocker, attacker } => {
                format!("{} blocks {}", name(blocker), name(attacker))
            }
            Choice::Done => "done".into(),
            Choice::Division(div) => div
                .iter()
                .map(|(t, n)| format!("{n} to {}", target(t)))
                .collect::<Vec<_>>()
                .join(", "),
            Choice::Discard { def, .. } => format!("discard {}", state.pool.get(*def).name),
            Choice::ScryBottom(ids) => format!("bottom {}", names(ids)),
            Choice::SurveilGraveyard(ids) => format!("graveyard {}", names(ids)),
            Choice::Trigger(i) => {
                let source = state
                    .pending_triggers
                    .get(*i)
                    .map_or("?", |t| state.name_of_or_lki(t.ability.source));
                format!("put {source}'s trigger on the stack")
            }
            Choice::Replacement { def, .. } => {
                format!("apply {}'s effect", state.pool.get(*def).name)
            }
            Choice::Activate {
                source,
                ability,
                sacrifice,
                discard,
            } => {
                let text = match state.pool.ability(*ability) {
                    AbilityDef::Activated(a) => a.text.as_str(),
                    _ => "?",
                };
                let mut out = format!("activate {}: {text}", name(source));
                if let Some(s) = sacrifice {
                    out = format!("{out} (sacrificing {})", name(s));
                }
                if let Some(c) = discard {
                    out = format!("{out} (discarding {})", name(c));
                }
                out
            }
            Choice::X(n) => format!("X = {n}"),
            Choice::Divide(parts) => format!(
                "divide {}",
                parts
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Choice::CastEvoke(id) => format!("cast {} for its evoke cost", name(id)),
            Choice::CastDash(id) => format!("cast {} for its dash cost", name(id)),
            Choice::CastAlt(id) => format!("cast {} for its alternative cost", name(id)),
            Choice::CastBestow(id) => format!("cast {} for its bestow cost", name(id)),
            Choice::CastOverload(id) => format!("cast {} for its overload cost", name(id)),
            Choice::Suspend(id) => format!("suspend {}", name(id)),
            Choice::Foretell(id) => format!("foretell {}", name(id)),
            Choice::CastFaceDown(id) => {
                let card = state
                    .objects
                    .get(*id)
                    .map_or("?", |o| state.pool.get(o.card).name.as_str());
                format!("cast {card} face down")
            }
            Choice::TurnFaceUp(id) => {
                let card = state
                    .objects
                    .get(*id)
                    .map_or("?", |o| state.pool.get(o.card).name.as_str());
                format!("turn {card} face up")
            }
            Choice::Color(c) => format!("{c:?}").to_lowercase(),
            Choice::Subtype(s) => state.pool.subtype_name(*s).to_string(),
            Choice::CastFace { card, face } => {
                let name = state
                    .objects
                    .get(*card)
                    .and_then(|o| state.pool.get(o.def).faces.get(*face as usize))
                    .map_or("?", |f| state.pool.get(*f).name.as_str());
                format!("cast {name}")
            }
            Choice::AttackTarget(t) => format!("attack {}", target(t)),
            Choice::Sacrifice(id) => format!("sacrifice {}", name(id)),
            Choice::KeepLegend { def, age, .. } => {
                let name = &state.pool.get(*def).name;
                if *age == 0 {
                    format!("keep the oldest {name}")
                } else {
                    format!("keep {name} (#{} by age)", age + 1)
                }
            }
            Choice::Mode(i) => format!("mode {}", i + 1),
            Choice::Accept => "yes".into(),
            Choice::Decline => "no".into(),
            Choice::CopyOf { def, .. } => format!("copy {}", state.pool.get(*def).name),
            Choice::NoCopy => "copy nothing".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Decision {
    pub player: PlayerId,
    pub kind: DecisionKind,
    /// Exhaustive list of legal choices. Never empty.
    pub options: Vec<Choice>,
}

#[derive(Clone, Debug)]
pub enum StepResult {
    Decision(Decision),
    Over(GameResult),
}

impl StepResult {
    pub fn decision(&self) -> Option<&Decision> {
        match self {
            StepResult::Decision(d) => Some(d),
            StepResult::Over(_) => None,
        }
    }

    pub fn is_over(&self) -> bool {
        matches!(self, StepResult::Over(_))
    }
}
