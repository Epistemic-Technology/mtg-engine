//! The dungeon cards (CR 309) as data: the three of Adventures in the
//! Forgotten Realms that "venture into the dungeon" chooses among, and
//! Undercity, which only "venture into Undercity" enters (the initiative,
//! CR 726). Each room is a name, the rooms its arrows lead to, and its room
//! ability, "When you move your venture marker into this room, <effect>"
//! (CR 309.4c), built from the engine's effects.
//!
//! They are added to a pool after its tokens (`add_dungeons`), so the
//! rooms that make Treasure tokens use the pool's own Treasure.

use crate::card::{
    AbilityDef, Builder, CardPool, CardRef, CardType, CounterKind, Effect, Keyword, Modification,
    Num, ObjFilter, PlayUntil, Recipient, Room, SearchTo, Supertype, TargetSpec, TriggerSpec,
    TriggeredAbility, TypeSet,
};
use crate::mana::Color;

fn ability(targets: Vec<TargetSpec>, effects: Vec<Effect>) -> AbilityDef {
    AbilityDef::Triggered(TriggeredAbility {
        trigger: TriggerSpec::Reflexive,
        condition: None,
        targets,
        effects,
    })
}

fn room(name: &str, next: &[u8], targets: Vec<TargetSpec>, effects: Vec<Effect>) -> Room {
    Room {
        name: name.to_string(),
        next: next.to_vec(),
        ability: ability(targets, effects),
        id: crate::card::AbilityId::default(),
    }
}

fn n(k: u32) -> Num {
    Num::N(k)
}

fn create(token: Option<CardRef>, count: u32) -> Vec<Effect> {
    token
        .map(|token| Effect::CreateToken {
            token,
            count: n(count),
            who: Recipient::You,
            tapped: false,
        })
        .into_iter()
        .collect()
}

fn scry(k: u32) -> Vec<Effect> {
    vec![Effect::Scry { count: n(k) }]
}

fn draw(k: u32) -> Vec<Effect> {
    vec![Effect::DrawCards {
        count: n(k),
        who: Recipient::You,
    }]
}

/// "<effect> until your next turn" on the first target.
fn until_next_turn(parts: Vec<Modification>) -> Vec<Effect> {
    vec![Effect::UntilYourNextTurn {
        to: Recipient::Target(0),
        parts,
    }]
}

fn filter(t: CardType) -> ObjFilter {
    ObjFilter {
        types: TypeSet::of([t]),
        ..Default::default()
    }
}

/// Add a token definition without taking its name from a corpus token of
/// the same name (`CardPool::lookup` keeps finding that one).
fn token(pool: &mut CardPool, def: crate::card::CardDef) -> CardRef {
    let prev = pool.lookup(&def.name);
    let name = def.name.clone();
    let r = pool.add(def);
    if let Some(p) = prev {
        pool.alias(&name, p);
    }
    r
}

/// Add the dungeon cards (and the tokens their rooms make) to `pool`.
#[allow(clippy::too_many_lines)] // one block per dungeon
pub fn add_dungeons(pool: &mut CardPool) {
    let treasure = pool.lookup("Treasure");
    let goblin = token(
        pool,
        Builder::new("Goblin", "")
            .token()
            .types([CardType::Creature])
            .subtypes(&["Goblin"])
            .color_indicator([Color::Red])
            .pt(1, 1)
            .build(),
    );
    let skeleton = token(
        pool,
        Builder::new("Skeleton", "")
            .token()
            .types([CardType::Creature])
            .subtypes(&["Skeleton"])
            .color_indicator([Color::Black])
            .pt(1, 1)
            .build(),
    );
    let big_skeleton = token(
        pool,
        Builder::new("Skeleton", "")
            .token()
            .types([CardType::Creature])
            .subtypes(&["Skeleton"])
            .color_indicator([Color::Black])
            .pt(4, 1)
            .keywords([Keyword::Menace])
            .build(),
    );
    let atropal = token(
        pool,
        Builder::new("The Atropal", "")
            .token()
            .supertypes([Supertype::Legendary])
            .types([CardType::Creature])
            .subtypes(&["God", "Horror"])
            .color_indicator([Color::Black])
            .pt(4, 4)
            .keywords([Keyword::Deathtouch])
            .build(),
    );
    let creature = vec![TargetSpec::Creature];

    // Lost Mine of Phandelver.
    let mut b = Builder::new("Lost Mine of Phandelver", "");
    for r in [
        room("Cave Entrance", &[1, 2], vec![], scry(1)),
        room("Goblin Lair", &[3, 4], vec![], create(Some(goblin), 1)),
        room("Mine Tunnels", &[4, 5], vec![], create(treasure, 1)),
        room(
            "Storeroom",
            &[6],
            creature.clone(),
            vec![Effect::AddCounters {
                kind: CounterKind::PlusOnePlusOne,
                count: n(1),
                to: Recipient::Target(0),
            }],
        ),
        room(
            "Dark Pool",
            &[6],
            vec![],
            vec![
                Effect::LoseLife {
                    amount: n(1),
                    who: Recipient::EachOpponent,
                },
                Effect::GainLife {
                    amount: n(1),
                    who: Recipient::You,
                },
            ],
        ),
        room(
            "Fungi Cavern",
            &[6],
            creature.clone(),
            until_next_turn(vec![Modification::ModifyPt {
                power: -4,
                toughness: 0,
            }]),
        ),
        room("Temple of Dumathoin", &[], vec![], draw(1)),
    ] {
        b = b.room(r);
    }
    pool.add(b.build());

    // Dungeon of the Mad Mage.
    let mut b = Builder::new("Dungeon of the Mad Mage", "");
    for r in [
        room(
            "Yawning Portal",
            &[1],
            vec![],
            vec![Effect::GainLife {
                amount: n(1),
                who: Recipient::You,
            }],
        ),
        room("Dungeon Level", &[2, 3], vec![], scry(1)),
        room("Goblin Bazaar", &[4], vec![], create(treasure, 1)),
        room(
            "Twisted Caverns",
            &[4],
            creature.clone(),
            until_next_turn(vec![Modification::AddKeywords(
                crate::card::KeywordSet::NONE.with(Keyword::CantAttack),
            )]),
        ),
        room("Lost Level", &[5, 6], vec![], scry(2)),
        room(
            "Runestone Caverns",
            &[7],
            vec![],
            vec![Effect::ExilePlayable {
                count: n(2),
                until: PlayUntil::Forever,
            }],
        ),
        room(
            "Muiral's Graveyard",
            &[7],
            vec![],
            create(Some(skeleton), 2),
        ),
        room("Deep Mines", &[8], vec![], scry(3)),
        room(
            "Mad Wizard's Lair",
            &[],
            vec![],
            vec![Effect::DrawCastOne { count: 3 }],
        ),
    ] {
        b = b.room(r);
    }
    pool.add(b.build());

    // Tomb of Annihilation.
    let any_of = ObjFilter {
        any_types: TypeSet::of([CardType::Creature, CardType::Artifact, CardType::Land]),
        ..Default::default()
    };
    let sac = |t: CardType| Effect::SacrificeChosen {
        who: Recipient::You,
        filter: filter(t),
        count: n(1),
    };
    let mut b = Builder::new("Tomb of Annihilation", "");
    for r in [
        room(
            "Trapped Entry",
            &[1, 3],
            vec![],
            vec![Effect::LoseLife {
                amount: n(1),
                who: Recipient::EachPlayer,
            }],
        ),
        room(
            "Veils of Fear",
            &[2],
            vec![],
            vec![Effect::PunishEach {
                life: 2,
                unless: None,
            }],
        ),
        room(
            "Sandfall Cell",
            &[4],
            vec![],
            vec![Effect::PunishEach {
                life: 2,
                unless: Some(any_of),
            }],
        ),
        room(
            "Oubliette",
            &[4],
            vec![],
            vec![
                Effect::Discard {
                    count: n(1),
                    who: Recipient::You,
                },
                sac(CardType::Creature),
                sac(CardType::Artifact),
                sac(CardType::Land),
            ],
        ),
        room(
            "Cradle of the Death God",
            &[],
            vec![],
            create(Some(atropal), 1),
        ),
    ] {
        b = b.room(r);
    }
    pool.add(b.build());

    // Undercity (Commander Legends: Battle for Baldur's Gate).
    let basic_land = ObjFilter {
        types: TypeSet::of([CardType::Land]),
        supertypes: crate::card::SupertypeSet::of([Supertype::Basic]),
        ..Default::default()
    };
    let mut b = Builder::new("Undercity", "");
    for r in [
        room(
            "Secret Entrance",
            &[1, 2],
            vec![],
            vec![Effect::Search {
                filter: basic_land,
                count: n(1),
                to: SearchTo::Hand,
            }],
        ),
        room(
            "Forge",
            &[3, 4],
            creature.clone(),
            vec![Effect::AddCounters {
                kind: CounterKind::PlusOnePlusOne,
                count: n(2),
                to: Recipient::Target(0),
            }],
        ),
        room("Lost Well", &[4, 5], vec![], scry(2)),
        room(
            "Trap!",
            &[6],
            vec![TargetSpec::Player],
            vec![Effect::LoseLife {
                amount: n(5),
                who: Recipient::Target(0),
            }],
        ),
        // CR 701.15: goaded until your next turn; with one opponent,
        // that is attacking each combat if able.
        room(
            "Arena",
            &[6, 7],
            creature.clone(),
            until_next_turn(vec![Modification::AddKeywords(
                crate::card::KeywordSet::NONE.with(Keyword::MustAttack),
            )]),
        ),
        room("Stash", &[7], vec![], create(treasure, 1)),
        room("Archives", &[8], vec![], draw(1)),
        room("Catacombs", &[8], vec![], create(Some(big_skeleton), 1)),
        room(
            "Throne of the Dead Three",
            &[],
            vec![],
            vec![
                Effect::RevealPutCreature {
                    count: 10,
                    counters: 3,
                },
                Effect::UntilYourNextTurn {
                    to: Recipient::CreatedObject,
                    parts: vec![Modification::AddKeywords(
                        crate::card::KeywordSet::NONE.with(Keyword::Hexproof),
                    )],
                },
            ],
        ),
    ] {
        b = b.room(r);
    }
    pool.add(b.build());
}
