//! Scenarios for forms no corpus card uses yet: each loads a test card
//! into a copy of the built-in pool and runs a scenario with it.

use std::sync::Arc;

use mtg_dsl::builtin_pool;

/// Run every scenario in `scenarios` with the cards in `cards` added to
/// the built-in pool; panics with the failures.
fn run_with(cards: &str, scenarios: &str) {
    let mut pool = builtin_pool();
    mtg_dsl::load_into(&mut pool, cards).expect("test card loads");
    let pool = Arc::new(pool);
    let parsed = mtg_scenario::parse(scenarios).expect("scenario parses");
    let failures: Vec<String> = parsed
        .iter()
        .map(|sc| mtg_scenario::run(&pool, sc))
        .filter_map(|r| {
            r.failure
                .map(|f| format!("\"{}\": line {}: {}", r.name, f.line, f.message))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `this attacked this combat` (`SourceHistory::AttackedThisCombat`): an
/// attack in an earlier combat phase this turn doesn't count.
#[test]
fn attacked_this_combat_is_the_current_combat() {
    run_with(
        r#"card "Test Raider" {
  cost {1}{G}
  type Creature — Elf Scout
  pt 2/2
  triggered whenever creature you control attacks if this attacked this combat: you gain 1 life
}
"#,
        r#"
scenario "both attack in the first combat, only the Bears in the second" {
  battlefield A: "Test Raider", "Grizzly Bears", 4 "Mountain", 2 "Forest"
  hand A: "Full Throttle"
  turn 1 A: cast "Full Throttle"
  turn 1 declare_attackers A: attack "Test Raider", "Grizzly Bears"
  turn 1 declare_attackers combat 2 A: attack "Grizzly Bears"
  turn 1 declare_attackers combat 3 A: attack "Grizzly Bears"
  run_to turn 1 end
  assert life A 22
  assert life B 12
}

scenario "the Raider attacks again in the second combat" {
  battlefield A: "Test Raider", "Grizzly Bears", 4 "Mountain", 2 "Forest"
  hand A: "Full Throttle"
  turn 1 A: cast "Full Throttle"
  turn 1 declare_attackers A: attack "Grizzly Bears"
  turn 1 declare_attackers combat 2 A: attack "Test Raider", "Grizzly Bears"
  turn 1 declare_attackers combat 3 A: attack "Grizzly Bears"
  run_to turn 1 end
  assert life A 22
  assert life B 12
}
"#,
    );
}
