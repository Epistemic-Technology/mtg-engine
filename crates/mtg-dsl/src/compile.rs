//! Compiles the syntax tree into `mtg_core::card::CardDef`s in a
//! `CardPool`: interns subtypes, resolves token and emblem names, registers
//! delayed triggered abilities, and fills in the rendered Oracle text.

use std::collections::HashMap;

use mtg_core::card::{
    AbilityDef, ActivatedAbility, Affects as CoreAffects, Builder, CardPool, CardRef,
    CdaCount as CoreCdaCount, ChoiceKind as CoreChoiceKind, CopyExcept, Cost as CoreCost,
    Effect as CoreEffect, EffectCond, KeywordSet, Mode as CoreMode, Modification, Num, ObjFilter,
    Recipient as CoreRecipient, Relation, ReplacementAbility, Replaces as CoreReplaces,
    SacCost as CoreSacCost, Same, SameOf, SameWhat, StaticAbility, StaticCondition, SubtypeSet,
    SupertypeSet, TargetSpec, TriggerObj as CoreTriggerObj, TriggerSpec, TriggeredAbility, TypeSet,
    Who,
};
use mtg_core::mana::{Color, ColorSet, Mana};
use thiserror::Error;

use crate::ast::{
    Ability, Affects, Card, CdaCount, ChoiceKind, Cost, Effect, Filter, InsteadCond, Kind,
    Mechanic, Mod, Recipient, Replacement, Replaces, SacCost, SameRef, Sentence, StaticCond,
    Target, Trigger, TriggerObj,
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
    Ok(pool)
}

/// Sort definitions into compile order: tokens, then emblems, then cards,
/// each group by name.
pub fn compile_order(cards: &mut [Card]) {
    let rank = |k: Kind| match k {
        Kind::Token => 0,
        Kind::Emblem => 1,
        Kind::Card => 2,
    };
    cards.sort_by(|a, b| {
        rank(a.kind)
            .cmp(&rank(b.kind))
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// Compile one definition into the pool.
#[allow(clippy::implicit_hasher)]
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
        };
        let def = c.card()?;
        let r = pool.add(def);
        if let Some(prev) = prev {
            pool.alias(&f.name, prev);
        }
        faces.push(r);
    }
    let mut c = Compiler {
        pool,
        card,
        lookup: by_name,
        groups: Vec::new(),
        many: None,
        implied: Vec::new(),
        implied_used: std::rc::Rc::default(),
    };
    let mut def = c.card()?;
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
    Ok(r)
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
        let mut b = Builder::new(
            card.display_name.as_deref().unwrap_or(&card.name),
            &card.cost.to_string(),
        )
        .supertypes(card.supertypes.iter().copied())
        .types(card.types.iter().copied())
        .subtypes(&card.subtypes.iter().map(String::as_str).collect::<Vec<_>>())
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
                Ability::TargetCost { delta, filter } => {
                    let f = self.filter(filter);
                    b = b.target_cost(*delta, f);
                }
                Ability::CondCost { delta, cond } => {
                    let c = self.static_cond(cond);
                    let i = self.pool.intern_condition(c);
                    b = b.cond_cost(*delta, i);
                }
                Ability::Mechanic(Mechanic::Buyback(cost)) => {
                    b = b.buyback(cost.clone());
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
        if let Some(colors) = &card.color_indicator {
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
        if let Some(k) = &card.kicker {
            b = b.kicker(k.clone());
        }
        if let Some(fb) = &card.flashback {
            if fb.tap || fb.discard > 0 || fb.sacrifice.is_some() || fb.discard_self {
                return self.err("a flashback cost may only be mana and life");
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
                    | Ability::SelfCost { .. }
                    | Ability::TargetCost { .. }
                    | Ability::CondCost { .. }
                    | Ability::Affinity(_)
                    | Ability::Mechanic(
                        Mechanic::Buyback(_)
                            | Mechanic::Evoke(_)
                            | Mechanic::Madness(_)
                            | Mechanic::Foretell(_)
                            | Mechanic::Suspend(..)
                            | Mechanic::Entwine(_)
                            | Mechanic::Escape(..)
                            | Mechanic::Morph(_)
                            | Mechanic::Megamorph(_)
                            | Mechanic::Disguise(_)
                    )
            ) {
                continue;
            }
            let compiled = self.ability(a)?;
            b = b.ability(compiled);
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
        Ok(b.build())
    }

    fn subtypes(&mut self, names: &[String]) -> SubtypeSet {
        names.iter().map(|s| self.pool.subtype(s)).collect()
    }

    fn filter(&mut self, f: &Filter) -> ObjFilter {
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
            chosen: f.chosen.is_some(),
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

    fn static_cond(&mut self, c: &StaticCond) -> StaticCondition {
        match c {
            StaticCond::CardsInGraveyard(n) => StaticCondition::CardsInGraveyard(*n),
            StaticCond::CardTypesInGraveyard(n) => StaticCondition::CardTypesInGraveyard(*n),
            StaticCond::YouControl { filter, at_least } => StaticCondition::YouControl {
                filter: self.filter(filter).who(Who::You),
                at_least: *at_least,
            },
            StaticCond::YourTurn | StaticCond::DuringYourTurn => StaticCondition::YourTurn,
            StaticCond::Attacking => StaticCondition::SourceAttacking,
            StaticCond::Enchanted => StaticCondition::SourceEnchanted,
            StaticCond::Equipped => StaticCondition::SourceEquipped,
            StaticCond::OpponentLifeAtMost(n) => StaticCondition::OpponentLifeAtMost(*n),
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
            StaticCond::YouControlNone(filter) => {
                StaticCondition::YouControlNone(self.filter(filter).who(Who::You))
            }
            StaticCond::AttackedThisTurn => StaticCondition::AttackedThisTurn,
            StaticCond::CreatureDiedThisTurn => StaticCondition::CreatureDiedThisTurn,
            StaticCond::OpponentDamagedThisTurn => StaticCondition::OpponentDamagedThisTurn,
            StaticCond::Escaped => StaticCondition::Escaped,
            StaticCond::Kicked => StaticCondition::Kicked,
            StaticCond::LandEnteredThisTurn => StaticCondition::LandEnteredThisTurn,
            StaticCond::ArtifactEnteredThisTurn => StaticCondition::ArtifactEnteredThisTurn,
            StaticCond::CardsDrawnThisTurn(n) => StaticCondition::CardsDrawnThisTurn(*n),
            StaticCond::CardsInHand { n, at_most } => StaticCondition::CardsInHand {
                n: *n,
                at_most: *at_most,
            },
            StaticCond::GraveyardCards {
                filter, at_least, ..
            } => StaticCondition::GraveyardCards {
                filter: self.filter(filter),
                at_least: *at_least,
            },
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
            StaticCond::CastThisTurn(filter) => StaticCondition::CastThisTurn(self.filter(filter)),
            StaticCond::DiscardedThisTurn => StaticCondition::DiscardedThisTurn,
            StaticCond::MoreCardsInHand => StaticCondition::MoreCardsInHand,
            StaticCond::OpponentPoison(n) => StaticCondition::OpponentPoison(*n),
            StaticCond::OwnCardInExile => StaticCondition::OwnCardInExile,
            StaticCond::TotalPower(n) => StaticCondition::TotalPower(*n),
            StaticCond::DifferentPowers(n) => StaticCondition::DifferentPowers(*n),
            StaticCond::During(t) => StaticCondition::During(*t),
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
            A::BlockingIt { beyond } => C::BlockingIt { beyond: *beyond },
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
            Target::GraveyardCard { filter, who } => {
                let f = self.filter(filter);
                TargetSpec::GraveyardCard {
                    filter: self.pool.intern_filter(f),
                    who: *who,
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
            crew: c.crew,
            remove_counters: c.remove_counters,
            energy: c.energy,
            chosen: c.chosen,
            life: c.life,
            discard: c.discard,
            discard_x: c.discard_x,
            discard_filter: c.discard_filter.as_ref().map(|f| self.filter(f)),
            loyalty: c.loyalty.unwrap_or(0),
            tap: c.tap,
            sacrifice: match &c.sacrifice {
                None => None,
                Some(SacCost::This) => Some(CoreSacCost::This),
                Some(SacCost::Filter(f)) => Some(CoreSacCost::Filter(self.filter(f))),
            },
        }
    }

    fn recipient(groups: &[u16], many: Option<usize>, r: Recipient) -> CoreRecipient {
        match r {
            Recipient::Group(i) => CoreRecipient::Group(groups[usize::from(i)]),
            Recipient::TargetAnd(t, i) => CoreRecipient::TargetAndGroup(t, groups[usize::from(i)]),
            // A variable group: its targets to the end of the list.
            Recipient::Target(i) if Some(i) == many => {
                CoreRecipient::Targets(u8::try_from(i).unwrap_or(u8::MAX), u8::MAX)
            }
            Recipient::Target(i) | Recipient::ThatTarget(i) => CoreRecipient::Target(i),
            Recipient::Targets(a, n) => CoreRecipient::Targets(
                u8::try_from(a).unwrap_or(u8::MAX),
                u8::try_from(n).unwrap_or(u8::MAX),
            ),
            Recipient::You => CoreRecipient::You,
            Recipient::EachOpponent => CoreRecipient::EachOpponent,
            Recipient::Source | Recipient::SourceNamed | Recipient::SourceIt => {
                CoreRecipient::Source
            }
            Recipient::TriggeringObject | Recipient::TriggeringIt => {
                CoreRecipient::TriggeringObject
            }
            Recipient::TriggeringPlayer => CoreRecipient::TriggeringPlayer,
            Recipient::DefendingPlayer => CoreRecipient::DefendingPlayer,
            Recipient::ControllerOf(i) | Recipient::ThatControllerOf(i) => {
                CoreRecipient::ControllerOf(i)
            }
            Recipient::OwnerOf(i) => CoreRecipient::OwnerOf(i),
            Recipient::EachCreature => CoreRecipient::EachCreature,
            Recipient::EachPlayer => CoreRecipient::EachPlayer,
            Recipient::EachCreatureYouControl => CoreRecipient::EachCreatureYouControl,
            Recipient::EachPermanentYouControl => CoreRecipient::EachPermanentYouControl,
            Recipient::SameObject => CoreRecipient::SameObject,
            Recipient::CreatedObject => CoreRecipient::CreatedObject,
            Recipient::Attached => CoreRecipient::Attached,
            Recipient::BlockingIt => CoreRecipient::BlockingIt,
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
            Effect::Destroy { to, no_regen } => {
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
            Effect::RemoveAllCounters { to } => {
                out.push(CoreEffect::RemoveAllCounters { to: r(*to) });
            }
            Effect::ReturnToHand { what } => {
                for w in what {
                    out.push(CoreEffect::ReturnToHand { what: r(*w) });
                }
            }
            Effect::ReturnToBattlefield { what, counters } => {
                out.push(CoreEffect::ReturnToBattlefield {
                    what: r(*what),
                    counters: *counters,
                });
            }
            Effect::Regenerate { to } => out.push(CoreEffect::Regenerate { to: r(*to) }),
            Effect::Exile { what } => {
                for w in what {
                    out.push(CoreEffect::Exile { what: r(*w) });
                }
            }
            Effect::ExileAllGraveyards => out.push(CoreEffect::ExileAllGraveyards),
            Effect::Sacrifice { what } => out.push(CoreEffect::Sacrifice { what: r(*what) }),
            Effect::TapOrUntap { to } => out.push(CoreEffect::TapOrUntap { to: r(*to) }),
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
            Effect::Attach { to } => out.push(CoreEffect::Attach { to: r(*to) }),
            Effect::CreateToken {
                token,
                count,
                tapped,
            } => {
                let Some(token_ref) = self.pool.lookup(token) else {
                    return self.err(format!("unknown token \"{token}\""));
                };
                out.push(CoreEffect::CreateToken {
                    token: token_ref,
                    count: *count,
                    who: CoreRecipient::You,
                    tapped: *tapped,
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
            Effect::Counter { to } => out.push(CoreEffect::Counter { to: r(*to) }),
            Effect::CounterUnlessPays { to, mana } => out.push(CoreEffect::CounterUnlessPays {
                to: r(*to),
                mana: mana.clone(),
            }),
            Effect::Scry(n) => out.push(CoreEffect::Scry { count: *n }),
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
            Effect::PutInLibrary { what, top } => out.push(CoreEffect::PutInLibrary {
                what: r(*what),
                top: *top,
            }),
            Effect::SacrificeChosen { who, count, filter } => {
                let filter = self.filter(filter);
                out.push(CoreEffect::SacrificeChosen {
                    who: r(*who),
                    filter,
                    count: *count,
                });
            }
            Effect::Strip { keywords, to } => out.push(CoreEffect::Strip {
                keywords: KeywordSet::of(keywords.iter().copied()),
                to: r(*to),
            }),
            Effect::RevealHand { who } | Effect::LookAtHand { who } => {
                out.push(CoreEffect::RevealHand { who: r(*who) });
            }
            Effect::DiscardRandom { count, who } => out.push(CoreEffect::DiscardRandom {
                count: *count,
                who: r(*who),
            }),
            Effect::DiscardChosen {
                who, filter, may, ..
            } => {
                let filter = filter.as_ref().map(|f| self.filter(f));
                out.push(CoreEffect::DiscardChosen {
                    who: r(*who),
                    filter,
                    optional: *may,
                });
            }
            Effect::Transform { what } => out.push(CoreEffect::Transform { what: r(*what) }),
            Effect::Search {
                filter, count, to, ..
            } => out.push(CoreEffect::Search {
                filter: self.filter(filter),
                count: *count,
                to: *to,
            }),
            Effect::Investigate => {
                let Some(token) = self.pool.lookup("Clue") else {
                    return self.err("unknown token \"Clue\" (cards/tokens/clue.card)");
                };
                out.push(CoreEffect::CreateToken {
                    token,
                    count: Num::N(1),
                    who: CoreRecipient::You,
                    tapped: false,
                });
            }
            Effect::Surveil(n) => out.push(CoreEffect::Surveil { count: *n }),
            Effect::Discard { count, who } => out.push(CoreEffect::Discard {
                count: *count,
                who: who.map_or(CoreRecipient::You, r),
            }),
            Effect::Fight { a, b } => out.push(CoreEffect::Fight { a: r(*a), b: r(*b) }),
            Effect::Bite { source, to } => out.push(CoreEffect::Bite {
                source: r(*source),
                to: r(*to),
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
                power,
                toughness,
                colors,
            } => {
                let subtypes = self.subtypes(subtypes);
                out.push(CoreEffect::Animate {
                    to: r(*to),
                    types: TypeSet::of(types.iter().copied()),
                    subtypes,
                    power: *power,
                    toughness: *toughness,
                    colors: (!colors.is_empty()).then(|| ColorSet::of(colors.iter().copied())),
                });
            }
            Effect::BecomeColor { to, colors } => out.push(CoreEffect::BecomeColor {
                to: r(*to),
                colors: ColorSet::of(colors.iter().copied()),
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
            } => {
                let kind = self.choice_kind(kind);
                out.push(CoreEffect::OfChoice {
                    to: r(*to),
                    kind,
                    protection: *protection,
                });
            }
            Effect::YouMay(inner) => {
                let effects = self.sentences(inner)?;
                out.push(CoreEffect::YouMay(effects));
            }
            Effect::IfTarget(n, f, inner) => {
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
            Effect::IfCondition(cond, inner) => {
                let effects = self.sentences(inner)?;
                let c = self.static_cond(cond);
                let i = self.pool.intern_condition(c);
                out.push(CoreEffect::IfCondition(i, effects));
            }
            Effect::IfYouDo(inner) => {
                let effects = self.sentences(inner)?;
                out.push(CoreEffect::IfYouDo(effects));
            }
            Effect::Instead { .. } => {
                return self.err("`instead` is a sentence of its own, after the one it replaces");
            }
            Effect::Pay { mana, life } => out.push(CoreEffect::Pay {
                mana: mana.clone(),
                life: *life,
            }),
            Effect::UnlessPays {
                payer,
                mana,
                life,
                effect,
            } => {
                let mut effects = Vec::new();
                self.effect(effect, &mut effects)?;
                out.push(CoreEffect::UnlessPays {
                    payer: r(*payer),
                    mana: mana.clone(),
                    life: *life,
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
                ..
            } => {
                let effects = self.sentences(effects)?;
                let trigger = self.trigger(trigger);
                let id = self
                    .pool
                    .register_ability(AbilityDef::Triggered(TriggeredAbility {
                        trigger,
                        condition: None,
                        targets: Vec::new(),
                        effects,
                    }));
                out.push(CoreEffect::Delayed {
                    ability: id,
                    about: r(*about),
                });
            }
            Effect::CreateReplacement(rep) => {
                let compiled = self.replacement(rep)?;
                out.push(CoreEffect::CreateReplacement(compiled));
            }
            Effect::Rule(k) => out.push(CoreEffect::Rule(*k)),
            Effect::Proliferate => out.push(CoreEffect::Proliferate),
            Effect::Explore { what } => out.push(CoreEffect::Explore { what: r(*what) }),
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
            Effect::Adapt(n) => out.push(CoreEffect::Adapt(*n)),
            Effect::Discover(n) => out.push(CoreEffect::Discover(*n)),
            Effect::MoveSourceCounters { to } => {
                out.push(CoreEffect::MoveSourceCounters { to: r(*to) });
            }
            Effect::CumulativeUpkeep(c) => out.push(CoreEffect::CumulativeUpkeep(c.clone())),
            Effect::Connive { what } => out.push(CoreEffect::Connive { what: r(*what) }),
            Effect::LookAtTop(n) => out.push(CoreEffect::LookAtTop { count: *n }),
            Effect::Dig {
                count,
                filter,
                optional,
                all,
                rest_graveyard,
                ..
            } => out.push(CoreEffect::Dig {
                count: *count,
                filter: filter.as_ref().map(|f| self.filter(f)),
                optional: *optional,
                all: *all,
                rest_graveyard: *rest_graveyard,
            }),
            Effect::Modes(modes) => {
                let modes = self.modes(modes, false, false)?;
                out.push(CoreEffect::Modes(modes));
            }
            Effect::SwitchPt { to } => out.push(CoreEffect::SwitchPt { to: r(*to) }),
            Effect::SkipNextUntap { to } => out.push(CoreEffect::SkipNextUntap { to: r(*to) }),
            Effect::BlocksIfAble { blocker, attacker } => out.push(CoreEffect::BlocksIfAble {
                blocker: r(*blocker),
                attacker: r(*attacker),
            }),
            Effect::ReturnChosen { count, filter } => out.push(CoreEffect::ReturnChosen {
                who: CoreRecipient::You,
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

    fn trigger(&mut self, t: &Trigger) -> TriggerSpec {
        match t {
            Trigger::When(inner) => self.trigger(inner),
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
            Trigger::Attacks(o) => TriggerSpec::Attacks(self.trigger_obj(o)),
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
            } => TriggerSpec::BlockPair {
                what: self.trigger_obj(obj),
                other: self.filter(other),
                blocks: *blocks,
                blocked: *blocked,
            },
            Trigger::DealsCombatDamageToPlayer(o) => {
                TriggerSpec::DealsCombatDamageToPlayer(self.trigger_obj(o))
            }
            Trigger::GainsLife { whose } => TriggerSpec::GainsLife { whose: *whose },
            Trigger::CastsSpell {
                whose,
                noncreature,
                instant_or_sorcery,
            } => TriggerSpec::CastsSpell {
                whose: *whose,
                noncreature: *noncreature,
                instant_or_sorcery: *instant_or_sorcery,
            },
            Trigger::BecomesTarget(o) | Trigger::BecomesTargetWhenever(o) => {
                TriggerSpec::BecomesTarget(self.trigger_obj(o))
            }
            Trigger::TargetedByOpponent => TriggerSpec::TargetedByOpponent,
            Trigger::TurnedFaceUp => TriggerSpec::TurnedFaceUp,
            Trigger::Exploits => TriggerSpec::Exploits,
            Trigger::DrawsNth { whose, n } => TriggerSpec::DrawsNth {
                whose: *whose,
                n: *n,
            },
            Trigger::BecomesTapped(o) => TriggerSpec::BecomesTapped(self.trigger_obj(o)),
            Trigger::BecomesUntapped(o) => TriggerSpec::BecomesUntapped(self.trigger_obj(o)),
            Trigger::DealtDamage(o) => TriggerSpec::DealtDamage(self.trigger_obj(o)),
            Trigger::DealsDamage { obj, to, combat } => TriggerSpec::DealsDamage {
                what: self.trigger_obj(obj),
                to: *to,
                combat: *combat,
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
            Trigger::CastsNth { whose, nth } => TriggerSpec::CastsNth {
                whose: *whose,
                nth: *nth,
            },
            Trigger::EntersBatch(f) => TriggerSpec::EntersBatch(self.filter(f)),
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
            Affects::Target(i) | Affects::ThatTarget(i) => CoreAffects::Target(*i),
            Affects::OtherThanTarget(f, i) => CoreAffects::OtherThanTarget(self.filter(f), *i),
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
            Mod::Control => Modification::Control,
            Mod::AddColors(c) => Modification::AddColors(ColorSet::of(c.iter().copied())),
            Mod::SetColors(c) => Modification::SetColors(ColorSet::of(c.iter().copied())),
            Mod::AddKeywords(k) => Modification::AddKeywords(KeywordSet::of(k.iter().copied())),
            Mod::LosesKeywords(k) => {
                Modification::RemoveKeywords(KeywordSet::of(k.iter().copied()))
            }
            Mod::RemoveAllAbilities => Modification::RemoveAllAbilities,
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
                plus_toughness,
            } => Modification::CdaPt {
                count: match count {
                    CdaCount::CardTypesInGraveyards => CoreCdaCount::CardTypesInGraveyards,
                    CdaCount::CreatureCardsInGraveyards => CoreCdaCount::CreatureCardsInGraveyards,
                    CdaCount::Permanents(f) => CoreCdaCount::Permanents(self.filter(f)),
                    CdaCount::ControllerLife => CoreCdaCount::ControllerLife,
                },
                plus_toughness: *plus_toughness,
            },
        }
    }

    #[allow(clippy::too_many_lines)] // one arm per ability kind
    fn ability(&mut self, a: &Ability) -> Result<AbilityDef, CompileError> {
        self.many = None;
        Ok(match a {
            Ability::Keyword(_) => return self.err("a keyword is not an ability definition"),
            Ability::DeckAnyNumber => return self.err("a deck-building rule is not an ability"),
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
            }),
            Ability::Triggered {
                trigger,
                condition,
                targets,
                effects,
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
                },
                targets: self.targets(targets),
                effects: self.sentences(effects)?,
            }),
            Ability::Replacement(r) => AbilityDef::Replacement(self.replacement(r)?),
            // CR 701.43: modeled as an attack trigger: exerting is "it
            // doesn't untap during your next untap step".
            Ability::Exert(effects) => {
                let mut inner = vec![Sentence::new(vec![crate::ast::Effect::SkipNextUntap {
                    to: Recipient::Source,
                }])];
                inner.extend(effects.iter().cloned());
                return self.ability(&Ability::Triggered {
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
                        parts.push(self.modification_or_grant(m)?);
                    }
                    parts
                },
                condition: condition.as_ref().map(|c| self.static_cond(c)),
            }),
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
            } => {
                let c = self.static_cond(cond);
                AbilityDef::AttackUnless {
                    condition: self.pool.intern_condition(c),
                    attack: *attack,
                    block: *block,
                }
            }
            Ability::Labeled { ability, .. } => return self.ability(ability),
            Ability::BlockRule(r) => AbilityDef::BlockRule(match r {
                crate::ast::BlockRule::NotBy(f) => mtg_core::card::BlockRule::NotBy(self.filter(f)),
                crate::ast::BlockRule::OnlyBy(f) => {
                    mtg_core::card::BlockRule::OnlyBy(self.filter(f))
                }
                crate::ast::BlockRule::AtMostOne => mtg_core::card::BlockRule::AtMostOne,
                crate::ast::BlockRule::BlocksOnly(f) => {
                    mtg_core::card::BlockRule::BlocksOnly(self.filter(f))
                }
                crate::ast::BlockRule::CantBlock(f) => {
                    mtg_core::card::BlockRule::CantBlock(self.filter(f))
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
            } => AbilityDef::CostChange(mtg_core::card::CostChange {
                spells: self.filter(filter),
                whose: *whose,
                generic: *generic,
            }),
            Ability::Protection(f) => AbilityDef::Protection(self.filter(f)),
            Ability::Mechanic(m) => match m {
                Mechanic::Toxic(n) => AbilityDef::Toxic(*n),
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
                | Mechanic::Evoke(_)
                | Mechanic::Madness(_)
                | Mechanic::Foretell(_)
                | Mechanic::Suspend(..)
                | Mechanic::Entwine(_)
                | Mechanic::Escape(..)
                | Mechanic::Morph(_)
                | Mechanic::Megamorph(_)
                | Mechanic::Disguise(_) => {
                    return self.err("a cost of the card, not an ability");
                }
                other => {
                    let expanded = expand_mechanic(other, self.card.pt);
                    return self.ability(&expanded);
                }
            },
            Ability::HandSize { whose, delta } => AbilityDef::HandSize {
                whose: *whose,
                delta: *delta,
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

/// What a keyword ability with a parameter stands for, as an ordinary
/// ability (CR 702).
#[allow(clippy::too_many_lines)] // one arm per mechanic
fn expand_mechanic(m: &Mechanic, pt: Option<(i32, i32)>) -> Ability {
    use crate::ast::{Effect as E, Join};
    let one = |e: E| vec![Sentence::new(vec![e])];
    match m {
        Mechanic::Bushido(n) => Ability::Triggered {
            trigger: Trigger::BlocksOrBlocked(TriggerObj::This),
            condition: None,
            targets: Vec::new(),
            effects: one(E::Pump {
                power: i32::try_from(*n).unwrap_or(0),
                toughness: i32::try_from(*n).unwrap_or(0),
                to: Recipient::Source,
            }),
        },
        Mechanic::Afflict(n) => Ability::Triggered {
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
            trigger: Trigger::BeginningOfStep {
                step: mtg_core::turn::Step::Upkeep,
                whose: Who::You,
            },
            condition: None,
            targets: Vec::new(),
            effects: one(E::CumulativeUpkeep(cost.clone())),
        },
        Mechanic::Soulshift(n) => Ability::Triggered {
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
            }],
            effects: one(E::YouMay(one(E::ReturnToHand {
                what: vec![Recipient::Target(0)],
            }))),
        },
        Mechanic::Ward(cost) => Ability::Triggered {
            trigger: Trigger::TargetedByOpponent,
            condition: None,
            targets: Vec::new(),
            effects: one(E::CounterUnlessPays {
                to: Recipient::TriggeringObject,
                mana: cost.clone(),
            }),
        },
        Mechanic::Echo(cost) => Ability::Triggered {
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
                        mana: cost.clone(),
                        life: 0,
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
            let (power, toughness) = pt.unwrap_or((0, 0));
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
                    power,
                    toughness,
                    colors: Vec::new(),
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
        | Mechanic::KickedWith(..)
        | Mechanic::Buyback(_)
        | Mechanic::Morph(_)
        | Mechanic::Megamorph(_)
        | Mechanic::Disguise(_) => unreachable!("not an expansion"),
    }
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
