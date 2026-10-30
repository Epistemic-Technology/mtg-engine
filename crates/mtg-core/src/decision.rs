//! The decision interface: what the engine hands to a player and what it
//! accepts back (design §4.7).

use serde::{Deserialize, Serialize};

use crate::ids::{ObjId, PlayerId};
use crate::state::{GameResult, Target};

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
