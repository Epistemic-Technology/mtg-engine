//! A one-ply heuristic agent: plays lands, casts the most expensive spell it
//! can, aims removal at the opponent's best creature, attacks when the
//! attacker will not lose a trade, and blocks when the block is favourable
//! or necessary. Deterministic. Useful as a stronger baseline than random
//! and as the default opponent in human play.

use mtg_core::card::{AbilityDef, Effect, Keyword, Modification};
use mtg_core::decision::{Choice, ChoiceId, Decision, DecisionKind};
use mtg_core::ids::{ObjId, PlayerId};
use mtg_core::state::{Characteristics, GameState, Target};

use crate::Agent;

pub struct GreedyAgent;

/// Rough worth of a creature for trade decisions.
fn value(c: &Characteristics) -> i32 {
    c.power + c.toughness + i32::try_from(c.keywords.iter().count()).unwrap_or(0)
}

/// Whether `a` deals lethal damage to `b` in combat, ignoring first strike.
fn kills(a: &Characteristics, b: &Characteristics, b_damage: u32) -> bool {
    if b.has(Keyword::Indestructible) {
        return false;
    }
    if a.power <= 0 {
        return false;
    }
    a.has(Keyword::Deathtouch) || a.power >= b.toughness - i32::try_from(b_damage).unwrap_or(0)
}

/// Whether `blocker` could legally block `attacker` (evasion only).
fn could_block(blocker: &Characteristics, attacker: &Characteristics) -> bool {
    !(attacker.has(Keyword::Flying)
        && !(blocker.has(Keyword::Flying) || blocker.has(Keyword::Reach)))
}

impl GreedyAgent {
    fn chars(state: &GameState, id: ObjId) -> Characteristics {
        state
            .characteristics(id)
            .expect("option refers to an object that exists")
    }

    fn untapped_creatures(state: &GameState, p: PlayerId) -> Vec<ObjId> {
        state
            .creatures_of(p)
            .filter(|id| !state.objects[*id].tapped)
            .collect()
    }

    /// Attack with `id` if no untapped opposing creature can block it
    /// profitably (kill it without dying).
    fn safe_attacker(state: &GameState, id: ObjId, me: PlayerId) -> bool {
        let a = Self::chars(state, id);
        let opp = state.opponent(me);
        Self::untapped_creatures(state, opp).into_iter().all(|b| {
            let bc = Self::chars(state, b);
            !could_block(&bc, &a)
                || !kills(&bc, &a, state.objects[id].damage)
                || kills(&a, &bc, state.objects[b].damage)
        })
    }

    fn block_score(state: &GameState, me: PlayerId, blocker: ObjId, attacker: ObjId) -> i32 {
        let b = Self::chars(state, blocker);
        let a = Self::chars(state, attacker);
        let life = state.player(me).life;
        let incoming: i32 = state.combat.as_ref().map_or(0, |c| {
            c.attackers
                .iter()
                .map(|at| Self::chars(state, at.id).power.max(0))
                .sum()
        });
        let mut score = 0;
        if kills(&b, &a, state.objects[attacker].damage) {
            score += value(&a);
        }
        if kills(&a, &b, state.objects[blocker].damage) {
            score -= value(&b);
        }
        if incoming >= life {
            // Chump when it keeps us alive.
            score += a.power.max(0) * 3;
        }
        score
    }

    fn target_score(state: &GameState, me: PlayerId, spell: ObjId, t: Target) -> i32 {
        let effects: Vec<Effect> = state.effects_of(spell).to_vec();
        let opp = state.opponent(me);
        // Auras: judge by the sign of their P/T change (or a granted keyword).
        let aura_delta: Option<i32> = state.def_of(spell).filter(|d| d.is_aura()).map(|d| {
            d.abilities
                .iter()
                .filter_map(|a| match a {
                    AbilityDef::Static(sa) => Some(sa),
                    _ => None,
                })
                .flat_map(|sa| sa.parts.iter())
                .map(|m| match m {
                    Modification::ModifyPt { power, toughness } => power + toughness,
                    Modification::AddKeywords(_) => 1,
                    Modification::RemoveAllAbilities => -1,
                    _ => 0,
                })
                .sum()
        });
        let helps_target = effects
            .iter()
            .any(|e| matches!(e, Effect::Pump { .. } | Effect::AddCounters { .. }))
            || aura_delta.is_some_and(|d| d > 0);
        let destroys_aura = aura_delta.is_some_and(|d| d < 0);
        let damage: u32 = effects
            .iter()
            .map(|e| match e {
                Effect::DealDamage { amount, .. } => *amount,
                _ => 0,
            })
            .sum();
        let destroys = effects.iter().any(|e| matches!(e, Effect::Destroy { .. }));
        match t {
            Target::Player(p) => {
                if helps_target {
                    return i32::MIN / 2;
                }
                if p == opp {
                    // Face damage: good when it ends the game, otherwise modest.
                    if i32::try_from(damage).unwrap_or(0) >= state.player(p).life {
                        1_000
                    } else {
                        1
                    }
                } else {
                    i32::MIN / 2
                }
            }
            Target::Object(id) => {
                let c = Self::chars(state, id);
                let mine = state.objects[id].controller == me;
                if helps_target {
                    if mine {
                        // Prefer a creature currently in combat.
                        let in_combat = state.combat.as_ref().is_some_and(|cb| cb.is_in_combat(id));
                        value(&c) + if in_combat { 10 } else { 0 }
                    } else {
                        i32::MIN / 2
                    }
                } else if mine {
                    i32::MIN / 2
                } else if destroys
                    || destroys_aura
                    || (damage > 0
                        && !c.has(Keyword::Indestructible)
                        && i32::try_from(damage + state.objects[id].damage).unwrap_or(0)
                            >= c.toughness)
                {
                    value(&c) + 2
                } else {
                    // Damage that does not kill: nearly worthless.
                    0
                }
            }
        }
    }

    fn best_by<F: Fn(&Choice) -> i64>(d: &Decision, score: F) -> ChoiceId {
        let mut best = (0usize, i64::MIN);
        for (i, c) in d.options.iter().enumerate() {
            let s = score(c);
            if s > best.1 {
                best = (i, s);
            }
        }
        u32::try_from(best.0).unwrap_or(0)
    }
}

impl Agent for GreedyAgent {
    #[allow(clippy::too_many_lines)] // one arm per decision kind
    fn choose(&mut self, state: &GameState, d: &Decision) -> ChoiceId {
        let me = d.player;
        match d.kind {
            DecisionKind::Mulligan => Self::best_by(d, |c| i64::from(*c == Choice::Keep)),
            DecisionKind::Bottom
            | DecisionKind::AssignCombatDamage { .. }
            | DecisionKind::OrderTriggers => 0,
            DecisionKind::PriorityAction => Self::best_by(d, |c| match c {
                Choice::PlayLand(_) => 1_000,
                Choice::Cast(id) => {
                    let def = state.def_of(*id).expect("card in hand");
                    let mv = i64::from(def.mana_cost.mana_value());
                    // Cast creatures freely; hold targeted spells for a
                    // worthwhile target (the target scorer decides, but a
                    // spell with only poor targets should stay in hand).
                    let has_good_target = def.spell_ability().is_none_or(|(specs, _)| {
                        specs.is_empty()
                            || state
                                .battlefield
                                .iter()
                                .map(|o| Target::Object(*o))
                                .chain(state.players_in_game().map(Target::Player))
                                .any(|t| {
                                    specs.iter().any(|s| state.is_legal_target(me, *s, t))
                                        && Self::target_score(state, me, *id, t) > 0
                                })
                    });
                    if has_good_target { 100 + mv } else { -1 }
                }
                Choice::Pass => 0,
                _ => -10,
            }),
            DecisionKind::ChooseTarget { spell, .. } => Self::best_by(d, |c| match c {
                Choice::Target(t) => i64::from(Self::target_score(state, me, spell, *t)),
                _ => i64::MIN,
            }),
            DecisionKind::DeclareAttackers => {
                let opp = state.opponent(me);
                let eligible: Vec<ObjId> = d
                    .options
                    .iter()
                    .flat_map(|c| match c {
                        Choice::Attackers(set) => set.clone(),
                        Choice::AddAttacker(id) => vec![*id],
                        _ => Vec::new(),
                    })
                    .collect();
                let total_power: i32 = eligible
                    .iter()
                    .map(|id| Self::chars(state, *id).power.max(0))
                    .sum();
                let alpha = total_power >= state.player(opp).life
                    && Self::untapped_creatures(state, opp).is_empty();
                let wanted: Vec<ObjId> = eligible
                    .iter()
                    .copied()
                    .filter(|id| alpha || Self::safe_attacker(state, *id, me))
                    .collect();
                Self::best_by(d, |c| match c {
                    Choice::Attackers(set) => {
                        let hits = set.iter().filter(|id| wanted.contains(id)).count();
                        let misses = set.len() - hits;
                        i64::try_from(hits).unwrap_or(0) * 10
                            - i64::try_from(misses).unwrap_or(0) * 10
                    }
                    Choice::AddAttacker(id) => {
                        if wanted.contains(id) {
                            1
                        } else {
                            -1
                        }
                    }
                    Choice::Done => 0,
                    _ => i64::MIN,
                })
            }
            DecisionKind::DeclareBlockers => Self::best_by(d, |c| match c {
                Choice::Blocks(set) => set
                    .iter()
                    .map(|(b, a)| i64::from(Self::block_score(state, me, *b, *a)))
                    .sum(),
                Choice::AddBlock { blocker, attacker } => {
                    let s = i64::from(Self::block_score(state, me, *blocker, *attacker));
                    if s > 0 { s } else { i64::MIN / 2 }
                }
                Choice::Done => 0,
                _ => i64::MIN,
            }),
            DecisionKind::Discard => Self::best_by(d, |c| match c {
                Choice::Discard(id) => {
                    let def = state.def_of(*id).expect("card in hand");
                    if def.is_land() {
                        10
                    } else {
                        -i64::from(def.mana_cost.mana_value())
                    }
                }
                _ => i64::MIN,
            }),
        }
    }
}
