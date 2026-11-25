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
fn put_into_a_graveyard_from_anywhere_for_a_group_is_rejected() {
    // It would compile to a dies trigger, missing a card discarded or
    // milled.
    let e = creature_error("triggered whenever creature is put into a graveyard: draw 1 card");
    assert!(e.contains("from the battlefield"), "{e}");
    assert!(compiles(
        "triggered whenever creature is put into a graveyard from the battlefield: draw 1 card"
    ));
}

#[test]
fn a_granted_static_that_changes_types_is_rejected() {
    // A granted static ability exists only from layer 6 on.
    let e = creature_error("static this: has ability { static this: add type artifact }");
    assert!(e.contains("granted static"), "{e}");
    assert!(compiles(
        "static this: has ability { static this: has hexproof as long as this is untapped }"
    ));
}

#[test]
fn a_delayed_triggered_mana_ability_only_adds_mana() {
    // High Tide's form compiles; one doing more than adding mana is a
    // compile error, not a panic.
    assert!(
        load_card(
            "Sorcery",
            "spell: until end of turn, delayed this: whenever Forest is tapped for mana additional, add {G}"
        )
        .is_ok()
    );
    let e = spell_error(
        "until end of turn, delayed this: whenever Forest is tapped for mana, draw 1 cards",
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
fn a_delayed_trigger_about_tokens_cant_watch_the_source_die() {
    // Leaving watches the source (Mysterio); dying has no such form.
    assert!(compiles(
        "triggered when this enters: create 2 \"1/1 white Soldier creature\"; delayed those tokens: exile those tokens when this leaves the battlefield",
    ));
    let e = creature_error(
        "triggered when this enters: create 2 \"1/1 white Soldier creature\"; delayed those tokens: exile those tokens when this dies",
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
fn a_power_range_is_an_error() {
    // "with power 4 or 5" would otherwise be read as power 4; a mana
    // value range is a filter of its own (`StatusQuals::mana_value_range`).
    let e = spell_error("destroy target creature with power 4 or 5");
    assert!(e.contains("range"), "{e}");
    assert!(
        load_card(
            "Sorcery",
            "spell: destroy target creature with mana value 4 or 5"
        )
        .is_ok()
    );
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
    // "the number of spells you've cast this turn" is kept for a resolving
    // effect; a static ability would read 0 (`CountExpr::in_static`). The
    // cards you've drawn this turn a static can read (Duelist of the Mind).
    assert!(compiles("static this: cda pt count creature you control"));
    assert!(compiles("static this: cda power drawn this turn"));
    for line in [
        "static this: cda pt spells cast this turn",
        "static this: cda power greatest power creature you control",
    ] {
        let e = creature_error(line);
        assert!(e.contains("can't use this count"), "{line}: {e}");
    }
}

#[test]
fn a_static_condition_the_layers_never_hold_is_rejected() {
    // `count N <expr>` with a resolution-only count read 0, and "not their
    // turn", "a creature is attacking" and a triggering-object check never
    // held, in a static ability (`layers::condition_holds`).
    assert!(compiles(
        "static this: has flying as long as count 2 colors among creature you control"
    ));
    for (line, why) in [
        (
            "static this: has flying as long as count 2 different powers among creature you control",
            "can't use this count",
        ),
        (
            "static this: has flying as long as not their turn",
            "no \"they\"",
        ),
        (
            "static this: has flying as long as attacker Rat",
            "not by a static ability",
        ),
        (
            "static this: has flying as long as attacker Rat at you",
            "not by a static ability",
        ),
    ] {
        let e = creature_error(line);
        assert!(e.contains(why), "{line}: {e}");
    }
    // A trigger's condition still reads them.
    assert!(compiles(
        "triggered whenever a player casts spell if not their turn: draw 1 card"
    ));
}

#[test]
fn a_mana_rider_effect_lasting_on_the_spell_is_rejected() {
    // "it" in `spent on ...` is the spell; an effect that lasts on it
    // would end as the spell becomes a permanent (CR 400.7). A keyword
    // given until end of turn is carried over (Generator Servant).
    assert!(compiles(
        "activated {T}: add {G} spent on Dragon creature spell: you gain 2 life"
    ));
    assert!(compiles(
        "activated {T}: add {R} spent on Dragon creature spell: it gains haste until end of turn"
    ));
    let e = creature_error(
        "activated {T}: add {R} spent on Dragon creature spell: it gets +1/+1 until end of turn",
    );
    assert!(e.contains("\"it\" is the spell"), "{e}");
}

#[test]
fn the_tapped_creatures_power_is_stations_only() {
    // Station's count reads toughness for Tapestry Warden's creatures; a
    // hand-written "the tapped creature's power" would too, so the card
    // language has no such count (`ast::this_way_verb`).
    assert!(compiles(
        "activated {1}, tap untapped creature you control: you gain count where Bear you control life"
    ));
    assert!(!compiles(
        "activated {1}, tap untapped creature you control: you gain count where power of the tapped creature life"
    ));
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
    // A part the ward trigger can't pay (tapping a permanent, exiling it).
    let e = creature_error("ward tap untapped creature you control");
    assert!(e.contains("a ward cost may be only"), "{e}");
    let e = creature_error("ward exert this");
    assert!(e.contains("a ward cost may be only"), "{e}");
    let e = creature_error("echo collect evidence 4");
    assert!(e.contains("an echo cost may be only"), "{e}");
    assert!(compiles("ward collect evidence 4"));
    assert!(compiles("ward sacrifice 3 nonland permanent"));
    assert!(compiles("ward blight 2"));
    assert!(compiles(
        "ward discard enchantment or instant or sorcery card"
    ));
    assert!(compiles("ward pay count equal power life"));
    assert!(compiles("ward you get 2 poison counters"));
    assert!(compiles("ward discard 1 card or {2}"));
    assert!(compiles("ward sacrifice creature"));
    assert!(compiles("ward waterbend {4}"));
    assert!(compiles("ward discard 1 card at random"));
}

#[test]
fn wrong_meaning_forms_of_the_sixth_correctness_wave_are_rejected() {
    // No triggering player in a spell.
    let e = spell_error("deal 2 damage to each creature that player controls");
    assert!(e.contains("that player controls"), "{e}");
    // `they control` counts an opponent's permanents, except for the
    // effects that act for each player in turn (life loss, draws, mills,
    // discards, sacrifices: `ThatPlayer::Acting`).
    let e = spell_error("each player gains count each creature they control life");
    assert!(e.contains("they control"), "{e}");
    assert!(
        load_card(
            "Sorcery",
            "spell: each player draws count each creature they control cards"
        )
        .is_ok()
    );
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

#[test]
fn granter_in_a_reflexive_ability_and_not_a_delayed_one() {
    let equipment = "Artifact — Equipment";
    // "When you do, <granter> deals ...": the reflexive ability carries
    // the granter.
    let r = load_card(
        equipment,
        "static equipped: has ability { triggered whenever this attacks: you may pay {1}; when you do, granter deals 1 damage to any target }",
    );
    assert!(r.is_ok(), "{r:?}");
    let e = creature_error(
        "triggered whenever this attacks: you may pay {1}; when you do, granter deals 1 damage to any target",
    );
    assert!(e.contains("granter"), "{e}");
    // A delayed ability doesn't.
    let e = load_card(
        equipment,
        "static equipped: has ability { activated {T}: tap target creature; delayed this: at beginning of the next end step, return granter to hand }",
    )
    .unwrap_err();
    assert!(e.contains("delayed"), "{e}");
    let r = load_card(
        equipment,
        "static equipped: has ability { activated {T}, unattach granter: tap target creature }",
    );
    assert!(r.is_ok(), "{r:?}");
    let r = load_card(
        equipment,
        "static equipped: has ability { activated {T}: remove 1 aim counter from granter }",
    );
    assert!(r.is_ok(), "{r:?}");
}
