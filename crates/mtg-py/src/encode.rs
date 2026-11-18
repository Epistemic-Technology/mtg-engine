#![allow(clippy::doc_markdown)]
//! Fixed-size tensor encodings of a player's view and of a decision's
//! options (design §9.1). Card identity is the card's index in the pool;
//! embedding it is the model's job. Everything here is plain data the
//! environment copies into NumPy arrays once per step.

use mtg_core::card::{CardType, Keyword};
use mtg_core::decision::{Choice, Decision, DecisionKind};
use mtg_core::ids::{ObjId, PlayerId};
use mtg_core::mana::Color;
use mtg_core::state::{GameState, Target, Zone};
use mtg_core::turn::Step;

/// Steps of a turn (CR 500.1), one-hot.
pub const STEPS: usize = 13;
/// Decision kinds, one-hot (with room to grow without changing the layout).
pub const DECISION_KINDS: usize = 24;
/// Option kinds, one-hot (likewise).
pub const CHOICE_KINDS: usize = 32;
/// Features per card slot.
pub const CARD_F: usize = 2 + 7 + 2 + 5 + 1 + 5 + Keyword::ALL.len();
pub const HAND_SLOTS: usize = 10;
pub const BATTLEFIELD_SLOTS: usize = 24;
pub const STACK_SLOTS: usize = 6;
/// Scalars before the card slots.
pub const GLOBAL_F: usize = 1 + STEPS + 2 + 8 + 1 + 1 + DECISION_KINDS;
pub const OBS_SIZE: usize = GLOBAL_F + (HAND_SLOTS + BATTLEFIELD_SLOTS + STACK_SLOTS) * CARD_F;
/// Features per option: kind one-hot, source card id, target card id,
/// target player (0 none, 1 you, 2 opponent), a count, a cost summary.
pub const OPT_F: usize = CHOICE_KINDS + 5;

pub const fn step_index(s: Step) -> usize {
    match s {
        Step::Untap => 0,
        Step::Upkeep => 1,
        Step::Draw => 2,
        Step::PrecombatMain => 3,
        Step::BeginningOfCombat => 4,
        Step::DeclareAttackers => 5,
        Step::DeclareBlockers => 6,
        Step::FirstStrikeDamage => 7,
        Step::CombatDamage => 8,
        Step::EndOfCombat => 9,
        Step::PostcombatMain => 10,
        Step::End => 11,
        Step::Cleanup => 12,
    }
}

pub const fn decision_kind_index(k: DecisionKind) -> usize {
    match k {
        DecisionKind::Mulligan => 0,
        DecisionKind::Bottom => 1,
        DecisionKind::PriorityAction => 2,
        DecisionKind::ChooseTarget { .. } => 3,
        DecisionKind::DeclareAttackers => 4,
        DecisionKind::DeclareBlockers => 5,
        DecisionKind::AssignCombatDamage { .. } => 6,
        DecisionKind::Discard => 7,
        DecisionKind::OrderTriggers => 8,
        DecisionKind::ChooseReplacement => 9,
        DecisionKind::LegendRule => 10,
        DecisionKind::ChooseMode { .. } => 11,
        DecisionKind::YouMay { .. } => 12,
        DecisionKind::ChooseCopy => 13,
        DecisionKind::ChooseX { .. } => 14,
        DecisionKind::ChooseSacrifice { .. } => 15,
        DecisionKind::ChooseDiscardCost { .. } => 16,
        DecisionKind::ChooseAttackTarget { .. } => 17,
        DecisionKind::Scry { .. } => 18,
        DecisionKind::Surveil { .. } => 19,
        DecisionKind::PayOrCounter { .. } => 20,
        DecisionKind::Kicker { .. } => 21,
        DecisionKind::ChooseColor => 22,
        DecisionKind::SearchLibrary => 23,
        DecisionKind::Edict => 24,
        DecisionKind::Buyback { .. } => 25,
        DecisionKind::PutFromHand => 26,
    }
}

pub const DECISION_KIND_NAMES: [&str; 27] = [
    "mulligan",
    "bottom",
    "priority",
    "target",
    "declare_attackers",
    "declare_blockers",
    "assign_damage",
    "discard",
    "order_triggers",
    "choose_replacement",
    "legend_rule",
    "mode",
    "you_may",
    "copy",
    "x",
    "sacrifice_cost",
    "discard_cost",
    "attack_target",
    "scry",
    "surveil",
    "pay_or_counter",
    "kicker",
    "choose_color",
    "search_library",
    "edict",
    "buyback",
    "put_from_hand",
];

pub fn decision_kind_name(k: DecisionKind) -> &'static str {
    DECISION_KIND_NAMES[decision_kind_index(k)]
}

pub const fn choice_kind_index(c: &Choice) -> usize {
    match c {
        Choice::Keep => 0,
        Choice::Mulligan => 1,
        Choice::Bottom(_) => 2,
        Choice::Pass => 3,
        Choice::PlayLand(_) => 4,
        Choice::Cast(_) => 5,
        Choice::Target(_) => 6,
        Choice::Attackers(_) => 7,
        Choice::AddAttacker(_) => 8,
        Choice::Blocks(_) => 9,
        Choice::AddBlock { .. } => 10,
        Choice::Done => 11,
        Choice::Division(_) => 12,
        Choice::Discard { .. } => 13,
        Choice::Trigger(_) => 14,
        Choice::Replacement { .. } => 15,
        Choice::Activate { .. } => 16,
        Choice::KeepLegend { .. } => 17,
        Choice::Mode(_) => 18,
        Choice::Accept => 19,
        Choice::Decline => 20,
        Choice::CopyOf { .. } => 21,
        Choice::NoCopy => 22,
        Choice::X(_) => 23,
        Choice::Sacrifice(_) => 24,
        Choice::AttackTarget(_) => 25,
        Choice::ScryBottom(_) => 26,
        Choice::SurveilGraveyard(_) => 27,
        Choice::Color(_) => 28,
        Choice::CastFace { .. } => 29,
        Choice::CastFaceDown(_) => 30,
        Choice::TurnFaceUp(_) => 31,
    }
}

pub const CHOICE_KIND_NAMES: [&str; 32] = [
    "keep",
    "mulligan",
    "bottom",
    "pass",
    "play_land",
    "cast",
    "target",
    "attackers",
    "add_attacker",
    "blocks",
    "add_block",
    "done",
    "division",
    "discard",
    "trigger",
    "replacement",
    "activate",
    "keep_legend",
    "mode",
    "accept",
    "decline",
    "copy_of",
    "no_copy",
    "x",
    "sacrifice",
    "attack_target",
    "scry_bottom",
    "surveil_graveyard",
    "color",
    "cast_face",
    "cast_face_down",
    "turn_face_up",
];

#[allow(clippy::cast_precision_loss)] // ids and counts are small
fn card_id(state: &GameState, id: ObjId) -> f32 {
    state.objects.get(id).map_or(-1.0, |o| o.def.0 as f32)
}

/// Fill one card slot. Zero when `id` is gone.
#[allow(clippy::cast_precision_loss)]
fn card_features(state: &GameState, id: ObjId, me: PlayerId, out: &mut [f32]) {
    let Some(o) = state.objects.get(id) else {
        return;
    };
    let Some(c) = state.characteristics(id) else {
        return;
    };
    let def = state.pool.get(o.def);
    let mut i = 0;
    let mut push = |v: f32| {
        out[i] = v;
        i += 1;
    };
    push(o.def.0 as f32);
    push(def.mana_cost.mana_value() as f32);
    for t in [
        CardType::Creature,
        CardType::Land,
        CardType::Artifact,
        CardType::Enchantment,
        CardType::Instant,
        CardType::Sorcery,
        CardType::Planeswalker,
    ] {
        push(f32::from(u8::from(c.types.contains(t))));
    }
    push(f32::from(c.power));
    push(f32::from(c.toughness));
    push(f32::from(u8::from(o.tapped)));
    push(f32::from(u8::from(
        o.zone == Zone::Battlefield && c.is_creature() && !o.controlled_since_turn_start,
    )));
    push(o.damage as f32);
    let combat = state.combat.as_ref();
    push(f32::from(u8::from(
        combat.is_some_and(|cb| cb.attacker(id).is_some()),
    )));
    push(f32::from(u8::from(
        combat.is_some_and(|cb| cb.blocked_by(id).is_some()),
    )));
    push(f32::from(u8::from(o.controller == me)));
    for col in Color::ALL {
        push(f32::from(u8::from(c.colors.contains(col))));
    }
    for k in Keyword::ALL {
        push(f32::from(u8::from(c.keywords.contains(k))));
    }
    debug_assert_eq!(i, CARD_F);
}

/// The observation of `me`: global scalars, then card slots for the hand,
/// the battlefield (both players, timestamp order), and the stack.
#[allow(clippy::cast_precision_loss)]
pub fn observation(state: &GameState, decision: Option<&Decision>, me: PlayerId) -> Vec<f32> {
    let mut out = vec![0.0f32; OBS_SIZE];
    let opp = state.opponent(me);
    let (mine, theirs) = (state.player(me), state.player(opp));
    let mut i = 0;
    out[i] = state.turn_number as f32 / 50.0;
    i += 1;
    out[i + step_index(state.step)] = 1.0;
    i += STEPS;
    out[i] = f32::from(u8::from(state.active_player == me));
    out[i + 1] = f32::from(u8::from(state.priority == me));
    i += 2;
    for v in [
        mine.life as f32 / 20.0,
        theirs.life as f32 / 20.0,
        mine.hand.len() as f32 / 10.0,
        theirs.hand.len() as f32 / 10.0,
        mine.library.len() as f32 / 60.0,
        theirs.library.len() as f32 / 60.0,
        mine.graveyard.len() as f32 / 60.0,
        theirs.graveyard.len() as f32 / 60.0,
    ] {
        out[i] = v;
        i += 1;
    }
    out[i] = mine.lands_played_this_turn as f32;
    i += 1;
    out[i] = state.stack.len() as f32 / 5.0;
    i += 1;
    if let Some(d) = decision {
        out[i + decision_kind_index(d.kind)] = 1.0;
    }
    i += DECISION_KINDS;
    debug_assert_eq!(i, GLOBAL_F);
    for id in mine.hand.iter().take(HAND_SLOTS) {
        card_features(state, *id, me, &mut out[i..i + CARD_F]);
        i += CARD_F;
    }
    i = GLOBAL_F + HAND_SLOTS * CARD_F;
    for id in state.battlefield.iter().take(BATTLEFIELD_SLOTS) {
        card_features(state, *id, me, &mut out[i..i + CARD_F]);
        i += CARD_F;
    }
    i = GLOBAL_F + (HAND_SLOTS + BATTLEFIELD_SLOTS) * CARD_F;
    for id in state.stack.iter().rev().take(STACK_SLOTS) {
        card_features(state, *id, me, &mut out[i..i + CARD_F]);
        i += CARD_F;
    }
    out
}

/// Per-option features for the first `max` options, row-major
/// `max × OPT_F`, and the mask of live rows.
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::too_many_lines)] // one arm per choice kind
pub fn option_features(state: &GameState, decision: &Decision, max: usize) -> (Vec<f32>, Vec<i8>) {
    let me = decision.player;
    let mut out = vec![0.0f32; max * OPT_F];
    let mut mask = vec![0i8; max];
    let target_player = |t: &Target| match t {
        Target::Player(p) => {
            if *p == me {
                1.0
            } else {
                2.0
            }
        }
        Target::Object(_) => 0.0,
    };
    let target_card = |t: &Target| match t {
        Target::Object(id) => card_id(state, *id),
        Target::Player(_) => -1.0,
    };
    for (n, c) in decision.options.iter().take(max).enumerate() {
        mask[n] = 1;
        let row = &mut out[n * OPT_F..(n + 1) * OPT_F];
        row[choice_kind_index(c)] = 1.0;
        let (source, target, player, count, cost) = match c {
            Choice::Bottom(id)
            | Choice::PlayLand(id)
            | Choice::AddAttacker(id)
            | Choice::Sacrifice(id)
            | Choice::TurnFaceUp(id) => (card_id(state, *id), -1.0, 0.0, 0.0, 0.0),
            Choice::CastFaceDown(id) => (card_id(state, *id), -1.0, 0.0, 0.0, 3.0),
            Choice::Cast(id) => {
                let cost = state
                    .objects
                    .get(*id)
                    .map_or(0.0, |o| state.pool.get(o.def).mana_cost.mana_value() as f32);
                (card_id(state, *id), -1.0, 0.0, 0.0, cost)
            }
            Choice::Target(t) | Choice::AttackTarget(t) => {
                (-1.0, target_card(t), target_player(t), 0.0, 0.0)
            }
            Choice::Attackers(set) => (
                set.first().map_or(-1.0, |id| card_id(state, *id)),
                -1.0,
                0.0,
                set.len() as f32,
                0.0,
            ),
            Choice::Blocks(set) => (
                set.first().map_or(-1.0, |(b, _)| card_id(state, *b)),
                set.first().map_or(-1.0, |(_, a)| card_id(state, *a)),
                0.0,
                set.len() as f32,
                0.0,
            ),
            Choice::AddBlock { blocker, attacker } => (
                card_id(state, *blocker),
                card_id(state, *attacker),
                0.0,
                0.0,
                0.0,
            ),
            Choice::Division(div) => (-1.0, -1.0, 0.0, div.len() as f32, 0.0),
            Choice::Discard { def, .. }
            | Choice::Replacement { def, .. }
            | Choice::KeepLegend { def, .. }
            | Choice::CopyOf { def, .. } => (def.0 as f32, -1.0, 0.0, 0.0, 0.0),
            Choice::Trigger(i) | Choice::Mode(i) => (-1.0, -1.0, 0.0, *i as f32, 0.0),
            Choice::Activate {
                source, ability, ..
            } => {
                let cost = match state.pool.ability(*ability) {
                    mtg_core::card::AbilityDef::Activated(a) => {
                        a.cost.mana.requirements(0).total() as f32
                    }
                    _ => 0.0,
                };
                (card_id(state, *source), -1.0, 0.0, 0.0, cost)
            }
            Choice::X(x) => (-1.0, -1.0, 0.0, *x as f32, *x as f32),
            Choice::Color(c) => (-1.0, -1.0, 0.0, f32::from(*c as u8), 0.0),
            Choice::CastFace { card, face } => {
                let cost = state
                    .objects
                    .get(*card)
                    .and_then(|o| state.pool.get(o.def).faces.get(*face as usize))
                    .map_or(0.0, |f| state.pool.get(*f).mana_cost.mana_value() as f32);
                (card_id(state, *card), -1.0, 0.0, f32::from(*face), cost)
            }
            Choice::ScryBottom(ids) | Choice::SurveilGraveyard(ids) => {
                (-1.0, -1.0, 0.0, ids.len() as f32, 0.0)
            }
            Choice::Keep
            | Choice::Mulligan
            | Choice::Pass
            | Choice::Done
            | Choice::Accept
            | Choice::Decline
            | Choice::NoCopy => (-1.0, -1.0, 0.0, 0.0, 0.0),
        };
        row[CHOICE_KINDS] = source;
        row[CHOICE_KINDS + 1] = target;
        row[CHOICE_KINDS + 2] = player;
        row[CHOICE_KINDS + 3] = count;
        row[CHOICE_KINDS + 4] = cost;
    }
    (out, mask)
}
