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
    cards.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.name.cmp(&b.name)));
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
            implied: Vec::new(),
            implied_used: std::rc::Rc::default(),
            scavenge: None,
            station: None,
            extort: None,
            token: None,
            cast_snapshot: None,
        };
        let def = c.card()?;
        let r = pool.add(def);
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
                implied: Vec::new(),
                implied_used: std::rc::Rc::default(),
                scavenge: None,
                station: None,
                extort: None,
                token: None,
                cast_snapshot: None,
            };
            let def = c.card()?;
            let r = pool.add(def);
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
        implied: Vec::new(),
        implied_used: std::rc::Rc::default(),
        scavenge: None,
        station: None,
        extort: None,
        token,
        cast_snapshot: None,
    };
    let mut def = c.card()?;
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
    if !faces.is_empty() {
        pool.alias(&card.name, r);
        // CR 709.5: a Room's locked and unlocked states.
        pool.add_room_doors(r);
        // A double-faced card's back face turns back into the front.
        if card.layout == mtg_core::card::Layout::Transform {
            pool.set_faces(faces[0], mtg_core::card::Layout::Transform, vec![r]);
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
            implied: Vec::new(),
            implied_used: std::rc::Rc::default(),
            scavenge: None,
            station: None,
            extort: None,
            token: None,
            cast_snapshot: None,
        };
        let mut d = c.card()?;
        link_exiles(&mut d);
        d.oracle_text.clone_from(&pool.get(r).oracle_text);
        refs.push(pool.add(d));
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
    /// Per group, the choice its filter implies ("of the color of your
    /// choice"), made as the effect using it resolves.
    implied: Vec<Option<CoreChoiceKind>>,
    /// Set as an effect's recipient uses such a group.
    implied_used: std::rc::Rc<std::cell::Cell<Option<CoreChoiceKind>>>,
    /// Scavenge's count (the card's power), once added to the counts.
    scavenge: Option<u8>,
    /// Station's count (the tapped creature's power), likewise.
    station: Option<u8>,
    /// Extort's count (the life lost this way), once added to the counts.
    extort: Option<u8>,
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
        let keywords = card
            .keywords
            .iter()
            .copied()
            .chain(card.abilities.iter().filter_map(|a| match a {
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
        let keywords = keywords.chain(protection).chain(evoke);
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
                Ability::Mechanic(Mechanic::Foretell(cost)) => {
                    b = b.foretell(cost.clone());
                }
                Ability::Mechanic(Mechanic::Suspend(n, cost)) => {
                    b = b.suspend(*n, cost.clone());
                }
                Ability::Mechanic(Mechanic::Entwine(cost)) => {
                    b = b.entwine(cost.clone());
                }
                Ability::Mechanic(Mechanic::Escape(cost, n)) => {
                    b = b.escape(cost.clone(), *n);
                }
                Ability::Mechanic(Mechanic::Modular(n)) => {
                    b = b.modular(*n);
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
                Ability::Mechanic(Mechanic::StartYourEngines) => {
                    b = b.marks(mtg_core::card::Marks::ENGINES);
                }
                Ability::Mechanic(Mechanic::BecomesDay) => {
                    b = b.marks(mtg_core::card::Marks::BECOMES_DAY);
                }
                Ability::Mechanic(Mechanic::Station) => {
                    // CR 702.184a: "equal to the tapped creature's power".
                    counts.push(mtg_core::card::CountExpr::MovedStat {
                        verb: mtg_core::card::ThisWay::Tapped,
                        stat: mtg_core::card::Stat::Power,
                    });
                    self.station = u8::try_from(counts.len() - 1).ok();
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
                Ability::Mechanic(m @ (Mechanic::Surge(cost) | Mechanic::Spectacle(cost))) => {
                    if card.alt_cost.is_some() {
                        return self.err("two alternative costs of the card's own");
                    }
                    let cond = if matches!(m, Mechanic::Surge(_)) {
                        StaticCondition::CastThisTurn {
                            filter: ObjFilter::default(),
                            at_least: 1,
                            other: true,
                        }
                    } else {
                        StaticCondition::OpponentLostLifeThisTurn
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
                )
            })
            .count();
        if spells > 1 {
            return self.err(
                "more than one `spell`/`choose one`; put the paragraphs in one `spell { }` block",
            );
        }
        for a in &card.abilities {
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
                            | Mechanic::Foretell(_)
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
                            | Mechanic::Replicate(_)
                            | Mechanic::Teamwork(_)
                            | Mechanic::Ripple(_)
                            | Mechanic::Strive(_)
                            | Mechanic::Ascend
                            | Mechanic::StartYourEngines
                            | Mechanic::BecomesDay
                            | Mechanic::Retrace
                            | Mechanic::JumpStart
                            | Mechanic::Surge(_)
                            | Mechanic::Spectacle(_)
                    )
            ) {
                continue;
            }
            let compiled = self.ability(a)?;
            b = b.ability(compiled);
            // CR 702.98a: unleash's second static ability, "This permanent
            // can't block as long as it has a +1/+1 counter on it."
            if let Ability::Mechanic(Mechanic::Unleash) = a {
                b = b.ability(unleash_static());
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
                Some(SameRef::SameNameAs(_) | SameRef::SharesColorWith(_)) => Relation::Any,
                _ => f.relation,
            },
            same: f.same.map(|r| {
                let target = |n: usize| SameOf::Target(u8::try_from(n).unwrap_or(u8::MAX));
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
                    SameRef::SharesColorWith(n) => Same {
                        of: target(n),
                        what: SameWhat::Color,
                    },
                    SameRef::LesserPower => Same {
                        of: SameOf::Source,
                        what: SameWhat::LesserPower,
                    },
                }
            }),
            named: None,
            enchanted: f.enchanted,
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
                    AttachedRef::Target(n) | AttachedRef::ThatTarget(n) => {
                        AttachedTo::Target(u8::try_from(n).unwrap_or(u8::MAX))
                    }
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
                name: m.name.clone(),
            });
        }
        let pair = |a: &CoreMode, b: &CoreMode| CoreMode {
            targets: a.targets.iter().chain(&b.targets).copied().collect(),
            effects: a.effects.iter().chain(&b.effects).cloned().collect(),
            text: format!("{} and {}", a.text, b.text),
            split: Some((
                u16::try_from(a.effects.len()).unwrap_or(u16::MAX),
                u16::try_from(a.targets.len()).unwrap_or(u16::MAX),
            )),
            name: None,
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

    #[allow(clippy::too_many_lines)] // one arm per condition
    fn static_cond(&mut self, c: &StaticCond) -> StaticCondition {
        match c {
            StaticCond::CardsInGraveyard(n) => StaticCondition::CardsInGraveyard(*n),
            StaticCond::CardTypesInGraveyard(n) => StaticCondition::CardTypesInGraveyard(*n),
            StaticCond::YouControl { filter, at_least } => StaticCondition::YouControl {
                filter: self.filter(filter).who(Who::You),
                at_least: *at_least,
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
            StaticCond::Or(a, b) => {
                StaticCondition::Or(Box::new(self.static_cond(a)), Box::new(self.static_cond(b)))
            }
            StaticCond::OpponentGraveyard(n) => StaticCondition::OpponentGraveyard(*n),
            StaticCond::ManaValuesInGraveyard(n) => StaticCondition::ManaValuesInGraveyard(*n),
            StaticCond::OpponentControls { filter, at_least } => {
                StaticCondition::OpponentControls {
                    filter: self.filter(filter),
                    at_least: *at_least,
                }
            }
            StaticCond::Is(filter) => StaticCondition::SourceIs(self.filter(filter)),
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
            StaticCond::Monarch => StaticCondition::Monarch,
            StaticCond::CompletedDungeon => StaticCondition::CompletedDungeon,
            StaticCond::CityBlessing => StaticCondition::CityBlessing,
            StaticCond::MaxSpeed => StaticCondition::MaxSpeed,
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
                n: *n,
                toughness: *toughness,
            },
            A::Life => C::Life,
            A::Times(n, inner) => C::Times(*n, Box::new(self.count_expr(inner))),
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
            A::SourceToughness => C::SourceToughness,
            A::TargetManaValue { n, .. } => C::TargetManaValue(*n),
            A::CardManaValue => C::CardManaValue,
            A::ColorsAmongPermanents => C::ColorsAmongPermanents,
            A::SpentColors => C::SpentColors,
            A::SpentTreasure => C::SpentTreasure,
            A::CardsInOpponentsGraveyards(f) => {
                C::CardsInOpponentsGraveyards(f.as_ref().map(|f| self.filter(f)))
            }
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
            A::TargetHand(n) => C::TargetHand(*n),
            A::GreatestToughness(f) => C::GreatestToughness(self.filter(f)),
            A::Sum(a, b) => C::Sum(Box::new(self.count_expr(a)), Box::new(self.count_expr(b))),
            A::LifeLostThisTurn => C::LifeLostThisTurn,
            A::DamageThisWay { excess } => C::DamageThisWay { excess: *excess },
            A::CardTypesInGraveyards => C::CardTypesInAllGraveyards,
            A::SourceCounters { kind, .. } => C::SourceCounters(*kind),
            A::HostGraveyard(f) => C::HostGraveyard(f.as_ref().map(|f| self.filter(f))),
            A::Devotion { colors, .. } => C::Devotion(ColorSet::of(colors.iter().copied())),
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
        ts.iter().map(|t| self.target(t)).collect()
    }

    fn cost(&mut self, c: &Cost) -> CoreCost {
        CoreCost {
            mana: c.mana.clone(),
            discard_self: c.discard_self,
            exile_self: c.exile_self,
            exile_this: c.exile_this,
            crew: c.crew,
            remove_counters: c.remove_counters,
            put_counters: c.put_counters,
            energy: c.energy,
            blight: c.blight,
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
        }
    }

    fn recipient(groups: &[u16], many: Option<usize>, r: Recipient) -> CoreRecipient {
        match r {
            Recipient::Group(i) => CoreRecipient::Group(groups[usize::from(i)]),
            Recipient::RandomOf(i) => CoreRecipient::RandomOf(groups[usize::from(i)]),
            Recipient::DamagedThisWay => CoreRecipient::DamagedThisWay,
            Recipient::OpponentsWithout(i) => {
                CoreRecipient::OpponentsWithout(groups[usize::from(i)])
            }
            Recipient::TargetAnd(t, i) => CoreRecipient::TargetAndGroup(t, groups[usize::from(i)]),
            // A variable group: its targets to the end of the list.
            Recipient::Target(i) if Some(i) == many => {
                CoreRecipient::Targets(u8::try_from(i).unwrap_or(u8::MAX), u8::MAX)
            }
            Recipient::Target(i) | Recipient::ThatTarget(i) | Recipient::TheTarget(i) => {
                CoreRecipient::Target(i)
            }
            Recipient::Targets(a, n)
            | Recipient::ThoseCards(a, n)
            | Recipient::ThoseTargets(a, n) => CoreRecipient::Targets(
                u8::try_from(a).unwrap_or(u8::MAX),
                u8::try_from(n).unwrap_or(u8::MAX),
            ),
            Recipient::You => CoreRecipient::You,
            Recipient::EachOpponent | Recipient::EachOtherPlayer => CoreRecipient::EachOpponent,
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
            | Recipient::TheControllerOf(i) => CoreRecipient::ControllerOf(i),
            Recipient::OwnerOf(i) | Recipient::ThatOwner(i) => CoreRecipient::OwnerOf(i),
            Recipient::EachCreature => CoreRecipient::EachCreature,
            Recipient::EachPlayer => CoreRecipient::EachPlayer,
            Recipient::EachCreatureYouControl => CoreRecipient::EachCreatureYouControl,
            Recipient::EachPermanentYouControl => CoreRecipient::EachPermanentYouControl,
            Recipient::SameObject | Recipient::SameThat | Recipient::SameTokens => {
                CoreRecipient::SameObject
            }
            Recipient::CreatedTokens => CoreRecipient::CreatedTokens,
            Recipient::CreatedObject => CoreRecipient::CreatedObject,
            Recipient::Attached => CoreRecipient::Attached,
            Recipient::AttachedController => CoreRecipient::AttachedController,
            Recipient::BlockingIt => CoreRecipient::BlockingIt,
            Recipient::ExiledCard => CoreRecipient::ExiledCard,
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
                let alt = self.sentences(&same_damage_recipients(effects, before))?;
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
        }
    }

    /// An effect; one on "all enchantments of the color of your choice"
    /// asks first (CR 608.2d), unless the resolution already chose.
    fn effect(&mut self, e: &Effect, out: &mut Vec<CoreEffect>) -> Result<(), CompileError> {
        let start = out.len();
        self.implied_used.set(None);
        self.effect_inner(e, out)?;
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
        let implied = self.implied.clone();
        let used = std::rc::Rc::clone(&self.implied_used);
        let r = |x: Recipient| {
            if let Recipient::Group(i) = x
                && let Some(Some(k)) = implied.get(usize::from(i))
            {
                used.set(Some(*k));
            }
            Self::recipient(&groups, many, x)
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
                for t in to {
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
            Effect::ReturnToHand { what } => {
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
            } => {
                out.push(CoreEffect::ReturnToBattlefield {
                    what: r(*what),
                    counters: *counters,
                    tapped: *tapped,
                    under_you: *under_you,
                    attached: attached.map(r),
                });
            }
            Effect::Regenerate { to } => out.push(CoreEffect::Regenerate { to: r(*to) }),
            Effect::Exile { what } => {
                for w in what {
                    out.push(CoreEffect::Exile { what: r(*w) });
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
            Effect::Grant { keywords, to } => out.push(CoreEffect::Grant {
                keywords: KeywordSet::of(keywords.iter().copied()),
                to: r(*to),
            }),
            Effect::GainControl { of } => out.push(CoreEffect::GainControl { of: r(*of) }),
            Effect::Attach { what, to } => out.push(CoreEffect::Attach {
                what: r(*what),
                to: r(*to),
            }),
            Effect::Unattach { what } => out.push(CoreEffect::Unattach { what: r(*what) }),
            Effect::CreateToken {
                token,
                count,
                tapped,
                who,
                attached,
                ..
            } => {
                let Some(token_ref) = self.pool.lookup(token) else {
                    return self.err(format!("unknown token \"{token}\""));
                };
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
            }
            Effect::CreateTokenCopy { of, except } => out.push(CoreEffect::CreateTokenCopy {
                of: r(*of),
                except: *except,
            }),
            Effect::BecomeCopy { of, except } => out.push(CoreEffect::BecomeCopy {
                of: r(*of),
                except: *except,
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
            Effect::ExileChosen { who, count, filter } => out.push(CoreEffect::ExileChosen {
                who: r(*who),
                filter: self.filter(filter),
                count: *count,
            }),
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
                ..
            } => {
                let filter = filter.as_ref().map(|f| self.filter(f));
                out.push(CoreEffect::DiscardChosen {
                    who: r(*who),
                    filter,
                    optional: *may,
                    exile: exile.is_some(),
                });
            }
            Effect::Transform { what } => out.push(CoreEffect::Transform { what: r(*what) }),
            Effect::Search {
                filter,
                count,
                to,
                reveal,
                who,
                may,
                ..
            } => out.push(CoreEffect::Search {
                filter: self.filter(filter),
                count: *count,
                to: *to,
                who: who.map_or(CoreRecipient::You, r),
                may: *may,
                reveal: *reveal,
            }),
            Effect::Investigate(_) | Effect::Investigates(_) => {
                let Some(token) = self.pool.lookup("Clue") else {
                    return self.err("unknown token \"Clue\" (cards/tokens/clue.card)");
                };
                let (n, who) = match e {
                    Effect::Investigates(w) => (1, r(*w)),
                    Effect::Investigate(n) => (*n, CoreRecipient::You),
                    _ => unreachable!(),
                };
                out.push(CoreEffect::CreateToken {
                    token,
                    count: Num::N(n),
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
            Effect::DoublePower { to, times } => out.push(CoreEffect::PumpVar {
                power: 1,
                toughness: 0,
                times: *times,
                to: r(*to),
            }),
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
                filter,
                ..
            } => {
                let filter = filter.as_ref().map(|f| self.filter(f));
                out.push(CoreEffect::ShuffleIntoLibrary {
                    who: who.or(*of).map_or(CoreRecipient::You, r),
                    hand: *hand,
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
                    Some(
                        CoreEffect::IfCondition(..)
                            | CoreEffect::Clash { .. }
                            | CoreEffect::IfCard { .. }
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
            Effect::UnlessPays {
                payer,
                mana,
                life,
                discard,
                effect,
            } => {
                let mut effects = Vec::new();
                self.effect(effect, &mut effects)?;
                out.push(CoreEffect::UnlessPays {
                    payer: r(*payer),
                    mana: mana.clone(),
                    life: *life,
                    discard: *discard,
                    effects,
                });
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
                let effects = self.sentences(effects)?;
                let trigger = self.trigger(trigger);
                let id = self
                    .pool
                    .register_ability(AbilityDef::Triggered(TriggeredAbility {
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
            Effect::Explore { what } => out.push(CoreEffect::Explore { what: r(*what) }),
            Effect::PhaseOut { what } => out.push(CoreEffect::PhaseOut { what: r(*what) }),
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
            Effect::ClearEcho => out.push(CoreEffect::ClearEcho),
            Effect::Manifest => out.push(CoreEffect::Manifest),
            Effect::ManifestDread => out.push(CoreEffect::ManifestDread),
            Effect::Blight(n) => out.push(CoreEffect::Blight(*n)),
            Effect::Adapt(n) => out.push(CoreEffect::Adapt(*n)),
            Effect::Bolster(n) => out.push(CoreEffect::Bolster(*n)),
            Effect::Monstrosity(n) => out.push(CoreEffect::Monstrosity(*n)),
            Effect::Populate => out.push(CoreEffect::Populate),
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
            Effect::MoveSourceCounters { to } => {
                out.push(CoreEffect::MoveSourceCounters { to: r(*to) });
            }
            Effect::CumulativeUpkeep(c) => out.push(CoreEffect::CumulativeUpkeep(c.clone())),
            Effect::Connive { what } => out.push(CoreEffect::Connive { what: r(*what) }),
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
            }),
            Effect::PutCard { what, to, .. } => {
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
                });
            }
            Effect::ExilePlayable { count, until, .. } => out.push(CoreEffect::ExilePlayable {
                count: *count,
                until: *until,
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
            }),
            Effect::Modes(modes) => {
                let modes = self.modes(modes, false, false)?;
                out.push(CoreEffect::Modes(modes));
            }
            Effect::SwitchPt { to } => out.push(CoreEffect::SwitchPt { to: r(*to) }),
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
            Effect::PutFromHand { filter, tapped } => out.push(CoreEffect::PutFromHand {
                filter: self.filter(filter),
                tapped: *tapped,
            }),
            Effect::IfThisWay(c, inner) => {
                let then = self.sentences(inner)?;
                let cond = self.this_way_cond(c);
                out.push(CoreEffect::IfThisWay { cond, then });
            }
            Effect::IfExcessDamage(inner) => {
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
            Trigger::AttacksBattle(o) => TriggerSpec::AttacksBattle(self.trigger_obj(o)),
            Trigger::UnlocksDoor => TriggerSpec::UnlocksDoor { door: 0 },
            Trigger::BecomesLevel(n) => TriggerSpec::BecomesStage(n - 1),
            Trigger::Chapter(ns) => TriggerSpec::Chapter(ns.iter().fold(0u16, |m, n| m | 1 << n)),
            Trigger::FullyUnlocks => TriggerSpec::FullyUnlocks { whose: Who::You },
            Trigger::DealsCombatDamageToPlayerOrBattle(o) => {
                TriggerSpec::DealsCombatDamageToPlayerOrBattle(self.trigger_obj(o))
            }
            Trigger::AttacksAlone(o) => TriggerSpec::AttacksAlone(self.trigger_obj(o)),
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
            Trigger::EntersFromGraveyard(f) => TriggerSpec::EntersFromGraveyard(self.filter(f)),
            Trigger::Expends(n) => TriggerSpec::Expends(*n),
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
            // A variable group: its targets to the end of the list.
            Affects::Target(i) if Some(*i) == self.many => {
                CoreAffects::Targets(u8::try_from(*i).unwrap_or(u8::MAX), u8::MAX)
            }
            Affects::Target(i) | Affects::ThatTarget(i) => CoreAffects::Target(*i),
            Affects::Targets(a, n) => CoreAffects::Targets(
                u8::try_from(*a).unwrap_or(u8::MAX),
                u8::try_from(*n).unwrap_or(u8::MAX),
            ),
            Affects::OtherThanTarget(f, i) => CoreAffects::OtherThanTarget(self.filter(f), *i),
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
        };
        Ok(ReplacementAbility {
            replaces,
            replacement: r.outcome,
            then: self.sentences(&r.then)?,
        })
    }

    /// A modification, registering a granted ability ("has \"...\"").
    fn modification_or_grant(&mut self, m: &Mod) -> Result<Modification, CompileError> {
        if let Mod::AddAbility(a) = m {
            let def = self.ability(a)?;
            return Ok(Modification::AddAbility(self.pool.register_ability(def)));
        }
        Ok(self.modification(m))
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
        Ok(match a {
            Ability::Keyword(_) => return self.err("a keyword is not an ability definition"),
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
            Ability::Activated { cost, .. } if cost.discard_x => {
                return self.err("`discard X cards` is only a spell's additional cost");
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
                from_hand: *cycling || cost.discard_self,
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
            } => AbilityDef::Triggered(TriggeredAbility {
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
                })
            }
            Ability::EntersTappedUnless(c) => {
                let c = self.static_cond(c);
                let i = self.pool.intern_condition(c);
                AbilityDef::Replacement(ReplacementAbility {
                    replaces: mtg_core::card::Replaces::Enters,
                    replacement: mtg_core::card::Replacement::EnterTappedUnless(i),
                    then: Vec::new(),
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
                let mut a = self.ability(ability)?;
                match &mut a {
                    AbilityDef::Static(sa) if sa.condition.is_none() => {
                        sa.condition = Some(StaticCondition::MaxSpeed);
                    }
                    AbilityDef::Activated(aa) => aa.conditions.push(StaticCondition::MaxSpeed),
                    _ => return self.err("max speed applies to a static or activated ability"),
                }
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
                })
            }
            Ability::Protection(f) => AbilityDef::Protection(self.filter(f)),
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
                    })
                }
                Mechanic::Buyback(_)
                | Mechanic::BuybackCost(_)
                | Mechanic::MorphCost(_)
                | Mechanic::Evoke(_)
                | Mechanic::Madness(_)
                | Mechanic::Foretell(_)
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
                | Mechanic::Replicate(_)
                | Mechanic::Teamwork(_)
                | Mechanic::Ripple(_)
                | Mechanic::Strive(_)
                | Mechanic::Ascend
                | Mechanic::StartYourEngines
                | Mechanic::BecomesDay => {
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
                }),
                // CR 702.184a: "Tap another untapped creature you control:
                // Put a number of charge counters on this permanent equal
                // to the tapped creature's power. Activate only as a
                // sorcery."
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
                    from_graveyard: false,
                    trigger: TriggerSpec::DealsCombatDamageToPlayer(CoreTriggerObj::This),
                    condition: None,
                    targets: Vec::new(),
                    effects: vec![CoreEffect::ExileTop {
                        who: CoreRecipient::TriggeringPlayer,
                    }],
                }),
                // CR 702.101a: "Whenever you cast a spell, you may pay
                // {W/B}. If you do, each opponent loses 1 life and you gain
                // life equal to the total life lost this way."
                Mechanic::Extort => {
                    let Some(i) = self.extort else {
                        return self.err("extort's count is missing");
                    };
                    let expanded = extort(i);
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
/// The token Incubate creates (CR 701.53a, 111.10i).
const INCUBATOR: &str = "Incubator";

/// Unleash's "This permanent can't block as long as it has a +1/+1 counter
/// on it." (CR 702.98a).
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
        then_first: false,
        joins: vec![Join::And],
    };
    Ability::Triggered {
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
        Mechanic::Evolve => Ability::Triggered {
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
        // CR 702.135a: "When this creature dies, create N 1/1 white and
        // black Spirit creature tokens with flying."
        Mechanic::Afterlife(n) => Ability::Triggered {
            from_graveyard: false,
            trigger: Trigger::Dies(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: one(E::CreateToken {
                token: AFTERLIFE_SPIRIT.to_string(),
                count: Num::N(*n),
                tapped: false,
                who: None,
                it_has: false,
                attached: None,
            }),
        },
        // CR 702.134a: "Whenever this creature attacks, put a +1/+1
        // counter on target attacking creature with lesser power."
        Mechanic::Mentor => Ability::Triggered {
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
        Mechanic::LivingWeapon => Ability::Triggered {
            from_graveyard: false,
            trigger: Trigger::Enters(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: vec![Sentence {
                effects: vec![
                    E::CreateToken {
                        token: GERM.to_string(),
                        count: Num::N(1),
                        tapped: false,
                        who: None,
                        it_has: false,
                        attached: None,
                    },
                    E::Attach {
                        what: Recipient::Source,
                        to: Recipient::CreatedObject,
                    },
                ],
                label: None,
                para: false,
                eot_first: false,
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
        }),
        // CR 702.39a: "Whenever this creature attacks, you may have target
        // creature defending player controls untap and block this creature
        // this turn if able."
        Mechanic::Provoke => Ability::Triggered {
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
        Mechanic::Afflict(n) => Ability::Triggered {
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
        // CR 702.24a.
        Mechanic::CumulativeUpkeep(cost) => Ability::Triggered {
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
            }))),
        },
        Mechanic::Ward(cost) => Ability::Triggered {
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
            trigger: Trigger::TargetedByOpponent,
            condition: None,
            from_graveyard: false,
            targets: Vec::new(),
            effects: one(E::UnlessPays {
                payer: Recipient::TriggeringController { that: false },
                mana: cost.mana.clone(),
                life: cost.life,
                discard: cost.discard,
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
                        effect: Box::new(E::Sacrifice {
                            what: Recipient::Source,
                        }),
                    },
                ],
                label: None,
                para: false,
                eot_first: false,
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
                    },
                    E::Grant {
                        keywords: vec![mtg_core::card::Keyword::Haste],
                        to: Recipient::Source,
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
                    },
                ],
                label: None,
                para: false,
                eot_first: false,
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
        | Mechanic::Foretell(_)
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
        | Mechanic::Replicate(_)
        | Mechanic::Teamwork(_)
        | Mechanic::Ripple(_)
        | Mechanic::Ascend
        | Mechanic::StartYourEngines
        | Mechanic::BecomesDay
        | Mechanic::Fabricate(_)
        | Mechanic::Renown(_)
        | Mechanic::Unleash
        | Mechanic::Station
        | Mechanic::Extort
        | Mechanic::Riot
        | Mechanic::Retrace
        | Mechanic::Ingest
        | Mechanic::Enlist
        | Mechanic::JumpStart
        | Mechanic::Surge(_)
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
