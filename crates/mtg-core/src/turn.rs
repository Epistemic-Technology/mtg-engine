//! Turn structure (CR 500–514).

use serde::{Deserialize, Serialize};

/// Phases of a turn (CR 500.1).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub enum Phase {
    Beginning,
    PrecombatMain,
    Combat,
    PostcombatMain,
    Ending,
}

/// Steps of a turn, in order. Main phases have no steps (CR 505.1), so they
/// appear here as single entries; this enum is really "the position in a
/// turn".
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub enum Step {
    Untap,
    Upkeep,
    Draw,
    PrecombatMain,
    BeginningOfCombat,
    DeclareAttackers,
    DeclareBlockers,
    /// First-strike combat damage step; exists only if a creature with first
    /// or double strike is in combat (CR 510.5).
    FirstStrikeDamage,
    CombatDamage,
    EndOfCombat,
    PostcombatMain,
    End,
    Cleanup,
}

impl Step {
    pub const ORDER: [Step; 13] = [
        Step::Untap,
        Step::Upkeep,
        Step::Draw,
        Step::PrecombatMain,
        Step::BeginningOfCombat,
        Step::DeclareAttackers,
        Step::DeclareBlockers,
        Step::FirstStrikeDamage,
        Step::CombatDamage,
        Step::EndOfCombat,
        Step::PostcombatMain,
        Step::End,
        Step::Cleanup,
    ];

    pub const fn phase(self) -> Phase {
        match self {
            Step::Untap | Step::Upkeep | Step::Draw => Phase::Beginning,
            Step::PrecombatMain => Phase::PrecombatMain,
            Step::BeginningOfCombat
            | Step::DeclareAttackers
            | Step::DeclareBlockers
            | Step::FirstStrikeDamage
            | Step::CombatDamage
            | Step::EndOfCombat => Phase::Combat,
            Step::PostcombatMain => Phase::PostcombatMain,
            Step::End | Step::Cleanup => Phase::Ending,
        }
    }

    /// The step after this one within the same turn, or `None` after cleanup.
    pub fn next(self) -> Option<Step> {
        let i = Step::ORDER.iter().position(|s| *s == self)?;
        Step::ORDER.get(i + 1).copied()
    }

    /// Players receive priority during this step (CR 502.4, 514.3, 117.3b).
    pub const fn has_priority(self) -> bool {
        !matches!(self, Step::Untap | Step::Cleanup)
    }

    pub const fn is_main(self) -> bool {
        matches!(self, Step::PrecombatMain | Step::PostcombatMain)
    }

    pub const fn is_combat_damage(self) -> bool {
        matches!(self, Step::FirstStrikeDamage | Step::CombatDamage)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Step::Untap => "untap",
            Step::Upkeep => "upkeep",
            Step::Draw => "draw",
            Step::PrecombatMain => "precombat main",
            Step::BeginningOfCombat => "beginning of combat",
            Step::DeclareAttackers => "declare attackers",
            Step::DeclareBlockers => "declare blockers",
            Step::FirstStrikeDamage => "first-strike combat damage",
            Step::CombatDamage => "combat damage",
            Step::EndOfCombat => "end of combat",
            Step::PostcombatMain => "postcombat main",
            Step::End => "end",
            Step::Cleanup => "cleanup",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_ordered_and_terminate() {
        let mut s = Step::Untap;
        let mut n = 1;
        while let Some(next) = s.next() {
            assert!(next > s);
            s = next;
            n += 1;
        }
        assert_eq!(s, Step::Cleanup);
        assert_eq!(n, Step::ORDER.len());
    }

    #[test]
    fn priority_steps() {
        assert!(!Step::Untap.has_priority());
        assert!(!Step::Cleanup.has_priority());
        assert!(Step::Upkeep.has_priority());
        assert!(Step::DeclareAttackers.has_priority());
    }
}
