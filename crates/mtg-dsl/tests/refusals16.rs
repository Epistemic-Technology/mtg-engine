//! Forms the sixteenth wave's bug fixes make the compiler refuse: each
//! gated Match but did something other than what it printed.

use mtg_core::card::CardPool;

/// Load one card with `line` (an ability line) into an empty pool; the
/// error's text, if any.
fn load_card(kind: &str, line: &str) -> Result<(), String> {
    let mut pool = CardPool::new();
    let text = format!("card \"Test\" {{\n  cost {{1}}\n  type {kind}\n  {line}\n}}\n");
    mtg_dsl::load_into(&mut pool, &text)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[test]
fn a_buyback_cost_sacrifices_one_permanent() {
    // "Buyback—Sacrifice three Islands." (Walk the Aeons) sacrificed one.
    let e = load_card(
        "Sorcery",
        "buyback sacrifice 3 Island\n  spell: target player takes 1 extra turn",
    )
    .unwrap_err();
    assert!(e.contains("buyback"), "{e}");
    let r = load_card(
        "Sorcery",
        "buyback sacrifice land\n  spell: target player takes 1 extra turn",
    );
    assert!(r.is_ok(), "{r:?}");
}

#[test]
fn a_cost_sacrifices_permanents_of_one_filter() {
    // "Sacrifice a black creature, a red creature, and a green creature"
    // (Dragon's Herald) kept only the last filter.
    let e = load_card(
        "Creature — Goblin\n  pt 1/1",
        "activated {T}, sacrifice black creature, sacrifice red creature: you draw 1 card",
    )
    .unwrap_err();
    assert!(e.contains("one filter"), "{e}");
    let r = load_card(
        "Creature — Goblin\n  pt 1/1",
        "activated {T}, sacrifice 2 red creature: you draw 1 card",
    );
    assert!(r.is_ok(), "{r:?}");
}

#[test]
fn do_once_each_turn_limits_a_you_may() {
    // Without a "you may" the limit limited nothing.
    let kind = "Creature — Human\n  pt 4/6";
    let e = load_card(
        kind,
        "triggered do once each turn whenever you gain life: put that much +1/+1 counters on each creature you control",
    )
    .unwrap_err();
    assert!(e.contains("do once"), "{e}");
    let r = load_card(
        kind,
        "triggered do once each turn whenever you gain life: you may put that much +1/+1 counters on each creature you control",
    );
    assert!(r.is_ok(), "{r:?}");
}

#[test]
fn a_delayed_ability_reads_the_values_of_the_target_it_is_about() {
    // "where X is that spell's mana value" in a delayed trigger about the
    // source read the delayed ability's own (missing) target: 0.
    let e = load_card(
        "Instant",
        "spell: counter target spell; delayed this: at beginning of your next first main, add X mana any combination count where mana value of that target 1",
    )
    .unwrap_err();
    assert!(e.contains("delayed"), "{e}");
    let r = load_card(
        "Instant",
        "spell: counter target spell; delayed target 1: at beginning of your next first main, add X mana any combination count where mana value of that target 1",
    );
    assert!(r.is_ok(), "{r:?}");
}
