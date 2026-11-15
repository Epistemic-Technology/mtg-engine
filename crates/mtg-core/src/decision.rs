//! The decision interface: what the engine hands to a player and what it
//! accepts back (design §4.7).

use serde::{Deserialize, Serialize};

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
    /// Divide `attacker`'s combat damage among its blockers (CR 510.1c).
    AssignCombatDamage { attacker: ObjId },
    /// Discard to maximum hand size (CR 514.1).
    Discard,
    /// Choose which of several triggered abilities to put on the stack next
    /// (CR 603.3b). The one chosen first resolves last.
    OrderTriggers,
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
    Discard(ObjId),
    /// Put the pending triggered ability at this index of
    /// `GameState::pending_triggers` on the stack.
    Trigger(usize),
}

impl Choice {
    /// Human-readable form using card names from `state`.
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
            Choice::Discard(id) => format!("discard {}", name(id)),
            Choice::Trigger(i) => {
                let source = state
                    .pending_triggers
                    .get(*i)
                    .map_or("?", |t| state.name_of_or_lki(t.ability.source));
                format!("put {source}'s trigger on the stack")
            }
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
