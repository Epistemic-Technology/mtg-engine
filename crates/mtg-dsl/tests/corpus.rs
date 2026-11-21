//! The built-in corpus: every card file parses, compiles, and survives a
//! format/parse round trip.

use std::collections::HashMap;

use mtg_core::card::{AbilityDef, CardDef, CardPool};
use mtg_dsl::{builtin_cards, builtin_pool, compile, format, parse};

fn comparable(mut def: CardDef) -> CardDef {
    // Texts are rendered from the same tree; compare the structure.
    def.oracle_text.clear();
    for a in &mut def.abilities {
        match a {
            AbilityDef::Activated(x) => x.text.clear(),
            AbilityDef::Modal(modes) => {
                for m in modes {
                    m.text.clear();
                }
            }
            _ => {}
        }
    }
    def
}

#[test]
fn corpus_compiles() {
    let pool = builtin_pool();
    assert!(pool.len() >= 130, "{} definitions", pool.len());
    for name in [
        "Lightning Bolt",
        "Grizzly Bears",
        "Forest",
        "Soldier",
        "Elspeth, Knight-Errant emblem",
    ] {
        assert!(pool.lookup(name).is_some(), "{name} missing");
    }
    let bolt = pool.get(pool.lookup("Lightning Bolt").unwrap());
    assert_eq!(
        bolt.oracle_text,
        "Lightning Bolt deals 3 damage to any target."
    );
    let sgc = pool.get(pool.lookup("Siege-Gang Commander").unwrap());
    assert_eq!(
        sgc.oracle_text,
        "When this creature enters, create three 1/1 red Goblin creature tokens.\n{1}{R}, Sacrifice a Goblin: This creature deals 2 damage to any target."
    );
}

#[test]
fn every_card_has_an_oracle_id_and_a_slug_file_name() {
    for (path, text) in mtg_dsl::card_files() {
        for card in parse(text).unwrap() {
            if card.kind == mtg_dsl::ast::Kind::Card {
                assert!(
                    card.oracle_id.is_some(),
                    "{}: {} has no oracle_id",
                    path,
                    card.name
                );
            }
            let slug = mtg_dsl::slug(&card.name);
            assert!(
                path.ends_with(&format!("{slug}.card")),
                "{} should be in {slug}.card",
                card.name
            );
        }
    }
}

#[test]
fn format_parse_compile_round_trips() {
    let cards = builtin_cards().unwrap();
    let mut reparsed = Vec::new();
    for card in &cards {
        let text = format::format(card);
        let mut again = parse(&text).unwrap_or_else(|e| panic!("{}: {e}\n{text}", card.name));
        assert_eq!(again.len(), 1);
        let mut again = again.remove(0);
        again.line = card.line;
        // A line the formatter splits ("mana {G} or {U}") moves the faces
        // after it; line numbers are not part of the round trip.
        for (f, g) in again.faces.iter_mut().zip(&card.faces) {
            f.line = g.line;
        }
        compile::split_mana(&mut again);
        assert_eq!(&again, card, "{}:\n{text}", card.name);
        reparsed.push(again);
    }
    let a = compile::compile_all(&cards).unwrap();
    let b = compile::compile_all(&reparsed).unwrap();
    for ((_, x), (_, y)) in a.iter().zip(b.iter()) {
        assert_eq!(comparable(x.clone()), comparable(y.clone()), "{}", x.name);
    }
}

#[test]
fn load_into_adds_a_file_to_an_existing_pool() {
    let mut pool = CardPool::new();
    let refs = mtg_dsl::load_into(
        &mut pool,
        "card \"Grizzly Bears\" {\n  cost {1}{G}\n  type Creature — Bear\n  pt 2/2\n}\n",
    )
    .unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(pool.get(refs[0]).name, "Grizzly Bears");
    let by_name: HashMap<&str, &mtg_dsl::ast::Card> = HashMap::new();
    let _ = by_name;
}

fn compiles(ability: &str) -> bool {
    let mut pool = CardPool::new();
    let text = format!("card \"Test\" {{\n  cost {{1}}\n  type Creature — Bear\n  {ability}\n}}\n");
    mtg_dsl::load_into(&mut pool, &text).is_ok()
}

#[test]
fn it_in_a_trigger_without_a_triggering_object_is_rejected() {
    // An upkeep trigger has no triggering object for `it` to be.
    assert!(!compiles(
        "triggered at beginning of your upkeep: it deals 2 damage to each opponent"
    ));
    assert!(compiles(
        "triggered at beginning of your upkeep: this deals 2 damage to each opponent"
    ));
    assert!(compiles(
        "triggered whenever another creature you control attacks: it gets +1/+0 until end of turn"
    ));
}

#[test]
fn it_after_a_target_in_a_trigger_is_that_target() {
    let card = |ability: &str| {
        let text = format!(
            "card \"Test\" {{\n  cost {{1}}\n  type Creature — Bear\n  pt 2/2\n  {ability}\n}}\n"
        );
        parse(&text).unwrap().remove(0).abilities
    };
    assert_eq!(
        card(
            "triggered whenever this attacks: return up to 1 target creature card from your graveyard to battlefield; you lose count equal mana value of it life"
        ),
        card(
            "triggered whenever this attacks: return up to 1 target creature card from your graveyard to battlefield; you lose count equal mana value of target 1 life"
        )
    );
    assert_eq!(
        card(
            "triggered whenever land you control enters: put 2 +1/+1 counters on target creature you control; it gains vigilance until end of turn"
        ),
        card(
            "triggered whenever land you control enters: put 2 +1/+1 counters on target creature you control; target 1 gains vigilance until end of turn"
        )
    );
}

#[test]
fn a_static_ability_rejects_a_resolution_only_count() {
    // "the number of cards you've drawn this turn" is kept for a resolving
    // effect; a static ability would read 0 (`CountExpr::in_static`).
    assert!(compiles("static this: cda pt count creature you control"));
    for line in [
        "static this: cda pt drawn this turn",
        "static this: cda power greatest power creature you control",
    ] {
        assert!(!compiles(line), "{line}");
    }
}
