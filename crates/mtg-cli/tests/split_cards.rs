//! Split-card casts the scenario runner can't script, driven choice by
//! choice: a fused split spell (CR 702.102), named like the card itself,
//! and an aftermath half with targets cast from the graveyard (CR
//! 702.127a), scripted as a cast of the card, whose own definition has no
//! targets. The other casts of these cards are in `scenarios/m5/fuse.scn`
//! and `scenarios/m5/aftermath.scn`.

use std::sync::Arc;

use mtg_core::decision::{Choice, ChoiceId};
use mtg_core::engine::Engine;
use mtg_core::ids::{ObjId, PlayerId};
use mtg_core::state::{GameState, Target, Zone};

type Pick<'a> = &'a dyn Fn(&GameState, &Target) -> bool;

/// Set up `setup` (scenario setup lines), take player A's first option
/// `is_cast` accepts, choose the targets `pick` accepts, in order, and
/// pass until the stack is empty again. Other decisions take the first
/// option.
fn cast_with(setup: &str, is_cast: &dyn Fn(&GameState, &Choice) -> bool, pick: &[Pick]) -> Engine {
    let pool = Arc::new(mtg_dsl::builtin_pool());
    let text = format!("scenario \"split\" {{\n{setup}\n  run_to turn 1 end\n}}\n");
    let sc = mtg_scenario::parse(&text).expect("scenario parses");
    let mut engine = mtg_scenario::start_engine(&pool, &sc[0]).expect("engine starts");
    let mut cast = false;
    let mut next = 0;
    for _ in 0..200 {
        let Some(d) = engine.current().decision().cloned() else {
            break;
        };
        let st = engine.state();
        let pass = d.options.iter().position(|c| *c == Choice::Pass);
        let idx = if !cast {
            match d.options.iter().position(|c| is_cast(st, c)) {
                Some(i) if d.player == PlayerId(0) => {
                    cast = true;
                    i
                }
                _ => pass.unwrap_or(0),
            }
        } else if d.options.iter().any(|c| matches!(c, Choice::Target(_))) {
            let i = d
                .options
                .iter()
                .position(|c| matches!(c, Choice::Target(t) if pick[next](st, t)))
                .unwrap_or_else(|| panic!("target #{} not offered: {:?}", next + 1, d.options));
            next += 1;
            i
        } else if st.stack.is_empty() {
            break;
        } else {
            pass.unwrap_or(0)
        };
        engine
            .step(ChoiceId::try_from(idx).unwrap())
            .expect("legal choice");
    }
    assert!(cast, "the spell was not offered");
    assert_eq!(next, pick.len(), "not every target was asked for");
    engine
}

fn cast_fused(setup: &str, pick: &[Pick]) -> Engine {
    cast_with(
        setup,
        &|_, c| matches!(c, Choice::CastFace { face: 2, .. }),
        pick,
    )
}

fn cast_aftermath(setup: &str, pick: &[Pick]) -> Engine {
    cast_with(
        setup,
        &|st, c| matches!(c, Choice::Cast(id) if st.objects[*id].zone == Zone::Graveyard),
        pick,
    )
}

fn named(name: &'static str) -> impl Fn(&GameState, &Target) -> bool {
    move |st, t| matches!(t, Target::Object(id) if st.name_of(*id) == name)
}

fn player(p: u8) -> impl Fn(&GameState, &Target) -> bool {
    move |_, t| *t == Target::Player(PlayerId(p))
}

fn names(st: &GameState, ids: &[ObjId]) -> Vec<String> {
    let mut v: Vec<String> = ids.iter().map(|id| st.name_of(*id).to_string()).collect();
    v.sort();
    v
}

fn find(st: &GameState, name: &str) -> ObjId {
    *st.battlefield
        .iter()
        .find(|id| st.name_of(**id) == name)
        .unwrap_or_else(|| panic!("no {name} on the battlefield"))
}

// ----- fuse ----------------------------------------------------------------

#[test]
fn far_away_fused_does_both_halves() {
    let e = cast_fused(
        "  battlefield A: 3 \"Island\", 2 \"Swamp\"\n  battlefield B: \"Grizzly Bears\", \"Hill Giant\"\n  hand A: \"Far // Away\"",
        &[&named("Grizzly Bears"), &player(1)],
    );
    let st = e.state();
    let b = st.player(PlayerId(1));
    assert_eq!(names(st, &b.hand), vec!["Grizzly Bears"]);
    assert_eq!(names(st, &b.graveyard), vec!["Hill Giant"]);
    assert_eq!(
        names(st, &st.player(PlayerId(0)).graveyard),
        vec!["Far // Away"]
    );
    // Both costs were paid: every land is tapped (CR 702.102c).
    assert!(
        st.battlefield
            .iter()
            .all(|id| st.objects[*id].owner == PlayerId(1) || st.objects[*id].tapped)
    );
}

#[test]
fn protect_serve_fused_may_target_one_creature_twice() {
    let e = cast_fused(
        "  battlefield A: 3 \"Plains\", 2 \"Island\"\n  battlefield B: \"Serra Angel\"\n  hand A: \"Protect // Serve\"",
        &[&named("Serra Angel"), &named("Serra Angel")],
    );
    let st = e.state();
    let c = st.characteristics(find(st, "Serra Angel")).expect("angel");
    assert_eq!((c.power, c.toughness), (0, 8));
}

#[test]
fn wear_tear_fused_each_half_its_own_target() {
    let e = cast_fused(
        "  battlefield A: 2 \"Mountain\", \"Plains\"\n  battlefield B: \"Ornithopter\", \"Grizzly Bears\", \"Pacifism\" on \"Grizzly Bears\"\n  hand A: \"Wear // Tear\"",
        &[&named("Ornithopter"), &named("Pacifism")],
    );
    let st = e.state();
    assert_eq!(
        names(st, &st.player(PlayerId(1)).graveyard),
        vec!["Ornithopter", "Pacifism"]
    );
}

#[test]
#[should_panic(expected = "target #1 not offered")]
fn wear_tear_fused_left_target_is_an_artifact() {
    cast_fused(
        "  battlefield A: 2 \"Mountain\", \"Plains\"\n  battlefield B: \"Ornithopter\", \"Grizzly Bears\", \"Pacifism\" on \"Grizzly Bears\"\n  hand A: \"Wear // Tear\"",
        &[&named("Pacifism"), &named("Ornithopter")],
    );
}

#[test]
fn down_dirty_fused_discards_then_returns() {
    let e = cast_fused(
        "  battlefield A: 4 \"Swamp\", 3 \"Forest\"\n  hand A: \"Down // Dirty\"\n  graveyard A: \"Grizzly Bears\"\n  hand B: \"Island\", \"Mountain\"",
        &[&player(1), &named("Grizzly Bears")],
    );
    let st = e.state();
    assert_eq!(st.player(PlayerId(1)).hand.len(), 0);
    assert_eq!(
        names(st, &st.player(PlayerId(0)).hand),
        vec!["Grizzly Bears"]
    );
}

#[test]
#[should_panic(expected = "the spell was not offered")]
fn far_away_not_fused_without_both_costs() {
    cast_fused(
        "  battlefield A: 2 \"Island\", 2 \"Swamp\"\n  battlefield B: \"Grizzly Bears\"\n  hand A: \"Far // Away\"",
        &[&named("Grizzly Bears"), &player(1)],
    );
}

#[test]
#[should_panic(expected = "the spell was not offered")]
fn far_away_not_fused_from_the_graveyard() {
    cast_with(
        "  battlefield A: 3 \"Island\", 2 \"Swamp\"\n  battlefield B: \"Grizzly Bears\"\n  graveyard A: \"Far // Away\"",
        &|st, c| match c {
            Choice::Cast(id) | Choice::CastFace { card: id, .. } => {
                st.objects[*id].zone == Zone::Graveyard
            }
            _ => false,
        },
        &[],
    );
}

// ----- aftermath halves with targets, from the graveyard -------------------

#[test]
fn fame_from_the_graveyard() {
    let e = cast_aftermath(
        "  battlefield A: 2 \"Mountain\", \"Grizzly Bears\"\n  graveyard A: \"Claim // Fame\"",
        &[&named("Grizzly Bears")],
    );
    let st = e.state();
    let bears = find(st, "Grizzly Bears");
    let c = st.characteristics(bears).expect("bears");
    assert_eq!((c.power, c.toughness), (4, 2));
    assert!(c.has(mtg_core::card::Keyword::Haste));
    assert_eq!(
        names(st, &st.player(PlayerId(0)).exile),
        vec!["Claim // Fame"]
    );
}

#[test]
fn oblivion_from_the_graveyard() {
    let e = cast_aftermath(
        "  battlefield A: 5 \"Swamp\"\n  graveyard A: \"Consign // Oblivion\"\n  hand B: \"Island\", \"Mountain\", \"Plains\"",
        &[&player(1)],
    );
    let st = e.state();
    assert_eq!(st.player(PlayerId(1)).hand.len(), 1);
    assert_eq!(
        names(st, &st.player(PlayerId(0)).exile),
        vec!["Consign // Oblivion"]
    );
}

#[test]
#[should_panic(expected = "target #1 not offered")]
fn oblivion_targets_an_opponent() {
    cast_aftermath(
        "  battlefield A: 5 \"Swamp\"\n  graveyard A: \"Consign // Oblivion\"\n  hand A: \"Island\", \"Mountain\"",
        &[&player(0)],
    );
}

#[test]
fn return_from_the_graveyard() {
    let e = cast_aftermath(
        "  battlefield A: 4 \"Swamp\"\n  graveyard A: \"Never // Return\"\n  graveyard B: \"Hill Giant\"",
        &[&named("Hill Giant")],
    );
    let st = e.state();
    assert_eq!(names(st, &st.player(PlayerId(1)).exile), vec!["Hill Giant"]);
    let zombie = find(st, "Zombie");
    let c = st.characteristics(zombie).expect("zombie");
    assert_eq!((c.power, c.toughness), (2, 2));
    assert_eq!(st.objects[zombie].controller, PlayerId(0));
    assert_eq!(
        names(st, &st.player(PlayerId(0)).exile),
        vec!["Never // Return"]
    );
}

#[test]
fn fight_from_the_graveyard() {
    let e = cast_aftermath(
        "  battlefield A: 4 \"Forest\", \"Hill Giant\"\n  battlefield B: \"Grizzly Bears\", \"Serra Angel\"\n  graveyard A: \"Prepare // Fight\"",
        &[&named("Hill Giant"), &named("Grizzly Bears")],
    );
    let st = e.state();
    assert_eq!(
        names(st, &st.player(PlayerId(1)).graveyard),
        vec!["Grizzly Bears"]
    );
    assert_eq!(st.objects[find(st, "Hill Giant")].damage, 2);
    assert_eq!(
        names(st, &st.player(PlayerId(0)).exile),
        vec!["Prepare // Fight"]
    );
}

#[test]
#[should_panic(expected = "target #1 not offered")]
fn fight_first_target_is_your_creature() {
    cast_aftermath(
        "  battlefield A: 4 \"Forest\", \"Hill Giant\"\n  battlefield B: \"Grizzly Bears\", \"Serra Angel\"\n  graveyard A: \"Prepare // Fight\"",
        &[&named("Serra Angel"), &named("Grizzly Bears")],
    );
}

#[test]
fn ruin_from_the_graveyard_counts_lands() {
    let e = cast_aftermath(
        "  battlefield A: 4 \"Mountain\"\n  battlefield B: \"Colossal Dreadmaw\"\n  graveyard A: \"Road // Ruin\"",
        &[&named("Colossal Dreadmaw")],
    );
    let st = e.state();
    assert_eq!(st.objects[find(st, "Colossal Dreadmaw")].damage, 4);
    assert_eq!(
        names(st, &st.player(PlayerId(0)).exile),
        vec!["Road // Ruin"]
    );
}

#[test]
fn finish_from_the_graveyard_sacrifices_a_creature() {
    let e = cast_aftermath(
        "  battlefield A: 3 \"Swamp\", \"Grizzly Bears\"\n  battlefield B: \"Serra Angel\"\n  graveyard A: \"Start // Finish\"",
        &[&named("Serra Angel")],
    );
    let st = e.state();
    assert_eq!(
        names(st, &st.player(PlayerId(1)).graveyard),
        vec!["Serra Angel"]
    );
    assert_eq!(
        names(st, &st.player(PlayerId(0)).graveyard),
        vec!["Grizzly Bears"]
    );
    assert_eq!(
        names(st, &st.player(PlayerId(0)).exile),
        vec!["Start // Finish"]
    );
}

#[test]
#[should_panic(expected = "the spell was not offered")]
fn finish_needs_a_creature_to_sacrifice() {
    cast_aftermath(
        "  battlefield A: 3 \"Swamp\"\n  battlefield B: \"Serra Angel\"\n  graveyard A: \"Start // Finish\"",
        &[&named("Serra Angel")],
    );
}
