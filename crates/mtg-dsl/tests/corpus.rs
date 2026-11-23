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

/// Load one card with `line` (an ability line) into a copy of the built-in
/// pool (for its tokens); the error's text, if any.
fn load_card(kind: &str, line: &str) -> Result<(), String> {
    static POOL: std::sync::OnceLock<CardPool> = std::sync::OnceLock::new();
    let mut pool = POOL.get_or_init(builtin_pool).clone();
    let text = format!("card \"Test\" {{\n  cost {{1}}\n  type {kind}\n  {line}\n}}\n");
    mtg_dsl::load_into(&mut pool, &text)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn compiles(ability: &str) -> bool {
    load_card("Creature — Bear\n  pt 2/2", ability).is_ok()
}

/// The error a creature with `ability` gets; panics if it loads.
fn creature_error(ability: &str) -> String {
    load_card("Creature — Bear\n  pt 2/2", ability).unwrap_err()
}

fn spell_error(spell: &str) -> String {
    load_card("Sorcery", &format!("spell: {spell}")).unwrap_err()
}

#[test]
fn it_in_a_trigger_without_a_triggering_object_is_rejected() {
    // An upkeep trigger has no triggering object for `it` to be.
    let e =
        creature_error("triggered at beginning of your upkeep: it deals 2 damage to each opponent");
    assert!(e.contains("no triggering object"), "{e}");
    assert!(compiles(
        "triggered at beginning of your upkeep: this deals 2 damage to each opponent"
    ));
    assert!(compiles(
        "triggered whenever another creature you control attacks: it gets +1/+0 until end of turn"
    ));
}

#[test]
fn a_delayed_triggered_mana_ability_is_an_error() {
    // A compile error, not a panic.
    let e = spell_error(
        "until end of turn, delayed this: whenever Forest is tapped for mana this turn, add {G}",
    );
    assert!(e.contains("mana ability"), "{e}");
}

#[test]
fn a_delayed_whenever_needs_a_duration() {
    // CR 603.7c: without a duration it would trigger once.
    let e = spell_error(
        "delayed this: whenever creature you control deals combat damage to a player, draw 1 card",
    );
    assert!(e.contains("needs a duration"), "{e}");
    assert!(
        load_card(
            "Sorcery",
            "spell: until end of turn, delayed this: whenever creature you control deals combat damage to a player, draw 1 card"
        )
        .is_ok()
    );
}

#[test]
fn that_much_needs_an_amount() {
    // A cast trigger's event has no amount; a damage trigger's does.
    let e = creature_error(
        "triggered whenever you cast noncreature spell: this deals that much damage to each opponent",
    );
    assert!(e.contains("no amount"), "{e}");
    assert!(compiles(
        "triggered whenever this is dealt damage: this deals that much damage to each opponent"
    ));
}

#[test]
fn a_delayed_trigger_about_tokens_cant_watch_the_source() {
    let e = creature_error(
        "triggered when this enters: create 2 \"1/1 white Soldier creature\"; delayed those tokens: exile those tokens when this leaves the battlefield",
    );
    assert!(e.contains("watch"), "{e}");
}

#[test]
fn a_target_becomes_a_copy() {
    // The subject is kept: the target becomes the copy, not the source.
    let text = "card \"Test\" {\n  cost {1}\n  type Instant\n  spell: target artifact you control becomes a copy of target artifact you control until end of turn\n}\n";
    let card = parse(text).unwrap().remove(0);
    let Some(mtg_dsl::ast::Ability::Spell { effects, .. }) = card.abilities.first() else {
        panic!("a spell");
    };
    assert!(
        matches!(
            effects.first().and_then(|s| s.effects.first()),
            Some(mtg_dsl::ast::Effect::BecomeCopy {
                who: mtg_dsl::ast::Recipient::Target(0),
                of: mtg_dsl::ast::Recipient::Target(1),
                until: mtg_core::card::CopyUntil::EndOfTurn,
                ..
            })
        ),
        "{effects:?}"
    );
}

#[test]
fn a_filter_with_three_alternatives_is_an_error() {
    // "attacking, blocking, or tapped" is a form of its own; another
    // alternative after "attacking or blocking" would be lost.
    let e =
        spell_error("deal 4 damage to target attacking or blocking creature with flying or reach");
    assert!(e.contains("three alternatives"), "{e}");
}

#[test]
fn a_mana_value_range_is_an_error() {
    // "with mana value 4 or 5" would otherwise be read as mana value 4.
    let e = spell_error("destroy target creature with mana value 4 or 5");
    assert!(e.contains("range"), "{e}");
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
        let e = creature_error(line);
        assert!(e.contains("can't use this count"), "{line}: {e}");
    }
}

#[test]
fn enchanted_creature_biting_any_other_target_is_rejected() {
    // Pain for All: "other" would exclude the Aura, not the creature.
    let e = load_card(
        "Enchantment — Aura\n  enchant creature you control",
        "triggered when this enters: enchanted creature bites another target any",
    )
    .unwrap_err();
    assert!(e.contains("exclude the Aura"), "{e}");
}

#[test]
fn a_ward_cost_part_the_trigger_would_drop_is_rejected() {
    let e = creature_error("ward collect evidence 4");
    assert!(e.contains("ward or echo cost"), "{e}");
    assert!(compiles("ward sacrifice creature"));
    assert!(compiles("ward waterbend {4}"));
    assert!(compiles("ward discard 1 card at random"));
}

#[test]
fn wrong_meaning_forms_of_the_sixth_correctness_wave_are_rejected() {
    // No triggering player in a spell.
    let e = spell_error("deal 2 damage to each creature that player controls");
    assert!(e.contains("that player controls"), "{e}");
    // `they control` counts an opponent's permanents (each player's own
    // only for "loses life", Stronghold Discipline).
    let e = spell_error("each player draws count each creature they control cards");
    assert!(e.contains("they control"), "{e}");
    assert!(
        load_card(
            "Sorcery",
            "spell: each player loses count each creature they control life"
        )
        .is_ok()
    );
    // `enchanted creature` in a target check is what the source enchants.
    let e = spell_error(
        "target creature gets +2/+2; if target 1 is enchanted creature, also target 1 gains hexproof",
    );
    assert!(e.contains("creature with aura"), "{e}");
    // An open delayed trigger would swallow the spell's later target.
    let e = spell_error(
        "choose target creature an opponent controls; delayed target 1: when that creature dies this turn, you gain 3 life; target creature you control fights that target 1",
    );
    assert!(e.contains("delayed trigger"), "{e}");
    assert!(
        load_card(
            "Sorcery",
            "spell: choose target creature an opponent controls; delayed target 1: { when that creature dies this turn, you gain 3 life }; target creature you control fights that target 1",
        )
        .is_ok()
    );
}
