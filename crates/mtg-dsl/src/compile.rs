//! Compiles the syntax tree into `mtg_core::card::CardDef`s in a
//! `CardPool`: interns subtypes, resolves token and emblem names, registers
//! delayed triggered abilities, and fills in the rendered Oracle text.

use std::collections::HashMap;

use mtg_core::card::{
    AbilityDef, ActivatedAbility, Affects as CoreAffects, Builder, CardPool, CardRef,
    ChoiceKind as CoreChoiceKind, CopyExcept, Cost as CoreCost, Effect as CoreEffect, EffectCond,
    KeywordSet, Mode as CoreMode, Modification, Num, ObjFilter, Recipient as CoreRecipient,
    Relation, ReplacementAbility, Replaces as CoreReplaces, SacCost as CoreSacCost, Same, SameOf,
    SameWhat, StaticAbility, StaticCondition, SubtypeSet, SupertypeSet, TargetSpec,
    TriggerObj as CoreTriggerObj, TriggerSpec, TriggeredAbility, TypeSet, Who,
};
use mtg_core::mana::{Color, ColorSet, Mana};
use thiserror::Error;

use crate::ast::{
    Ability, Affects, Card, ChoiceKind, Cost, Effect, Filter, InsteadCond, Kind, Mechanic, Mod,
    Recipient, Replacement, Replaces, SacCost, SameRef, Sentence, StaticCond, Target, Trigger,
    TriggerObj,
};
use crate::render::{self, Lookup};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{card}: {message}")]
pub struct CompileError {
    pub card: String,
    pub message: String,
}

/// Compile definitions into a pool, in the order given. Tokens and emblems
/// must come before the cards that create them.
pub fn compile_all(cards: &[Card]) -> Result<CardPool, CompileError> {
    let mut pool = CardPool::new();
    let by_name: HashMap<&str, &Card> = cards.iter().map(|c| (c.name.as_str(), c)).collect();
    for c in cards {
        add_card(&mut pool, c, &by_name)?;
    }
    // The dungeon cards (CR 309), after the tokens their rooms make.
    mtg_core::dungeons::add_dungeons(&mut pool);
    Ok(pool)
}

/// Mobilize's triggered ability, its count and its tokens' attacking
/// filled in by `expand_mechanic`.
/// Keyword abilities whose expansion is written in the DSL (CR 702): `N`
/// stands for the keyword's number. No groups or counts: they would be
/// another card's.
const KEYWORD_EXPANSIONS: &str = "card \"Keywords\" {
  triggered whenever this attacks: defending player sacrifices N permanent
  triggered from graveyard whenever creature is put into your graveyard from battlefield: you may pay COST; if you do, return this to hand; otherwise, exile this
}";

/// The `i`th ability of `KEYWORD_EXPANSIONS`, its `N` replaced by `n` and
/// `COST` by `cost`.
fn keyword_expansion(i: usize, n: u32, cost: Option<&mtg_core::mana::ManaCost>) -> Ability {
    let line = KEYWORD_EXPANSIONS
        .lines()
        .nth(i + 1)
        .expect("a keyword expansion")
        .replace("COST", &cost.map(ToString::to_string).unwrap_or_default())
        .replace('N', &n.to_string());
    let mut cards = crate::parse(&format!("card \"Keywords\" {{\n{line}\n}}"))
        .expect("keyword expansions parse");
    cards.remove(0).abilities.remove(0)
}

const MOBILIZE: &str = "card \"Mobilize\" {
  triggered whenever this attacks: create 1 tapped \"1/1 red Warrior creature\"; delayed those tokens: sacrifice those tokens at beginning of the next end step
}";

/// The token Empower Jace creates.
pub const JACE_TOKEN: &str = "Jace planeswalker";

/// Sort definitions into compile order: tokens, then emblems, then cards,
/// each group by name.
pub fn compile_order(cards: &mut [Card]) {
    // A token whose ability creates a token ("When this token dies, create
    // a Food token") comes after the tokens that don't.
    let makes_tokens = |c: &Card| {
        c.abilities.iter().any(|a| match a {
            Ability::Triggered { effects, .. }
            | Ability::Activated { effects, .. }
            | Ability::Spell { effects, .. } => effects
                .iter()
                .flat_map(|s| &s.effects)
                .any(|e| matches!(e, Effect::CreateToken { .. })),
            _ => false,
        })
    };
    let rank = |c: &Card| match c.kind {
        Kind::Token if makes_tokens(c) => 1,
        Kind::Token => 0,
        Kind::Emblem => 2,
        Kind::Card => 3,
    };
    // A `Card` is large, and a sort moves each one many times; with the
    // whole corpus that was a quarter of start-up. Sort the positions,
    // then move each card once.
    let ranks: Vec<u8> = cards.iter().map(rank).collect();
    let mut order: Vec<usize> = (0..cards.len()).collect();
    order.sort_by(|&a, &b| {
        ranks[a]
            .cmp(&ranks[b])
            .then_with(|| cards[a].name.cmp(&cards[b].name))
            .then(a.cmp(&b))
    });
    // `order[i]` is the card that belongs at `i`: follow each cycle.
    for start in 0..order.len() {
        let mut i = start;
        while order[i] != start && order[i] != i {
            let from = order[i];
            cards.swap(i, from);
            order[i] = i;
            i = from;
        }
        order[i] = i;
    }
    debug_assert!(cards.is_sorted_by(|a, b| (rank(a), &a.name) <= (rank(b), &b.name)));
}

/// Compile one definition into the pool.
#[allow(clippy::implicit_hasher, clippy::too_many_lines)] // one step per kind of face
pub fn add_card(
    pool: &mut CardPool,
    card: &Card,
    by_name: &HashMap<&str, &Card>,
) -> Result<CardRef, CompileError> {
    // Faces first (CR 709, 712, 715): each is its own definition, under its
    // own name unless another definition already has that name.
    let mut faces = Vec::new();
    for f in &card.faces {
        let prev = pool.lookup(&f.name);
        let mut c = Compiler {
            pool,
            card: f,
            lookup: by_name,
            groups: Vec::new(),
            many: None,
            tmap: TargetMap::default(),
            implied: Vec::new(),
            implied_used: std::rc::Rc::default(),
            scavenge: None,
            station: None,
            sunburst: None,
            extort: None,
            rampage: None,
            battle_cry: None,
            token: None,
            cast_snapshot: None,
        };
        let from = c.pool.ability_count();
        let def = c.card()?;
        let to = pool.ability_count();
        let r = pool.add(def);
        pool.set_counts_owner(from, to, r);
        if let Some(prev) = prev {
            pool.alias(&f.name, prev);
        }
        faces.push(r);
    }
    // The token a keyword makes of the card (embalm, eternalize,
    // offspring): its own definition, under a key, named like the card.
    let token = match derived_token(card) {
        Some(t) => {
            let mut c = Compiler {
                pool,
                card: &t,
                lookup: by_name,
                groups: Vec::new(),
                many: None,
                tmap: TargetMap::default(),
                implied: Vec::new(),
                implied_used: std::rc::Rc::default(),
                scavenge: None,
                station: None,
                sunburst: None,
                extort: None,
                rampage: None,
                battle_cry: None,
                token: None,
                cast_snapshot: None,
            };
            let from = c.pool.ability_count();
            let def = c.card()?;
            let to = pool.ability_count();
            let r = pool.add(def);
            pool.set_counts_owner(from, to, r);
            pool.alias(&t.name, r);
            Some(r)
        }
        None => None,
    };
    // A Class's levels or a Case's states (CR 716, 719): the card itself
    // is the first, each a definition with the abilities it has then.
    let stages = stage_cards(card);
    let first = stages.first().unwrap_or(card);
    let mut c = Compiler {
        pool,
        card: first,
        lookup: by_name,
        groups: Vec::new(),
        many: None,
        tmap: TargetMap::default(),
        implied: Vec::new(),
        implied_used: std::rc::Rc::default(),
        scavenge: None,
        station: None,
        sunburst: None,
        extort: None,
        rampage: None,
        battle_cry: None,
        token,
        cast_snapshot: None,
    };
    let from = c.pool.ability_count();
    let mut def = c.card()?;
    let to = pool.ability_count();
    link_exiles(&mut def);
    if !stages.is_empty() {
        def.oracle_text = render::oracle_text(card, by_name);
    }
    if !faces.is_empty() {
        def.layout = card.layout;
        def.faces.clone_from(&faces);
        if card.layout == mtg_core::card::Layout::Split {
            // CR 709.4: outside the stack a split card has both halves'
            // characteristics combined.
            let mut cost = String::new();
            for f in &faces {
                let fd = pool.get(*f);
                cost.push_str(&fd.mana_cost.to_string());
                def.types = def.types.union(fd.types);
            }
            def.mana_cost = mtg_core::mana::ManaCost::parse(&cost).unwrap_or_default();
        } else if let Some((front, _)) = card.name.split_once(" // ") {
            def.name = front.to_string();
        }
    }
    // A token known by a key keeps its plain name off whatever definition
    // already had it.
    let prev = card.display_name.as_deref().and_then(|n| pool.lookup(n));
    let r = pool.add(def);
    // A count in an ability this card grants or creates is read from
    // this card's definition, not from the permanent that has it.
    pool.set_counts_owner(from, to, r);
    if !faces.is_empty() {
        pool.alias(&card.name, r);
        // CR 709.5: a Room's locked and unlocked states.
        pool.add_room_doors(r);
        // A double-faced card's back face turns back into the front (a
        // modal one's too, CR 712.3).
        if matches!(
            card.layout,
            mtg_core::card::Layout::Transform | mtg_core::card::Layout::ModalDfc
        ) {
            pool.set_faces(faces[0], card.layout, vec![r]);
        }
    }
    if let Some(display) = &card.display_name {
        pool.alias(&card.name, r);
        if let Some(prev) = prev {
            pool.alias(display, prev);
        }
    }
    if !stages.is_empty() {
        add_stages(pool, card, &stages[1..], r, by_name)?;
    }
    Ok(r)
}

/// Compile a Class's later levels or a solved Case (`stage_cards`) and
/// link them to the card `r` (`CardDef::stages`).
fn add_stages(
    pool: &mut CardPool,
    card: &Card,
    later: &[Card],
    r: CardRef,
    by_name: &HashMap<&str, &Card>,
) -> Result<(), CompileError> {
    let mut refs = vec![r];
    for s in later {
        let mut c = Compiler {
            pool,
            card: s,
            lookup: by_name,
            groups: Vec::new(),
            many: None,
            tmap: TargetMap::default(),
            implied: Vec::new(),
            implied_used: std::rc::Rc::default(),
            scavenge: None,
            station: None,
            extort: None,
            rampage: None,
            battle_cry: None,
            token: None,
            cast_snapshot: None,
            sunburst: None,
        };
        let from = c.pool.ability_count();
        let mut d = c.card()?;
        let to = pool.ability_count();
        link_exiles(&mut d);
        d.oracle_text.clone_from(&pool.get(r).oracle_text);
        let s = pool.add(d);
        pool.set_counts_owner(from, to, s);
        refs.push(s);
    }
    // Each stage registered under the card's name; the card keeps it.
    pool.alias(&card.name, r);
    pool.set_stages(&refs);
    Ok(())
}

/// A Class's levels (CR 716) or a Case's unsolved and solved states (CR
/// 719), each the card with the abilities it has then; empty for any
/// other card. A Class at level N has the abilities before its level N+1
/// marker, but of the level markers only that one (CR 716.2a); a Case
/// unsolved has its to-solve ability and not its solved ones, and solved
/// the reverse.
fn stage_cards(card: &Card) -> Vec<Card> {
    let levels: Vec<usize> = card
        .abilities
        .iter()
        .enumerate()
        .filter(|(_, a)| matches!(a, Ability::ClassLevel { .. }))
        .map(|(i, _)| i)
        .collect();
    if !levels.is_empty() {
        let mut out = Vec::new();
        for k in 0..=levels.len() {
            let end = levels.get(k).copied().unwrap_or(card.abilities.len());
            let abilities = card.abilities[..end]
                .iter()
                .filter(|a| !matches!(a, Ability::ClassLevel { .. }))
                .chain(levels.get(k).map(|i| &card.abilities[*i]))
                .cloned()
                .collect();
            out.push(Card {
                abilities,
                ..card.clone()
            });
        }
        return out;
    }
    let case = card
        .abilities
        .iter()
        .any(|a| matches!(a, Ability::ToSolve(_) | Ability::Solved(_)));
    if !case {
        return Vec::new();
    }
    let unsolved = card
        .abilities
        .iter()
        .filter(|a| !matches!(a, Ability::Solved(_)))
        .cloned()
        .collect();
    let solved = card
        .abilities
        .iter()
        .filter(|a| !matches!(a, Ability::ToSolve(_)))
        .map(|a| match a {
            Ability::Solved(inner) => inner.as_ref().clone(),
            other => other.clone(),
        })
        .collect();
    vec![
        Card {
            abilities: unsolved,
            ..card.clone()
        },
        Card {
            abilities: solved,
            ..card.clone()
        },
    ]
}

struct Compiler<'a> {
    pool: &'a mut CardPool,
    card: &'a Card,
    lookup: &'a dyn Lookup,
    /// The pool's filter id of each of the card's groups.
    groups: Vec<u16>,
    /// The index of the variable target group (CR 601.2c) of the ability
    /// being compiled, whose targets run to the end of the list.
    many: Option<usize>,
    /// How the targets as written map to the engine's (`target_index`).
    tmap: TargetMap,
    /// Per group, the choice its filter implies ("of the color of your
    /// choice"), made as the effect using it resolves.
    implied: Vec<Option<CoreChoiceKind>>,
    /// Set as an effect's recipient uses such a group.
    implied_used: std::rc::Rc<std::cell::Cell<Option<CoreChoiceKind>>>,
    /// Scavenge's count (the card's power), once added to the counts.
    scavenge: Option<u8>,
    /// Station's count (the tapped creature's power), likewise.
    station: Option<u8>,
    /// Sunburst's count (colors of mana spent), in `counts`.
    sunburst: Option<u8>,
    /// Extort's count (the life lost this way), once added to the counts.
    extort: Option<u8>,
    /// Rampage's count (creatures blocking it beyond the first), likewise.
    rampage: Option<u8>,
    /// Battle cry's group ("each other attacking creature"), in `groups`.
    battle_cry: Option<u16>,
    /// The token embalm, eternalize, or offspring creates, compiled
    /// before the card (`derived_token`).
    token: Option<CardRef>,
    /// The condition a spell's `AsCast` condition judges as it is cast.
    cast_snapshot: Option<StaticCondition>,
}

impl Compiler<'_> {
    fn err<T>(&self, message: impl Into<String>) -> Result<T, CompileError> {
        Err(CompileError {
            card: self.card.name.clone(),
            message: message.into(),
        })
    }

    #[allow(clippy::too_many_lines)] // one step per header part
    fn card(&mut self) -> Result<mtg_core::card::CardDef, CompileError> {
        let card = self.card;
        // A leveler's keywords after its first level symbol are that
        // level's (CR 711.2), not the card's.
        let unbanded = card
            .abilities
            .iter()
            .position(|a| matches!(a, Ability::LevelBand { .. }))
            .unwrap_or(card.abilities.len());
        let keywords =
            card.keywords
                .iter()
                .copied()
                .chain(card.abilities[..unbanded].iter().filter_map(|a| match a {
                    Ability::Keyword(k) => Some(*k),
                    _ => None,
                }));
        let protection = card
            .abilities
            .iter()
            .any(|a| matches!(a, Ability::Protection(_)))
            .then_some(mtg_core::card::Keyword::ProtectionFromFilter);
        let evoke = card
            .abilities
            .iter()
            .any(|a| matches!(a, Ability::Mechanic(Mechanic::Evoke(_))))
            .then_some(mtg_core::card::Keyword::Evoke);
        let hexproof_from = card
            .abilities
            .iter()
            .any(|a| matches!(a, Ability::HexproofFrom(_)))
            .then_some(mtg_core::card::Keyword::HexproofFromFilter);
        let keywords = keywords.chain(protection).chain(evoke).chain(hexproof_from);
        // "Stonework Packbeast is also a Cleric, Rogue, Warrior, and
        // Wizard.": the types apply in every zone, so they are the card's
        // own (its static ability adds them again on the battlefield).
        let mut subtypes: Vec<&str> = card.subtypes.iter().map(String::as_str).collect();
        for a in &card.abilities {
            if let Ability::Static {
                filter,
                mods,
                condition: None,
                ..
            } = a
                && render::also_subtypes(filter, mods).is_some()
            {
                for m in mods {
                    if let Mod::AddSubtype(x) = m
                        && !subtypes.contains(&x.as_str())
                    {
                        subtypes.push(x);
                    }
                }
            }
        }
        let mut b = Builder::new(
            card.display_name.as_deref().unwrap_or(&card.name),
            &card.cost.to_string(),
        )
        .supertypes(card.supertypes.iter().copied())
        .types(card.types.iter().copied())
        .subtypes(&subtypes)
        .keywords(keywords)
        .text(&render::oracle_text(card, self.lookup));
        if let Some((p, t)) = card.pt {
            b = b.pt(p, t);
        }
        let mut counts: Vec<mtg_core::card::CountExpr> = card
            .counts
            .iter()
            .map(|c| self.count_expr(&c.expr))
            .collect();
        for a in &card.abilities {
            match unlabeled(a) {
                Ability::SelfCost { per, times } => {
                    b = b.self_cost(mtg_core::card::SelfCost {
                        per: *per,
                        times: *times,
                    });
                }
                Ability::TargetCost { delta, filter, .. } => {
                    let f = self.filter(filter);
                    b = b.target_cost(*delta, f);
                }
                Ability::Uncounterable => {
                    b = b.uncounterable();
                }
                Ability::CondCost { delta, cond } => {
                    let c = self.static_cond(cond);
                    let i = self.pool.intern_condition(c);
                    b = b.cond_cost(*delta, i);
                }
                Ability::Mechanic(Mechanic::Buyback(cost)) => {
                    b = b.buyback(cost.clone());
                }
                Ability::Mechanic(Mechanic::BuybackCost(cost)) => {
                    let Some(crate::ast::SacCost::Filter(f)) = &cost.sacrifice else {
                        return self.err("a buyback cost other than mana sacrifices a permanent");
                    };
                    if !cost.mana.is_empty() || cost.life > 0 || cost.discard > 0 || cost.tap {
                        return self.err("a buyback cost other than mana sacrifices a permanent");
                    }
                    let f = self.filter(f);
                    b = b.buyback_sacrifice(f);
                }
                Ability::Mechanic(Mechanic::MorphCost(cost)) => {
                    if cost.tap || cost.discard_self || cost.discard_x || cost.count > 1 {
                        return self.err("a morph cost may be mana, life, a discard, or a return");
                    }
                    let c = self.cost(cost);
                    b = b.morph_cost(c);
                }
                Ability::Mechanic(Mechanic::Evoke(cost)) => {
                    b = b.evoke(cost.clone());
                }
                Ability::Mechanic(Mechanic::Madness(cost)) => {
                    b = b.madness(cost.clone());
                }
                Ability::Mechanic(Mechanic::Mayhem(cost)) => {
                    b = b.mayhem(cost.clone());
                }
                Ability::Mechanic(Mechanic::Foretell(cost)) => {
                    b = b.foretell(cost.clone());
                }
                Ability::Mechanic(Mechanic::Plot(cost)) => {
                    b = b.plot(cost.clone());
                }
                Ability::Mechanic(Mechanic::Gift(kind)) => {
                    let effects = self.gift_effects(*kind)?;
                    b = b.gift(effects);
                }
                Ability::Spree(modes) => {
                    b = b.spree(modes.iter().map(|(c, _)| c.clone()).collect());
                }
                Ability::Mechanic(Mechanic::Sneak(cost)) => {
                    b = b.sneak(cost.clone());
                }
                Ability::Mechanic(Mechanic::Harmonize(cost)) => {
                    b = b.harmonize(cost.clone());
                }
                Ability::Mechanic(Mechanic::WebSlinging(cost)) => {
                    b = b.web_slinging(cost.clone());
                }
                Ability::Mechanic(Mechanic::Suspend(n, cost)) => {
                    b = b.suspend(*n, cost.clone());
                }
                Ability::Mechanic(Mechanic::Entwine(cost)) => {
                    b = b.entwine(cost.clone());
                }
                // CR 700.2: "If ..., choose both instead."
                Ability::ModalIf { may, cond, .. } => {
                    let c = self.static_cond(cond);
                    let cond = self.pool.intern_condition(c);
                    b = b.modes_if(mtg_core::card::ModesIf { cond, must: !*may });
                }
                Ability::Mechanic(Mechanic::Escape(cost, n)) => {
                    b = b.escape(cost.clone(), *n);
                }
                Ability::Mechanic(Mechanic::Modular(n)) => {
                    b = b.modular(*n);
                }
                Ability::Mechanic(Mechanic::Graft(n)) => {
                    b = b.graft(*n);
                }
                Ability::Mechanic(Mechanic::Vanishing(n)) => {
                    b = b.vanishing(*n);
                }
                Ability::Mechanic(Mechanic::Fading(n)) => {
                    b = b.fading(*n);
                }
                // CR 702.120a: with two modes, choosing both pays the
                // escalate cost once: entwine's cost on the both-mode.
                Ability::Mechanic(Mechanic::Escalate(c)) => {
                    if card
                        .abilities
                        .iter()
                        .any(|a| matches!(a, Ability::ModalOf(crate::ast::ModeHead::OneOrMore, _)))
                    {
                        b = b.escalate(c.clone());
                    } else if card
                        .abilities
                        .iter()
                        .any(|a| matches!(a, Ability::ModalBoth(_)))
                    {
                        b = b.entwine(c.clone());
                    } else {
                        return self.err(
                            "escalate on a `choose one or both` or `choose one or more` spell",
                        );
                    }
                }
                // CR 702.76a: "You may cast this for its prowl cost if you
                // dealt combat damage to a player this turn with a [creature
                // of this card's creature types]."
                Ability::Mechanic(Mechanic::Prowl(c)) => {
                    if card.alt_cost.is_some() {
                        return self.err("prowl and an alternative cost of the card's own");
                    }
                    let types: Vec<mtg_core::card::Subtype> =
                        card.subtypes.iter().map(|s| self.pool.subtype(s)).collect();
                    if types.is_empty() {
                        return self.err("prowl on a card without creature types");
                    }
                    self.pool.note_prowl(&types);
                    let cond = self
                        .pool
                        .intern_condition(StaticCondition::ProwlDamage(types));
                    b = b.prowl(c.clone(), cond);
                }
                Ability::Mechanic(Mechanic::Miracle(c)) => {
                    b = b.miracle(c.clone());
                }
                Ability::Mechanic(Mechanic::Emerge(c)) => {
                    if card.alt_cost.is_some() {
                        return self.err("emerge and an alternative cost of the card's own");
                    }
                    b = b.emerge(c.clone());
                }
                Ability::Mechanic(Mechanic::Blitz(c)) => {
                    if card.alt_cost.is_some() {
                        return self.err("blitz and an alternative cost of the card's own");
                    }
                    b = b.blitz(self.cost(c));
                }
                Ability::Mechanic(Mechanic::Prototype(c, p, t)) => {
                    if card.alt_cost.is_some() {
                        return self.err("prototype and an alternative cost of the card's own");
                    }
                    b = b.prototype(c.clone(), *p, *t);
                }
                Ability::Mechanic(Mechanic::Impending(n, cost)) => {
                    b = b.impending(*n, cost.clone());
                }
                Ability::Mechanic(Mechanic::Morph(cost)) => {
                    b = b.morph(cost.clone(), mtg_core::card::MorphKind::Morph);
                }
                Ability::Mechanic(Mechanic::Megamorph(cost)) => {
                    b = b.morph(cost.clone(), mtg_core::card::MorphKind::Megamorph);
                }
                Ability::Mechanic(Mechanic::Disguise(cost)) => {
                    b = b.morph(cost.clone(), mtg_core::card::MorphKind::Disguise);
                }
                Ability::Mechanic(Mechanic::Dash(cost)) => {
                    b = b.dash(cost.clone());
                }
                Ability::Mechanic(Mechanic::Warp(cost)) => {
                    b = b.warp(cost.clone());
                }
                Ability::Mechanic(Mechanic::Overload(cost)) => {
                    b = b.overload(cost.clone());
                }
                Ability::Mechanic(Mechanic::Bestow(cost)) => {
                    b = b.bestow(cost.clone());
                }
                Ability::Mechanic(Mechanic::Splice(cost)) => {
                    b = b.splice(mtg_core::card::Cost {
                        mana: cost.clone(),
                        ..mtg_core::card::Cost::default()
                    });
                }
                Ability::Mechanic(Mechanic::SpliceCost(cost)) => {
                    if cost.tap
                        || cost.discard > 0
                        || cost.discard_self
                        || cost.discard_x
                        || matches!(cost.sacrifice, Some(SacCost::This))
                    {
                        return self.err(
                            "a splice cost may be mana, life, a chosen permanent, or cards exiled from the graveyard",
                        );
                    }
                    let c = self.cost(cost);
                    b = b.splice(c);
                }
                Ability::Mechanic(Mechanic::Offspring(cost)) => {
                    b = b.offspring(cost.clone());
                }
                Ability::Mechanic(Mechanic::Strive(cost)) => {
                    b = b.strive(cost.clone());
                }
                Ability::Mechanic(Mechanic::Bargain) => {
                    b = b.bargain();
                }
                Ability::Mechanic(Mechanic::Conspire) => {
                    b = b.conspire();
                }
                Ability::Mechanic(Mechanic::Awaken(n, c)) => {
                    let Ok(n) = u8::try_from(*n) else {
                        return self.err("awaken is at most 255");
                    };
                    b = b.awaken(n, c.clone());
                }
                Ability::Mechanic(Mechanic::Storm) => {
                    b = b.storm();
                }
                Ability::Mechanic(Mechanic::Multikicker(c)) => {
                    b = b.multikicker(c.clone());
                }
                Ability::Mechanic(Mechanic::Disturb(c)) => {
                    b = b.disturb(c.clone());
                }
                Ability::Mechanic(Mechanic::GraveyardExile) => {
                    b = b.graveyard_exile();
                }
                Ability::Mechanic(Mechanic::Replicate(c)) => {
                    b = b.replicate(c.clone());
                }
                Ability::Mechanic(Mechanic::Dredge(n)) => {
                    let Ok(n) = u8::try_from(*n) else {
                        return self.err("dredge is at most 255");
                    };
                    b = b.dredge(n);
                }
                Ability::Mechanic(Mechanic::Casualty(n)) => {
                    let Ok(n) = u8::try_from(*n) else {
                        return self.err("casualty is at most 255");
                    };
                    b = b.casualty(n);
                }
                Ability::Mechanic(Mechanic::Ripple(n)) => {
                    let Ok(n) = u8::try_from(*n) else {
                        return self.err("ripple is at most 255");
                    };
                    b = b.ripple(n);
                }
                Ability::Mechanic(Mechanic::Teamwork(n)) => {
                    let Ok(n) = u8::try_from(*n) else {
                        return self.err("teamwork is at most 255");
                    };
                    b = b.teamwork(n);
                }
                Ability::AuraStays => {
                    b = b.aura_stays();
                }
                Ability::Mechanic(Mechanic::Ascend) => {
                    b = b.marks(mtg_core::card::Marks::ASCEND);
                }
                Ability::Mechanic(Mechanic::Storied) => {
                    b = b.marks(mtg_core::card::Marks::STORIED);
                }
                Ability::Mechanic(Mechanic::StartYourEngines) => {
                    b = b.marks(mtg_core::card::Marks::ENGINES);
                }
                Ability::Mechanic(Mechanic::BecomesDay) => {
                    b = b.marks(mtg_core::card::Marks::BECOMES_DAY);
                }
                Ability::Mechanic(Mechanic::Daybound) => {
                    b = b.marks(mtg_core::card::Marks::DAYBOUND);
                }
                Ability::Mechanic(Mechanic::Nightbound) => {
                    b = b.marks(mtg_core::card::Marks::NIGHTBOUND);
                }
                Ability::Mechanic(Mechanic::Station) => {
                    // CR 702.184a: "equal to the tapped creature's power".
                    counts.push(mtg_core::card::CountExpr::MovedStat {
                        verb: mtg_core::card::ThisWay::Tapped,
                        stat: mtg_core::card::Stat::Power,
                    });
                    self.station = u8::try_from(counts.len() - 1).ok();
                }
                Ability::Mechanic(Mechanic::Sunburst) => {
                    // CR 702.44a: "for each color of mana spent to cast it".
                    counts.push(mtg_core::card::CountExpr::SpentColors);
                    self.sunburst = u8::try_from(counts.len() - 1).ok();
                }
                Ability::Mechanic(Mechanic::Scavenge(_)) => {
                    // CR 702.97a: "equal to this card's power", as the card
                    // last existed in the graveyard.
                    counts.push(mtg_core::card::CountExpr::SourcePower);
                    self.scavenge = u8::try_from(counts.len() - 1).ok();
                }
                // CR 702.133a: jump-start, "You may cast this card from your
                // graveyard by discarding a card in addition to paying its
                // other costs. Then exile this card." Flashback's casting
                // and exile, for the mana cost and a discard.
                Ability::Mechanic(Mechanic::JumpStart) => {
                    if card.flashback.is_some() {
                        return self.err("jump-start and flashback together");
                    }
                    b = b.flashback(mtg_core::card::Cost {
                        mana: card.cost.clone(),
                        discard: 1,
                        ..mtg_core::card::Cost::default()
                    });
                }
                // CR 702.81a: retrace, "You may cast this card from your
                // graveyard by discarding a land card in addition to paying
                // its other costs." (It isn't exiled.)
                Ability::Mechanic(Mechanic::Retrace) => {
                    if card.flashback.is_some() {
                        return self.err("retrace and flashback together");
                    }
                    let land = ObjFilter {
                        types: TypeSet::of([mtg_core::card::CardType::Land]),
                        ..ObjFilter::default()
                    };
                    b = b.retrace(land);
                }
                // CR 702.117a: surge, "You may cast this spell for its surge
                // cost if you or a teammate has cast another spell this
                // turn." CR 702.137a: spectacle, "... rather than its mana
                // cost if an opponent lost life this turn."
                // CR 702.173a: freerunning, "You may pay [cost] rather than
                // pay this spell's mana cost if a player was dealt combat
                // damage this turn by a creature that, at the time it dealt
                // that damage, was an Assassin creature or a commander under
                // your control."
                Ability::Mechanic(
                    m @ (Mechanic::Surge(cost)
                    | Mechanic::Spectacle(cost)
                    | Mechanic::Freerunning(cost)),
                ) => {
                    if card.alt_cost.is_some() {
                        return self.err("two alternative costs of the card's own");
                    }
                    let cond = match m {
                        Mechanic::Surge(_) => StaticCondition::CastThisTurn {
                            filter: ObjFilter::default(),
                            at_least: 1,
                            other: true,
                        },
                        Mechanic::Freerunning(_) => {
                            self.pool.note_freerunning();
                            StaticCondition::AssassinDealtCombatDamage
                        }
                        _ => StaticCondition::OpponentLostLifeThisTurn,
                    };
                    let condition = Some(self.pool.intern_condition(cond));
                    b = b.alt_cost(mtg_core::card::AltCost {
                        cost: mtg_core::card::Cost {
                            mana: cost.clone(),
                            ..mtg_core::card::Cost::default()
                        },
                        count: 1,
                        condition,
                        others_gain: 0,
                    });
                }
                Ability::Mechanic(Mechanic::Extort) => {
                    // CR 702.101a: "you gain life equal to the total life
                    // lost this way".
                    counts.push(mtg_core::card::CountExpr::LifeLostThisWay);
                    self.extort = u8::try_from(counts.len() - 1).ok();
                }
                Ability::Mechanic(Mechanic::Rampage(_)) if self.rampage.is_none() => {
                    // CR 702.23a: "for each creature blocking it beyond the
                    // first".
                    counts.push(mtg_core::card::CountExpr::BlockingIt { beyond: 1 });
                    self.rampage = u8::try_from(counts.len() - 1).ok();
                }
                Ability::Affinity(f) => {
                    let mut f = self.filter(f);
                    f.who = Some(mtg_core::card::Who::You);
                    counts.push(mtg_core::card::CountExpr::Permanents(f));
                    let i = u8::try_from(counts.len() - 1).expect("few counts");
                    b = b.self_cost(mtg_core::card::SelfCost {
                        per: 1,
                        times: Num::Count(i),
                    });
                }
                _ => {}
            }
        }
        if !counts.is_empty() {
            b = b.counts(counts);
        }
        if let Some(n) = card.loyalty {
            b = b.loyalty(n);
        }
        // CR 701.15: "enchanted creature is goaded".
        if card
            .abilities
            .iter()
            .any(|a| matches!(a, Ability::Static { mods, .. } if mods.contains(&Mod::Goaded)))
        {
            b = b.goads();
        }
        if let Some(n) = card.defense {
            b = b.defense(n);
        }
        if let Some(colors) = &card.color_indicator {
            b = b.color_indicator(colors.iter().copied());
        } else if let Some(colors) = card.abilities.iter().find_map(|a| match a {
            Ability::Static { filter, mods, .. } if filter.relation == Relation::This => {
                mods.iter().find_map(|m| match m {
                    Mod::CdaColors(cs) => Some(cs),
                    _ => None,
                })
            }
            _ => None,
        }) {
            // CR 604.3: a color-defining ability functions in every zone
            // ("Ghostfire is colorless." on the stack), so it is the
            // object's color everywhere, as a color indicator would be
            // (CR 204); on the battlefield its static applies too.
            b = b.color_indicator(colors.iter().copied());
        }
        match card.kind {
            Kind::Token => b = b.token(),
            Kind::Emblem => b = b.emblem(),
            Kind::Card => {}
        }
        if let Some(cost) = &card.additional {
            b = b.additional_cost(self.cost(cost));
        }
        if let Some(cost) = &card.additional_or {
            b = b.additional_or(self.cost(cost));
        } else if card
            .additional
            .as_ref()
            .and_then(|c| c.behold.as_ref())
            .is_some_and(|b| b.optional)
        {
            // "you may behold a Dragon": an optional additional cost, the
            // other of two costs being nothing (CR 601.2b).
            b = b.additional_or(CoreCost::default());
        }
        if let Some(cost) = &card.optional_cost {
            b = b.optional_cost(self.cost(cost));
        }
        if let Some(a) = &card.alt_cost {
            if a.cost.conditions.len() > 1 || a.cost.tap || a.cost.discard > 0 {
                return self.err("an alternative cost is mana and one kind of permanent");
            }
            let condition = match a.cost.conditions.first() {
                Some(c) => {
                    let c = self.static_cond(c);
                    Some(self.pool.intern_condition(c))
                }
                None => None,
            };
            b = b.alt_cost(mtg_core::card::AltCost {
                cost: self.cost(&a.cost),
                count: a.count,
                condition,
                others_gain: a.others_gain,
            });
        }
        if let Some(c) = &card.flash_if {
            let c = self.static_cond(c);
            b = b.flash_if(self.pool.intern_condition(c));
        }
        if card.flash_cleanup {
            b = b.flash_cleanup();
        }
        if let Some(n) = card.unkicked_targets {
            b = b.unkicked_targets(u8::try_from(n).unwrap_or(u8::MAX));
        }
        if let Some(n) = card.kicked_drops {
            b = b.kicked_drops(u8::try_from(n).unwrap_or(u8::MAX));
        }
        if let Some(c) = &card.cast_only {
            let c = self.static_cond(c);
            b = b.cast_only(self.pool.intern_condition(c));
        }
        if let Some(k) = &card.kicker {
            b = b.kicker(k.clone());
        }
        if let Some(n) = card.kicker_targets {
            b = b.kicker_targets(n);
        }
        if let Some(fb) = &card.flashback {
            // "Flashback—Sacrifice three creatures.", "Flashback—Tap three
            // untapped white creatures you control." settle the permanent
            // the way an additional cost does, so not alongside one.
            if fb.tap || fb.discard > 0 || fb.discard_self {
                return self.err("a flashback cost may only be mana, life, and a chosen permanent");
            }
            if fb.sacrifice.is_some() && (card.additional.is_some() || card.additional_or.is_some())
            {
                return self.err("a flashback cost choosing a permanent with an additional cost");
            }
            b = b.flashback(self.cost(fb));
        }
        self.groups = Vec::new();
        self.implied = Vec::new();
        for g in &card.groups {
            let f = self.filter(g);
            let id = self.pool.intern_filter(f);
            self.groups.push(id);
            let implied = g
                .chosen
                .and_then(crate::ast::OfChosen::implied)
                .map(|k| self.choice_kind(&k));
            self.implied.push(implied);
        }
        // CR 702.91a: battle cry's "each other attacking creature".
        if card
            .abilities
            .iter()
            .any(|a| matches!(a, Ability::Mechanic(Mechanic::BattleCry)))
        {
            let f = self.filter(&crate::ast::Filter {
                types: vec![mtg_core::card::CardType::Creature],
                attacking: true,
                relation: mtg_core::card::Relation::Other,
                ..crate::ast::Filter::default()
            });
            let id = self.pool.intern_filter(f);
            self.groups.push(id);
            self.implied.push(None);
            self.battle_cry = u16::try_from(self.groups.len() - 1).ok();
        }
        let spells = card
            .abilities
            .iter()
            .filter(|a| {
                matches!(
                    a,
                    Ability::Spell { .. }
                        | Ability::Modal(_)
                        | Ability::ModalBoth(_)
                        | Ability::ModalTwo(_)
                        | Ability::ModalOf(..)
                        | Ability::ModalIf { .. }
                        | Ability::Spree(_)
                        | Ability::Pawprint { .. }
                )
            })
            .count();
        if spells > 1 {
            return self.err(
                "more than one `spell`/`choose one`; put the paragraphs in one `spell { }` block",
            );
        }
        // CR 711.2: the level symbol the abilities being compiled follow.
        let mut band: Option<StaticCond> = None;
        for a in &card.abilities {
            if let Ability::LevelBand { min, max, pt } = a {
                let at_least = StaticCond::SourceCounters {
                    kind: mtg_core::card::CounterKind::Level,
                    n: *min,
                    at_least: true,
                };
                let cond = match max {
                    Some(m) => StaticCond::And(
                        Box::new(at_least),
                        Box::new(StaticCond::SourceCounters {
                            kind: mtg_core::card::CounterKind::Level,
                            n: *m,
                            at_least: false,
                        }),
                    ),
                    None => at_least,
                };
                // CR 711.2a-b: "it has base power and toughness [P/T]".
                let set = Ability::Static {
                    filter: crate::ast::Filter::this(),
                    mods: vec![Mod::SetPt {
                        power: mtg_core::card::PtValue::Fixed(pt.0),
                        toughness: mtg_core::card::PtValue::Fixed(pt.1),
                    }],
                    condition: Some(cond.clone()),
                    cond_first: false,
                    cond_if: false,
                };
                b = b.ability(self.ability(&set)?);
                band = Some(cond);
                continue;
            }
            if let Some(cond) = &band {
                // "and has [abilities]": a keyword of the level.
                let compiled = if let Ability::Keyword(k) = a {
                    self.ability(&Ability::Static {
                        filter: crate::ast::Filter::this(),
                        mods: vec![Mod::AddKeywords(vec![*k])],
                        condition: Some(cond.clone()),
                        cond_first: false,
                        cond_if: false,
                    })?
                } else {
                    let compiled = self.ability(a)?;
                    if matches!(
                        compiled,
                        AbilityDef::Static(_) | AbilityDef::Activated(_) | AbilityDef::Triggered(_)
                    ) {
                        let c = self.static_cond(cond);
                        self.while_holds(compiled, c, "a level symbol")?
                    } else {
                        // A blocking restriction, protection, and the like:
                        // granted to itself while the level holds.
                        self.ability(&Ability::Static {
                            filter: crate::ast::Filter::this(),
                            mods: vec![Mod::AddAbility(Box::new(a.clone()))],
                            condition: Some(cond.clone()),
                            cond_first: false,
                            cond_if: false,
                        })?
                    }
                };
                b = b.ability(compiled);
                continue;
            }
            // CR 702.152a: blitz's "When this creature dies, draw a
            // card.", the creature's while its blitz cost was paid.
            if let Ability::Mechanic(Mechanic::Blitz(_)) = a {
                let dies = Ability::Triggered {
                    limit: mtg_core::card::TurnLimit::None,
                    from_graveyard: false,
                    trigger: crate::ast::Trigger::Dies(crate::ast::TriggerObj::This),
                    condition: Some(crate::ast::TrigCond::Core(
                        mtg_core::card::Condition::Blitzed,
                    )),
                    targets: Vec::new(),
                    effects: vec![Sentence::new(vec![crate::ast::Effect::DrawCards {
                        count: Num::N(1),
                        who: None,
                    }])],
                };
                b = b.ability(self.ability(&dies)?);
            }
            // CR 702.174b: an instant's or sorcery's gift happens as it
            // resolves; a permanent's is a triggered ability.
            if matches!(unlabeled(a), Ability::Mechanic(Mechanic::Gift(_)))
                && card.types.iter().any(|t| {
                    matches!(
                        t,
                        mtg_core::card::CardType::Instant | mtg_core::card::CardType::Sorcery
                    )
                })
            {
                continue;
            }
            if matches!(
                unlabeled(a),
                Ability::Keyword(_)
                    | Ability::DeckAnyNumber
                    | Ability::Uncounterable
                    | Ability::AuraStays
                    | Ability::SelfCost { .. }
                    | Ability::TargetCost { .. }
                    | Ability::CondCost { .. }
                    | Ability::Affinity(_)
                    | Ability::Mechanic(
                        Mechanic::Buyback(_)
                            | Mechanic::BuybackCost(_)
                            | Mechanic::MorphCost(_)
                            | Mechanic::Evoke(_)
                            | Mechanic::Madness(_)
                            | Mechanic::Mayhem(_)
                            | Mechanic::Foretell(_)
                            | Mechanic::Plot(_)
                            | Mechanic::Sneak(_)
                            | Mechanic::Harmonize(_)
                            | Mechanic::WebSlinging(_)
                            | Mechanic::Suspend(..)
                            | Mechanic::Entwine(_)
                            | Mechanic::Escape(..)
                            | Mechanic::Morph(_)
                            | Mechanic::Megamorph(_)
                            | Mechanic::Disguise(_)
                            | Mechanic::Dash(_)
                            | Mechanic::Warp(_)
                            | Mechanic::Bestow(_)
                            | Mechanic::Overload(_)
                            | Mechanic::Splice(_)
                            | Mechanic::SpliceCost(_)
                            | Mechanic::Bargain
                            | Mechanic::Conspire
                            | Mechanic::Casualty(_)
                            | Mechanic::Dredge(_)
                            | Mechanic::Storm
                            | Mechanic::Awaken(..)
                            | Mechanic::Assist
                            | Mechanic::Myriad
                            | Mechanic::Prototype(..)
                            | Mechanic::Blitz(_)
                            | Mechanic::Escalate(_)
                            | Mechanic::Prowl(_)
                            | Mechanic::Miracle(_)
                            | Mechanic::Emerge(_)
                            | Mechanic::Replicate(_)
                            | Mechanic::Multikicker(_)
                            | Mechanic::Disturb(_)
                            | Mechanic::GraveyardExile
                            | Mechanic::Teamwork(_)
                            | Mechanic::Ripple(_)
                            | Mechanic::Strive(_)
                            | Mechanic::Ascend
                            | Mechanic::Storied
                            | Mechanic::StartYourEngines
                            | Mechanic::BecomesDay
                            | Mechanic::Daybound
                            | Mechanic::Nightbound
                            | Mechanic::Retrace
                            | Mechanic::JumpStart
                            | Mechanic::Surge(_)
                            | Mechanic::Freerunning(_)
                            | Mechanic::Spectacle(_)
                    )
            ) {
                continue;
            }
            let compiled = self.ability(a)?;
            b = b.ability(compiled);
            // "Equip—Pay {3} or discard a card.": the second equip ability.
            if let Ability::EquipOr { other, targets, .. } = a {
                b = b.ability(self.ability(&equip_ability((**other).clone(), targets))?);
            }
            // CR 702.98a: unleash's second static ability, "This permanent
            // can't block as long as it has a +1/+1 counter on it."
            if let Ability::Mechanic(Mechanic::Unleash) = a {
                b = b.ability(unleash_static());
            }
            // CR 702.176a: impending's "As long as this permanent's
            // impending cost was paid and it has a time counter on it, it's
            // not a creature": its other card types replace its own.
            if let Ability::Mechanic(Mechanic::Impending(..)) = a {
                b = b.ability(AbilityDef::Static(StaticAbility {
                    affected: ObjFilter::this(),
                    parts: vec![Modification::SetTypes(mtg_core::card::TypeSet::of(
                        card.types
                            .iter()
                            .copied()
                            .filter(|t| *t != mtg_core::card::CardType::Creature),
                    ))],
                    condition: Some(StaticCondition::Impending),
                }));
            }
            // CR 702.95a: soulbond's second trigger, "Whenever another
            // creature you control enters, ... you may pair that creature
            // with this creature".
            if let Ability::Mechanic(Mechanic::Soulbond) = a {
                let second = soulbond_trigger(true);
                b = b.ability(self.ability(&second)?);
            }
            // "As this creature enters or is turned face up, put ...
            // counters on it": also an enters-with replacement (CR
            // 614.1c).
            if let Ability::AsTurnedFaceUp {
                enters: true,
                kind,
                n,
            } = a
            {
                b = b.ability(AbilityDef::Replacement(ReplacementAbility {
                    replaces: mtg_core::card::Replaces::Enters,
                    replacement: mtg_core::card::Replacement::EnterWithCounters {
                        kind: *kind,
                        n: *n,
                    },
                    then: Vec::new(),
                    condition: None,
                }));
            }
            // "... and with first strike": abilities the permanent has as
            // long as it was kicked (fixed as it enters, CR 702.33c).
            if let Ability::Mechanic(Mechanic::KickedWith(_, ks)) = a
                && !ks.is_empty()
            {
                b = b.ability(AbilityDef::Static(StaticAbility {
                    affected: mtg_core::card::ObjFilter::this(),
                    parts: vec![Modification::AddKeywords(KeywordSet::of(
                        ks.iter().copied(),
                    ))],
                    condition: Some(StaticCondition::Kicked),
                }));
            }
        }
        if let Some(x) = &card.except {
            let mut abilities = Vec::new();
            for a in &x.abilities {
                abilities.push(self.ability(a)?);
            }
            b = b.copy_except(CopyExcept {
                subtypes: x.subtypes.clone(),
                keywords: KeywordSet::of(x.keywords.iter().copied()),
                abilities,
                not_legendary: x.not_legendary,
                types: mtg_core::card::TypeSet::of(x.types.iter().copied()),
                pt: x.pt,
                colors: x
                    .colors
                    .iter()
                    .fold(mtg_core::mana::ColorSet::default(), |s, c| s.with(*c)),
            });
        }
        if let Some(c) = self.cast_snapshot.take() {
            b = b.cast_snapshot(self.pool.intern_condition(c));
        }
        Ok(b.build())
    }

    fn subtypes(&mut self, names: &[String]) -> SubtypeSet {
        names.iter().map(|s| self.pool.subtype(s)).collect()
    }

    #[allow(clippy::too_many_lines)] // one line per filter field
    fn filter(&mut self, f: &Filter) -> ObjFilter {
        // "an outlaw" (CR 700.12): one alternative per creature type.
        if let Some(i) = f.subtypes.iter().position(|s| s == crate::ast::OUTLAW) {
            let mut chain = f.or.clone();
            for t in crate::ast::OUTLAW_TYPES.iter().rev() {
                let mut alt = f.clone();
                alt.subtypes[i] = (*t).to_string();
                alt.or = chain;
                chain = Some(Box::new(alt));
            }
            return self.filter(&chain.expect("five alternatives"));
        }
        if let Some(h) = historic(f) {
            return self.filter(&h);
        }
        ObjFilter {
            who: f.who,
            types: TypeSet::of(f.types.iter().copied()),
            not_types: TypeSet::of(f.not_types.iter().copied()),
            supertypes: SupertypeSet::of(f.supertypes.iter().copied()),
            not_supertypes: SupertypeSet::of(f.not_supertypes.iter().copied()),
            colors: f.colors.as_ref().map(|c| ColorSet::of(c.iter().copied())),
            not_colors: ColorSet::of(f.not_colors.iter().copied()),
            keywords: KeywordSet::of(f.keywords.iter().copied()),
            not_keywords: KeywordSet::of(f.not_keywords.iter().copied()),
            subtypes: self.subtypes(&f.subtypes),
            not_subtypes: self.subtypes(&f.not_subtypes),
            // "and all other creatures with the same name as that
            // creature": other than the target (`TargetAndGroup`), not the
            // source.
            relation: match f.same {
                Some(
                    SameRef::SameNameAs(_) | SameRef::SharesColorWith(_) | SameRef::ShareCardType,
                ) => Relation::Any,
                _ => f.relation,
            },
            same: f.same.map(|r| {
                let tmap = self.tmap;
                let target = |n: usize| {
                    SameOf::Target(u8::try_from(target_index(tmap, n)).unwrap_or(u8::MAX))
                };
                match r {
                    SameRef::NamedThis => Same {
                        of: SameOf::Source,
                        what: SameWhat::Name,
                    },
                    SameRef::NotNamedThis => Same {
                        of: SameOf::Source,
                        what: SameWhat::NotName,
                    },
                    SameRef::SameNameAs(n) => Same {
                        of: target(n),
                        what: SameWhat::Name,
                    },
                    SameRef::ShareCardType => Same {
                        of: target(0),
                        what: SameWhat::SharesType,
                    },
                    SameRef::SharesColorWith(n) => Same {
                        of: target(n),
                        what: SameWhat::Color,
                    },
                    SameRef::LesserPower => Same {
                        of: SameOf::Source,
                        what: SameWhat::LesserPower,
                    },
                    SameRef::ManaValueAtMostPower => Same {
                        of: SameOf::Source,
                        what: SameWhat::ManaValueAtMostPower,
                    },
                }
            }),
            // `named "Keimi"`: a token definition's name; an unknown one
            // matches nothing.
            named: f.named_card.as_ref().map(|n| {
                (
                    self.pool
                        .lookup(n)
                        .unwrap_or(mtg_core::card::CardRef(u32::MAX)),
                    true,
                )
            }),
            enchanted: f.enchanted,
            paired: f.paired.is_some(),
            no_abilities: f.no_abilities,
            any_types: TypeSet::of(f.any_types.iter().copied()),
            power: f.power,
            toughness: f.toughness,
            mana_value: f.mana_value,
            color_count: f.color_count,
            tapped: f.tapped,
            attacking: f.attacking,
            blocking: f.blocking,
            damaged: f.damaged,
            status: f.status,
            chosen: f.chosen.is_some(),
            attached_to: f.attached_to.map(|a| {
                use crate::ast::AttachedRef;
                use mtg_core::card::AttachedTo;
                match a {
                    AttachedRef::Source => AttachedTo::Source,
                    AttachedRef::Host => AttachedTo::SourceHost,
                    AttachedRef::TriggeringObject => AttachedTo::TriggeringObject,
                    AttachedRef::Target(n) | AttachedRef::ThatTarget(n) => AttachedTo::Target(
                        u8::try_from(target_index(self.tmap, n)).unwrap_or(u8::MAX),
                    ),
                }
            }),
            combat_with: f.combat_with.map(|r| {
                use crate::ast::CombatRef;
                use mtg_core::card::CombatWith;
                match r {
                    CombatRef::BlockingThis | CombatRef::BlockingIt => CombatWith::Blocking,
                    CombatRef::BlockedByThis | CombatRef::BlockedByIt => CombatWith::BlockedBy,
                    CombatRef::BlockingOrBlockedByThis => CombatWith::BlockingOrBlockedBy,
                }
            }),
            that_player: f.that_player,
            not_attached: f.not_attached,
            enchanted_player: f.enchanted_player,
            controlled_by: None,
            or: f.or.as_ref().map(|a| Box::new(self.filter(a))),
            targeting: f.targeting.as_ref().map(|t| {
                Box::new(mtg_core::card::SpellTargets {
                    you: t.you,
                    player: t.player,
                    filter: t.filter.as_ref().map(|g| self.filter(g)),
                })
            }),
        }
    }

    /// A mode list; with `both`, a third mode that is the two together
    /// ("choose one or both", CR 700.2); with `two`, only the pairs of
    /// different modes ("choose two", CR 700.2d), each made like "both".
    fn modes(
        &mut self,
        modes: &[crate::ast::Mode],
        both: bool,
        two: bool,
    ) -> Result<Vec<CoreMode>, CompileError> {
        let mut out = Vec::new();
        for m in modes {
            out.push(CoreMode {
                targets: self.targets(&m.targets),
                effects: self.sentences(&m.effects)?,
                text: render::mode_text(self.card, m, self.lookup)
                    .trim_end_matches('.')
                    .to_string(),
                split: None,
                split2: None,
                more: Vec::new(),
                once: mtg_core::card::ModeOnce::Any,
                name: m.name.clone(),
            });
        }
        let pair = |a: &CoreMode, b: &CoreMode| {
            // The first mode ends with a variable target group (CR
            // 601.2c), which the engine takes to the end of the targets:
            // the second mode's targets go first. Its effects still happen
            // second, each part seeing its own targets.
            if matches!(a.targets.last(), Some(TargetSpec::Many(_))) && !b.targets.is_empty() {
                return CoreMode {
                    targets: b.targets.iter().chain(&a.targets).copied().collect(),
                    effects: a.effects.iter().chain(&b.effects).cloned().collect(),
                    text: format!("{} and {}", a.text, b.text),
                    split: Some((0, u16::try_from(b.targets.len()).unwrap_or(u16::MAX))),
                    split2: Some((u16::try_from(a.effects.len()).unwrap_or(u16::MAX), 0)),
                    more: Vec::new(),
                    once: mtg_core::card::ModeOnce::Any,
                    name: None,
                };
            }
            CoreMode {
                targets: a.targets.iter().chain(&b.targets).copied().collect(),
                effects: a.effects.iter().chain(&b.effects).cloned().collect(),
                text: format!("{} and {}", a.text, b.text),
                split: Some((
                    u16::try_from(a.effects.len()).unwrap_or(u16::MAX),
                    u16::try_from(a.targets.len()).unwrap_or(u16::MAX),
                )),
                split2: None,
                more: Vec::new(),
                once: mtg_core::card::ModeOnce::Any,
                name: None,
            }
        };
        if two {
            // The chosen mode is kept in three bits (`CastInfo`): at most
            // four modes, six pairs. The pairs alone are the modes; the
            // single modes are read back from them.
            if !(3..=4).contains(&out.len()) || modes.iter().any(|m| m.name.is_some()) {
                return self.err("`choose two` takes three or four unnamed modes");
            }
            let mut pairs = Vec::new();
            for i in 0..out.len() {
                for j in i + 1..out.len() {
                    pairs.push(pair(&out[i], &out[j]));
                }
            }
            return Ok(pairs);
        }
        if both {
            let [a, b] = out.as_slice() else {
                return self.err("`choose one or both` takes two modes");
            };
            let both = pair(a, b);
            out.push(both);
        }
        Ok(out)
    }

    /// An ability the source has only while `c` holds ("Max speed —
    /// ...", "2+ | ..."): a static ability's or cost change's condition,
    /// an activated ability's activation condition, a triggered
    /// ability's condition as it triggers. A triggered ability checks it
    /// again on resolution; neither speed nor charge counters normally
    /// fall in between.
    fn while_holds(
        &mut self,
        mut a: AbilityDef,
        c: StaticCondition,
        what: &str,
    ) -> Result<AbilityDef, CompileError> {
        match &mut a {
            AbilityDef::Static(sa) => {
                sa.condition = Some(match sa.condition.take() {
                    Some(own) => StaticCondition::And(Box::new(own), Box::new(c)),
                    None => c,
                });
            }
            AbilityDef::Activated(aa) => aa.conditions.push(c),
            AbilityDef::Triggered(ta) if ta.condition.is_none() => {
                let i = self.pool.intern_condition(c);
                ta.condition = Some(mtg_core::card::Condition::Static(i));
            }
            // A trigger with an "if" of its own: both.
            AbilityDef::Triggered(ta)
                if let Some(mtg_core::card::Condition::Static(j)) = ta.condition =>
            {
                let both =
                    StaticCondition::And(Box::new(c), Box::new(self.pool.condition(j).clone()));
                let i = self.pool.intern_condition(both);
                ta.condition = Some(mtg_core::card::Condition::Static(i));
            }
            AbilityDef::CostChange(cc) if cc.cond.is_none() => {
                cc.cond = Some(self.pool.intern_condition(c));
            }
            AbilityDef::Replacement(r) if r.condition.is_none() => {
                r.condition = Some(self.pool.intern_condition(c));
            }
            _ => {
                return self.err(format!(
                    "{what} applies to a static, activated, triggered, or cost ability"
                ));
            }
        }
        Ok(a)
    }

    /// What a gift gives the chosen opponent (CR 702.174d-h); the game
    /// has one opponent, so it is each opponent.
    fn gift_effects(
        &mut self,
        kind: crate::ast::GiftKind,
    ) -> Result<Vec<CoreEffect>, CompileError> {
        use crate::ast::{Effect as E, GiftKind, Recipient as R};
        let token = |name: &str, tapped: bool| E::CreateToken {
            token: name.to_string(),
            count: Num::N(1),
            tapped,
            attacking: false,
            who: Some(R::EachOpponent),
            it_has: false,
            attached: None,
            sized: None,
        };
        let e = match kind {
            GiftKind::Card => E::DrawCards {
                count: Num::N(1),
                who: Some(R::EachOpponent),
            },
            GiftKind::Food => token("Food", false),
            GiftKind::TappedFish => token("1/1 blue Fish creature", true),
            GiftKind::Treasure => token("Treasure", false),
        };
        self.sentences(&[Sentence::new(vec![e])])
    }

    /// The modes under another head (CR 700.2). "Choose one or more":
    /// every nonempty set of the printed modes, the set with bits `b` at
    /// index `b - 1` (as spree). "Choose up to N": the sets of at most N
    /// modes in the same order, then no mode at all. "Choose one that
    /// hasn't been chosen": the printed modes, each chosen at most once
    /// (`Mode::once`).
    fn head_modes(
        &mut self,
        head: crate::ast::ModeHead,
        modes: &[crate::ast::Mode],
    ) -> Result<Vec<CoreMode>, CompileError> {
        use crate::ast::ModeHead;
        let mut singles = self.modes(modes, false, false)?;
        let n = singles.len();
        let most = match head {
            ModeHead::Unchosen { this_turn } => {
                let once = if this_turn {
                    mtg_core::card::ModeOnce::ThisTurn
                } else {
                    mtg_core::card::ModeOnce::Ever
                };
                for m in &mut singles {
                    m.once = once;
                }
                return Ok(singles);
            }
            ModeHead::OneOrMore => n,
            ModeHead::UpTo(k) => usize::from(k).min(n),
        };
        // The chosen mode is kept in four bits (`CastInfo`).
        if !(2..=4).contains(&n) {
            return self.err("`choose one or more` and `choose up to` take two to four modes");
        }
        let mut out = Vec::new();
        for bits in 1..(1usize << n) {
            if bits.count_ones() as usize > most {
                continue;
            }
            let parts: Vec<&CoreMode> = (0..n)
                .filter(|i| bits & (1 << i) != 0)
                .map(|i| &singles[i])
                .collect();
            out.push(combined_mode(&parts));
        }
        if matches!(head, ModeHead::UpTo(_)) {
            out.push(combined_mode(&[]));
        }
        if out.len() > 16 {
            return self.err("at most sixteen sets of modes");
        }
        Ok(out)
    }

    /// A spree spell's modes (CR 702.172a): every nonempty set of the
    /// printed modes, the set with bits `b` at index `b - 1`, each made of
    /// its modes' targets and effects in printed order (`split`, `split2`
    /// where the second and third begin).
    fn spree_modes(
        &mut self,
        modes: &[(mtg_core::mana::ManaCost, crate::ast::Mode)],
    ) -> Result<Vec<CoreMode>, CompileError> {
        let singles: Vec<crate::ast::Mode> = modes.iter().map(|(_, m)| m.clone()).collect();
        let singles = self.modes(&singles, false, false)?;
        let n = singles.len();
        let mut out = Vec::new();
        for bits in 1..(1usize << n) {
            let parts: Vec<&CoreMode> = (0..n)
                .filter(|i| bits & (1 << i) != 0)
                .map(|i| &singles[i])
                .collect();
            let mut m = CoreMode {
                targets: Vec::new(),
                effects: Vec::new(),
                text: String::new(),
                split: None,
                split2: None,
                more: Vec::new(),
                once: mtg_core::card::ModeOnce::Any,
                name: None,
            };
            for (k, p) in parts.iter().enumerate() {
                let at = (
                    u16::try_from(m.effects.len()).unwrap_or(u16::MAX),
                    u16::try_from(m.targets.len()).unwrap_or(u16::MAX),
                );
                match k {
                    0 => {}
                    1 => m.split = Some(at),
                    _ => m.split2 = Some(at),
                }
                m.targets.extend(p.targets.iter().copied());
                m.effects.extend(p.effects.iter().cloned());
            }
            m.text = parts
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join(" and ");
            out.push(m);
        }
        Ok(out)
    }

    /// "Choose up to five {P} worth of modes. You may choose the same mode
    /// more than once." (CR 700.2d, 700.2i): every nonempty combination of
    /// the modes, repeats allowed, whose pawprints total at most `limit`,
    /// each mode's instances in printed order. The combinations are listed
    /// in dictionary order of their modes' numbers (1, 1+1, 1+1+1, ...,
    /// 1+2, ..., 2, ...).
    fn pawprint_modes(
        &mut self,
        limit: u8,
        modes: &[(u8, crate::ast::Mode)],
    ) -> Result<Vec<CoreMode>, CompileError> {
        fn combos(
            weights: &[u8],
            from: usize,
            left: u8,
            cur: &mut Vec<usize>,
            out: &mut Vec<Vec<usize>>,
        ) {
            for i in from..weights.len() {
                if weights[i] > left {
                    continue;
                }
                cur.push(i);
                out.push(cur.clone());
                combos(weights, i, left - weights[i], cur, out);
                cur.pop();
            }
        }
        let singles: Vec<crate::ast::Mode> = modes.iter().map(|(_, m)| m.clone()).collect();
        let singles = self.modes(&singles, false, false)?;
        let weights: Vec<u8> = modes.iter().map(|(w, _)| *w).collect();
        let mut sets = Vec::new();
        combos(&weights, 0, limit, &mut Vec::new(), &mut sets);
        let mut out = Vec::new();
        for set in sets {
            let mut m = CoreMode {
                targets: Vec::new(),
                effects: Vec::new(),
                text: String::new(),
                split: None,
                split2: None,
                more: Vec::new(),
                once: mtg_core::card::ModeOnce::Any,
                name: None,
            };
            for (k, i) in set.iter().enumerate() {
                let p = &singles[*i];
                let at = (
                    u16::try_from(m.effects.len()).unwrap_or(u16::MAX),
                    u16::try_from(m.targets.len()).unwrap_or(u16::MAX),
                );
                match k {
                    0 => {}
                    1 => m.split = Some(at),
                    2 => m.split2 = Some(at),
                    _ => m.more.push(at),
                }
                m.targets.extend(p.targets.iter().copied());
                m.effects.extend(p.effects.iter().cloned());
            }
            m.text = set
                .iter()
                .map(|i| singles[*i].text.as_str())
                .collect::<Vec<_>>()
                .join(" and ");
            out.push(m);
        }
        Ok(out)
    }

    #[allow(clippy::too_many_lines)] // one arm per condition
    fn static_cond(&mut self, c: &StaticCond) -> StaticCondition {
        match c {
            StaticCond::CardsInGraveyard(n) => StaticCondition::CardsInGraveyard(*n),
            StaticCond::CardTypesInGraveyard(n) => StaticCondition::CardTypesInGraveyard(*n),
            StaticCond::YouControl { filter, at_least } => StaticCondition::YouControl {
                filter: self.filter(filter).who(Who::You),
                at_least: *at_least,
            },
            StaticCond::YouControlOneOrMore(filter) => StaticCondition::YouControl {
                filter: self.filter(filter).who(Who::You),
                at_least: 1,
            },
            StaticCond::YourTurn | StaticCond::DuringYourTurn => StaticCondition::YourTurn,
            StaticCond::NotYourTurn => StaticCondition::NotYourTurn,
            StaticCond::CastDuringMain | StaticCond::ControlledAsCast(_) => {
                let inner = match c {
                    StaticCond::ControlledAsCast(f) => StaticCondition::YouControl {
                        filter: self.filter(f).who(Who::You),
                        at_least: 1,
                    },
                    _ => StaticCondition::During(mtg_core::card::TurnTime::YourMain),
                };
                self.cast_snapshot = Some(inner.clone());
                StaticCondition::AsCast(Box::new(inner))
            }
            StaticCond::Attacking => StaticCondition::SourceAttacking,
            StaticCond::Enchanted => StaticCondition::SourceEnchanted,
            StaticCond::Equipped => StaticCondition::SourceEquipped,
            StaticCond::Monstrous => StaticCondition::SourceMonstrous,
            StaticCond::Suspected(b) => StaticCondition::SourceSuspected(*b),
            StaticCond::OpponentLifeAtMost(n) => StaticCondition::OpponentLifeAtMost(*n),
            StaticCond::MoreLifeThanOpponent => StaticCondition::MoreLifeThanOpponent,
            StaticCond::BasicLandTypes(n) => StaticCondition::BasicLandTypes(*n),
            StaticCond::YourLife { n, at_least } => StaticCondition::YourLife {
                n: *n,
                at_least: *at_least,
            },
            StaticCond::AnyPlayerLifeAtMost(n) => StaticCondition::AnyPlayerLifeAtMost(*n),
            StaticCond::OpponentPoisonAtLeast(n) => StaticCondition::OpponentPoisonAtLeast(*n),
            StaticCond::SourceCounters { kind, n, at_least } => StaticCondition::SourceCounters {
                kind: *kind,
                n: *n,
                at_least: *at_least,
            },
            StaticCond::SourceCountersThere { kind, n } => StaticCondition::SourceCounters {
                kind: *kind,
                n: *n,
                at_least: true,
            },
            StaticCond::DefenderControls { filter, at_least } => {
                StaticCondition::DefenderControls {
                    filter: self.filter(filter).who(Who::You),
                    at_least: *at_least,
                }
            }
            StaticCond::OnBattlefield { filter, at_least } => StaticCondition::OnBattlefield {
                filter: self.filter(filter),
                at_least: *at_least,
            },
            StaticCond::DefenderPoisoned => StaticCondition::DefenderPoisoned,
            StaticCond::OthersInCombat {
                filter,
                at_least,
                also,
                block,
            } => StaticCondition::OthersInCombat {
                filter: self.filter(filter),
                at_least: *at_least,
                also: *also,
                block: *block,
            },
            StaticCond::MoreCreaturesThanOpponent { attacking } => {
                StaticCondition::MoreCreaturesThanOpponent {
                    attacking: *attacking,
                }
            }
            StaticCond::YouControlNone(filter) | StaticCond::YouDontControl(filter) => {
                StaticCondition::YouControlNone(self.filter(filter).who(Who::You))
            }
            StaticCond::YouControlAtMost { filter, n } => StaticCondition::YouControlAtMost {
                filter: self.filter(filter).who(Who::You),
                n: *n,
            },
            StaticCond::YouControlEach(filters) => StaticCondition::YouControlEach(
                filters
                    .iter()
                    .map(|f| self.filter(f).who(Who::You))
                    .collect(),
            ),
            StaticCond::AttackedThisTurn => StaticCondition::AttackedThisTurn,
            StaticCond::UnlockedDoors(n) => StaticCondition::UnlockedDoors(*n),
            StaticCond::AttackedWith(f) => StaticCondition::AttackedWith(self.filter(f)),
            StaticCond::EnteredWith { filter, had } => StaticCondition::EnteredWith {
                filter: self.filter(filter),
                had: *had,
            },
            StaticCond::AttackedBattleThisTurn => StaticCondition::SourceAttackedBattle,
            StaticCond::AttackedLastTurn => StaticCondition::AttackedLastTurn,
            StaticCond::CreatureDiedThisTurn => StaticCondition::CreatureDiedThisTurn,
            StaticCond::OpponentDamagedThisTurn => StaticCondition::OpponentDamagedThisTurn,
            StaticCond::OpponentLostLifeThisTurn => StaticCondition::OpponentLostLifeThisTurn,
            StaticCond::PermanentLeftThisTurn => StaticCondition::PermanentLeftThisTurn,
            StaticCond::Escaped => StaticCondition::Escaped,
            StaticCond::Kicked => StaticCondition::Kicked,
            StaticCond::Bargained => StaticCondition::Bargained,
            StaticCond::Teamwork => StaticCondition::Teamwork,
            StaticCond::Beheld => StaticCondition::Beheld,
            StaticCond::NoManaSpent => StaticCondition::NoManaSpent,
            StaticCond::NotKicked => StaticCondition::NotKicked,
            StaticCond::Impending => StaticCondition::Impending,
            StaticCond::SneakPaid(p) => StaticCondition::SneakPaid(*p),
            StaticCond::ProwlPaid => StaticCondition::ProwlPaid,
            StaticCond::WebSlung(p) => StaticCondition::WebSlung(*p),
            StaticCond::EvidenceCollected | StaticCond::AdditionalCostPaid => {
                StaticCondition::OptionalCostPaid
            }
            StaticCond::Saddled => StaticCondition::Saddled,
            StaticCond::LandEnteredThisTurn => StaticCondition::LandEnteredThisTurn,
            StaticCond::ArtifactEnteredThisTurn => StaticCondition::ArtifactEnteredThisTurn,
            StaticCond::CardsDrawnThisTurn(n) => StaticCondition::CardsDrawnThisTurn(*n),
            StaticCond::CardsInHand { n, at_most } => StaticCondition::CardsInHand {
                n: *n,
                at_most: *at_most,
            },
            StaticCond::NoCardInHand(f) => StaticCondition::NoCardInHand(self.filter(f)),
            StaticCond::GraveyardCards {
                filter, at_least, ..
            } => StaticCondition::GraveyardCards {
                filter: self.filter(filter),
                at_least: *at_least,
            },
            StaticCond::Or(a, b) | StaticCond::OrIf(a, b) => {
                StaticCondition::Or(Box::new(self.static_cond(a)), Box::new(self.static_cond(b)))
            }
            StaticCond::And(a, b) => {
                StaticCondition::And(Box::new(self.static_cond(a)), Box::new(self.static_cond(b)))
            }
            StaticCond::CastBoth(both) => {
                let (a, b) = &**both;
                let cast = |c: &mut Self, f: &Filter| StaticCondition::CastThisTurn {
                    filter: c.filter(f),
                    at_least: 1,
                    other: false,
                };
                StaticCondition::And(Box::new(cast(self, a)), Box::new(cast(self, b)))
            }
            StaticCond::CreaturesDied { n, yours } => StaticCondition::CreaturesDied {
                n: *n,
                yours: *yours,
            },
            StaticCond::OpponentMoreLands => StaticCondition::OpponentMoreLands,
            StaticCond::OpponentHasMore(w) => StaticCondition::OpponentHasMore(*w),
            StaticCond::Void => StaticCondition::Void,
            StaticCond::NotYourMainPhase => StaticCondition::NotYourMainPhase,
            StaticCond::NotCastFromHand => {
                StaticCondition::NotCastFrom(mtg_core::state::Zone::Hand)
            }
            StaticCond::WasCast => StaticCondition::WasCast,
            StaticCond::PutAnyCounterThisTurn => StaticCondition::PutAnyCounterThisTurn,
            StaticCond::AttackedWithAtLeast(f, n) => {
                StaticCondition::AttackedWithAtLeast(self.filter(f), *n)
            }
            StaticCond::TotalToughness(n) => StaticCondition::TotalToughness(*n),
            StaticCond::OpponentGraveyard(n) => StaticCondition::OpponentGraveyard(*n),
            StaticCond::ManaValuesInGraveyard(n) => StaticCondition::ManaValuesInGraveyard(*n),
            StaticCond::OpponentControls { filter, at_least } => {
                StaticCondition::OpponentControls {
                    filter: self.filter(filter),
                    at_least: *at_least,
                }
            }
            StaticCond::Is(filter) => StaticCondition::SourceIs(self.filter(filter)),
            StaticCond::IsntToken => StaticCondition::SourceIs(self.filter(&Filter {
                status: mtg_core::card::StatusQuals {
                    token: Some(false),
                    ..Default::default()
                },
                ..Filter::default()
            })),
            StaticCond::Modified => StaticCondition::SourceModified,
            StaticCond::AttackingAlone => StaticCondition::SourceAttackingAlone,
            StaticCond::Blocked => StaticCondition::SourceBlocked,
            StaticCond::BeingBlocked => StaticCondition::SourceBeingBlocked,
            StaticCond::AttachedIs(filter) => StaticCondition::AttachedIs(self.filter(filter)),
            StaticCond::DrawnThisTurn(n) => StaticCondition::DrawnThisTurn(*n),
            StaticCond::CastThisTurn {
                filter,
                at_least,
                other,
            } => StaticCondition::CastThisTurn {
                filter: self.filter(filter),
                at_least: *at_least,
                other: *other,
            },
            StaticCond::CastFrom(z) => StaticCondition::CastFrom(*z),
            StaticCond::GainedLifeThisTurn { or_lost } => {
                StaticCondition::GainedLifeThisTurn { or_lost: *or_lost }
            }
            StaticCond::DescendedThisTurn => StaticCondition::DescendedThisTurn,
            StaticCond::SpellNumber(n) => StaticCondition::SpellNumber(*n),
            StaticCond::SurveilledThisTurn => StaticCondition::SurveilledThisTurn,
            StaticCond::CounterPutThisTurn => StaticCondition::CounterPutThisTurn,
            StaticCond::FaceDownEnteredThisTurn => StaticCondition::FaceDownEnteredThisTurn,
            StaticCond::CreatureEnteredThisTurn { other } => {
                StaticCondition::CreatureEnteredThisTurn { other: *other }
            }
            StaticCond::NonlandEnteredThisTurn(n) => StaticCondition::NonlandEnteredThisTurn(*n),
            StaticCond::PutIntoGraveyardThisTurn(t) => StaticCondition::PutIntoGraveyardThisTurn(
                mtg_core::card::TypeSet::of(t.iter().copied()),
            ),
            StaticCond::DiscardedThisTurn => StaticCondition::DiscardedThisTurn,
            StaticCond::Night => StaticCondition::Night,
            StaticCond::LifeGainedThisTurn(n) => StaticCondition::LifeGainedThisTurn(*n),
            StaticCond::CrimeThisTurn => StaticCondition::CrimeThisTurn,
            StaticCond::YourMainPhase => StaticCondition::YourMainPhase,
            StaticCond::YourCombatPhase => StaticCondition::YourCombatPhase,
            StaticCond::FirstCombatPhase => StaticCondition::FirstCombatPhase,
            StaticCond::PutCounterThisTurn => StaticCondition::PutCounterThisTurn,
            StaticCond::NthResolution { n, .. } => StaticCondition::NthResolution(*n),
            StaticCond::OpponentNoncombatDamagedThisTurn { .. } => {
                StaticCondition::OpponentNoncombatDamagedThisTurn
            }
            StaticCond::LeftGraveyardThisTurn => StaticCondition::LeftGraveyardThisTurn,
            StaticCond::CardToGraveyardThisTurn(t) => StaticCondition::CardToGraveyardThisTurn(
                mtg_core::card::TypeSet::of(t.iter().copied()),
            ),
            StaticCond::SacrificedThisTurn(t) => {
                StaticCondition::SacrificedThisTurn(mtg_core::card::TypeSet::of(t.iter().copied()))
            }
            StaticCond::MoreCardsInHand => StaticCondition::MoreCardsInHand,
            StaticCond::OpponentPoison(n) => StaticCondition::OpponentPoison(*n),
            StaticCond::OwnCardInExile => StaticCondition::OwnCardInExile,
            StaticCond::ExiledWith(n) => StaticCondition::ExiledWith(*n),
            StaticCond::TypesExiledWith(n) => StaticCondition::TypesExiledWith(*n),
            StaticCond::Monarch => StaticCondition::Monarch,
            StaticCond::CompletedDungeon => StaticCondition::CompletedDungeon,
            StaticCond::CityBlessing => StaticCondition::CityBlessing,
            StaticCond::EnduringStory => StaticCondition::EnduringStory,
            StaticCond::NoEnduringStory => StaticCondition::NoEnduringStory,
            StaticCond::MaxSpeed => StaticCondition::MaxSpeed,
            StaticCond::NoSpellsCastThisTurn { from_hand } => {
                StaticCondition::NoSpellsCastThisTurn {
                    from_hand: *from_hand,
                }
            }
            StaticCond::TotalPower(n) => StaticCondition::TotalPower(*n),
            StaticCond::DifferentPowers(n) => StaticCondition::DifferentPowers(*n),
            StaticCond::During(t) => StaticCondition::During(*t),
            StaticCond::ManaSpent {
                kind,
                at_least,
                that,
            } => StaticCondition::ManaSpent {
                kind: *kind,
                at_least: *at_least,
                that: *that,
            },
        }
    }

    fn count_expr(&mut self, c: &crate::ast::CountExpr) -> mtg_core::card::CountExpr {
        use crate::ast::CountExpr as A;
        use mtg_core::card::CountExpr as C;
        match c {
            A::Permanents(f) => C::Permanents(self.filter(f)),
            A::Domain => C::Domain,
            A::CardsInHand => C::CardsInHand,
            A::CardsInGraveyard(f) => C::CardsInGraveyard(f.as_ref().map(|f| self.filter(f))),
            A::SourcePower => C::SourcePower,
            A::TargetStat { n, toughness, .. } => C::TargetStat {
                n: target_index(self.tmap, *n),
                toughness: *toughness,
            },
            A::Life => C::Life,
            A::Times(n, inner) => C::Times(*n, Box::new(self.count_expr(inner))),
            A::Divided { by, up, inner } => C::Divided {
                by: *by,
                up: *up,
                inner: Box::new(self.count_expr(inner)),
            },
            A::CardsInGraveyards { filter, .. } => {
                C::CardsInAllGraveyards(filter.as_ref().map(|f| self.filter(f)))
            }
            A::Plus { n, inner, .. } => C::Plus(*n, Box::new(self.count_expr(inner))),
            A::ThisWay { verb, filter } => {
                C::ThisWay(*verb, filter.as_ref().map(|f| self.filter(f)))
            }
            A::LifeLostThisWay => C::LifeLostThisWay,
            A::Votes { index, .. } => C::Votes(*index),
            A::BlockingIt { beyond } => C::BlockingIt { beyond: *beyond },
            A::GreatestPower(f) => C::GreatestPower(self.filter(f)),
            A::DifferentlyNamed(f) => C::DifferentlyNamed(self.filter(f)),
            A::Party => {
                // CR 700.8: "up to one each of Cleric, Rogue, Warrior, and
                // Wizard".
                let role = |c: &mut Self, name: &str| {
                    mtg_core::card::ObjFilter::creatures()
                        .who(Who::You)
                        .subtype(c.pool.subtype(name))
                };
                C::Party(Box::new([
                    role(self, "Cleric"),
                    role(self, "Rogue"),
                    role(self, "Warrior"),
                    role(self, "Wizard"),
                ]))
            }
            A::GreatestManaValue => C::GreatestManaValue,
            A::CardsDrawnThisTurn => C::CardsDrawnThisTurn,
            A::SpellsCastThisTurn { other } => C::SpellsCastThisTurn { other: *other },
            A::OtherInstantSorceryCast { before } => C::OtherInstantSorceryCast { before: *before },
            A::SourceToughness => C::SourceToughness,
            A::TargetManaValue { n, .. } => C::TargetManaValue(target_index(self.tmap, *n)),
            A::CardManaValue(_) => C::CardManaValue,
            A::ColorsAmongPermanents => C::ColorsAmongPermanents,
            A::SpentColors(_) => C::SpentColors,
            A::TimesKicked => C::TimesKicked,
            A::SpentTreasure => C::SpentTreasure,
            A::CardsInOpponentsGraveyards(f) => {
                C::CardsInOpponentsGraveyards(f.as_ref().map(|f| self.filter(f)))
            }
            A::ExiledWith(f) => C::ExiledWith(f.as_ref().map(|f| self.filter(f))),
            A::CardsInExile { filter, graveyard } => C::CardsInExile {
                filter: filter.as_ref().map(|f| self.filter(f)),
                graveyard: *graveyard,
            },
            A::OpponentsAttacked => C::OpponentsAttacked,
            A::AttackedWithThisTurn => C::AttackedWithThisTurn,
            A::AndOr(inner) => self.count_expr(inner),
            A::DiedThisTurn(f) => C::DiedThisTurn(self.filter(f)),
            A::GreatestPowerInGraveyard(f) => C::GreatestPowerInGraveyard(self.filter(f)),
            A::TriggerStat { stat, .. } => C::TriggerStat(*stat),
            A::AttachedStat(stat) => C::AttachedStat(*stat),
            A::MovedStat { verb, stat, .. } => C::MovedStat {
                verb: *verb,
                stat: *stat,
            },
            A::TargetHand(n) => C::TargetHand(target_index(self.tmap, *n)),
            A::GreatestToughness(f) => C::GreatestToughness(self.filter(f)),
            A::Sum(a, b) => C::Sum(Box::new(self.count_expr(a)), Box::new(self.count_expr(b))),
            A::LifeLostThisTurn => C::LifeLostThisTurn,
            A::DamageThisWay { excess, .. } => C::DamageThisWay { excess: *excess },
            A::CardTypesInGraveyards => C::CardTypesInAllGraveyards,
            A::SourceCounters { kind, .. } => C::SourceCounters(*kind),
            A::TriggerCounters(kind) => C::TriggerCounters(*kind),
            A::Those(_) => C::ThoseObjects,
            A::HostGraveyard(f) => C::HostGraveyard(f.as_ref().map(|f| self.filter(f))),
            A::Devotion { colors, .. } => C::Devotion(ColorSet::of(colors.iter().copied())),
            A::LifeGainedThisTurn { .. } => C::LifeGainedThisTurn,
            A::CardTypesInGraveyard => C::CardTypesInGraveyard,
            A::SourceAllCounters { .. } => C::SourceAllCounters,
            A::SacrificedThisTurn => C::SacrificedThisTurn,
            A::TwoToX => C::TwoToX,
            A::AttackedThisTurn => C::AttackedThisTurn,
            A::OpponentsLostLife => C::OpponentsLostLife,
            A::CreaturesEnteredThisTurn => C::CreaturesEnteredThisTurn,
            A::DiscardedThisTurn => C::DiscardedThisTurn,

            A::OpponentsMoreCards => C::OpponentsMoreCards,
            A::Total(stat, f) => C::Total(*stat, self.filter(f)),
        }
    }

    fn target(&mut self, t: &Target) -> TargetSpec {
        match t {
            Target::Spec(s) => *s,
            Target::Filter(f) => {
                let f = self.filter(f);
                TargetSpec::Filtered(self.pool.intern_filter(f))
            }
            Target::CreatureOfSubtype(name) => {
                TargetSpec::CreatureWithSubtype(self.pool.subtype(name))
            }
            Target::Spell(f) => {
                let f = self.filter(f);
                TargetSpec::SpellFiltered(self.pool.intern_filter(f))
            }
            Target::SpellOr(f) => {
                let f = self.filter(f);
                TargetSpec::SpellOr(self.pool.intern_filter(f))
            }
            Target::GraveyardCard {
                filter,
                who,
                graveyards,
            } => {
                let f = self.filter(filter);
                TargetSpec::GraveyardCard {
                    filter: self.pool.intern_filter(f),
                    who: *who,
                    graveyards: *graveyards,
                }
            }
            Target::UpTo(t) => {
                let inner = self.target(t);
                self.pool.intern_up_to(inner)
            }
            Target::Many { target, min, max } => {
                let spec = self.target(target);
                self.pool.intern_many(mtg_core::card::ManyTargets {
                    spec,
                    min: *min,
                    max: *max,
                })
            }
        }
    }

    /// An ability's targets; its effects, compiled next, refer to a
    /// variable group as all its targets.
    fn targets(&mut self, ts: &[Target]) -> Vec<TargetSpec> {
        self.many = ts.iter().position(|t| matches!(t, Target::Many { .. }));
        // "Tap one or two target untapped creatures you control. They each
        // deal damage ... to target creature an opponent controls.": the
        // group goes last (CR 601.2c names no order among target words).
        self.tmap = TargetMap {
            group: self
                .many
                .filter(|m| m + 1 < ts.len())
                .map(|m| (m, ts.len())),
            shift: 0,
        };
        let mut specs: Vec<TargetSpec> = ts.iter().map(|t| self.target(t)).collect();
        if let Some((m, _)) = self.tmap.group {
            let g = specs.remove(m);
            specs.push(g);
        }
        specs
    }

    fn cost(&mut self, c: &Cost) -> CoreCost {
        CoreCost {
            craft: None,
            mana: c.mana.clone(),
            discard_self: c.discard_self,
            exile_self: c.exile_self,
            exile_this: c.exile_this,
            crew: c.crew,
            remove_counters: c.remove_counters,
            put_counters: c.put_counters,
            energy: c.energy,
            blight: c.blight,
            evidence: c.evidence,
            forage: c.forage,
            waterbend: c.waterbend,
            exert: c.exert,
            discard_hand: c.discard_hand,
            life_x: c.life_x,
            chosen: c.chosen,
            life: c.life,
            discard: c.discard,
            discard_x: c.discard_x,
            discard_random: c.discard_random,
            discard_filter: c.discard_filter.as_ref().map(|f| self.filter(f)),
            loyalty: c.loyalty.unwrap_or(0),
            tap: c.tap,
            sacrifice: match &c.sacrifice {
                None => None,
                Some(SacCost::This) => Some(CoreSacCost::This),
                Some(SacCost::Filter(f)) => Some(CoreSacCost::Filter(self.filter(f))),
            },
            count: c.count,
            exile_cards: c.exile_cards.as_ref().map(|g| {
                Box::new(mtg_core::card::GraveExile {
                    n: g.n,
                    x: g.x,
                    filter: g.filter.as_ref().map(|f| self.filter(f)),
                    top: g.top,
                })
            }),
            reveal_hand: c.reveal_hand,
            reveal_self: c.reveal_this,
            behold: c.behold.as_ref().map(|b| {
                Box::new(mtg_core::card::Behold {
                    filter: self.filter(&b.filter),
                    n: b.n,
                    exile: b.exile,
                })
            }),
        }
    }

    fn recipient(
        groups: &[u16],
        many: Option<usize>,
        tmap: TargetMap,
        r: Recipient,
    ) -> CoreRecipient {
        let ti = |i: usize| target_index(tmap, i);
        match r {
            Recipient::Group(i) => CoreRecipient::Group(groups[usize::from(i)]),
            Recipient::RandomOf(i) => CoreRecipient::RandomOf(groups[usize::from(i)]),
            Recipient::ExiledWith(i) => CoreRecipient::ExiledWith(groups[usize::from(i)]),
            Recipient::DamagedThisWay => CoreRecipient::DamagedThisWay,
            Recipient::OpponentsWithout(i) => {
                CoreRecipient::OpponentsWithout(groups[usize::from(i)])
            }
            Recipient::TargetAnd(t, i) => {
                CoreRecipient::TargetAndGroup(ti(t), groups[usize::from(i)])
            }
            // A variable group: its targets to the end of the list.
            Recipient::Target(i) if Some(i) == many => {
                CoreRecipient::Targets(u8::try_from(ti(i)).unwrap_or(u8::MAX), u8::MAX)
            }
            Recipient::Target(i)
            | Recipient::ThatTarget(i)
            | Recipient::TheTarget(i)
            | Recipient::ChosenTarget(i) => CoreRecipient::Target(ti(i)),
            Recipient::Targets(a, n)
            | Recipient::ThoseCards(a, n)
            | Recipient::ThoseTargets(a, n) => CoreRecipient::Targets(
                u8::try_from(ti(a)).unwrap_or(u8::MAX),
                u8::try_from(n).unwrap_or(u8::MAX),
            ),
            Recipient::You => CoreRecipient::You,
            Recipient::EachOpponent
            | Recipient::EachOtherPlayer
            | Recipient::EachOfYourOpponents
            | Recipient::AnOpponent => CoreRecipient::EachOpponent,
            Recipient::Source
            | Recipient::SourceNamed
            | Recipient::SourceCard
            | Recipient::SourceIt => CoreRecipient::Source,
            Recipient::TriggeringCard => CoreRecipient::TriggeringCard,
            Recipient::TriggeringController { .. } | Recipient::TheTriggeringController => {
                CoreRecipient::TriggeringController
            }
            Recipient::TriggeringObject
            | Recipient::TriggeringIt
            | Recipient::TheTriggeringObject => CoreRecipient::TriggeringObject,
            Recipient::TriggeringPlayer => CoreRecipient::TriggeringPlayer,
            Recipient::DefendingPlayer | Recipient::DefendingPlayerWord => {
                CoreRecipient::DefendingPlayer
            }
            Recipient::ControllerOf(i)
            | Recipient::ThatControllerOf(i)
            | Recipient::TheControllerOf(i) => CoreRecipient::ControllerOf(ti(i)),
            Recipient::OwnerOf(i) | Recipient::ThatOwner(i) => CoreRecipient::OwnerOf(ti(i)),
            Recipient::EachCreature => CoreRecipient::EachCreature,
            Recipient::EachPlayer => CoreRecipient::EachPlayer,
            Recipient::EachCreatureYouControl => CoreRecipient::EachCreatureYouControl,
            Recipient::EachPermanentYouControl => CoreRecipient::EachPermanentYouControl,
            Recipient::SameObject
            | Recipient::SameCard
            | Recipient::SameThat
            | Recipient::SameToken
            | Recipient::SameTokens
            | Recipient::SameThem => CoreRecipient::SameObject,
            Recipient::CreatedTokens | Recipient::CreatedThem | Recipient::CreatedThose => {
                CoreRecipient::CreatedTokens
            }
            Recipient::Manifested
            | Recipient::CreatedObject
            | Recipient::CreatedThat
            | Recipient::CreatedThatToken
            | Recipient::CreatedThatLand => CoreRecipient::CreatedObject,
            Recipient::Attached => CoreRecipient::Attached,
            Recipient::AttachedController => CoreRecipient::AttachedController,
            Recipient::BlockingIt => CoreRecipient::BlockingIt,
            Recipient::ExiledCard => CoreRecipient::ExiledCard,
            Recipient::Those(_) => CoreRecipient::ThoseObjects,
        }
    }

    fn sentences(&mut self, ss: &[Sentence]) -> Result<Vec<CoreEffect>, CompileError> {
        let mut out = Vec::new();
        // Where the previous sentence's effects start, and that sentence.
        let mut prev: Option<(usize, &Sentence)> = None;
        for s in ss {
            if let [Effect::Instead { cond, effects, .. }] = s.effects.as_slice() {
                // CR 608.2c: the sentence before happens unless the
                // condition holds; these happen instead.
                let Some((start, before)) = prev.take() else {
                    return self.err("an `instead` sentence needs a sentence before it");
                };
                let base = out.split_off(start);
                // A kicked spell's own targets replace the others (CR
                // 601.2c): numbered from 0 among the kicked spell's.
                let drops = matches!(
                    s.effects.as_slice(),
                    [Effect::Instead {
                        cond: crate::ast::InsteadCond::Kicked
                            | crate::ast::InsteadCond::Static(crate::ast::StaticCond::Teamwork),
                        ..
                    }]
                )
                .then_some(self.card.kicked_drops)
                .flatten();
                let outer = self.tmap.shift;
                if let Some(n) = drops {
                    self.tmap.shift = n;
                }
                let alt = self.sentences(&same_damage_recipients(effects, before));
                self.tmap.shift = outer;
                let mut alt = alt?;
                // "... becomes an artifact creature with base power and
                // toughness 4/3. If evidence was collected, it has base
                // power and toughness 1/1 until end of turn instead.": the
                // later text modifies only the P/T of the earlier
                // instruction (CR 608.2c), so it still becomes one.
                if let (
                    [CoreEffect::Animate { pt: Some(_), .. }],
                    [
                        CoreEffect::SetBasePt {
                            power, toughness, ..
                        },
                    ],
                ) = (base.as_slice(), alt.as_slice())
                {
                    let mut modified = base[0].clone();
                    if let CoreEffect::Animate { pt, .. } = &mut modified {
                        *pt = Some((*power, *toughness));
                    }
                    alt = vec![modified];
                }
                let cond = match cond {
                    InsteadCond::Kicked => EffectCond::Kicked,
                    InsteadCond::Static(c) => {
                        let c = self.static_cond(c);
                        EffectCond::Static(self.pool.intern_condition(c))
                    }
                    InsteadCond::Target { n, filter, .. } => EffectCond::Target {
                        n: u8::try_from(*n).unwrap_or(u8::MAX),
                        filter: Box::new(self.filter(filter)),
                    },
                    InsteadCond::ThisWay(c) => EffectCond::ThisWay(self.this_way_cond(c)),
                };
                out.push(CoreEffect::Instead { cond, base, alt });
                continue;
            }
            let start = out.len();
            for e in &s.effects {
                self.effect(e, &mut out)?;
            }
            // CR 611.2b: "... until your next turn", "... for as long as
            // this creature remains on the battlefield".
            if let Some(lasting) = s.lasting {
                let effects = out.split_off(start);
                out.push(CoreEffect::Lasting { lasting, effects });
            }
            prev = Some((start, s));
        }
        Ok(out)
    }

    fn this_way_cond(&mut self, c: &crate::ast::ThisWayCond) -> mtg_core::card::ThisWayCond {
        mtg_core::card::ThisWayCond {
            verb: c.verb,
            filter: c.filter.as_ref().map(|f| Box::new(self.filter(f))),
            negated: c.negated,
            by_you: c.wording == crate::ast::ThisWayWording::You,
        }
    }

    fn choice_kind(&mut self, k: &ChoiceKind) -> CoreChoiceKind {
        match k {
            ChoiceKind::Color { except } => CoreChoiceKind::Color { except: *except },
            ChoiceKind::Colors => CoreChoiceKind::Colors,
            ChoiceKind::CreatureType { except } => CoreChoiceKind::CreatureType {
                except: except.as_ref().map(|s| self.pool.subtype(s)),
            },
            ChoiceKind::LandType(lands) => {
                CoreChoiceKind::LandType(ColorSet::of(lands.iter().copied()))
            }
            ChoiceKind::Named(names) => {
                CoreChoiceKind::Named(u8::try_from(names.len()).unwrap_or(u8::MAX))
            }
            ChoiceKind::CardName { nonland } => CoreChoiceKind::CardName { nonland: *nonland },
        }
    }

    /// An effect; one on "all enchantments of the color of your choice"
    /// asks first (CR 608.2d), unless the resolution already chose.
    fn effect(&mut self, e: &Effect, out: &mut Vec<CoreEffect>) -> Result<(), CompileError> {
        let start = out.len();
        self.implied_used.set(None);
        self.effect_inner(e, out)?;
        // "Tap up to two target creatures. Put a stun counter on each of
        // them.": with no tokens created before it, "them" is what the
        // effect before acted on.
        if let Effect::AddCounters {
            to: Recipient::CreatedThem,
            ..
        } = e
            && !out[..start].iter().any(|c| {
                matches!(
                    c,
                    CoreEffect::CreateToken { .. }
                        | CoreEffect::CreateTokenCopy { .. }
                        | CoreEffect::Amass { .. }
                )
            })
        {
            // One instruction per target, as the tap was compiled.
            let mut before: Vec<CoreRecipient> = out[..start]
                .iter()
                .rev()
                .map_while(|c| match c {
                    CoreEffect::Tap { to } | CoreEffect::Pump { to, .. } => Some(*to),
                    _ => None,
                })
                .collect();
            before.reverse();
            if before.is_empty() {
                return self
                    .err("`each of them` needs tokens created, or a tap or pump, before it");
            }
            let put = out.split_off(start);
            for to in before {
                for c in &put {
                    let mut c = c.clone();
                    if let CoreEffect::AddCounters { to: t, .. } = &mut c {
                        *t = to;
                    }
                    out.push(c);
                }
            }
        }
        if let Some(k) = self.implied_used.take()
            && !out[..start]
                .iter()
                .any(|e| matches!(e, CoreEffect::Choose(_)))
        {
            out.insert(start, CoreEffect::Choose(k));
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn effect_inner(&mut self, e: &Effect, out: &mut Vec<CoreEffect>) -> Result<(), CompileError> {
        let groups = self.groups.clone();
        let many = self.many;
        let tmap = self.tmap;
        let implied = self.implied.clone();
        let used = std::rc::Rc::clone(&self.implied_used);
        let r = |x: Recipient| {
            if let Recipient::Group(i) = x
                && let Some(Some(k)) = implied.get(usize::from(i))
            {
                used.set(Some(*k));
            }
            Self::recipient(&groups, many, tmap, x)
        };
        match e {
            Effect::DealDamage { to, .. } if to.is_empty() => {
                return self.err("`deal N damage` without `to` only in an `instead` sentence");
            }
            // "it deals 1 damage to each opponent" about the triggering
            // creature: that creature is the source (CR 120.3).
            Effect::DealDamage {
                amount,
                to,
                by:
                    by @ (Recipient::TriggeringIt | Recipient::TriggeringObject | Recipient::Attached),
            } => {
                for t in to {
                    out.push(CoreEffect::DamageFrom {
                        source: r(*by),
                        amount: *amount,
                        to: r(*t),
                    });
                }
            }
            Effect::DealDamage { amount, to, .. } => {
                for t in to {
                    out.push(CoreEffect::DealDamage {
                        amount: *amount,
                        to: r(*t),
                    });
                }
            }
            Effect::Pump {
                power,
                toughness,
                to,
            } => out.push(CoreEffect::Pump {
                power: *power,
                toughness: *toughness,
                to: r(*to),
            }),
            Effect::Destroy { to, no_regen, .. } => {
                // One destruction of all of them (CR 608.2): a group
                // judged by what it's attached to ("all permanents
                // attached to creatures") goes first, before destroying the
                // hosts would unattach it.
                let attached_to_host = |t: &Recipient| {
                    matches!(t, Recipient::Group(i) if self.card.groups.get(usize::from(*i))
                        .is_some_and(|f| f.status.on.is_some()))
                };
                let mut to = to.clone();
                to.sort_by_key(|t| !attached_to_host(t));
                for t in &to {
                    out.push(if *no_regen {
                        CoreEffect::DestroyNoRegen { to: r(*t) }
                    } else {
                        CoreEffect::Destroy { to: r(*t) }
                    });
                }
            }
            Effect::GainLife { amount, who } => out.push(CoreEffect::GainLife {
                amount: *amount,
                who: r(*who),
            }),
            Effect::LoseLife { amount, who } => out.push(CoreEffect::LoseLife {
                amount: *amount,
                who: r(*who),
            }),
            Effect::LoseHalfLife { who } => out.push(CoreEffect::LoseHalfLife { who: r(*who) }),
            Effect::LoseGame { who } => out.push(CoreEffect::LoseGame { who: r(*who) }),
            Effect::DrawCards { count, who } => out.push(CoreEffect::DrawCards {
                count: *count,
                who: who.map_or(CoreRecipient::You, r),
            }),
            Effect::Mill { count, who } => out.push(CoreEffect::Mill {
                count: *count,
                who: who.map_or(CoreRecipient::You, r),
            }),
            Effect::AddCounters { kind, count, to } => out.push(CoreEffect::AddCounters {
                kind: *kind,
                count: *count,
                to: r(*to),
            }),
            // One instruction per kind, in printed order (CR 122.6: each is
            // its own event).
            Effect::AddCounterList { counters, to } => {
                for (kind, count) in counters {
                    out.push(CoreEffect::AddCounters {
                        kind: *kind,
                        count: *count,
                        to: r(*to),
                    });
                }
            }
            Effect::DealDamageDivided { amount, to } => {
                out.push(CoreEffect::DealDamageDivided {
                    amount: *amount,
                    to: r(*to),
                });
            }
            Effect::DistributeCounters { kind, count, to } => {
                out.push(CoreEffect::DistributeCounters {
                    kind: *kind,
                    count: *count,
                    to: r(*to),
                });
            }
            Effect::RemoveCounters { kind, count, to } => out.push(CoreEffect::RemoveCounters {
                kind: *kind,
                count: *count,
                to: r(*to),
            }),
            Effect::RemoveCounterChosen { to } => {
                out.push(CoreEffect::RemoveCounterChosen { to: r(*to) });
            }
            Effect::RemoveAllCounters { to } => {
                out.push(CoreEffect::RemoveAllCounters { to: r(*to) });
            }
            Effect::ReturnToHand { what, .. } => {
                for w in what {
                    out.push(CoreEffect::ReturnToHand { what: r(*w) });
                }
            }
            Effect::ReturnToBattlefield {
                what,
                counters,
                tapped,
                under_you,
                attached,
                put: _,
                attacking,
                additional: _,
            } => {
                out.push(CoreEffect::ReturnToBattlefield {
                    what: r(*what),
                    counters: *counters,
                    tapped: *tapped,
                    under_you: *under_you,
                    attached: attached.map(r),
                });
                if *attacking {
                    out.push(CoreEffect::EnteredAttack);
                }
            }
            Effect::Regenerate { to } => out.push(CoreEffect::Regenerate { to: r(*to) }),
            Effect::Exile { what } => {
                for w in what {
                    out.push(CoreEffect::Exile { what: r(*w) });
                }
            }
            Effect::GraveyardToBattlefield { filter } => {
                out.push(CoreEffect::GraveyardToBattlefield {
                    filter: self.filter(filter),
                });
            }
            Effect::CloakTop => out.push(CoreEffect::CloakTop {
                of: CoreRecipient::You,
            }),
            Effect::ExchangeControl { what } => match what[..] {
                [one] => out.push(CoreEffect::ExchangeControl { a: r(one), b: None }),
                [x, y] => out.push(CoreEffect::ExchangeControl {
                    a: r(x),
                    b: Some(r(y)),
                }),
                _ => return self.err("exchange control of two permanents"),
            },
            Effect::ExchangeLife { with } => {
                out.push(CoreEffect::ExchangeLife { with: r(*with) });
            }
            Effect::GiveControl { of, to } => out.push(CoreEffect::GiveControl {
                of: r(*of),
                to: r(*to),
            }),
            Effect::Airbend { what } => {
                for w in what {
                    out.push(CoreEffect::Airbend { what: r(*w) });
                }
            }
            Effect::ExileUntil { what } => out.push(CoreEffect::ExileLinked {
                what: r(*what),
                until: true,
            }),
            Effect::ExileAllGraveyards => out.push(CoreEffect::ExileAllGraveyards),
            Effect::ExileGraveyard { who } => out.push(CoreEffect::ExileGraveyard { who: r(*who) }),
            Effect::Sacrifice { what } | Effect::SacrificeBy { what, .. } => {
                out.push(CoreEffect::Sacrifice { what: r(*what) });
            }
            Effect::TapOrUntap { to } => out.push(CoreEffect::TapOrUntap { to: r(*to) }),
            Effect::NextSpellLess(n) => out.push(CoreEffect::NextSpellLess(*n)),
            Effect::ExtraLandPlays(n) => out.push(CoreEffect::ExtraLandPlays(*n)),
            Effect::FaceDownSpellsLess(n) => out.push(CoreEffect::FaceDownSpellsLess(*n)),
            Effect::Goad { what } => out.push(CoreEffect::Goad { what: r(*what) }),
            Effect::TurnFaceUp { what } => out.push(CoreEffect::TurnFaceUp { what: r(*what) }),
            Effect::TurnFaceDown { what } => out.push(CoreEffect::TurnFaceDown { what: r(*what) }),
            Effect::LookAtFaceDown { what } => {
                out.push(CoreEffect::LookAtFaceDown { what: r(*what) });
            }
            Effect::LookAtPlayer { who } => out.push(CoreEffect::LookAtPlayer { who: r(*who) }),
            // CR 701.35a: until the detaining player's next turn.
            Effect::Detain { what } => out.push(CoreEffect::UntilYourNextTurn {
                to: r(*what),
                parts: vec![Modification::AddKeywords(KeywordSet::DETAINED)],
            }),
            Effect::Tap { to } => {
                for t in to {
                    out.push(CoreEffect::Tap { to: r(*t) });
                }
            }
            Effect::Untap { to } => {
                for t in to {
                    out.push(CoreEffect::Untap { to: r(*t) });
                }
            }
            Effect::Grant { keywords, to, .. } => out.push(CoreEffect::Grant {
                keywords: KeywordSet::of(keywords.iter().copied()),
                to: r(*to),
            }),
            Effect::GainControl { of, forever } => out.push(CoreEffect::GainControl {
                of: r(*of),
                forever: *forever,
            }),
            Effect::Attach { what, to } => out.push(CoreEffect::Attach {
                what: r(*what),
                to: r(*to),
            }),
            Effect::Unattach { what } => out.push(CoreEffect::Unattach { what: r(*what) }),
            Effect::CreateToken {
                token,
                count,
                tapped,
                attacking,
                who,
                attached,
                sized,
                ..
            } => {
                let Some(token_ref) = self.pool.lookup(token) else {
                    return self.err(format!("unknown token \"{token}\""));
                };
                // CR 111.3: X fixed before the token exists.
                if let Some(n) = sized {
                    out.push(CoreEffect::TokenPtX(*n));
                }
                if attached.is_some() && self.pool.get(token_ref).enchant().is_none() {
                    return self.err(format!("token \"{token}\" created attached is not an Aura"));
                }
                out.push(CoreEffect::CreateToken {
                    token: token_ref,
                    count: *count,
                    who: who.map_or(CoreRecipient::You, r),
                    tapped: *tapped,
                    attached: attached.map(r),
                });
                // CR 508.4: "that's tapped and attacking".
                if *attacking {
                    out.push(CoreEffect::CreatedAttack);
                }
            }
            Effect::CreateTokenCopy { of, except, count } => {
                out.push(CoreEffect::CreateTokenCopy {
                    of: r(*of),
                    except: *except,
                    count: *count,
                });
            }
            Effect::CopySpells {
                what,
                times,
                new_targets,
                except,
            } => out.push(CoreEffect::CopySpells {
                what: r(*what),
                times: *times,
                new_targets: *new_targets,
                except: *except,
            }),
            Effect::BecomeCopy {
                of,
                except,
                who,
                until,
            } => out.push(CoreEffect::BecomeCopy {
                of: r(*of),
                except: *except,
                who: (!matches!(
                    who,
                    Recipient::Source | Recipient::SourceNamed | Recipient::SourceIt
                ))
                .then(|| r(*who)),
                until: *until,
            }),
            Effect::Counter { to, dest } => out.push(CoreEffect::Counter {
                to: r(*to),
                dest: *dest,
            }),
            Effect::CounterUnlessPays { to, mana, x, dest } => {
                out.push(CoreEffect::CounterUnlessPays {
                    to: r(*to),
                    mana: mana.clone(),
                    x: *x,
                    dest: *dest,
                });
            }
            Effect::Scry(n) => out.push(CoreEffect::Scry {
                count: *n,
                who: CoreRecipient::You,
            }),
            Effect::ScryEach { who, count, .. } => out.push(CoreEffect::Scry {
                count: *count,
                who: r(*who),
            }),
            Effect::AddMana(m) => out.push(CoreEffect::AddMana(m.clone())),
            Effect::AddManaOneOf(m) => out.push(CoreEffect::AddManaOneOf(m.clone())),
            Effect::AddManaTimes { mana, count } => out.push(CoreEffect::AddManaTimes {
                mana: *mana,
                count: *count,
            }),
            Effect::AddManaCombination {
                count,
                colors,
                different,
            } => out.push(CoreEffect::AddManaCombination {
                count: *count,
                colors: *colors,
                different: *different,
            }),
            Effect::AddManaLandCould { whose, basic } => out.push(CoreEffect::AddManaLandCould {
                whose: *whose,
                basic: *basic,
            }),
            Effect::PumpVar {
                power,
                toughness,
                times,
                to,
            } => out.push(CoreEffect::PumpVar {
                power: *power,
                toughness: *toughness,
                times: *times,
                to: r(*to),
            }),
            Effect::PutInLibrary { what, place, .. } => out.push(CoreEffect::PutInLibrary {
                what: r(*what),
                place: *place,
            }),
            Effect::SacrificeChosen {
                who,
                count,
                filter,
                and,
                ..
            } => {
                let filter = self.filter(filter);
                out.push(CoreEffect::SacrificeChosen {
                    who: r(*who),
                    filter,
                    count: *count,
                });
                // "an artifact and a land of their choice": the second
                // choice after the first (no permanent is both chosen).
                if let Some(b) = and {
                    let (count, filter) = &**b;
                    let filter = self.filter(filter);
                    out.push(CoreEffect::SacrificeChosen {
                        who: r(*who),
                        filter,
                        count: *count,
                    });
                }
            }
            Effect::CastFree {
                what,
                filter,
                max,
                exile,
            } => {
                let filter = filter.as_ref().map(|f| self.filter(f));
                out.push(CoreEffect::CastFree {
                    what: what.map(r),
                    filter,
                    max_mv: *max,
                    exile: *exile,
                });
            }
            Effect::IsNow { to, types } => out.push(CoreEffect::Animate {
                to: r(*to),
                types: TypeSet::of(types.iter().copied()),
                subtypes: self.subtypes(&[]),
                pt: None,
                colors: None,
                keywords: KeywordSet::NONE,
                indefinite: true,
                loses_abilities: false,
                in_addition: false,
            }),
            // Layer 6 alone: an animation that only removes abilities.
            Effect::LoseAllAbilities { to } => out.push(CoreEffect::Animate {
                to: r(*to),
                types: TypeSet::of([]),
                subtypes: self.subtypes(&[]),
                pt: None,
                colors: None,
                keywords: KeywordSet::NONE,
                indefinite: false,
                loses_abilities: true,
                in_addition: false,
            }),
            Effect::Strip { keywords, to } => out.push(CoreEffect::Strip {
                keywords: KeywordSet::of(keywords.iter().copied()),
                to: r(*to),
            }),
            Effect::RevealRandom { who } => out.push(CoreEffect::RevealRandom { who: r(*who) }),
            Effect::RevealHand { who } | Effect::LookAtHand { who } => {
                out.push(CoreEffect::RevealHand { who: r(*who) });
            }
            Effect::DiscardRandom { count, who, .. } => out.push(CoreEffect::DiscardRandom {
                count: *count,
                who: r(who.unwrap_or(Recipient::You)),
            }),
            Effect::RollDie { sides, rows } => {
                let Ok(sides) = u8::try_from(*sides) else {
                    return self.err("a die has at most 255 sides");
                };
                let mut core_rows = Vec::with_capacity(rows.len());
                for row in rows {
                    let byte = |n: u32| u8::try_from(n).unwrap_or(u8::MAX);
                    core_rows.push(mtg_core::card::DieRow {
                        lo: byte(row.lo),
                        hi: row.hi.map_or(u8::MAX, byte),
                        effects: self.sentences(&row.effects)?,
                    });
                }
                out.push(CoreEffect::RollDie {
                    sides,
                    rows: core_rows,
                });
            }
            // Wording: the target is the ability's, chosen as it is put on
            // the stack.
            Effect::ChooseTarget(_) => {}
            Effect::FlipCoin => out.push(CoreEffect::FlipCoin { won: Vec::new() }),
            Effect::Clash => out.push(CoreEffect::Clash { won: Vec::new() }),
            // What winning the clash just before does.
            Effect::IfWonClash(inner) => {
                let then = self.sentences(inner)?;
                match out.last_mut() {
                    Some(CoreEffect::Clash { won }) if won.is_empty() => *won = then,
                    _ => return self.err("`if you win` needs a `clash` before it"),
                }
            }
            Effect::Vote(words) => out.push(CoreEffect::Vote {
                options: u8::try_from(words.len()).unwrap_or(u8::MAX),
            }),
            Effect::VoteReturn(f) => out.push(CoreEffect::VoteReturn {
                filter: self.filter(f),
            }),
            // What winning the flip just before does.
            Effect::IfWonFlip(inner) => {
                let then = self.sentences(inner)?;
                match out.last_mut() {
                    Some(CoreEffect::FlipCoin { won }) if won.is_empty() => *won = then,
                    _ => return self.err("`if you win the flip` needs a `flip coin` before it"),
                }
            }
            Effect::ExileChosen {
                who,
                count,
                filter,
                graveyard,
            } => {
                out.push(CoreEffect::ExileChosen {
                    who: r(*who),
                    filter: self.filter(filter),
                    count: *count,
                });
                if *graveyard {
                    out.push(CoreEffect::ExileGraveyard { who: r(*who) });
                }
            }
            Effect::ExileFromHand {
                count,
                who,
                graveyard,
            } => out.push(CoreEffect::ExileFromHand {
                count: *count,
                who: r(*who),
                graveyard: *graveyard,
            }),
            Effect::DrawDiscardDrawn { draw, discard } => {
                out.push(CoreEffect::DrawDiscardDrawn {
                    draw: *draw,
                    discard: *discard,
                });
            }
            Effect::ExileFromGraveyardOrHand { who, filter } => {
                out.push(CoreEffect::ExileFromGraveyardOrHand {
                    who: r(*who),
                    filter: self.filter(filter),
                });
            }
            Effect::DiscardChosen {
                who,
                filter,
                may,
                exile,
                until,
                ..
            } => {
                let filter = filter.as_ref().map(|f| self.filter(f));
                out.push(CoreEffect::DiscardChosen {
                    who: r(*who),
                    filter,
                    optional: *may,
                    exile: exile.is_some(),
                    until: *until,
                });
            }
            Effect::Transform { what } => out.push(CoreEffect::Transform { what: r(*what) }),
            Effect::Search {
                count,
                up_to,
                to,
                graveyard,
                name,
                or_card,
                ..
            } if (*graveyard || name.is_some())
                && (*count != Num::N(1)
                    || *up_to
                    || *or_card != crate::ast::OrCard::No
                    || !matches!(
                        to,
                        mtg_core::card::SearchTo::Hand
                            | mtg_core::card::SearchTo::Battlefield { .. }
                    )) =>
            {
                return self.err(
                    "`search library or graveyard` and `card named \"...\"` find one card for a hand or the battlefield",
                );
            }
            Effect::Search {
                filter,
                count,
                to,
                reveal,
                who,
                may,
                graveyard,
                name,
                ..
            } => out.push(CoreEffect::Search {
                filter: self.filter(filter),
                count: *count,
                to: *to,
                who: who.map_or(CoreRecipient::You, r),
                may: *may,
                reveal: *reveal,
                graveyard: *graveyard,
                name: name.as_deref().map(Box::from),
            }),
            Effect::Investigate(_) | Effect::Investigates(_) | Effect::InvestigateCount { .. } => {
                let Some(token) = self.pool.lookup("Clue") else {
                    return self.err("unknown token \"Clue\" (cards/tokens/clue.card)");
                };
                let (n, who) = match e {
                    Effect::Investigates(w) => (Num::N(1), r(*w)),
                    Effect::Investigate(n) => (Num::N(*n), CoreRecipient::You),
                    Effect::InvestigateCount { count, .. } => (*count, CoreRecipient::You),
                    _ => unreachable!(),
                };
                out.push(CoreEffect::CreateToken {
                    token,
                    count: n,
                    who,
                    tapped: false,
                    attached: None,
                });
            }
            Effect::Surveil(n) => out.push(CoreEffect::Surveil { count: *n }),
            // CR 701.48a: with no Lesson outside the game, "discard a card
            // to draw a card", which the player may do.
            Effect::Learn => {
                let discard = Sentence::new(vec![Effect::Discard {
                    count: Num::N(1),
                    who: None,
                }]);
                let draw = Sentence::new(vec![Effect::DrawCards {
                    count: Num::N(1),
                    who: None,
                }]);
                out.push(CoreEffect::YouMay(self.sentences(&[discard])?));
                out.push(CoreEffect::IfYouDo(self.sentences(&[draw])?));
            }
            Effect::DoublePt { to, toughness, .. } => out.push(CoreEffect::DoublePt {
                to: r(*to),
                toughness: *toughness,
            }),
            Effect::DoubleCounters { kind, to } => out.push(CoreEffect::DoubleCounters {
                kind: *kind,
                to: r(*to),
            }),
            Effect::DoubleLife { who } => out.push(CoreEffect::DoubleLife { who: r(*who) }),
            Effect::Discard { count, who } => out.push(CoreEffect::Discard {
                count: *count,
                who: who.map_or(CoreRecipient::You, r),
            }),
            Effect::HandToLibrary {
                count,
                who,
                bottom,
                shuffle,
            } => {
                out.push(CoreEffect::HandToLibrary {
                    count: *count,
                    who: who.map_or(CoreRecipient::You, r),
                    bottom: *bottom,
                    shuffle: *shuffle,
                });
            }
            Effect::ShuffleZone {
                who,
                of,
                hand,
                both,
                filter,
                ..
            } => {
                let filter = filter.as_ref().map(|f| self.filter(f));
                out.push(CoreEffect::ShuffleIntoLibrary {
                    who: who.or(*of).map_or(CoreRecipient::You, r),
                    hand: *hand,
                    both: *both,
                    filter,
                });
            }
            Effect::Fight { a, b } => out.push(CoreEffect::Fight { a: r(*a), b: r(*b) }),
            Effect::Bite { source, to, twice } => out.push(CoreEffect::Bite {
                source: r(*source),
                to: r(*to),
                twice: *twice,
            }),
            Effect::CreateEmblem { emblem } => {
                let Some(emblem_ref) = self.pool.lookup(emblem) else {
                    return self.err(format!("unknown emblem \"{emblem}\""));
                };
                out.push(CoreEffect::CreateEmblem { emblem: emblem_ref });
            }
            Effect::Animate {
                to,
                types,
                subtypes,
                pt,
                base: _,
                colors,
                keywords,
                indefinite,
                loses_abilities,
                in_addition,
            } => {
                let subtypes = self.subtypes(subtypes);
                out.push(CoreEffect::Animate {
                    to: r(*to),
                    types: TypeSet::of(types.iter().copied()),
                    subtypes,
                    pt: *pt,
                    colors: (!colors.is_empty()).then(|| ColorSet::of(colors.iter().copied())),
                    keywords: KeywordSet::of(keywords.iter().copied()),
                    indefinite: *indefinite,
                    loses_abilities: *loses_abilities,
                    in_addition: *in_addition,
                });
            }
            Effect::BecomeColor {
                to,
                colors,
                indefinite,
            } => out.push(CoreEffect::BecomeColor {
                to: r(*to),
                colors: ColorSet::of(colors.iter().copied()),
                indefinite: *indefinite,
            }),
            Effect::BecomeLandType {
                to,
                land,
                indefinite,
            } => out.push(CoreEffect::BecomeLandType {
                to: r(*to),
                land: *land,
                indefinite: *indefinite,
            }),
            Effect::Choose(kind) => {
                let kind = self.choice_kind(kind);
                out.push(CoreEffect::Choose(kind));
            }
            Effect::BecomeChosen { to } => out.push(CoreEffect::BecomeChosen { to: r(*to) }),
            Effect::ProtectionFromChosen { to } => {
                out.push(CoreEffect::ProtectionFromChosen { to: r(*to) });
            }
            Effect::OfChoice {
                to,
                kind,
                protection,
                in_addition,
            } => {
                let kind = self.choice_kind(kind);
                out.push(CoreEffect::OfChoice {
                    to: r(*to),
                    kind,
                    protection: *protection,
                    in_addition: *in_addition,
                });
            }
            Effect::YouMay(inner) => {
                let effects = self.sentences(inner)?;
                out.push(CoreEffect::YouMay(effects));
            }
            Effect::PlayerMay { who, inner } => {
                let effects = self.sentences(inner)?;
                out.push(CoreEffect::PlayerMay {
                    who: r(*who),
                    effects,
                });
            }
            Effect::IfTarget(n, f, inner, _) => {
                let then = self.sentences(inner)?;
                out.push(CoreEffect::IfTarget {
                    n: u8::try_from(*n).unwrap_or(u8::MAX),
                    filter: Box::new(self.filter(f)),
                    then,
                });
            }
            Effect::KeepMana(inner) => {
                let effects = self.sentences(inner)?;
                out.push(CoreEffect::KeepMana(effects));
            }
            Effect::IfKicked(inner) => {
                let effects = self.sentences(inner)?;
                out.push(CoreEffect::IfKicked(effects));
            }
            // "unless": nothing if the condition holds, otherwise the
            // effects.
            Effect::IfCondition(cond, inner, crate::ast::IfWording::Unless) => {
                let effects = self.sentences(inner)?;
                let c = self.static_cond(cond);
                let i = self.pool.intern_condition(c);
                out.push(CoreEffect::IfCondition(i, Vec::new()));
                out.push(CoreEffect::Otherwise(effects));
            }
            Effect::EachPutsFromGraveyard { who, filter } => {
                out.push(CoreEffect::EachPutsFromGraveyard {
                    who: r(*who),
                    filter: self.filter(filter),
                });
            }
            Effect::ForEachCouldnt { inner, .. } => {
                let effects = self.sentences(inner)?;
                out.push(CoreEffect::ForEachCouldnt(effects));
            }
            Effect::IfCondition(cond, inner, _) => {
                let effects = self.sentences(inner)?;
                let c = self.static_cond(cond);
                let i = self.pool.intern_condition(c);
                out.push(CoreEffect::IfCondition(i, effects));
            }
            Effect::WhenYouDo { targets, effects } => {
                let effects = self.sentences(effects)?;
                let targets = self.targets(targets);
                let id = self
                    .pool
                    .register_ability(AbilityDef::Triggered(TriggeredAbility {
                        limit: mtg_core::card::TurnLimit::None,
                        from_graveyard: false,
                        trigger: TriggerSpec::Reflexive,
                        condition: None,
                        targets,
                        effects,
                    }));
                out.push(CoreEffect::Reflexive(id));
            }
            Effect::IfYouDo(inner) => {
                let effects = self.sentences(inner)?;
                out.push(CoreEffect::IfYouDo(effects));
            }
            Effect::Otherwise(inner) => {
                if !matches!(
                    out.last(),
                    // "You may tap two untapped creatures ... . If you do,
                    // draw a card. Otherwise, sacrifice this enchantment."
                    // (CR 608.2c): the optional action wasn't taken.
                    Some(
                        CoreEffect::IfCondition(..)
                            | CoreEffect::Clash { .. }
                            | CoreEffect::IfCard { .. }
                            | CoreEffect::IfYouDo(..)
                    )
                ) {
                    return self.err("`otherwise` needs a conditional sentence before it");
                }
                let effects = self.sentences(inner)?;
                out.push(CoreEffect::Otherwise(effects));
            }
            Effect::Instead { .. } => {
                return self.err("`instead` is a sentence of its own, after the one it replaces");
            }
            Effect::Waterbend(cost) => out.push(CoreEffect::Waterbend(cost.clone())),
            Effect::Pay { mana, life } => out.push(CoreEffect::Pay {
                mana: mana.clone(),
                life: *life,
            }),
            Effect::UnlessYou { act, effect } => {
                let mut acts = Vec::new();
                self.effect(act, &mut acts)?;
                let mut effects = Vec::new();
                self.effect(effect, &mut effects)?;
                out.push(CoreEffect::UnlessYou { act: acts, effects });
            }
            Effect::UnlessSacrifices {
                payer,
                filter,
                or_discard,
                effect,
            } => {
                let mut effects = Vec::new();
                self.effect(effect, &mut effects)?;
                out.push(CoreEffect::UnlessSacrifices {
                    payer: r(*payer),
                    filter: self.filter(filter),
                    or_discard: *or_discard,
                    effects,
                });
            }
            Effect::UnlessPays {
                payer,
                mana,
                life,
                discard,
                waterbend,
                discard_random,
                effect,
            } => {
                let mut effects = Vec::new();
                self.effect(effect, &mut effects)?;
                out.push(CoreEffect::UnlessPays {
                    payer: r(*payer),
                    mana: mana.clone(),
                    life: *life,
                    discard: *discard,
                    waterbend: *waterbend,
                    discard_random: *discard_random,
                    effects,
                });
            }
            Effect::SetBasePtVar { to, n } => {
                out.push(CoreEffect::SetBasePtVar { to: r(*to), n: *n });
            }
            Effect::SetBasePt {
                to,
                power,
                toughness,
            } => out.push(CoreEffect::SetBasePt {
                to: r(*to),
                power: *power,
                toughness: *toughness,
            }),
            Effect::Delayed {
                about,
                trigger,
                effects,
                this_turn,
                ..
            } => {
                // CR 603.7c: a delayed trigger triggers only once unless it
                // has a duration. One printed "Whenever ..." without "this
                // turn" or "until end of turn" would read as recurring and
                // fire once.
                if !*this_turn
                    && !matches!(trigger, Trigger::Next(_))
                    && !crate::format::natural_when(trigger)
                {
                    return self.err(
                        "a delayed `whenever` trigger needs a duration (`... this turn`, `until end of turn, ...`)",
                    );
                }
                // "Exile those tokens when this leaves the battlefield"
                // about the tokens: the trigger would watch the tokens
                // (a delayed trigger's `this` is what it is about) while
                // printing the source. Only "... this turn" names the
                // object it is about ("When that creature dies this
                // turn").
                let this_obj = |t: &Trigger| {
                    matches!(
                        t,
                        Trigger::Leaves(TriggerObj::This) | Trigger::Dies(TriggerObj::This)
                    )
                };
                if !*this_turn
                    && !matches!(about, Recipient::Source)
                    && (this_obj(trigger) || matches!(trigger, Trigger::When(t) if this_obj(t)))
                {
                    return self.err(
                        "a delayed trigger about another object can't watch `this` leave or die",
                    );
                }
                let effects = self.sentences(effects)?;
                // A delayed triggered mana ability ("whenever a land is
                // tapped for mana this turn, add ...", CR 605.1b) has no
                // form here.
                if matches!(trigger, Trigger::TappedForMana(_))
                    || matches!(trigger, Trigger::When(t) | Trigger::Next(t)
                        if matches!(**t, Trigger::TappedForMana(_)))
                {
                    return self.err("a delayed trigger can't be a triggered mana ability");
                }
                let trigger = self.trigger(trigger);
                let id = self
                    .pool
                    .register_ability(AbilityDef::Triggered(TriggeredAbility {
                        limit: mtg_core::card::TurnLimit::None,
                        from_graveyard: false,
                        trigger,
                        condition: None,
                        targets: Vec::new(),
                        effects,
                    }));
                out.push(CoreEffect::Delayed {
                    ability: id,
                    about: r(*about),
                    this_turn: *this_turn,
                });
            }
            Effect::CreateReplacement(rep) => {
                let compiled = self.replacement(rep)?;
                out.push(CoreEffect::CreateReplacement(compiled));
            }
            Effect::Rule(k) => out.push(CoreEffect::Rule(*k)),
            Effect::DamageUnpreventable => out.push(CoreEffect::DamageUnpreventable),
            Effect::ExcessToController => out.push(CoreEffect::ExcessToController),
            Effect::PlayerRule { who, kind } => out.push(CoreEffect::PlayerRule {
                who: r(*who),
                kind: *kind,
            }),
            Effect::Proliferate => out.push(CoreEffect::Proliferate),
            Effect::RingTempts => out.push(CoreEffect::RingTempts),
            Effect::BecomeMonarch => out.push(CoreEffect::BecomeMonarch {
                who: CoreRecipient::You,
            }),
            Effect::TakeInitiative => out.push(CoreEffect::TakeInitiative {
                who: CoreRecipient::You,
            }),
            Effect::Venture { undercity } => out.push(CoreEffect::Venture {
                undercity: *undercity,
            }),
            Effect::Explore { what, twice } => {
                out.push(CoreEffect::Explore { what: r(*what) });
                if *twice {
                    out.push(CoreEffect::Explore { what: r(*what) });
                }
            }
            Effect::PhaseOut { what } => out.push(CoreEffect::PhaseOut { what: r(*what) }),
            Effect::Endure { what, count } => {
                // CR 701.63a: an N/N white Spirit; X/X is a 0/0 made that
                // size as it resolves.
                let n = match count {
                    Num::N(n) => *n,
                    _ => 0,
                };
                let key = format!("{n}/{n} white Spirit creature");
                let Some(token) = self.pool.lookup(&key) else {
                    return self.err(format!("unknown token \"{key}\""));
                };
                out.push(CoreEffect::Endure {
                    what: r(*what),
                    count: *count,
                    token,
                });
            }
            Effect::Amass { kind, count } => {
                // "Amass Zombies 2": a Zombie Army token.
                let key = format!("0/0 black {} Army creature", kind.trim_end_matches('s'));
                let Some(token) = self.pool.lookup(&key) else {
                    return self.err(format!("unknown token \"{key}\""));
                };
                out.push(CoreEffect::Amass {
                    count: *count,
                    token,
                });
            }
            Effect::CreatedAttack => out.push(CoreEffect::CreatedAttack),
            Effect::EmpowerJace { count } => {
                let Some(token) = self.pool.lookup(JACE_TOKEN) else {
                    return self.err(format!("unknown token \"{JACE_TOKEN}\""));
                };
                out.push(CoreEffect::EmpowerJace {
                    count: *count,
                    token,
                });
            }
            Effect::ClearEcho => out.push(CoreEffect::ClearEcho),
            Effect::Manifest => out.push(CoreEffect::Manifest),
            Effect::ManifestDread => out.push(CoreEffect::ManifestDread),
            Effect::Blight(n) => out.push(CoreEffect::Blight(*n)),
            Effect::CollectEvidence(n) => out.push(CoreEffect::CollectEvidence(*n)),
            Effect::Forage => out.push(CoreEffect::Forage),
            Effect::Adapt(n) => out.push(CoreEffect::Adapt(*n)),
            Effect::Bolster(n) => out.push(CoreEffect::Bolster(*n)),
            Effect::Monstrosity(n) => out.push(CoreEffect::Monstrosity(*n)),
            Effect::Suspect { what, on } => out.push(CoreEffect::Suspect {
                what: r(*what),
                on: *on,
            }),
            Effect::BlightBy { who, n } => out.push(CoreEffect::BlightBy {
                who: r(*who),
                n: *n,
            }),
            Effect::Firebend(n) => out.push(CoreEffect::Firebend(*n)),
            Effect::Earthbend { count, to } => out.push(CoreEffect::Earthbend {
                count: *count,
                to: r(*to),
            }),
            Effect::Populate => out.push(CoreEffect::Populate),
            Effect::EndTurn => out.push(CoreEffect::EndTurn),
            // CR 701.41a: one counter on each target.
            Effect::Support(to) => {
                for t in to {
                    out.push(CoreEffect::AddCounters {
                        kind: mtg_core::card::CounterKind::PlusOnePlusOne,
                        count: Num::N(1),
                        to: r(*t),
                    });
                }
            }
            Effect::Recruit { .. } => {
                let Some(token) = self.pool.lookup(HUMAN_SOLDIER) else {
                    return self.err(format!("unknown token \"{HUMAN_SOLDIER}\""));
                };
                out.push(CoreEffect::Recruit { token });
            }
            Effect::Incubate(count) => {
                let Some(token) = self.pool.lookup(INCUBATOR) else {
                    return self.err(format!("unknown token \"{INCUBATOR}\""));
                };
                out.push(CoreEffect::Incubate {
                    count: *count,
                    token,
                });
            }
            Effect::Discover(n) => out.push(CoreEffect::Discover(*n)),
            Effect::Soulbond { triggering } => {
                out.push(CoreEffect::Soulbond {
                    triggering: *triggering,
                });
            }
            Effect::MoveSourceCounters { to } => {
                out.push(CoreEffect::MoveSourceCounters { to: r(*to) });
            }
            Effect::CumulativeUpkeep(c) => out.push(CoreEffect::CumulativeUpkeep(c.clone())),
            Effect::Connive { what } => out.push(CoreEffect::Connive { what: r(*what) }),
            Effect::BecomePrepared { what } => {
                out.push(CoreEffect::BecomePrepared { what: r(*what) });
            }
            Effect::LookAtTop { count, of, by, .. } => out.push(CoreEffect::LookAtTop {
                count: *count,
                who: r(of.unwrap_or(Recipient::You)),
                looker: r(by.unwrap_or(Recipient::You)),
            }),
            Effect::RevealTop { who, look, .. } => out.push(CoreEffect::RevealTop {
                who: r(who.unwrap_or(Recipient::You)),
                look: *look,
            }),
            Effect::Shuffle { who } => out.push(CoreEffect::ShuffleLibrary { who: r(*who) }),
            Effect::MoveCounters {
                from,
                to,
                kind,
                count,
            } => out.push(CoreEffect::MoveCounters {
                from: r(*from),
                to: r(*to),
                kind: *kind,
                count: *count,
            }),
            Effect::ExileUntilCard { who, filter } => out.push(CoreEffect::ExileUntilCard {
                who: who.map_or(CoreRecipient::You, r),
                filter: self.filter(filter),
            }),
            Effect::MayPlayCard {
                until, cast, free, ..
            } => out.push(CoreEffect::MayPlayCard {
                until: *until,
                cast: *cast,
                free: *free,
            }),
            Effect::RevealUntil { who, filter } => out.push(CoreEffect::RevealUntil {
                who: r(who.unwrap_or(Recipient::You)),
                filter: self.filter(filter),
            }),
            Effect::IfCard {
                filter,
                negated,
                shares_type,
                then,
                ..
            } => {
                let then = self.sentences(then)?;
                out.push(CoreEffect::IfCard {
                    filter: self.filter(filter),
                    negated: *negated,
                    shares_type: *shares_type,
                    then,
                });
            }
            Effect::RevealIt => out.push(CoreEffect::RevealCard),
            // "If you don't, ..." / "If you can't, ...": the optional or
            // chosen action before it set whether it happened (CR 608.2c).
            Effect::IfYouDont { then, .. } => {
                let effects = self.sentences(then)?;
                out.push(CoreEffect::Otherwise(effects));
            }
            Effect::IfYouDontPut { then, .. } => {
                if !matches!(out.last(), Some(CoreEffect::IfCard { .. })) {
                    return self.err("`if you dont put it` needs `if it is ...` before it");
                }
                let effects = self.sentences(then)?;
                out.push(CoreEffect::Otherwise(effects));
            }
            Effect::DrawReveal => out.push(CoreEffect::DrawReveal),
            Effect::DiscardIt => out.push(CoreEffect::PutCard {
                what: mtg_core::card::CardSet::It,
                to: mtg_core::card::CardDest::Graveyard,
                random: false,
            }),
            Effect::PutCard {
                what,
                to,
                random,
                rest,
                ..
            } => {
                use crate::ast::CardWords;
                use mtg_core::card::CardSet;
                out.push(CoreEffect::PutCard {
                    what: match what {
                        CardWords::It | CardWords::ThatCard => CardSet::It,
                        CardWords::AllRevealed | CardWords::Those | CardWords::TheRevealed => {
                            CardSet::Revealed
                        }
                        CardWords::OtherRevealed => CardSet::OtherRevealed,
                    },
                    to: *to,
                    random: *random,
                });
                // "... and the rest on the bottom of your library in a
                // random order": the other revealed cards.
                if let Some((to, random)) = rest {
                    out.push(CoreEffect::PutCard {
                        what: CardSet::OtherRevealed,
                        to: *to,
                        random: *random,
                    });
                }
            }
            Effect::ExileTop { count, who } => out.push(CoreEffect::ExileTop {
                who: who.map_or(CoreRecipient::You, r),
                count: *count,
            }),
            Effect::ExilePlayable {
                count,
                until,
                choose,
                from,
                ..
            } => out.push(CoreEffect::ExilePlayable {
                count: *count,
                until: *until,
                choose: *choose,
                from: from.map(r),
            }),
            Effect::Dig {
                count,
                filter,
                optional,
                all,
                rest_graveyard,
                take,
                also,
                random,
                top,
                onto,
                ..
            } => out.push(CoreEffect::Dig {
                count: *count,
                filter: filter.as_ref().map(|f| self.filter(f)),
                optional: *optional,
                all: *all,
                rest_graveyard: *rest_graveyard,
                take: *take,
                also: also.as_ref().map(|f| self.filter(f)),
                random: *random,
                top: *top,
                onto: *onto,
            }),
            Effect::Modes(modes) => {
                let modes = self.modes(modes, false, false)?;
                out.push(CoreEffect::Modes(modes));
            }
            // "Choose two —": the pairs of modes are the modes.
            Effect::ModesTwo(modes) => {
                let modes = self.modes(modes, false, true)?;
                out.push(CoreEffect::Modes(modes));
            }
            Effect::ModesOf(head, modes) => {
                let modes = self.head_modes(*head, modes)?;
                out.push(CoreEffect::Modes(modes));
            }
            Effect::SwitchPt { to } => out.push(CoreEffect::SwitchPt { to: r(*to) }),
            Effect::GrantFlashback { to, harmonize, .. } => {
                out.push(CoreEffect::GrantFlashback {
                    to: r(*to),
                    harmonize: *harmonize,
                });
            }
            // One or the other, chosen as it resolves (CR 608.2d).
            Effect::ProtectionOr { to, filter } => {
                let mut list = Vec::new();
                self.effect(
                    &Effect::GrantAbility {
                        ability: Box::new(Ability::Protection(filter.clone())),
                        to: *to,
                    },
                    &mut list,
                )?;
                list.push(CoreEffect::OfChoice {
                    to: r(*to),
                    kind: mtg_core::card::ChoiceKind::Color { except: None },
                    protection: true,
                    in_addition: false,
                });
                out.push(CoreEffect::OneOf(list));
            }
            Effect::CreatureTypes { to, all } => out.push(CoreEffect::CreatureTypes {
                to: r(*to),
                all: *all,
            }),
            Effect::SkipNext { who, what } => out.push(CoreEffect::SkipNext {
                who: r(*who),
                what: *what,
            }),
            Effect::SkipNextUntap { to, .. } => out.push(CoreEffect::SkipNextUntap { to: r(*to) }),
            Effect::ExtraTurns { who, n } => out.push(CoreEffect::ExtraTurns {
                who: r(*who),
                n: *n,
            }),
            Effect::AdditionalPhases { combats, main, .. } => {
                out.push(CoreEffect::AdditionalPhases {
                    combats: *combats,
                    main: *main,
                });
            }
            Effect::CantBeRegenerated { to } => {
                out.push(CoreEffect::CantBeRegenerated { to: r(*to) });
            }
            Effect::AssignsNoCombatDamage { to } => {
                out.push(CoreEffect::AssignsNoCombatDamage { to: r(*to) });
            }
            Effect::BlocksIfAble { blocker, attacker } => out.push(CoreEffect::BlocksIfAble {
                blocker: r(*blocker),
                attacker: r(*attacker),
            }),
            Effect::CantBlockThis { blocker, attacker } => out.push(CoreEffect::CantBlockThis {
                blocker: r(*blocker),
                attacker: r(*attacker),
            }),
            Effect::ReturnChosen { count, filter, who } => out.push(CoreEffect::ReturnChosen {
                who: who.map_or(CoreRecipient::You, r),
                filter: self.filter(filter).who(Who::You),
                count: *count,
            }),
            Effect::GrantAbility { ability, to } => {
                let def = self.ability(ability)?;
                let id = self.pool.register_ability(def);
                out.push(CoreEffect::GrantAbility {
                    ability: id,
                    to: r(*to),
                });
            }
            // "a creature card and/or a land card": a choice of each in
            // turn, either declined.
            Effect::PutFromHand {
                filter,
                tapped,
                also,
                attacking,
            } => {
                for f in std::iter::once(filter).chain(also) {
                    out.push(CoreEffect::PutFromHand {
                        filter: self.filter(f),
                        tapped: *tapped,
                    });
                    if *attacking {
                        out.push(CoreEffect::EnteredAttack);
                    }
                }
            }
            Effect::IfThisWay(c, inner) => {
                let then = self.sentences(inner)?;
                let cond = self.this_way_cond(c);
                out.push(CoreEffect::IfThisWay { cond, then });
            }
            Effect::IfExcessDamage(inner, _) => {
                let then = self.sentences(inner)?;
                out.push(CoreEffect::IfExcessDamage(then));
            }
            Effect::ChooseAct {
                act,
                count,
                up_to,
                filter,
                ..
            } => out.push(CoreEffect::ChooseAct {
                act: *act,
                filter: self.filter(filter),
                count: *count,
                up_to: *up_to,
            }),
            Effect::OneOf(options) => {
                let mut list = Vec::new();
                for o in options {
                    let mut one = Vec::new();
                    self.effect(o, &mut one)?;
                    if one.len() != 1 {
                        return self.err("each alternative of `or` must be one effect");
                    }
                    list.extend(one);
                }
                out.push(CoreEffect::OneOf(list));
            }
            Effect::GrantChoice { to, keywords } => out.push(CoreEffect::OneOf(
                keywords
                    .iter()
                    .map(|k| CoreEffect::Grant {
                        keywords: KeywordSet::of([*k]),
                        to: r(*to),
                    })
                    .collect(),
            )),
            Effect::DiscardUnless { who, count, filter } => {
                out.push(CoreEffect::DiscardUnless {
                    who: who.map_or(CoreRecipient::You, r),
                    count: *count,
                    filter: self.filter(filter),
                });
            }
            Effect::DiscardAll { who, filter, .. } => {
                let filter = filter.as_ref().map(|f| self.filter(f));
                out.push(CoreEffect::DiscardAll {
                    who: who.map_or(CoreRecipient::You, r),
                    filter,
                });
            }
            Effect::AddPoison { count, who } => out.push(CoreEffect::AddPoison {
                count: *count,
                who: r(*who),
            }),
            Effect::AddEnergy { count, who } => out.push(CoreEffect::AddEnergy {
                count: *count,
                who: r(*who),
            }),
            Effect::PayEnergy(n) => out.push(CoreEffect::PayEnergy(*n)),
        }
        Ok(())
    }

    fn trigger_obj(&mut self, t: &TriggerObj) -> CoreTriggerObj {
        match t {
            TriggerObj::This => CoreTriggerObj::This,
            TriggerObj::Filter(f) | TriggerObj::ThisOr(f) => CoreTriggerObj::Filter(self.filter(f)),
        }
    }

    #[allow(clippy::too_many_lines)] // one arm per trigger
    fn trigger(&mut self, t: &Trigger) -> TriggerSpec {
        match t {
            Trigger::When(inner) => self.trigger(inner),
            Trigger::State(c) => {
                let c = self.static_cond(c);
                TriggerSpec::State(self.pool.intern_condition(c))
            }
            Trigger::Enters(o) => TriggerSpec::Enters(self.trigger_obj(o)),
            Trigger::Dies(o) => TriggerSpec::Dies(self.trigger_obj(o)),
            Trigger::EveryStep(step) => TriggerSpec::BeginningOfStep {
                step: *step,
                whose: Who::Any,
            },
            Trigger::BeginningOfStep { step, whose } => TriggerSpec::BeginningOfStep {
                step: *step,
                whose: *whose,
            },
            Trigger::AttachedStep(step) => TriggerSpec::AttachedControllersStep(*step),
            Trigger::Attacks(o) => TriggerSpec::Attacks(self.trigger_obj(o)),
            Trigger::AttacksFirstTime(o) => TriggerSpec::AttacksFirstTime(self.trigger_obj(o)),
            Trigger::AttacksBattle(o) => TriggerSpec::AttacksBattle(self.trigger_obj(o)),
            Trigger::UnlocksDoor => TriggerSpec::UnlocksDoor { door: 0 },
            Trigger::BecomesLevel(n) => TriggerSpec::BecomesStage(n - 1),
            Trigger::Chapter(ns) => TriggerSpec::Chapter(ns.iter().fold(0u16, |m, n| m | 1 << n)),
            Trigger::FullyUnlocks => TriggerSpec::FullyUnlocks { whose: Who::You },
            Trigger::DealsCombatDamageToPlayerOrBattle(o) => {
                TriggerSpec::DealsCombatDamageToPlayerOrBattle(self.trigger_obj(o))
            }
            Trigger::AttacksAlone(o) => TriggerSpec::AttacksAlone(self.trigger_obj(o)),
            Trigger::CreaturesYouControlAttack => TriggerSpec::AttacksWith {
                this: false,
                min: 1,
            },
            Trigger::AttacksWith { this, min } => TriggerSpec::AttacksWith {
                this: *this,
                min: *min,
            },
            Trigger::AttacksUnblocked(o) => TriggerSpec::AttacksUnblocked(self.trigger_obj(o)),
            Trigger::BecomesBlocked(o) => TriggerSpec::BecomesBlocked(self.trigger_obj(o)),
            Trigger::BlocksOrBlocked(o) => TriggerSpec::BlocksOrBlocked(self.trigger_obj(o)),
            Trigger::AttacksOrBlocks(o) => TriggerSpec::AttacksOrBlocks(self.trigger_obj(o)),
            Trigger::Leaves(o) => TriggerSpec::Leaves(self.trigger_obj(o)),
            Trigger::Blocks(o) => TriggerSpec::Blocks(self.trigger_obj(o)),
            Trigger::BlockPair {
                obj,
                other,
                blocks,
                blocked,
                any,
            } => TriggerSpec::BlockPair {
                what: self.trigger_obj(obj),
                other: self.filter(other),
                blocks: *blocks,
                blocked: *blocked,
                any: *any,
            },
            Trigger::DealsCombatDamageToPlayer(o) | Trigger::DealsCombatDamageToDefending(o) => {
                TriggerSpec::DealsCombatDamageToPlayer(self.trigger_obj(o))
            }
            Trigger::GainsLife { whose } => TriggerSpec::GainsLife { whose: *whose },
            Trigger::LosesLife { whose } => TriggerSpec::LosesLife { whose: *whose },
            Trigger::GainsLifeFirst { whose } => TriggerSpec::GainsLifeFirst { whose: *whose },
            Trigger::RollsDice { whose } => TriggerSpec::RollsDice { whose: *whose },
            Trigger::DayNight => TriggerSpec::DayNight,
            Trigger::Sacrificed { obj, whose } => TriggerSpec::Sacrificed {
                what: self.trigger_obj(obj),
                whose: *whose,
            },
            Trigger::WinsFlip { whose } => TriggerSpec::WinsFlip { whose: *whose },
            Trigger::CastsSpell {
                whose,
                noncreature,
                instant_or_sorcery,
                or_copy,
            } => TriggerSpec::CastsSpell {
                whose: *whose,
                noncreature: *noncreature,
                instant_or_sorcery: *instant_or_sorcery,
                or_copy: *or_copy,
            },
            Trigger::BecomesTarget(o) | Trigger::BecomesTargetWhenever(o) => {
                TriggerSpec::BecomesTarget(self.trigger_obj(o))
            }
            Trigger::TargetedByOpponent => TriggerSpec::TargetedByOpponent,
            Trigger::TargetedByYouFirstTime => TriggerSpec::TargetedByYouFirstTime,
            Trigger::TurnedFaceUp(o) => TriggerSpec::TurnedFaceUp(self.trigger_obj(o)),
            Trigger::Exploits => TriggerSpec::Exploits,
            Trigger::BecomesPlotted => TriggerSpec::BecomesPlotted,
            Trigger::DrawsNth { whose, n, .. } => TriggerSpec::DrawsNth {
                whose: *whose,
                n: *n,
            },
            Trigger::BecomesTapped(o) => TriggerSpec::BecomesTapped(self.trigger_obj(o)),
            Trigger::BecomesAttachedTo(f) => TriggerSpec::BecomesAttachedTo(self.filter(f)),
            // Compiled as `AbilityDef::ManaTrigger` by `ability`.
            Trigger::TappedForMana(_) => unreachable!("a triggered mana ability"),
            Trigger::BecomesUntapped(o) => TriggerSpec::BecomesUntapped(self.trigger_obj(o)),
            Trigger::CountersPut { obj, kind } => TriggerSpec::CountersPut {
                what: self.trigger_obj(obj),
                kind: *kind,
            },
            Trigger::NthCounterPut { kind, n } => TriggerSpec::NthCounterPut { kind: *kind, n: *n },
            Trigger::YouPutCounters { obj, kind } => TriggerSpec::YouPutCounters {
                what: self.trigger_obj(obj),
                kind: *kind,
            },
            Trigger::DealtDamage(o) => TriggerSpec::DealtDamage(self.trigger_obj(o)),
            Trigger::DealtCombatDamage(o) => TriggerSpec::DealtCombatDamage(self.trigger_obj(o)),
            Trigger::DealsDamage { obj, to, combat } => TriggerSpec::DealsDamage {
                what: self.trigger_obj(obj),
                to: *to,
                combat: *combat,
            },
            Trigger::DealsDamageTo { obj, filter } => TriggerSpec::DealsDamageTo {
                what: self.trigger_obj(obj),
                filter: self.filter(filter),
            },
            Trigger::CastsFiltered { whose, filter } => TriggerSpec::CastsFiltered {
                whose: *whose,
                filter: self.filter(filter),
            },
            Trigger::Cycles {
                this,
                another,
                or_discard,
            } => TriggerSpec::Cycles {
                this: *this,
                another: *another,
                or_discard: *or_discard,
            },
            Trigger::CastThis { or_cycle: false } => TriggerSpec::CastThis,
            Trigger::Next(inner) => TriggerSpec::Next(Box::new(self.trigger(inner))),
            Trigger::Batch(inner) => TriggerSpec::Batch(Box::new(self.trigger(inner))),
            Trigger::Each(inner) => TriggerSpec::Each(Box::new(self.trigger(inner))),
            Trigger::OnNextTurn(inner) => TriggerSpec::OnNextTurn(Box::new(self.trigger(inner))),
            Trigger::Sacrifices(f) => TriggerSpec::Sacrifices(self.filter(f)),
            Trigger::CastThis { or_cycle: true } => TriggerSpec::Either(Box::new((
                TriggerSpec::CastThis,
                TriggerSpec::Cycles {
                    this: true,
                    another: false,
                    or_discard: false,
                },
            ))),
            Trigger::CastsNth { whose, nth } => TriggerSpec::CastsNth {
                whose: *whose,
                nth: *nth,
            },
            Trigger::EntersBatch(f) => TriggerSpec::EntersBatch(self.filter(f)),
            Trigger::LeavesBatch { filter, dies } => TriggerSpec::LeavesBatch {
                filter: self.filter(filter),
                dies: *dies,
            },
            Trigger::Draws { whose } => TriggerSpec::Draws { whose: *whose },
            Trigger::Discards { whose, filter } => TriggerSpec::Discards {
                whose: *whose,
                filter: self.filter(filter),
            },
            Trigger::LeavesGraveyard(f) => TriggerSpec::LeavesGraveyard(self.filter(f)),
            Trigger::PutIntoGraveyard(f) => TriggerSpec::PutIntoGraveyard(self.filter(f)),
            Trigger::PutIntoGraveyardFromBattlefield(f) => {
                TriggerSpec::PutIntoGraveyardFromBattlefield(self.filter(f))
            }
            Trigger::EntersFromGraveyard(f) => TriggerSpec::EntersFromGraveyard(self.filter(f)),
            Trigger::Expends(n) => TriggerSpec::Expends(*n),
            Trigger::CollectsEvidence => TriggerSpec::CollectsEvidence { whose: Who::You },
            Trigger::Forages => TriggerSpec::Forages { whose: Who::You },
            Trigger::CreaturesDealCombatDamage(f) => {
                TriggerSpec::CreaturesDealCombatDamage(self.filter(f))
            }
            Trigger::NoncombatDamageToOpponent { yours, batch } => {
                TriggerSpec::NoncombatDamageToOpponent {
                    yours: *yours,
                    batch: *batch,
                }
            }
            Trigger::CommitsCrime { your_turn } => TriggerSpec::CommitsCrime {
                your_turn: *your_turn,
            },
            Trigger::OpponentGainsControl => TriggerSpec::OpponentGainsControl,
            Trigger::YouTurnFaceUp => TriggerSpec::TurnedFaceUp(self.trigger_obj(
                &crate::ast::TriggerObj::Filter(Filter {
                    who: Some(Who::You),
                    ..Filter::default()
                }),
            )),
            Trigger::DiscardsBatch { whose } => TriggerSpec::DiscardsBatch { whose: *whose },
            Trigger::SacrificesBatch(f) => TriggerSpec::SacrificesBatch(self.filter(f)),
            Trigger::AttacksWithFilter { filter, player } => TriggerSpec::AttacksWithFilter {
                filter: self.filter(filter),
                player: *player,
            },
            Trigger::GainsOrLosesLife { your_turn } => TriggerSpec::GainsOrLosesLife {
                your_turn: *your_turn,
            },
            Trigger::ScriesOrSurveils => TriggerSpec::ScriesOrSurveils,
            Trigger::Explores(o) => TriggerSpec::Explores(self.trigger_obj(o)),
            Trigger::YouTap(f) => TriggerSpec::YouTap(self.filter(f)),
            Trigger::Activates { whose, kind } => TriggerSpec::Activates {
                whose: *whose,
                kind: *kind,
            },
            Trigger::Transforms => TriggerSpec::Transforms,
            Trigger::Or { first, second, .. } => {
                TriggerSpec::Either(Box::new((self.trigger(first), self.trigger(second))))
            }
            Trigger::Either(a, b) => {
                TriggerSpec::Either(Box::new((self.trigger(a), self.trigger(b))))
            }
        }
    }

    fn affects(&mut self, a: &Affects) -> CoreAffects {
        match a {
            Affects::Any => CoreAffects::Any,
            Affects::This => CoreAffects::This,
            Affects::You => CoreAffects::You,
            Affects::Players(w) => CoreAffects::Players(*w),
            Affects::Objects(f) => CoreAffects::Objects(self.filter(f)),
            Affects::YouAnd(f) => CoreAffects::YouAnd(self.filter(f)),
            Affects::OpponentsAnd(f) => CoreAffects::OpponentsAnd(self.filter(f)),
            // A variable group: its targets to the end of the list.
            Affects::Target(i) if Some(*i) == self.many => CoreAffects::Targets(
                u8::try_from(target_index(self.tmap, *i)).unwrap_or(u8::MAX),
                u8::MAX,
            ),
            Affects::Target(i) | Affects::ThatTarget(i) => {
                CoreAffects::Target(target_index(self.tmap, *i))
            }
            Affects::Targets(a, n) => CoreAffects::Targets(
                u8::try_from(target_index(self.tmap, *a)).unwrap_or(u8::MAX),
                u8::try_from(*n).unwrap_or(u8::MAX),
            ),
            Affects::OtherThanTarget(f, i) => {
                CoreAffects::OtherThanTarget(self.filter(f), target_index(self.tmap, *i))
            }
            Affects::Chosen(f) => CoreAffects::Chosen(self.filter(f)),
            Affects::DamagedThisWay => CoreAffects::DamagedThisWay,
        }
    }

    fn replacement(&mut self, r: &Replacement) -> Result<ReplacementAbility, CompileError> {
        let replaces = match &r.event {
            Replaces::Damage {
                from, to, combat, ..
            } => CoreReplaces::Damage {
                from: from.as_ref().map(|f| self.affects(f)),
                to: self.affects(to),
                combat: *combat,
            },
            Replaces::DamageToAndBy { what, combat } => CoreReplaces::DamageToAndBy {
                what: self.affects(what),
                combat: *combat,
            },
            Replaces::GainLife { whose } => CoreReplaces::GainLife { whose: *whose },
            Replaces::Draw {
                whose,
                library_empty,
            } => CoreReplaces::Draw {
                whose: *whose,
                library_empty: *library_empty,
            },
            Replaces::ToGraveyard {
                whose,
                from_battlefield,
            } => CoreReplaces::ToGraveyard {
                whose: *whose,
                from_battlefield: *from_battlefield,
            },
            Replaces::ThisToGraveyard => CoreReplaces::ThisToGraveyard,
            Replaces::CreateTokens {
                creature,
                effect,
                artifact,
                you,
            } => CoreReplaces::CreateTokens {
                creature: *creature,
                effect: *effect,
                artifact: *artifact,
                you: *you,
            },
            Replaces::PlusCounters { on } => CoreReplaces::PlusCounters {
                on: self.filter(on),
            },
            Replaces::Dies { to } => CoreReplaces::Dies {
                to: self.affects(to),
            },
            Replaces::Destroy { to } => CoreReplaces::Destroy {
                to: self.affects(to),
            },
            Replaces::Enters => CoreReplaces::Enters,
            Replaces::OthersEnter { what, .. } => CoreReplaces::OthersEnter {
                what: self.filter(what),
            },
        };
        let mut replacement = r.outcome;
        if let mtg_core::card::Replacement::PlusToken { token, .. }
        | mtg_core::card::Replacement::TokensInstead { token } = &mut replacement
        {
            let name = r.token.as_deref().unwrap_or_default();
            let Some(t) = self.pool.lookup(name) else {
                return self.err(format!("unknown token \"{name}\""));
            };
            *token = t;
        }
        Ok(ReplacementAbility {
            replaces,
            replacement,
            then: self.sentences(&r.then)?,
            condition: None,
        })
    }

    /// A modification, registering a granted ability ("has \"...\"").
    fn modification_or_grant(&mut self, m: &Mod) -> Result<Modification, CompileError> {
        if let Mod::AddAbility(a) = m {
            let def = self.ability(a)?;
            return Ok(Modification::AddAbility(self.pool.register_ability(def)));
        }
        let out = self.modification(m);
        // A count only a resolving effect has (its targets, what it did)
        // would read 0 here (`CountExpr::in_static`).
        if let Modification::ModifyPtPer { count, .. } | Modification::CdaPt { count, .. } = &out
            && !count.in_static()
        {
            return self.err(
                "a static ability can't use this count: it exists only while an effect resolves",
            );
        }
        Ok(out)
    }

    fn modification(&mut self, m: &Mod) -> Modification {
        match m {
            Mod::AddAbility(_) => unreachable!("compiled by modification_or_grant"),
            Mod::ModifyPtPer {
                power,
                toughness,
                count,
            } => {
                let expr = match count {
                    mtg_core::card::Num::Count(i) => self
                        .card
                        .counts
                        .get(usize::from(*i))
                        .map(|c| c.expr.clone()),
                    _ => None,
                };
                match expr {
                    Some(e) => Modification::ModifyPtPer {
                        power: *power,
                        toughness: *toughness,
                        count: self.count_expr(&e),
                    },
                    None => Modification::ModifyPt {
                        power: *power,
                        toughness: *toughness,
                    },
                }
            }
            Mod::AddTypes(t) => Modification::AddTypes(TypeSet::of(t.iter().copied())),
            Mod::AddLandType(c) => Modification::AddLandType(*c),
            Mod::SetLandType(c) => Modification::SetLandType(*c),
            // A static ability compiles the rest (`Ability::Static`).
            Mod::SetLandTypes(lands) => {
                Modification::SetLandType(lands.first().copied().unwrap_or(Color::Green))
            }
            Mod::ChosenLandType => Modification::ChosenLandType,
            Mod::AddChosenType => Modification::AddChosenType,
            Mod::AddSubtype(s) => Modification::AddSubtype(self.pool.subtype(s)),
            Mod::SetSubtype(s) => Modification::SetCreatureType(self.pool.subtype(s)),
            // A static ability compiles all five (`Ability::Static`).
            Mod::AddAllLandTypes => Modification::AddLandType(Color::White),
            Mod::Control => Modification::Control,
            Mod::AddColors(c) => Modification::AddColors(ColorSet::of(c.iter().copied())),
            Mod::SetColors(c) => Modification::SetColors(ColorSet::of(c.iter().copied())),
            Mod::AddKeywords(k) => Modification::AddKeywords(KeywordSet::of(k.iter().copied())),
            Mod::LosesKeywords(k) => {
                Modification::RemoveKeywords(KeywordSet::of(k.iter().copied()))
            }
            Mod::RemoveAllAbilities => Modification::RemoveAllAbilities,
            // The card is marked as goading too (`Builder::goads`).
            Mod::Goaded => Modification::AddKeywords(
                KeywordSet::NONE.with(mtg_core::card::Keyword::MustAttack),
            ),
            Mod::AllCreatureTypes { .. } => Modification::AllCreatureTypes,
            Mod::NoCreatureTypes => Modification::NoCreatureTypes,
            Mod::SetTypes(t) => Modification::SetTypes(TypeSet::of(t.iter().copied())),
            Mod::SetPt { power, toughness } => Modification::SetPt {
                power: *power,
                toughness: *toughness,
            },
            Mod::ModifyPt { power, toughness } => Modification::ModifyPt {
                power: *power,
                toughness: *toughness,
            },
            Mod::SwitchPt => Modification::SwitchPt,
            Mod::CdaColors(c) => Modification::CdaColors(ColorSet::of(c.iter().copied())),
            Mod::ChosenColor => Modification::ChosenColor,
            Mod::ProtectionFromChosen => Modification::ProtectionFromChosen,
            Mod::CdaPt {
                count,
                sets,
                plus_toughness,
                noun: _,
            } => Modification::CdaPt {
                count: self.count_expr(count),
                sets: *sets,
                plus_toughness: *plus_toughness,
            },
        }
    }

    /// CR 702.165a: "When this creature enters, put N +1/+1 counters on
    /// target creature. If that's another creature, it gains the following
    /// abilities until end of turn." The following abilities are those
    /// written after the `backup` line (CR 702.165b: printed below it).
    fn backup(&mut self, a: &Ability, n: u32) -> Result<AbilityDef, CompileError> {
        let card = self.card;
        let at = card
            .abilities
            .iter()
            .position(|x| std::ptr::eq(x, a))
            .map_or(card.abilities.len(), |i| i + 1);
        let mut keywords = KeywordSet::NONE;
        let mut grants = Vec::new();
        for x in &card.abilities[at..] {
            match x {
                Ability::Keyword(k) => keywords = keywords.with(*k),
                other => {
                    let def = self.ability(other)?;
                    let id = self.pool.register_ability(def);
                    grants.push(CoreEffect::GrantAbility {
                        ability: id,
                        to: CoreRecipient::Target(0),
                    });
                }
            }
        }
        if keywords != KeywordSet::NONE {
            grants.insert(
                0,
                CoreEffect::Grant {
                    keywords,
                    to: CoreRecipient::Target(0),
                },
            );
        }
        let mut effects = vec![CoreEffect::AddCounters {
            kind: mtg_core::card::CounterKind::PlusOnePlusOne,
            count: Num::N(n),
            to: CoreRecipient::Target(0),
        }];
        if !grants.is_empty() {
            effects.push(CoreEffect::IfTarget {
                n: 0,
                filter: Box::new(ObjFilter {
                    relation: Relation::Other,
                    ..ObjFilter::default()
                }),
                then: grants,
            });
        }
        Ok(AbilityDef::Triggered(TriggeredAbility {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: TriggerSpec::Enters(CoreTriggerObj::This),
            condition: None,
            targets: vec![TargetSpec::Creature],
            effects,
        }))
    }

    /// A keyword's activated ability, compiled directly (ninjutsu from the
    /// hand; scavenge, embalm, eternalize from the graveyard as a sorcery).
    fn keyword_ability(
        &mut self,
        a: &Ability,
        cost: CoreCost,
        targets: Vec<TargetSpec>,
        effects: Vec<CoreEffect>,
        keyword: Option<mtg_core::card::ActivatedKeyword>,
    ) -> AbilityDef {
        let from_hand = keyword.is_some();
        AbilityDef::Activated(ActivatedAbility {
            from_graveyard: cost.exile_self,
            sorcery_speed: cost.exile_self,
            cost,
            targets,
            effects,
            per_turn: 0,
            conditions: Vec::new(),
            once_ever: false,
            from_hand,
            cycling: false,
            keyword,
            reduction: None,
            text: render::ability_text(self.card, a, self.lookup),
        })
    }

    #[allow(clippy::too_many_lines)] // one arm per ability kind
    fn ability(&mut self, a: &Ability) -> Result<AbilityDef, CompileError> {
        self.many = None;
        self.tmap = TargetMap::default();
        Ok(match a {
            Ability::Keyword(_) => return self.err("a keyword is not an ability definition"),
            Ability::LevelBand { .. } => {
                return self.err("a level symbol is not an ability definition");
            }
            Ability::DeckAnyNumber => return self.err("a deck-building rule is not an ability"),
            Ability::Uncounterable => {
                return self.err("\"can't be countered\" is a property of the spell");
            }
            Ability::AuraStays => {
                return self.err("`protection doesn't remove this` is not an ability");
            }
            Ability::Mana(m) => {
                // One ability per choice; `Ability::Mana` with several
                // choices is split by the caller through `abilities()`.
                match m.as_slice() {
                    [one] => AbilityDef::TapForMana(*one),
                    _ => return self.err("a `mana` line with several choices must be split"),
                }
            }
            Ability::SacMana(m) => AbilityDef::SacForMana(*m),
            Ability::RestrictedMana { mana, restriction } => AbilityDef::RestrictedMana {
                mana: *mana,
                restriction: mtg_core::card::ManaRestriction {
                    spells: restriction.spells.as_ref().map(|f| self.filter(f)),
                    abilities: restriction.abilities,
                    ability_source: restriction.ability_source.as_ref().map(|f| self.filter(f)),
                    foretell: restriction.foretell,
                    disturb: restriction.disturb,
                    wording: restriction.wording,
                },
            },
            Ability::XManaColor(c) => AbilityDef::XManaColor(*c),
            Ability::Enchant(t) => AbilityDef::Enchant(self.target(t)),
            Ability::Spell { targets, effects } => AbilityDef::Spell {
                targets: self.targets(targets),
                effects: self.sentences(effects)?,
            },
            // CR 702.42a: entwined, every mode.
            Ability::Modal(modes) => AbilityDef::Modal(
                self.modes(
                    modes,
                    self.card
                        .abilities
                        .iter()
                        .any(|a| matches!(a, Ability::Mechanic(Mechanic::Entwine(_)))),
                    false,
                )?,
            ),
            Ability::ModalBoth(modes) => AbilityDef::Modal(self.modes(modes, true, false)?),
            Ability::ModalTwo(modes) => AbilityDef::Modal(self.modes(modes, false, true)?),
            Ability::ModalOf(head, modes) => AbilityDef::Modal(self.head_modes(*head, modes)?),
            Ability::ModalIf {
                more: false, modes, ..
            } => {
                if modes.len() != 2 {
                    return self.err("`choose one or both if` takes two modes");
                }
                AbilityDef::Modal(self.modes(modes, true, false)?)
            }
            Ability::ModalIf {
                more: true, modes, ..
            } => AbilityDef::Modal(self.head_modes(crate::ast::ModeHead::OneOrMore, modes)?),
            Ability::Spree(modes) => AbilityDef::Modal(self.spree_modes(modes)?),
            Ability::Pawprint { limit, modes } => {
                AbilityDef::Modal(self.pawprint_modes(*limit, modes)?)
            }
            Ability::Activated { cost, .. } if cost.discard_x => {
                return self.err("`discard X cards` is only a spell's additional cost");
            }
            // The first of the two equip abilities; the card's ability list
            // gets the second (`equip_or_second`).
            Ability::EquipOr { mana, targets, .. } => {
                return self.ability(&equip_ability(
                    Cost {
                        mana: mana.clone(),
                        ..Default::default()
                    },
                    targets,
                ));
            }
            Ability::Activated {
                cost,
                targets,
                effects,
                cycling,
                ..
            } => AbilityDef::Activated(ActivatedAbility {
                cost: self.cost(cost),
                targets: self.targets(targets),
                effects: self.sentences(effects)?,
                sorcery_speed: cost.sorcery,
                per_turn: cost.per_turn,
                conditions: cost
                    .conditions
                    .iter()
                    .map(|c| self.static_cond(c))
                    .collect(),
                once_ever: cost.once_ever,
                from_hand: *cycling || cost.discard_self || cost.reveal_this,
                cycling: *cycling,
                from_graveyard: cost.from_graveyard || cost.exile_self,
                keyword: cost.keyword,
                text: render::ability_text(self.card, a, self.lookup),
                reduction: cost.reduce.as_deref().map(|(by, how)| {
                    Box::new(mtg_core::card::ActivationReduction {
                        by: by.clone(),
                        each: match how {
                            crate::ast::ReduceBy::Each(e) => Some(self.count_expr(e)),
                            crate::ast::ReduceBy::If(_) => None,
                        },
                        condition: match how {
                            crate::ast::ReduceBy::If(c) => Some(self.static_cond(c)),
                            crate::ast::ReduceBy::Each(_) => None,
                        },
                    })
                }),
            }),
            // CR 605.1b: a triggered mana ability, resolving at once.
            Ability::Triggered {
                trigger: Trigger::TappedForMana(o),
                condition,
                targets,
                effects,
                ..
            } => {
                let TriggerObj::Filter(f) = o else {
                    return self.err("`is tapped for mana` needs a filter (`enchanted land`)");
                };
                let effects = self.sentences(effects)?;
                if condition.is_some()
                    || !targets.is_empty()
                    || effects.is_empty()
                    || !effects.iter().all(CoreEffect::adds_mana)
                {
                    return self.err("a triggered mana ability only adds mana");
                }
                AbilityDef::ManaTrigger {
                    what: self.filter(f),
                    effects,
                }
            }
            Ability::Triggered {
                trigger,
                condition,
                targets,
                effects,
                from_graveyard,
                limit,
            } => AbilityDef::Triggered(TriggeredAbility {
                limit: *limit,
                trigger: self.trigger(trigger),
                condition: match condition {
                    None => None,
                    Some(crate::ast::TrigCond::Core(c)) => Some(*c),
                    Some(crate::ast::TrigCond::Static(c)) => {
                        let c = self.static_cond(c);
                        Some(mtg_core::card::Condition::Static(
                            self.pool.intern_condition(c),
                        ))
                    }
                    // "When this enters and whenever this attacks while
                    // saddled": the condition is the second event's.
                    Some(crate::ast::TrigCond::While(c))
                        if matches!(trigger, crate::ast::Trigger::Either(..))
                            || matches!(trigger, crate::ast::Trigger::When(t)
                                if matches!(**t, crate::ast::Trigger::Either(..))) =>
                    {
                        let c = self.static_cond(c);
                        Some(mtg_core::card::Condition::WhileSecond(
                            self.pool.intern_condition(c),
                        ))
                    }
                    Some(crate::ast::TrigCond::While(c)) => {
                        let c = self.static_cond(c);
                        Some(mtg_core::card::Condition::While(
                            self.pool.intern_condition(c),
                        ))
                    }
                },
                targets: self.targets(targets),
                effects: self.sentences(effects)?,
                from_graveyard: *from_graveyard,
            }),
            Ability::Replacement(r) => AbilityDef::Replacement(self.replacement(r)?),
            // CR 701.43: modeled as an attack trigger: exerting is "it
            // doesn't untap during your next untap step".
            Ability::Exert(effects) => {
                let mut inner = vec![Sentence::new(vec![crate::ast::Effect::SkipNextUntap {
                    to: Recipient::Source,
                    steps: false,
                }])];
                inner.extend(effects.iter().cloned());
                return self.ability(&Ability::Triggered {
                    limit: mtg_core::card::TurnLimit::None,
                    from_graveyard: false,
                    trigger: Trigger::Attacks(TriggerObj::This),
                    condition: None,
                    targets: Vec::new(),
                    effects: vec![Sentence::new(vec![crate::ast::Effect::YouMay(inner)])],
                });
            }
            Ability::Static {
                filter,
                mods,
                condition,
                ..
            } => AbilityDef::Static(StaticAbility {
                affected: self.filter(filter),
                parts: {
                    let mut parts = Vec::new();
                    for m in mods {
                        // "is a Mountain, Forest, and Plains": the first
                        // replaces its land types, the rest add to it.
                        if *m == Mod::AddAllLandTypes {
                            parts.extend(Color::ALL.iter().map(|c| Modification::AddLandType(*c)));
                            continue;
                        }
                        if let Mod::SetLandTypes(lands) = m {
                            for (i, c) in lands.iter().enumerate() {
                                parts.push(if i == 0 {
                                    Modification::SetLandType(*c)
                                } else {
                                    Modification::AddLandType(*c)
                                });
                            }
                            continue;
                        }
                        // "has defender, and loses all other abilities": the
                        // loss applies before what it gains in layer 6 (CR
                        // 613.1f: the effect's parts in order).
                        if *m == Mod::RemoveAllAbilities {
                            parts.insert(0, Modification::RemoveAllAbilities);
                            continue;
                        }
                        parts.push(self.modification_or_grant(m)?);
                    }
                    parts
                },
                condition: condition.as_ref().map(|c| self.static_cond(c)),
            }),
            Ability::EntersWithIf { n, kind, cond, .. } => {
                let c = self.static_cond(cond);
                let cond = self.pool.intern_condition(c);
                AbilityDef::Replacement(ReplacementAbility {
                    replaces: mtg_core::card::Replaces::Enters,
                    replacement: mtg_core::card::Replacement::EnterWithCountersIf {
                        cond,
                        kind: *kind,
                        n: *n,
                    },
                    then: Vec::new(),
                    condition: None,
                })
            }
            // "enters tapped if it's not your turn" is "unless it's your
            // turn" (CR 614.1d).
            Ability::EntersTappedIfNotYourTurn => {
                let c = self.static_cond(&StaticCond::YourTurn);
                let i = self.pool.intern_condition(c);
                AbilityDef::Replacement(ReplacementAbility {
                    replaces: mtg_core::card::Replaces::Enters,
                    replacement: mtg_core::card::Replacement::EnterTappedUnless(i),
                    then: Vec::new(),
                    condition: None,
                })
            }
            Ability::EntersTappedUnless(c) => {
                let c = self.static_cond(c);
                let i = self.pool.intern_condition(c);
                AbilityDef::Replacement(ReplacementAbility {
                    replaces: mtg_core::card::Replaces::Enters,
                    replacement: mtg_core::card::Replacement::EnterTappedUnless(i),
                    then: Vec::new(),
                    condition: None,
                })
            }
            Ability::AttackUnless {
                cond,
                attack,
                block,
                negated,
            } => {
                let c = self.static_cond(cond);
                AbilityDef::AttackUnless {
                    condition: self.pool.intern_condition(c),
                    attack: *attack,
                    block: *block,
                    negated: *negated,
                }
            }
            // CR 702.179e: "Max speed — [ability]" means "As long as you
            // have max speed, this object has [ability]": the ability's
            // own condition.
            Ability::Labeled { label, ability } if label == "Max speed" => {
                let a = self.ability(ability)?;
                return self.while_holds(a, StaticCondition::MaxSpeed, "max speed");
            }
            // CR 721.2a: "As long as this permanent has N or more charge
            // counters on it, it has [ability]."
            Ability::StationAbility { n, ability } => {
                let a = self.ability(ability)?;
                let c = self.static_cond(&StaticCond::SourceCounters {
                    kind: mtg_core::card::CounterKind::Charge,
                    n: *n,
                    at_least: true,
                });
                return self.while_holds(a, c, "a station level");
            }
            // "• Khans — ...": the ability while that option was chosen as
            // the permanent entered (CR 614.12).
            Ability::Labeled { label, ability } if let Some(i) = named_option(self.card, label) => {
                let a = self.ability(ability)?;
                return self.while_holds(a, StaticCondition::Chose(i), "a named option");
            }
            // CR 702.57a: "Forecast — [Activated ability]" means "[Activated
            // ability]. Activate only during your upkeep and only once each
            // turn." (from the hand, revealing the card).
            Ability::Labeled { label, ability } if label == "Forecast" => {
                let mut a = self.ability(ability)?;
                let AbilityDef::Activated(aa) = &mut a else {
                    return self.err("forecast labels an activated ability");
                };
                if !aa.from_hand {
                    return self.err("a forecast ability reveals the card from the hand");
                }
                aa.conditions.push(StaticCondition::During(
                    mtg_core::card::TurnTime::YourUpkeep,
                ));
                aa.per_turn = 1;
                return Ok(a);
            }
            Ability::Labeled { ability, .. } => return self.ability(ability),
            Ability::BlockRule(r) => AbilityDef::BlockRule(match r {
                crate::ast::BlockRule::NotBy(f) => mtg_core::card::BlockRule::NotBy(self.filter(f)),
                crate::ast::BlockRule::OnlyBy(f) => {
                    mtg_core::card::BlockRule::OnlyBy(self.filter(f))
                }
                crate::ast::BlockRule::AtMostOne => mtg_core::card::BlockRule::AtMostOne,
                crate::ast::BlockRule::NotByLesserPower => {
                    mtg_core::card::BlockRule::NotByLesserPower
                }
                crate::ast::BlockRule::BlocksOnly(f) => {
                    mtg_core::card::BlockRule::BlocksOnly(self.filter(f))
                }
                crate::ast::BlockRule::CantBlock(f) => {
                    mtg_core::card::BlockRule::CantBlock(self.filter(f))
                }
                crate::ast::BlockRule::MinBlockers(n) => mtg_core::card::BlockRule::MinBlockers(*n),
                crate::ast::BlockRule::BlocksShadow { had } => {
                    mtg_core::card::BlockRule::BlocksShadow { had: *had }
                }
                crate::ast::BlockRule::BlocksLandwalk => mtg_core::card::BlockRule::BlocksLandwalk,
                crate::ast::BlockRule::BlocksAdditional(n) => {
                    mtg_core::card::BlockRule::BlocksAdditional(*n)
                }
            }),
            Ability::EntersAsCopy(f) => AbilityDef::EntersAsCopy(self.filter(f)),
            Ability::AsEntersChoose { kind, .. } => {
                AbilityDef::AsEntersChoose(self.choice_kind(kind))
            }
            Ability::CostChange {
                filter,
                whose,
                generic,
                cond,
            } => {
                let cond = cond.as_ref().map(|c| {
                    let c = self.static_cond(c);
                    self.pool.intern_condition(c)
                });
                AbilityDef::CostChange(mtg_core::card::CostChange {
                    spells: self.filter(filter),
                    whose: *whose,
                    generic: *generic,
                    cond,
                    grant: None,
                })
            }
            Ability::SpellsHave {
                filter,
                from_hand,
                keywords,
                storm,
                affinity,
                uncounterable,
            } => AbilityDef::CostChange(mtg_core::card::CostChange {
                spells: self.filter(filter),
                whose: Who::You,
                generic: 0,
                cond: None,
                grant: Some(Box::new(mtg_core::card::SpellGrant {
                    keywords: KeywordSet::of(keywords.iter().copied()),
                    storm: *storm,
                    affinity: affinity.as_ref().map(|f| self.filter(f)),
                    from_hand: *from_hand,
                    uncounterable: *uncounterable,
                })),
            }),
            Ability::Protection(f) => AbilityDef::Protection(self.filter(f)),
            Ability::HexproofFrom(f) => AbilityDef::HexproofFrom(self.filter(f)),
            Ability::OpeningHand => AbilityDef::OpeningHand,
            Ability::TriggerAgain {
                filter,
                entering,
                battlefield,
                cause,
                suppress,
            } => AbilityDef::TriggerAgain {
                filter: self.filter(filter),
                entering: entering.as_ref().map(|f| self.filter(f)),
                battlefield: *battlefield,
                cause: *cause,
                suppress: *suppress,
            },
            Ability::ActivationCost {
                filter,
                by,
                min_one,
                keyword,
            } => {
                let f = self.filter(filter);
                AbilityDef::PlayerStatic {
                    kind: mtg_core::card::PlayerStaticKind::ActivationCost {
                        sources: self.pool.intern_filter(f),
                        by: *by,
                        min_one: *min_one,
                        keyword: *keyword,
                    },
                    condition: None,
                }
            }
            Ability::PlayerStatic { kind, cond } => {
                let condition = match cond {
                    Some(c) => {
                        let c = self.static_cond(c);
                        Some(self.pool.intern_condition(c))
                    }
                    None => None,
                };
                AbilityDef::PlayerStatic {
                    kind: *kind,
                    condition,
                }
            }
            Ability::Mechanic(m) => match m {
                Mechanic::Toxic(n) => AbilityDef::Toxic(*n),
                Mechanic::CounterChoice(kinds, escapes) => {
                    let cond =
                        escapes.then(|| self.pool.intern_condition(StaticCondition::Escaped));
                    AbilityDef::Replacement(ReplacementAbility {
                        replaces: mtg_core::card::Replaces::Enters,
                        replacement: mtg_core::card::Replacement::EnterWithCounterChoice {
                            kinds: *kinds,
                            cond,
                        },
                        then: Vec::new(),
                        condition: None,
                    })
                }
                Mechanic::Bloodthirst(n)
                | Mechanic::EscapesWith(n)
                | Mechanic::KickedWith(n, _) => {
                    let cond = self.pool.intern_condition(match m {
                        Mechanic::EscapesWith(_) => StaticCondition::Escaped,
                        Mechanic::KickedWith(..) => StaticCondition::Kicked,
                        _ => StaticCondition::OpponentDamagedThisTurn,
                    });
                    AbilityDef::Replacement(ReplacementAbility {
                        replaces: mtg_core::card::Replaces::Enters,
                        replacement: mtg_core::card::Replacement::EnterWithCountersIf {
                            cond,
                            kind: mtg_core::card::CounterKind::PlusOnePlusOne,
                            n: *n,
                        },
                        then: Vec::new(),
                        condition: None,
                    })
                }
                Mechanic::Buyback(_)
                | Mechanic::BuybackCost(_)
                | Mechanic::MorphCost(_)
                | Mechanic::Evoke(_)
                | Mechanic::Madness(_)
                | Mechanic::Mayhem(_)
                | Mechanic::Foretell(_)
                | Mechanic::Plot(_)
                | Mechanic::Sneak(_)
                | Mechanic::Harmonize(_)
                | Mechanic::WebSlinging(_)
                | Mechanic::Suspend(..)
                | Mechanic::Entwine(_)
                | Mechanic::Escape(..)
                | Mechanic::Morph(_)
                | Mechanic::Megamorph(_)
                | Mechanic::Disguise(_)
                | Mechanic::Dash(_)
                | Mechanic::Warp(_)
                | Mechanic::Bestow(_)
                | Mechanic::Overload(_)
                | Mechanic::Splice(_)
                | Mechanic::SpliceCost(_)
                | Mechanic::Bargain
                | Mechanic::Conspire
                | Mechanic::Casualty(_)
                | Mechanic::Dredge(_)
                | Mechanic::Storm
                | Mechanic::Awaken(..)
                | Mechanic::Assist
                | Mechanic::Myriad
                | Mechanic::Prototype(..)
                | Mechanic::Blitz(_)
                | Mechanic::Escalate(_)
                | Mechanic::Prowl(_)
                | Mechanic::Miracle(_)
                | Mechanic::Emerge(_)
                | Mechanic::Replicate(_)
                | Mechanic::Multikicker(_)
                | Mechanic::Disturb(_)
                | Mechanic::GraveyardExile
                | Mechanic::Teamwork(_)
                | Mechanic::Ripple(_)
                | Mechanic::Strive(_)
                | Mechanic::Ascend
                | Mechanic::Storied
                | Mechanic::StartYourEngines
                | Mechanic::BecomesDay
                | Mechanic::Daybound
                | Mechanic::Nightbound => {
                    return self.err("a cost of the card, not an ability");
                }
                // CR 702.175a: "When this permanent enters, if its
                // offspring cost was paid, create a token that's a copy of
                // it, except it's 1/1."
                Mechanic::Offspring(_) => {
                    let Some(token) = self.token else {
                        return self.err("the token is missing");
                    };
                    let cond = self.pool.intern_condition(StaticCondition::OffspringPaid);
                    AbilityDef::Triggered(TriggeredAbility {
                        limit: mtg_core::card::TurnLimit::None,
                        from_graveyard: false,
                        trigger: TriggerSpec::Enters(CoreTriggerObj::This),
                        condition: Some(mtg_core::card::Condition::Static(cond)),
                        targets: Vec::new(),
                        effects: vec![CoreEffect::CreateToken {
                            token,
                            count: Num::N(1),
                            who: CoreRecipient::You,
                            tapped: false,
                            attached: None,
                        }],
                    })
                }
                Mechanic::Ninjutsu(cost) => self.keyword_ability(
                    a,
                    CoreCost {
                        mana: cost.clone(),
                        ..CoreCost::default()
                    },
                    Vec::new(),
                    vec![CoreEffect::EnterAttacking],
                    Some(mtg_core::card::ActivatedKeyword::Ninjutsu),
                ),
                // CR 702.97a: "[Cost], Exile this card from your graveyard:
                // Put a number of +1/+1 counters equal to the power of the
                // card you exiled on target creature. Activate only as a
                // sorcery."
                Mechanic::Scavenge(cost) => {
                    let Some(i) = self.scavenge else {
                        return self.err("scavenge's count is missing");
                    };
                    self.keyword_ability(
                        a,
                        CoreCost {
                            mana: cost.clone(),
                            exile_self: true,
                            ..CoreCost::default()
                        },
                        vec![TargetSpec::Creature],
                        vec![CoreEffect::AddCounters {
                            kind: mtg_core::card::CounterKind::PlusOnePlusOne,
                            count: Num::Count(i),
                            to: CoreRecipient::Target(0),
                        }],
                        None,
                    )
                }
                Mechanic::Backup(n) => self.backup(a, *n)?,
                // CR 702.123a: "When this permanent enters, you may put N
                // +1/+1 counters on it. If you don't, create N 1/1
                // colorless Servo artifact creature tokens."
                Mechanic::Fabricate(n) => {
                    let Some(token) = self.pool.lookup(SERVO) else {
                        return self.err(format!("unknown token \"{SERVO}\""));
                    };
                    AbilityDef::Triggered(TriggeredAbility {
                        limit: mtg_core::card::TurnLimit::None,
                        from_graveyard: false,
                        trigger: TriggerSpec::Enters(CoreTriggerObj::This),
                        condition: None,
                        targets: Vec::new(),
                        effects: vec![CoreEffect::Fabricate { count: *n, token }],
                    })
                }
                // CR 702.112a: "When this creature deals combat damage to a
                // player, if it isn't renowned, put N +1/+1 counters on it
                // and it becomes renowned."
                Mechanic::Renown(n) => AbilityDef::Triggered(TriggeredAbility {
                    limit: mtg_core::card::TurnLimit::None,
                    from_graveyard: false,
                    trigger: TriggerSpec::DealsCombatDamageToPlayer(CoreTriggerObj::This),
                    condition: Some(mtg_core::card::Condition::NotRenowned),
                    targets: Vec::new(),
                    effects: vec![
                        CoreEffect::AddCounters {
                            kind: mtg_core::card::CounterKind::PlusOnePlusOne,
                            count: Num::N(*n),
                            to: CoreRecipient::Source,
                        },
                        CoreEffect::BecomeRenowned,
                    ],
                }),
                // CR 702.98a: the first of unleash's two static abilities
                // (the card adds the second, `unleash_static`).
                // CR 702.136a: riot, "You may have this permanent enter
                // with an additional +1/+1 counter on it. If you don't, it
                // gains haste."
                Mechanic::Unleash | Mechanic::Riot => AbilityDef::Replacement(ReplacementAbility {
                    replaces: mtg_core::card::Replaces::Enters,
                    replacement: mtg_core::card::Replacement::EnterWithCountersMay {
                        kind: mtg_core::card::CounterKind::PlusOnePlusOne,
                        n: 1,
                        haste: matches!(m, Mechanic::Riot),
                    },
                    then: Vec::new(),
                    condition: None,
                }),
                // CR 702.184a: "Tap another untapped creature you control:
                // Put a number of charge counters on this permanent equal
                // to the tapped creature's power. Activate only as a
                // sorcery."
                // CR 702.44a: "If this object is entering as a creature,
                // ignoring any type-changing effects that would affect it,
                // it enters with a +1/+1 counter on it for each color of
                // mana spent to cast it. Otherwise, it enters with a charge
                // counter on it for each color of mana spent to cast it."
                // The printed card decides which (CR 702.44a's "ignoring
                // type-changing effects"); a permanent not cast spent no
                // mana (CR 702.44b).
                // CR 702.53a: "[Cost], Discard this card: Search your
                // library for a card with the same mana value as the
                // discarded card, reveal that card, and put it into your
                // hand. Then shuffle your library. Activate only as a
                // sorcery." The discarded card's mana value is the
                // printed card's.
                Mechanic::Transmute(cost) => {
                    let mv = u8::try_from(self.card.cost.mana_value()).unwrap_or(u8::MAX);
                    let ability = Ability::Activated {
                        cost: Cost {
                            mana: cost.clone(),
                            discard_self: true,
                            sorcery: true,
                            restrict: vec![crate::ast::Restrict::Sorcery],
                            ..Cost::default()
                        },
                        targets: Vec::new(),
                        effects: vec![Sentence::new(vec![Effect::Search {
                            filter: Filter {
                                mana_value: Some((mtg_core::card::Cmp::Exactly, mv)),
                                ..Filter::default()
                            },
                            count: Num::N(1),
                            up_to: false,
                            to: mtg_core::card::SearchTo::Hand,
                            reveal: true,
                            that_card: false,
                            or_card: crate::ast::OrCard::No,
                            graveyard: false,
                            name: None,
                            who: None,
                            may: false,
                        }])],
                        equip: false,
                        cycling: false,
                    };
                    return self.ability(&ability);
                }
                Mechanic::Sunburst => {
                    let Some(i) = self.sunburst else {
                        return self.err("sunburst's count is missing");
                    };
                    let kind = if self
                        .card
                        .types
                        .contains(&mtg_core::card::CardType::Creature)
                    {
                        mtg_core::card::CounterKind::PlusOnePlusOne
                    } else {
                        mtg_core::card::CounterKind::Charge
                    };
                    AbilityDef::Replacement(ReplacementAbility {
                        replaces: mtg_core::card::Replaces::Enters,
                        replacement: mtg_core::card::Replacement::EnterWithCounters {
                            kind,
                            n: Num::Count(i),
                        },
                        then: Vec::new(),
                        condition: None,
                    })
                }
                Mechanic::Station => {
                    let Some(i) = self.station else {
                        return self.err("station's count is missing");
                    };
                    let mut f = mtg_core::card::ObjFilter::creatures()
                        .who(mtg_core::card::Who::You)
                        .other();
                    f.tapped = Some(false);
                    AbilityDef::Activated(ActivatedAbility {
                        cost: CoreCost {
                            sacrifice: Some(CoreSacCost::Filter(f)),
                            chosen: mtg_core::card::ChosenCost::Tap,
                            ..CoreCost::default()
                        },
                        targets: Vec::new(),
                        effects: vec![CoreEffect::AddCounters {
                            kind: mtg_core::card::CounterKind::Charge,
                            count: Num::Count(i),
                            to: CoreRecipient::Source,
                        }],
                        sorcery_speed: true,
                        per_turn: 0,
                        conditions: Vec::new(),
                        once_ever: false,
                        from_hand: false,
                        cycling: false,
                        from_graveyard: false,
                        keyword: None,
                        reduction: None,
                        text: render::ability_text(self.card, a, self.lookup),
                    })
                }
                // CR 702.167a: "[Cost], Exile this artifact, Exile [the
                // materials] from among permanents you control and/or cards
                // in your graveyard: Return this card transformed under its
                // owner's control. Craft only as a sorcery."
                Mechanic::Craft(f, cost) => {
                    let f = self.filter(f);
                    AbilityDef::Activated(ActivatedAbility {
                        cost: CoreCost {
                            mana: cost.clone(),
                            exile_this: true,
                            craft: Some(Box::new(f)),
                            ..CoreCost::default()
                        },
                        targets: Vec::new(),
                        effects: vec![CoreEffect::ReturnTransformed],
                        sorcery_speed: true,
                        per_turn: 0,
                        conditions: Vec::new(),
                        once_ever: false,
                        from_hand: false,
                        cycling: false,
                        from_graveyard: false,
                        keyword: None,
                        reduction: None,
                        text: render::ability_text(self.card, a, self.lookup),
                    })
                }
                Mechanic::Saddle(n) => AbilityDef::Activated(ActivatedAbility {
                    cost: CoreCost {
                        crew: *n,
                        ..CoreCost::default()
                    },
                    targets: Vec::new(),
                    effects: vec![CoreEffect::BecomeSaddled],
                    sorcery_speed: true,
                    per_turn: 0,
                    conditions: Vec::new(),
                    once_ever: false,
                    from_hand: false,
                    cycling: false,
                    from_graveyard: false,
                    keyword: None,
                    reduction: None,
                    text: render::ability_text(self.card, a, self.lookup),
                }),
                // CR 702.128a, 702.129a: "[Cost], Exile this card from your
                // graveyard: Create a token that's a copy of it, except
                // ... Activate only as a sorcery."
                Mechanic::Embalm(cost) | Mechanic::Eternalize(cost) => {
                    let Some(token) = self.token else {
                        return self.err("the token is missing");
                    };
                    self.keyword_ability(
                        a,
                        CoreCost {
                            mana: cost.clone(),
                            exile_self: true,
                            ..CoreCost::default()
                        },
                        Vec::new(),
                        vec![CoreEffect::CreateToken {
                            token,
                            count: Num::N(1),
                            who: CoreRecipient::You,
                            tapped: false,
                            attached: None,
                        }],
                        None,
                    )
                }
                // CR 702.141a: "[Cost], Exile this card from your
                // graveyard: For each opponent, create a token copy that
                // attacks that opponent this turn if able. They gain haste.
                // Sacrifice them at the beginning of the next end step.
                // Activate only as a sorcery." One token: the game has one
                // opponent, and "attacks this turn if able" is attacking
                // whatever it can.
                Mechanic::Encore(cost) => {
                    let Some(token) = self.token else {
                        return self.err("the token is missing");
                    };
                    let sacrifice =
                        self.pool
                            .register_ability(AbilityDef::Triggered(TriggeredAbility {
                                limit: mtg_core::card::TurnLimit::None,
                                from_graveyard: false,
                                trigger: TriggerSpec::BeginningOfStep {
                                    step: mtg_core::turn::Step::End,
                                    whose: Who::Any,
                                },
                                condition: None,
                                targets: Vec::new(),
                                effects: vec![CoreEffect::Sacrifice {
                                    what: CoreRecipient::SameObject,
                                }],
                            }));
                    self.keyword_ability(
                        a,
                        CoreCost {
                            mana: cost.clone(),
                            exile_self: true,
                            ..CoreCost::default()
                        },
                        Vec::new(),
                        vec![
                            CoreEffect::CreateToken {
                                token,
                                count: Num::N(1),
                                who: CoreRecipient::You,
                                tapped: false,
                                attached: None,
                            },
                            CoreEffect::Grant {
                                keywords: KeywordSet::NONE
                                    .with(mtg_core::card::Keyword::Haste)
                                    .with(mtg_core::card::Keyword::MustAttack),
                                to: CoreRecipient::CreatedTokens,
                            },
                            CoreEffect::Delayed {
                                ability: sacrifice,
                                about: CoreRecipient::CreatedTokens,
                                this_turn: false,
                            },
                        ],
                        None,
                    )
                }
                // CR 702.154a: "As this creature attacks, you may tap up to
                // one untapped creature you control that you didn't choose
                // to attack with and that either has haste or has been
                // under your control continuously since this turn began.
                // When you do, this creature gets +X/+0 until end of turn,
                // where X is the tapped creature's power." Approximated as
                // an attack trigger (the choice is made as it resolves).
                Mechanic::Enlist => AbilityDef::Triggered(TriggeredAbility {
                    limit: mtg_core::card::TurnLimit::None,
                    from_graveyard: false,
                    trigger: TriggerSpec::Attacks(CoreTriggerObj::This),
                    condition: None,
                    targets: Vec::new(),
                    effects: vec![CoreEffect::YouMay(vec![CoreEffect::ChooseAct {
                        act: mtg_core::card::ChooseAct::Enlist,
                        filter: mtg_core::card::ObjFilter::creatures(),
                        count: Num::N(1),
                        up_to: false,
                    }])],
                }),
                // CR 702.115a: "Whenever this creature deals combat damage
                // to a player, that player exiles the top card of their
                // library."
                Mechanic::Ingest => AbilityDef::Triggered(TriggeredAbility {
                    limit: mtg_core::card::TurnLimit::None,
                    from_graveyard: false,
                    trigger: TriggerSpec::DealsCombatDamageToPlayer(CoreTriggerObj::This),
                    condition: None,
                    targets: Vec::new(),
                    effects: vec![CoreEffect::ExileTop {
                        who: CoreRecipient::TriggeringPlayer,
                        count: mtg_core::card::Num::N(1),
                    }],
                }),
                // CR 702.101a: "Whenever you cast a spell, you may pay
                // {W/B}. If you do, each opponent loses 1 life and you gain
                // life equal to the total life lost this way."
                // CR 702.174b: "When this permanent enters, if its gift
                // cost was paid, [effect]."
                Mechanic::Gift(kind) => AbilityDef::Triggered(TriggeredAbility {
                    limit: mtg_core::card::TurnLimit::None,
                    from_graveyard: false,
                    trigger: TriggerSpec::Enters(CoreTriggerObj::This),
                    condition: Some(mtg_core::card::Condition::Kicked),
                    targets: Vec::new(),
                    effects: self.gift_effects(*kind)?,
                }),
                Mechanic::Extort => {
                    let Some(i) = self.extort else {
                        return self.err("extort's count is missing");
                    };
                    let expanded = extort(i);
                    return self.ability(&expanded);
                }
                // "Ward—..." and "Echo—..." are paid as the trigger resolves:
                // mana (waterbend too), life, and discarded cards only. Any
                // other part would be dropped, so it is an error.
                Mechanic::WardCost(cost) | Mechanic::EchoCost(cost)
                    if *cost
                        != crate::ast::Cost {
                            mana: cost.mana.clone(),
                            life: cost.life,
                            discard: cost.discard,
                            waterbend: cost.waterbend,
                            discard_random: cost.discard_random,
                            order: cost.order.clone(),
                            ..Default::default()
                        } =>
                {
                    return self.err(
                        "a ward or echo cost may be only mana (or waterbend), life, and discarded cards",
                    );
                }
                // CR 702.23a: "Whenever this creature becomes blocked, it
                // gets +N/+N until end of turn for each creature blocking it
                // beyond the first." (CR 702.23b: counted as it resolves.)
                Mechanic::Rampage(n) => {
                    let Some(i) = self.rampage else {
                        return self.err("rampage's count is missing");
                    };
                    let n = i32::try_from(*n).unwrap_or(0);
                    let expanded = Ability::Triggered {
                        limit: mtg_core::card::TurnLimit::None,
                        from_graveyard: false,
                        trigger: Trigger::BecomesBlocked(TriggerObj::This),
                        condition: None,
                        targets: Vec::new(),
                        effects: vec![Sentence::new(vec![crate::ast::Effect::PumpVar {
                            power: n,
                            toughness: n,
                            times: Num::Count(i),
                            to: Recipient::Source,
                        }])],
                    };
                    return self.ability(&expanded);
                }
                // CR 702.91a: "Whenever this creature attacks, each other
                // attacking creature gets +1/+0 until end of turn."
                Mechanic::BattleCry => {
                    let Some(g) = self.battle_cry else {
                        return self.err("battle cry's group is missing");
                    };
                    let expanded = Ability::Triggered {
                        limit: mtg_core::card::TurnLimit::None,
                        from_graveyard: false,
                        trigger: Trigger::Attacks(TriggerObj::This),
                        condition: None,
                        targets: Vec::new(),
                        effects: vec![Sentence::new(vec![crate::ast::Effect::Pump {
                            power: 1,
                            toughness: 0,
                            to: Recipient::Group(g),
                        }])],
                    };
                    return self.ability(&expanded);
                }
                other => {
                    let expanded = expand_mechanic(other);
                    return self.ability(&expanded);
                }
            },
            Ability::HandSize { whose, delta } => AbilityDef::HandSize {
                whose: *whose,
                delta: *delta,
            },
            // CR 721.2a-b: "As long as this permanent has N or more charge
            // counters on it, it has [abilities] and is a creature with
            // base power and toughness [P/T]" (the card's own P/T).
            // CR 716.2a: "{R}{G}: Level 2" is activated only as a sorcery,
            // and only while the Class is level 1: it is an ability of the
            // level 1 stage alone.
            Ability::ClassLevel { level, cost } => AbilityDef::Activated(ActivatedAbility {
                cost: CoreCost {
                    mana: cost.clone(),
                    ..CoreCost::default()
                },
                targets: Vec::new(),
                effects: vec![CoreEffect::BecomeStage(level - 1)],
                sorcery_speed: true,
                per_turn: 0,
                conditions: Vec::new(),
                once_ever: false,
                from_hand: false,
                cycling: false,
                from_graveyard: false,
                keyword: None,
                reduction: None,
                text: render::ability_text(self.card, a, self.lookup),
            }),
            // CR 719.2a: "At the beginning of your end step, if <condition>
            // and this Case is not solved, it becomes solved." It is an
            // ability of the unsolved stage alone.
            Ability::ToSolve(cond) => {
                let c = self.static_cond(cond);
                let i = self.pool.intern_condition(c);
                AbilityDef::Triggered(mtg_core::card::TriggeredAbility {
                    limit: mtg_core::card::TurnLimit::None,
                    trigger: TriggerSpec::BeginningOfStep {
                        step: mtg_core::turn::Step::End,
                        whose: mtg_core::card::Who::You,
                    },
                    condition: Some(mtg_core::card::Condition::Static(i)),
                    targets: Vec::new(),
                    effects: vec![CoreEffect::BecomeStage(1)],
                    from_graveyard: false,
                })
            }
            Ability::Solved(inner) => return self.ability(inner),
            Ability::StationLevel { n, keywords } => {
                let mut mods = Vec::new();
                if self.card.pt.is_some() {
                    mods.push(Mod::AddTypes(vec![mtg_core::card::CardType::Creature]));
                }
                mods.push(Mod::AddKeywords(keywords.clone()));
                return self.ability(&Ability::Static {
                    filter: crate::ast::Filter::this(),
                    mods,
                    condition: Some(StaticCond::SourceCounters {
                        kind: mtg_core::card::CounterKind::Charge,
                        n: *n,
                        at_least: true,
                    }),
                    cond_first: false,
                    cond_if: false,
                });
            }
            Ability::ExtraVote { optional } => AbilityDef::ExtraVote {
                optional: *optional,
            },
            Ability::AsTurnedFaceUp { kind, n, .. } => {
                AbilityDef::AsTurnedFaceUp { kind: *kind, n: *n }
            }
            Ability::PlayFromTop {
                lands,
                spells,
                once,
            } => AbilityDef::PlayFromTop {
                lands: *lands,
                spells: spells.as_ref().map(|f| self.filter(f)),
                once: *once,
            },
            Ability::PlayExiled {
                lands,
                spells,
                your_turn,
                owned,
            } => AbilityDef::PlayExiled {
                lands: *lands,
                spells: spells.as_ref().map(|f| self.filter(f)),
                your_turn: *your_turn,
                owned: *owned,
            },
            Ability::TopRevealed { players } => AbilityDef::TopRevealed { players: *players },
            Ability::MayLook {
                top,
                face_down,
                opponents,
            } => AbilityDef::MayLook {
                top: *top,
                face_down: *face_down,
                opponents: *opponents,
            },
            Ability::CrewsAs {
                saddles,
                toughness,
                plus,
            } => AbilityDef::CrewsAs {
                saddles: *saddles,
                toughness: *toughness,
                plus: *plus,
            },
            Ability::SelfCost { .. }
            | Ability::TargetCost { .. }
            | Ability::CondCost { .. }
            | Ability::Affinity(_) => {
                return self.err("a cost reduction of the spell itself is not an ability");
            }
        })
    }
}

/// The token Fabricate creates (CR 702.123a).
const SERVO: &str = "1/1 colorless Servo artifact creature";
/// The token afterlife creates (CR 702.135a).
const AFTERLIFE_SPIRIT: &str = "1/1 white and black Spirit creature with flying";
/// The token living weapon creates (CR 702.92a).
const GERM: &str = "0/0 black Phyrexian Germ creature";
/// The token job select creates (CR 702.182a).
const HERO: &str = "1/1 colorless Hero creature";
/// The token For Mirrodin! creates (CR 702.163a).
const REBEL: &str = "2/2 red Rebel creature";
/// The token recruit creates (CR 701.70a).
const HUMAN_SOLDIER: &str = "1/1 white Human Soldier creature";
/// The token Incubate creates (CR 701.53a, 111.10i).
const INCUBATOR: &str = "Incubator";

/// Soulbond's two triggers (CR 702.95a): as this creature enters
/// (`triggering` false: pair it with a creature chosen), and as another
/// creature its controller controls enters (pair it with that one).
fn soulbond_trigger(triggering: bool) -> Ability {
    let trigger = if triggering {
        Trigger::Enters(TriggerObj::Filter(Filter {
            types: vec![mtg_core::card::CardType::Creature],
            who: Some(Who::You),
            relation: mtg_core::card::Relation::Other,
            ..Default::default()
        }))
    } else {
        Trigger::Enters(TriggerObj::This)
    };
    Ability::Triggered {
        limit: mtg_core::card::TurnLimit::None,
        from_graveyard: false,
        trigger,
        condition: None,
        targets: Vec::new(),
        effects: vec![Sentence::new(vec![Effect::Soulbond { triggering }])],
    }
}

/// Unleash's "This permanent can't block as long as it has a +1/+1 counter
/// on it." (CR 702.98a).
/// An equip ability for this cost (CR 702.6a): attach to the target as a
/// sorcery.
fn equip_ability(cost: Cost, targets: &[Target]) -> Ability {
    Ability::Activated {
        cost: Cost {
            sorcery: true,
            ..cost
        },
        targets: targets.to_vec(),
        effects: vec![Sentence::new(vec![crate::ast::Effect::Attach {
            what: Recipient::Source,
            to: Recipient::Target(0),
        }])],
        equip: true,
        cycling: false,
    }
}

pub(crate) fn unleash_static() -> AbilityDef {
    AbilityDef::Static(StaticAbility {
        affected: ObjFilter::this(),
        parts: vec![Modification::AddKeywords(KeywordSet::of([
            mtg_core::card::Keyword::CantBlock,
        ]))],
        condition: Some(StaticCondition::SourceCounters {
            kind: mtg_core::card::CounterKind::PlusOnePlusOne,
            n: 1,
            at_least: true,
        }),
    })
}

/// Extort's triggered ability (CR 702.101a), `i` the card's count of the
/// life lost this way.
fn extort(i: u8) -> Ability {
    use crate::ast::{Effect as E, Join};
    let pay = E::YouMay(vec![Sentence::new(vec![E::Pay {
        mana: mtg_core::mana::ManaCost::parse("{W/B}").expect("a mana cost"),
        life: 0,
    }])]);
    let drain = Sentence {
        effects: vec![
            E::LoseLife {
                amount: Num::N(1),
                who: Recipient::EachOpponent,
            },
            E::GainLife {
                amount: Num::Count(i),
                who: Recipient::You,
            },
        ],
        label: None,
        para: false,
        eot_first: false,
        lasting: None,
        then_first: false,
        joins: vec![Join::And],
    };
    Ability::Triggered {
        limit: mtg_core::card::TurnLimit::None,
        from_graveyard: false,
        trigger: Trigger::CastsSpell {
            whose: Who::You,
            noncreature: false,
            instant_or_sorcery: false,
            or_copy: false,
        },
        condition: None,
        targets: Vec::new(),
        effects: vec![
            Sentence::new(vec![pay]),
            Sentence::new(vec![E::IfYouDo(vec![drain])]),
        ],
    }
}

/// What a keyword ability with a parameter stands for, as an ordinary
/// ability (CR 702).
#[allow(clippy::too_many_lines)] // one arm per mechanic
fn expand_mechanic(m: &Mechanic) -> Ability {
    use crate::ast::{Effect as E, Join};
    let one = |e: E| vec![Sentence::new(vec![e])];
    match m {
        // CR 702.100a: "Whenever a creature you control enters, if that
        // creature's power is greater than this creature's power and/or
        // that creature's toughness is greater than this creature's
        // toughness, put a +1/+1 counter on this creature."
        // CR 702.191a: "Whenever you cast a spell, if the amount of mana
        // spent to cast that spell is greater than this creature's power or
        // this creature's toughness, put a +1/+1 counter on this creature."
        Mechanic::Increment => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::CastsSpell {
                whose: Who::You,
                noncreature: false,
                instant_or_sorcery: false,
                or_copy: false,
            },
            condition: Some(crate::ast::TrigCond::Core(
                mtg_core::card::Condition::Increments,
            )),
            targets: Vec::new(),
            effects: one(E::AddCounters {
                kind: mtg_core::card::CounterKind::PlusOnePlusOne,
                count: Num::N(1),
                to: Recipient::Source,
            }),
        },
        Mechanic::Evolve => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::Enters(TriggerObj::Filter(Filter {
                types: vec![mtg_core::card::CardType::Creature],
                who: Some(Who::You),
                ..Filter::default()
            })),
            condition: Some(crate::ast::TrigCond::Core(
                mtg_core::card::Condition::Evolves,
            )),
            targets: Vec::new(),
            effects: one(E::AddCounters {
                kind: mtg_core::card::CounterKind::PlusOnePlusOne,
                count: Num::N(1),
                to: Recipient::Source,
            }),
        },
        // CR 702.181a: "Whenever this creature attacks, create N tapped
        // and attacking 1/1 red Warrior creature tokens. Sacrifice them at
        // the beginning of the next end step."
        Mechanic::Mobilize(n) => {
            let mut cards = crate::parse(MOBILIZE).expect("mobilize's expansion parses");
            let mut a = cards.remove(0).abilities.remove(0);
            if let Ability::Triggered { effects, .. } = &mut a {
                if let Some(E::CreateToken { count, .. }) = effects[0].effects.first_mut() {
                    *count = *n;
                }
                effects.insert(1, Sentence::new(vec![E::CreatedAttack]));
            }
            a
        }
        // CR 702.135a: "When this creature dies, create N 1/1 white and
        // black Spirit creature tokens with flying."
        Mechanic::Afterlife(n) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::Dies(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: one(E::CreateToken {
                token: AFTERLIFE_SPIRIT.to_string(),
                count: Num::N(*n),
                tapped: false,
                attacking: false,
                who: None,
                it_has: false,
                attached: None,
                sized: None,
            }),
        },
        // CR 702.134a: "Whenever this creature attacks, put a +1/+1
        // counter on target attacking creature with lesser power."
        Mechanic::Mentor => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::Attacks(TriggerObj::This),
            condition: None,
            targets: vec![Target::Filter(Filter {
                types: vec![mtg_core::card::CardType::Creature],
                attacking: true,
                same: Some(crate::ast::SameRef::LesserPower),
                ..Filter::default()
            })],
            effects: one(E::AddCounters {
                kind: mtg_core::card::CounterKind::PlusOnePlusOne,
                count: Num::N(1),
                to: Recipient::Target(0),
            }),
        },
        // CR 702.92a: "When this Equipment enters, create a 0/0 black
        // Phyrexian Germ creature token, then attach this Equipment to it."
        // CR 702.182a: job select likewise, a "1/1 colorless Hero creature".
        Mechanic::LivingWeapon | Mechanic::JobSelect => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::Enters(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: vec![Sentence {
                effects: vec![
                    E::CreateToken {
                        token: if matches!(m, Mechanic::JobSelect) {
                            HERO
                        } else {
                            GERM
                        }
                        .to_string(),
                        count: Num::N(1),
                        tapped: false,
                        attacking: false,
                        who: None,
                        it_has: false,
                        attached: None,
                        sized: None,
                    },
                    E::Attach {
                        what: Recipient::Source,
                        to: Recipient::CreatedObject,
                    },
                ],
                label: None,
                para: false,
                eot_first: false,
                lasting: None,
                then_first: false,
                joins: vec![Join::Then],
            }],
        },
        // CR 702.77a: "[Cost], Discard this card: Put N +1/+1 counters on
        // target creature."
        Mechanic::Reinforce(n, cost) => Ability::Activated {
            cost: Cost {
                mana: cost.clone(),
                discard_self: true,
                ..Default::default()
            },
            targets: vec![Target::Filter(Filter {
                types: vec![mtg_core::card::CardType::Creature],
                ..Filter::default()
            })],
            effects: one(E::AddCounters {
                kind: mtg_core::card::CounterKind::PlusOnePlusOne,
                count: Num::N(*n),
                to: Recipient::Target(0),
            }),
            equip: false,
            cycling: false,
        },
        Mechanic::Bushido(n) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::BlocksOrBlocked(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: one(E::Pump {
                power: i32::try_from(*n).unwrap_or(0),
                toughness: i32::try_from(*n).unwrap_or(0),
                to: Recipient::Source,
            }),
        },
        // CR 702.70a: "Whenever this creature deals combat damage to a
        // player, that player gets N poison counters."
        Mechanic::Poisonous(n) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::DealsCombatDamageToPlayer(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: one(E::AddPoison {
                count: Num::N(*n),
                who: Recipient::TriggeringPlayer,
            }),
        },
        // CR 702.68a: "Whenever this creature attacks and isn't blocked,
        // it gets +N/+0 until end of turn."
        Mechanic::Frenzy(n) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::AttacksUnblocked(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: one(E::Pump {
                power: i32::try_from(*n).unwrap_or(0),
                toughness: 0,
                to: Recipient::Source,
            }),
        },
        // CR 702.64a: "If a source would deal damage to this creature,
        // prevent N of that damage." A static `prevent next N` prevents N
        // of each event (see `Replacement::PreventNext`).
        Mechanic::Absorb(n) => Ability::Replacement(crate::ast::Replacement {
            event: crate::ast::Replaces::Damage {
                from: None,
                to: crate::ast::Affects::This,
                combat: None,
                wording: crate::ast::DamageWording::default(),
            },
            outcome: mtg_core::card::Replacement::PreventNext(Num::N(*n)),
            then: Vec::new(),
            token: None,
        }),
        // CR 702.39a: "Whenever this creature attacks, you may have target
        // creature defending player controls untap and block this creature
        // this turn if able."
        Mechanic::Provoke => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::Attacks(TriggerObj::This),
            condition: None,
            targets: vec![Target::Filter(Filter {
                types: vec![mtg_core::card::CardType::Creature],
                who: Some(Who::Opponents),
                defending: true,
                ..Filter::default()
            })],
            effects: one(E::YouMay(vec![Sentence::new(vec![
                E::Untap {
                    to: vec![Recipient::Target(0)],
                },
                E::BlocksIfAble {
                    blocker: Recipient::Target(0),
                    attacker: Recipient::Source,
                },
            ])])),
        },
        // CR 702.189a: "Whenever this creature attacks, add N {R}. Until
        // end of combat, you don't lose this mana as steps and phases end."
        Mechanic::Firebending(n) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::Attacks(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: one(E::Firebend(*n)),
        },
        Mechanic::Afflict(n) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::BecomesBlocked(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: one(E::LoseLife {
                amount: Num::N(*n),
                who: Recipient::DefendingPlayer,
            }),
        },
        // CR 702.43a: "When this dies, you may put its +1/+1 counters on
        // target artifact creature." (The counters it enters with are the
        // card's `modular`.)
        Mechanic::Modular(_) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::Dies(TriggerObj::This),
            condition: None,
            targets: vec![Target::Filter(Filter {
                types: vec![
                    mtg_core::card::CardType::Artifact,
                    mtg_core::card::CardType::Creature,
                ],
                ..Default::default()
            })],
            effects: one(E::YouMay(one(E::MoveSourceCounters {
                to: Recipient::Target(0),
            }))),
        },
        // CR 702.58a: "Whenever another creature enters, if this permanent
        // has a +1/+1 counter on it, you may move a +1/+1 counter from this
        // permanent onto that creature." (The counters it enters with are
        // the card's `graft`.)
        Mechanic::Graft(_) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::Enters(TriggerObj::Filter(Filter {
                types: vec![mtg_core::card::CardType::Creature],
                relation: mtg_core::card::Relation::Other,
                ..Default::default()
            })),
            condition: Some(crate::ast::TrigCond::Static(StaticCond::SourceCounters {
                kind: mtg_core::card::CounterKind::PlusOnePlusOne,
                n: 1,
                at_least: true,
            })),
            targets: Vec::new(),
            effects: one(E::YouMay(one(E::MoveCounters {
                from: Recipient::Source,
                to: Recipient::TriggeringIt,
                kind: Some(mtg_core::card::CounterKind::PlusOnePlusOne),
                count: Some(1),
            }))),
        },
        // CR 702.63a: "At the beginning of your upkeep, if this permanent
        // has a time counter on it, remove a time counter from it" and
        // "When the last time counter is removed from this permanent,
        // sacrifice it." The sacrifice is part of the upkeep trigger here
        // rather than a trigger of its own: it happens as the last counter
        // is removed this way (not when another effect removes it).
        Mechanic::Vanishing(_) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::BeginningOfStep {
                step: mtg_core::turn::Step::Upkeep,
                whose: Who::You,
            },
            condition: Some(crate::ast::TrigCond::Static(StaticCond::SourceCounters {
                kind: mtg_core::card::CounterKind::Time,
                n: 1,
                at_least: true,
            })),
            targets: Vec::new(),
            effects: vec![
                Sentence::new(vec![E::RemoveCounters {
                    kind: mtg_core::card::CounterKind::Time,
                    count: Num::N(1),
                    to: Recipient::Source,
                }]),
                Sentence::new(vec![E::IfCondition(
                    StaticCond::SourceCounters {
                        kind: mtg_core::card::CounterKind::Time,
                        n: 0,
                        at_least: false,
                    },
                    one(E::Sacrifice {
                        what: Recipient::Source,
                    }),
                    crate::ast::IfWording::Lead,
                )]),
            ],
        },
        Mechanic::Soulbond => soulbond_trigger(false),
        // CR 702.32a: "At the beginning of your upkeep, remove a fade
        // counter from this permanent. If you can't, sacrifice the
        // permanent." (The counters it enters with are the card's
        // `fading`.)
        Mechanic::Fading(_) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::BeginningOfStep {
                step: mtg_core::turn::Step::Upkeep,
                whose: Who::You,
            },
            condition: None,
            targets: Vec::new(),
            effects: vec![
                Sentence::new(vec![E::IfCondition(
                    StaticCond::SourceCounters {
                        kind: mtg_core::card::CounterKind::Fade,
                        n: 0,
                        at_least: false,
                    },
                    one(E::Sacrifice {
                        what: Recipient::Source,
                    }),
                    crate::ast::IfWording::Lead,
                )]),
                Sentence::new(vec![E::RemoveCounters {
                    kind: mtg_core::card::CounterKind::Fade,
                    count: Num::N(1),
                    to: Recipient::Source,
                }]),
            ],
        },
        // CR 702.86a: "Whenever this creature attacks, defending player
        // sacrifices N permanents of their choice."
        Mechanic::Annihilator(n) => keyword_expansion(0, *n, None),
        // CR 702.59a: "When a creature is put into your graveyard from the
        // battlefield, you may pay [cost]. If you do, return this card
        // from your graveyard to your hand. Otherwise, exile this card."
        Mechanic::Recover(c) => keyword_expansion(1, 0, Some(c)),
        // CR 702.149a: "Whenever this creature and at least one other
        // creature with power greater than this creature's power attack,
        // put a +1/+1 counter on this creature."
        Mechanic::Training => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::Attacks(TriggerObj::This),
            condition: Some(crate::ast::TrigCond::Core(
                mtg_core::card::Condition::Trains,
            )),
            targets: Vec::new(),
            effects: one(E::AddCounters {
                kind: mtg_core::card::CounterKind::PlusOnePlusOne,
                count: Num::N(1),
                to: Recipient::Source,
            }),
        },
        // CR 702.105a: "Whenever this creature attacks the player with the
        // most life or tied for most life, put a +1/+1 counter on this
        // creature."
        Mechanic::Dethrone => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::Attacks(TriggerObj::This),
            condition: Some(crate::ast::TrigCond::Core(
                mtg_core::card::Condition::Dethrones,
            )),
            targets: Vec::new(),
            effects: one(E::AddCounters {
                kind: mtg_core::card::CounterKind::PlusOnePlusOne,
                count: Num::N(1),
                to: Recipient::Source,
            }),
        },
        // CR 702.121a: "Whenever this creature attacks, it gets +1/+1
        // until end of turn for each opponent you attacked with a creature
        // this combat." With one opponent, +1/+1 if that player was
        // attacked.
        Mechanic::Melee => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::Attacks(TriggerObj::This),
            condition: Some(crate::ast::TrigCond::Core(
                mtg_core::card::Condition::AttackedOpponent,
            )),
            targets: Vec::new(),
            effects: one(E::Pump {
                power: 1,
                toughness: 1,
                to: Recipient::Source,
            }),
        },
        // CR 702.163a: "When this Equipment enters, create a 2/2 red Rebel
        // creature token, then attach this Equipment to it."
        Mechanic::ForMirrodin => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::Enters(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: vec![Sentence {
                effects: vec![
                    E::CreateToken {
                        token: REBEL.to_string(),
                        count: Num::N(1),
                        tapped: false,
                        attacking: false,
                        who: None,
                        it_has: false,
                        attached: None,
                        sized: None,
                    },
                    E::Attach {
                        what: Recipient::Source,
                        to: Recipient::CreatedObject,
                    },
                ],
                label: None,
                para: false,
                eot_first: false,
                lasting: None,
                then_first: false,
                joins: vec![Join::Then],
            }],
        },
        // CR 702.176a: "At the beginning of your end step, if this
        // permanent's impending cost was paid and it has a time counter on
        // it, remove a time counter from it."
        Mechanic::Impending(..) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::BeginningOfStep {
                step: mtg_core::turn::Step::End,
                whose: Who::You,
            },
            condition: Some(crate::ast::TrigCond::Static(StaticCond::Impending)),
            targets: Vec::new(),
            effects: vec![Sentence::new(vec![E::RemoveCounters {
                kind: mtg_core::card::CounterKind::Time,
                count: Num::N(1),
                to: Recipient::Source,
            }])],
        },
        // CR 702.24a.
        Mechanic::CumulativeUpkeep(cost) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::BeginningOfStep {
                step: mtg_core::turn::Step::Upkeep,
                whose: Who::You,
            },
            condition: None,
            targets: Vec::new(),
            effects: one(E::CumulativeUpkeep(cost.clone())),
        },
        Mechanic::Soulshift(n) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::Dies(TriggerObj::This),
            condition: None,
            targets: vec![Target::GraveyardCard {
                filter: Filter {
                    subtypes: vec!["Spirit".to_string()],
                    mana_value: Some((
                        mtg_core::card::Cmp::AtMost,
                        u8::try_from(*n).unwrap_or(u8::MAX),
                    )),
                    ..Default::default()
                },
                who: Who::You,
                graveyards: false,
            }],
            effects: one(E::YouMay(one(E::ReturnToHand {
                what: vec![Recipient::Target(0)],
                owner_hand: false,
            }))),
        },
        Mechanic::Ward(cost) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::TargetedByOpponent,
            condition: None,
            targets: Vec::new(),
            effects: one(E::CounterUnlessPays {
                to: Recipient::TriggeringObject,
                mana: cost.clone(),
                x: Num::N(0),
                dest: crate::ast::CounteredTo::Graveyard,
            }),
        },
        // "Ward—Pay 3 life." (CR 702.21a).
        Mechanic::WardCost(cost) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            trigger: Trigger::TargetedByOpponent,
            condition: None,
            from_graveyard: false,
            targets: Vec::new(),
            effects: one(E::UnlessPays {
                payer: Recipient::TriggeringController { that: false },
                mana: cost.mana.clone(),
                life: cost.life,
                discard: cost.discard,
                waterbend: cost.waterbend,
                discard_random: cost.discard_random,
                effect: Box::new(E::Counter {
                    to: Recipient::TriggeringObject,
                    dest: crate::ast::CounteredTo::Graveyard,
                }),
            }),
        },
        Mechanic::Echo(cost) => expand_mechanic(&Mechanic::EchoCost(crate::ast::Cost {
            mana: cost.clone(),
            ..Default::default()
        })),
        // CR 702.30a: "Echo—Discard a card."
        Mechanic::EchoCost(cost) => Ability::Triggered {
            limit: mtg_core::card::TurnLimit::None,
            from_graveyard: false,
            trigger: Trigger::BeginningOfStep {
                step: mtg_core::turn::Step::Upkeep,
                whose: Who::You,
            },
            condition: Some(crate::ast::TrigCond::Core(
                mtg_core::card::Condition::EchoDue,
            )),
            targets: Vec::new(),
            effects: vec![Sentence {
                effects: vec![
                    E::ClearEcho,
                    E::UnlessPays {
                        payer: Recipient::You,
                        mana: cost.mana.clone(),
                        life: cost.life,
                        discard: cost.discard,
                        waterbend: cost.waterbend,
                        discard_random: cost.discard_random,
                        effect: Box::new(E::Sacrifice {
                            what: Recipient::Source,
                        }),
                    },
                ],
                label: None,
                para: false,
                eot_first: false,
                lasting: None,
                then_first: false,
                joins: vec![Join::Then],
            }],
        },
        Mechanic::Unearth(cost) => Ability::Activated {
            cost: Cost {
                mana: cost.clone(),
                sorcery: true,
                from_graveyard: true,
                ..Default::default()
            },
            targets: Vec::new(),
            effects: vec![Sentence {
                effects: vec![
                    E::ReturnToBattlefield {
                        what: Recipient::Source,
                        counters: None,
                        tapped: false,
                        under_you: false,
                        attached: None,
                        put: false,
                        attacking: false,
                        additional: false,
                    },
                    E::Grant {
                        keywords: vec![mtg_core::card::Keyword::Haste],
                        to: Recipient::Source,
                        each: false,
                    },
                    E::Delayed {
                        about: Recipient::Source,
                        trigger: Trigger::BeginningOfStep {
                            step: mtg_core::turn::Step::End,
                            whose: Who::Any,
                        },
                        effects: one(E::Exile {
                            what: vec![Recipient::SameObject],
                        }),
                        trigger_first: false,
                        this_turn: false,
                        it: false,
                    },
                ],
                label: None,
                para: false,
                eot_first: false,
                lasting: None,
                then_first: false,
                joins: vec![Join::And, Join::And],
            }],
            equip: false,
            cycling: false,
        },
        Mechanic::Crew(n) => {
            // The Vehicle keeps its printed power and toughness (CR
            // 301.7a).
            Ability::Activated {
                cost: Cost {
                    crew: *n,
                    ..Default::default()
                },
                targets: Vec::new(),
                effects: one(E::Animate {
                    to: Recipient::Source,
                    types: vec![
                        mtg_core::card::CardType::Artifact,
                        mtg_core::card::CardType::Creature,
                    ],
                    subtypes: Vec::new(),
                    pt: None,
                    base: false,
                    colors: Vec::new(),
                    keywords: Vec::new(),
                    indefinite: false,
                    loses_abilities: false,
                    in_addition: false,
                }),
                equip: false,
                cycling: false,
            }
        }
        // CR 702.87a: "[Cost]: Put a level counter on this permanent.
        // Activate only as a sorcery."
        Mechanic::LevelUp(cost) => Ability::Activated {
            cost: Cost {
                mana: cost.clone(),
                sorcery: true,
                ..Default::default()
            },
            targets: Vec::new(),
            effects: one(E::AddCounters {
                kind: mtg_core::card::CounterKind::Level,
                count: Num::N(1),
                to: Recipient::Source,
            }),
            equip: false,
            cycling: false,
        },
        Mechanic::Outlast(cost) => Ability::Activated {
            cost: Cost {
                mana: cost.clone(),
                tap: true,
                sorcery: true,
                ..Default::default()
            },
            targets: Vec::new(),
            effects: one(E::AddCounters {
                kind: mtg_core::card::CounterKind::PlusOnePlusOne,
                count: Num::N(1),
                to: Recipient::Source,
            }),
            equip: false,
            cycling: false,
        },
        Mechanic::Toxic(_)
        | Mechanic::Bloodthirst(_)
        | Mechanic::Evoke(_)
        | Mechanic::Madness(_)
        | Mechanic::Mayhem(_)
        | Mechanic::Foretell(_)
        | Mechanic::Plot(_)
        | Mechanic::Gift(_)
        | Mechanic::Sneak(_)
        | Mechanic::Harmonize(_)
        | Mechanic::WebSlinging(_)
        | Mechanic::Suspend(..)
        | Mechanic::Entwine(_)
        | Mechanic::Escape(..)
        | Mechanic::EscapesWith(_)
        | Mechanic::CounterChoice(..)
        | Mechanic::KickedWith(..)
        | Mechanic::Buyback(_)
        | Mechanic::BuybackCost(_)
        | Mechanic::MorphCost(_)
        | Mechanic::Morph(_)
        | Mechanic::Megamorph(_)
        | Mechanic::Disguise(_)
        | Mechanic::Ninjutsu(_)
        | Mechanic::Scavenge(_)
        | Mechanic::Dash(_)
        | Mechanic::Warp(_)
        | Mechanic::Bestow(_)
        | Mechanic::Overload(_)
        | Mechanic::Splice(_)
        | Mechanic::SpliceCost(_)
        | Mechanic::Embalm(_)
        | Mechanic::Encore(_)
        | Mechanic::Craft(..)
        | Mechanic::Eternalize(_)
        | Mechanic::Backup(_)
        | Mechanic::Offspring(_)
        | Mechanic::Strive(_)
        | Mechanic::Bargain
        | Mechanic::Conspire
        | Mechanic::Casualty(_)
        | Mechanic::Dredge(_)
        | Mechanic::Storm
        | Mechanic::Awaken(..)
        | Mechanic::Assist
        | Mechanic::Myriad
        | Mechanic::Prototype(..)
        | Mechanic::Blitz(_)
        | Mechanic::Escalate(_)
        | Mechanic::Prowl(_)
        | Mechanic::Miracle(_)
        | Mechanic::Emerge(_)
        | Mechanic::BattleCry
        | Mechanic::Rampage(_)
        | Mechanic::Replicate(_)
        | Mechanic::Multikicker(_)
        | Mechanic::Disturb(_)
        | Mechanic::GraveyardExile
        | Mechanic::Teamwork(_)
        | Mechanic::Ripple(_)
        | Mechanic::Ascend
        | Mechanic::Storied
        | Mechanic::StartYourEngines
        | Mechanic::BecomesDay
        | Mechanic::Daybound
        | Mechanic::Nightbound
        | Mechanic::Fabricate(_)
        | Mechanic::Renown(_)
        | Mechanic::Unleash
        | Mechanic::Station
        | Mechanic::Transmute(_)
        | Mechanic::Sunburst
        | Mechanic::Extort
        | Mechanic::Riot
        | Mechanic::Retrace
        | Mechanic::Ingest
        | Mechanic::Enlist
        | Mechanic::JumpStart
        | Mechanic::Surge(_)
        | Mechanic::Freerunning(_)
        | Mechanic::Spectacle(_)
        | Mechanic::Saddle(_) => unreachable!("not an expansion"),
    }
}

/// The token embalm or eternalize (CR 702.128a, 702.129a) makes of the
/// card: a copy of it, except a white (black) Zombie with no mana cost
/// (eternalize: 4/4). The token's copiable values are these (CR 707.9b),
/// so it is a definition of its own, keyed `"<name> (token)"`.
/// CR 607.2a: when one of a card's abilities says "the exiled card"
/// (Journey to Nowhere's "When this enchantment leaves the battlefield,
/// return the exiled card ..."), the card's triggered abilities that exile
/// are linked to it: they remember what they exiled.
fn link_exiles(def: &mut mtg_core::card::CardDef) {
    use mtg_core::card::AbilityDef;
    // "cards exiled with this artifact", "cards exiled with Rona": every
    // card its abilities exile is linked to the permanent (the engine
    // records them as the abilities resolve).
    if def.oracle_text.contains("exiled with") {
        def.links_exile = true;
    }
    let refers = def.abilities.iter().any(|a| {
        a.all_effects().iter().any(|e| {
            matches!(
                e,
                CoreEffect::ReturnToBattlefield {
                    what: CoreRecipient::ExiledCard,
                    ..
                }
            )
        })
    });
    if !refers {
        return;
    }
    for a in &mut def.abilities {
        if let AbilityDef::Triggered(t) = a {
            for e in &mut t.effects {
                if let CoreEffect::Exile { what } = *e {
                    *e = CoreEffect::ExileLinked { what, until: false };
                }
            }
        }
    }
}

fn derived_token(card: &Card) -> Option<Card> {
    let (color, pt) = card.abilities.iter().find_map(|a| match a {
        Ability::Mechanic(Mechanic::Embalm(_)) => Some((Some(Color::White), None)),
        Ability::Mechanic(Mechanic::Encore(_)) => Some((None, None)),
        Ability::Mechanic(Mechanic::Eternalize(_)) => Some((Some(Color::Black), Some((4, 4)))),
        Ability::Mechanic(Mechanic::Offspring(_)) => Some((None, Some((1, 1)))),
        _ => None,
    })?;
    let mut t = card.clone();
    t.kind = Kind::Token;
    t.display_name = Some(card.name.clone());
    t.name = format!("{} (token)", card.name);
    t.oracle_id = None;
    if let Some(color) = color {
        t.cost = mtg_core::mana::ManaCost::default();
        t.color_indicator = Some(vec![color]);
        if !t.subtypes.iter().any(|s| s == "Zombie") {
            t.subtypes.insert(0, "Zombie".to_string());
        }
    }
    if pt.is_some() {
        t.pt = pt;
    }
    // A token in a graveyard ceases to exist (CR 704.5d), and a token copy
    // was never cast with offspring: the keyword that made it does nothing
    // on it.
    t.abilities.retain(|a| {
        !matches!(
            a,
            Ability::Mechanic(
                Mechanic::Embalm(_)
                    | Mechanic::Eternalize(_)
                    | Mechanic::Offspring(_)
                    | Mechanic::Encore(_)
            )
        )
    });
    Some(t)
}

/// An ability without its ability word.
fn unlabeled(a: &Ability) -> &Ability {
    match a {
        Ability::Labeled { ability, .. } => unlabeled(ability),
        other => other,
    }
}

/// Split multi-choice mana abilities so each compiles to one
/// `TapForMana`.
pub fn split_mana(card: &mut Card) {
    let mut out = Vec::with_capacity(card.abilities.len());
    for a in card.abilities.drain(..) {
        match a {
            Ability::Mana(m) if m.len() > 1 => {
                for one in m {
                    out.push(Ability::Mana(vec![one]));
                }
            }
            other => out.push(other),
        }
    }
    card.abilities = out;
    // A face's own ("Old-Growth Grove": "{T}: Add {B} or {G}.").
    for f in &mut card.faces {
        split_mana(f);
    }
}

impl From<Mana> for Ability {
    fn from(m: Mana) -> Self {
        Ability::Mana(vec![m])
    }
}

/// `deal 4 damage instead`: damage without a recipient goes where the
/// sentence before dealt its damage ("it deals 4 damage instead").
fn same_damage_recipients(effects: &[Sentence], before: &Sentence) -> Vec<Sentence> {
    let earlier = before.effects.iter().find_map(|e| match e {
        Effect::DealDamage { to, .. } => Some(to.clone()),
        _ => None,
    });
    let mut out = effects.to_vec();
    if let Some(earlier) = earlier {
        for s in &mut out {
            for e in &mut s.effects {
                if let Effect::DealDamage { to, .. } = e
                    && to.is_empty()
                {
                    to.clone_from(&earlier);
                }
            }
        }
    }
    out
}

/// "historic" (CR 700.6): an artifact, a legendary, or a Saga, one
/// alternative each; `None` for a filter without the word.
fn historic(f: &Filter) -> Option<Filter> {
    let i = f.subtypes.iter().position(|s| s == crate::ast::HISTORIC)?;
    let mut base = f.clone();
    base.subtypes.remove(i);
    let mut saga = base.clone();
    saga.subtypes.push("Saga".to_string());
    let mut legendary = base.clone();
    legendary
        .supertypes
        .push(mtg_core::card::Supertype::Legendary);
    legendary.or = Some(Box::new(saga));
    let mut artifact = base;
    artifact.types.push(mtg_core::card::CardType::Artifact);
    artifact.or = Some(Box::new(legendary));
    Some(artifact)
}

/// How the ability's targets as written map to the engine's.
#[derive(Copy, Clone, Default)]
struct TargetMap {
    /// The variable group's index and the number of targets, when it
    /// isn't the last target: the engine takes a variable group's targets
    /// to the end of the list, so it is compiled last and the targets
    /// after it move up one.
    group: Option<(usize, usize)>,
    /// Inside a kicked spell's "instead" with its own targets: how many
    /// targets the kicked spell doesn't have (`CardDef::kicked_drops`).
    shift: usize,
}

/// The engine's index of the ability's `i`th target (`TargetMap`).
fn target_index(tmap: TargetMap, i: usize) -> usize {
    let i = match tmap.group {
        Some((m, n)) if i == m => n - 1,
        Some((m, _)) if i > m => i - 1,
        _ => i,
    };
    i.saturating_sub(tmap.shift)
}

/// Modes chosen together, as one mode: their targets and effects in
/// printed order, each later part's start in `split`, `split2`, `more`.
/// No parts is choosing no mode ("choose up to one").
fn combined_mode(parts: &[&CoreMode]) -> CoreMode {
    let mut m = CoreMode {
        targets: Vec::new(),
        effects: Vec::new(),
        text: String::new(),
        split: None,
        split2: None,
        more: Vec::new(),
        once: mtg_core::card::ModeOnce::Any,
        name: None,
    };
    for (k, p) in parts.iter().enumerate() {
        let at = (
            u16::try_from(m.effects.len()).unwrap_or(u16::MAX),
            u16::try_from(m.targets.len()).unwrap_or(u16::MAX),
        );
        match k {
            0 => {}
            1 => m.split = Some(at),
            2 => m.split2 = Some(at),
            _ => m.more.push(at),
        }
        m.targets.extend(p.targets.iter().copied());
        m.effects.extend(p.effects.iter().cloned());
    }
    m.text = if parts.is_empty() {
        "no mode".to_string()
    } else {
        parts
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join(" and ")
    };
    m
}

/// The index of `label` among the card's "As this enters, choose <A> or
/// <B>" options, if it is one.
pub(crate) fn named_option(card: &Card, label: &str) -> Option<u8> {
    card.abilities.iter().find_map(|a| match a {
        Ability::AsEntersChoose {
            kind: ChoiceKind::Named(names),
            ..
        } => names
            .iter()
            .position(|n| n == label)
            .and_then(|i| u8::try_from(i).ok()),
        _ => None,
    })
}
