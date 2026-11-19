//! Compiles the syntax tree into `mtg_core::card::CardDef`s in a
//! `CardPool`: interns subtypes, resolves token and emblem names, registers
//! delayed triggered abilities, and fills in the rendered Oracle text.

use std::collections::HashMap;

use mtg_core::card::{
    AbilityDef, ActivatedAbility, Affects as CoreAffects, Builder, CardPool, CardRef,
    CdaCount as CoreCdaCount, CopyExcept, Cost as CoreCost, Effect as CoreEffect, KeywordSet,
    Mode as CoreMode, Modification, Num, ObjFilter, Recipient as CoreRecipient, ReplacementAbility,
    Replaces as CoreReplaces, SacCost as CoreSacCost, StaticAbility, StaticCondition, SubtypeSet,
    SupertypeSet, TargetSpec, TriggerObj as CoreTriggerObj, TriggerSpec, TriggeredAbility, TypeSet,
    Who,
};
use mtg_core::mana::{ColorSet, Mana};
use thiserror::Error;

use crate::ast::{
    Ability, Affects, Card, CdaCount, Cost, Effect, Filter, Kind, Mechanic, Mod, Recipient,
    Replacement, Replaces, SacCost, Sentence, StaticCond, Target, Trigger, TriggerObj,
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
        for g in &card.groups {
            let f = self.filter(g);
            let id = self.pool.intern_filter(f);
            self.groups.push(id);
        }
        let spells = card
            .abilities
            .iter()
            .filter(|a| {
                matches!(
                    a,
                    Ability::Spell { .. } | Ability::Modal(_) | Ability::ModalBoth(_)
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
                    | Ability::SelfCost { .. }
                    | Ability::TargetCost { .. }
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
            relation: f.relation,
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
    /// ("choose one or both", CR 700.2).
    fn modes(
        &mut self,
        modes: &[crate::ast::Mode],
        both: bool,
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
            });
        }
        if both {
            let [a, b] = out.as_slice() else {
                return self.err("`choose one or both` takes two modes");
            };
            let split = (
                u16::try_from(a.effects.len()).unwrap_or(u16::MAX),
                u16::try_from(a.targets.len()).unwrap_or(u16::MAX),
            );
            let both = CoreMode {
                targets: a.targets.iter().chain(&b.targets).copied().collect(),
                effects: a.effects.iter().chain(&b.effects).cloned().collect(),
                text: format!("{} and {}", a.text, b.text),
                split: Some(split),
            };
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
            StaticCond::DefenderControls { filter, at_least } => {
                StaticCondition::DefenderControls {
                    filter: self.filter(filter).who(Who::You),
                    at_least: *at_least,
                }
            }
            StaticCond::YouControlNone(filter) => {
                StaticCondition::YouControlNone(self.filter(filter).who(Who::You))
            }
            StaticCond::AttackedThisTurn => StaticCondition::AttackedThisTurn,
            StaticCond::CreatureDiedThisTurn => StaticCondition::CreatureDiedThisTurn,
            StaticCond::OpponentDamagedThisTurn => StaticCondition::OpponentDamagedThisTurn,
            StaticCond::Escaped => StaticCondition::Escaped,
            StaticCond::CardsInHand { n, at_most } => StaticCondition::CardsInHand {
                n: *n,
                at_most: *at_most,
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
            A::TargetPower(i) => C::TargetPower(*i),
            A::Life => C::Life,
            A::Times(n, inner) => C::Times(*n, Box::new(self.count_expr(inner))),
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
        }
    }

    fn targets(&mut self, ts: &[Target]) -> Vec<TargetSpec> {
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

    fn recipient(groups: &[u16], r: Recipient) -> CoreRecipient {
        match r {
            Recipient::Group(i) => CoreRecipient::Group(groups[usize::from(i)]),
            Recipient::Target(i) | Recipient::ThatTarget(i) => CoreRecipient::Target(i),
            Recipient::Targets(a, n) => CoreRecipient::Targets(
                u8::try_from(a).unwrap_or(u8::MAX),
                u8::try_from(n).unwrap_or(u8::MAX),
            ),
            Recipient::You => CoreRecipient::You,
            Recipient::EachOpponent => CoreRecipient::EachOpponent,
            Recipient::Source => CoreRecipient::Source,
            Recipient::TriggeringObject => CoreRecipient::TriggeringObject,
            Recipient::TriggeringPlayer => CoreRecipient::TriggeringPlayer,
            Recipient::DefendingPlayer => CoreRecipient::DefendingPlayer,
            Recipient::ControllerOf(i) => CoreRecipient::ControllerOf(i),
            Recipient::OwnerOf(i) => CoreRecipient::OwnerOf(i),
            Recipient::EachCreature => CoreRecipient::EachCreature,
            Recipient::EachPlayer => CoreRecipient::EachPlayer,
            Recipient::EachCreatureYouControl => CoreRecipient::EachCreatureYouControl,
            Recipient::EachPermanentYouControl => CoreRecipient::EachPermanentYouControl,
            Recipient::SameObject => CoreRecipient::SameObject,
            Recipient::CreatedObject => CoreRecipient::CreatedObject,
            Recipient::Attached => CoreRecipient::Attached,
        }
    }

    fn sentences(&mut self, ss: &[Sentence]) -> Result<Vec<CoreEffect>, CompileError> {
        let mut out = Vec::new();
        for s in ss {
            for e in &s.effects {
                self.effect(e, &mut out)?;
            }
        }
        Ok(out)
    }

    #[allow(clippy::too_many_lines)]
    fn effect(&mut self, e: &Effect, out: &mut Vec<CoreEffect>) -> Result<(), CompileError> {
        let groups = self.groups.clone();
        let r = |x: Recipient| Self::recipient(&groups, x);
        match e {
            Effect::DealDamage { amount, to } => {
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
            Effect::CreateToken { token, count } => {
                let Some(token_ref) = self.pool.lookup(token) else {
                    return self.err(format!("unknown token \"{token}\""));
                };
                out.push(CoreEffect::CreateToken {
                    token: token_ref,
                    count: *count,
                    who: CoreRecipient::You,
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
            Effect::DiscardChosen { who, filter } => {
                let filter = filter.as_ref().map(|f| self.filter(f));
                out.push(CoreEffect::DiscardChosen {
                    who: r(*who),
                    filter,
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
            Effect::BecomeLandType { to, land } => out.push(CoreEffect::BecomeLandType {
                to: r(*to),
                land: *land,
            }),
            Effect::YouMay(inner) => {
                let effects = self.sentences(inner)?;
                out.push(CoreEffect::YouMay(effects));
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
                let modes = self.modes(modes, false)?;
                out.push(CoreEffect::Modes(modes));
            }
            Effect::SwitchPt { to } => out.push(CoreEffect::SwitchPt { to: r(*to) }),
            Effect::SkipNextUntap { to } => out.push(CoreEffect::SkipNextUntap { to: r(*to) }),
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
            TriggerObj::Filter(f) => CoreTriggerObj::Filter(self.filter(f)),
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
            Trigger::BecomesBlocked(o) => TriggerSpec::BecomesBlocked(self.trigger_obj(o)),
            Trigger::BlocksOrBlocked(o) => TriggerSpec::BlocksOrBlocked(self.trigger_obj(o)),
            Trigger::AttacksOrBlocks(o) => TriggerSpec::AttacksOrBlocks(self.trigger_obj(o)),
            Trigger::Leaves(o) => TriggerSpec::Leaves(self.trigger_obj(o)),
            Trigger::Blocks(o) => TriggerSpec::Blocks(self.trigger_obj(o)),
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
            Affects::Target(i) => CoreAffects::Target(*i),
        }
    }

    fn replacement(&mut self, r: &Replacement) -> Result<ReplacementAbility, CompileError> {
        let replaces = match &r.event {
            Replaces::Damage { from, to, combat } => CoreReplaces::Damage {
                from: from.as_ref().map(|f| self.filter(f)),
                to: self.affects(to),
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
        Ok(match a {
            Ability::Keyword(_) => return self.err("a keyword is not an ability definition"),
            Ability::Mana(m) => {
                // One ability per choice; `Ability::Mana` with several
                // choices is split by the caller through `abilities()`.
                match m.as_slice() {
                    [one] => AbilityDef::TapForMana(*one),
                    _ => return self.err("a `mana` line with several choices must be split"),
                }
            }
            Ability::SacMana(m) => AbilityDef::SacForMana(*m),
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
                )?,
            ),
            Ability::ModalBoth(modes) => AbilityDef::Modal(self.modes(modes, true)?),
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
                from_hand: *cycling || cost.discard_self,
                from_graveyard: cost.from_graveyard || cost.exile_self,
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
                parts: mods
                    .iter()
                    .map(|m| self.modification_or_grant(m))
                    .collect::<Result<_, _>>()?,
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
                Mechanic::Bloodthirst(n) | Mechanic::EscapesWith(n) => {
                    let cond =
                        self.pool
                            .intern_condition(if matches!(m, Mechanic::EscapesWith(_)) {
                                StaticCondition::Escaped
                            } else {
                                StaticCondition::OpponentDamagedThisTurn
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
            Ability::SelfCost { .. } | Ability::TargetCost { .. } | Ability::Affinity(_) => {
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
                para: false,
                eot_first: false,
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
                para: false,
                eot_first: false,
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
