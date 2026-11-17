//! Compiles the syntax tree into `mtg_core::card::CardDef`s in a
//! `CardPool`: interns subtypes, resolves token and emblem names, registers
//! delayed triggered abilities, and fills in the rendered Oracle text.

use std::collections::HashMap;

use mtg_core::card::{
    AbilityDef, ActivatedAbility, Affects as CoreAffects, Builder, CardPool, CardRef,
    CdaCount as CoreCdaCount, CopyExcept, Cost as CoreCost, Effect as CoreEffect, KeywordSet,
    Mode as CoreMode, Modification, ObjFilter, Recipient as CoreRecipient, ReplacementAbility,
    Replaces as CoreReplaces, SacCost as CoreSacCost, StaticAbility, SubtypeSet, SupertypeSet,
    TargetSpec, TriggerObj as CoreTriggerObj, TriggerSpec, TriggeredAbility, TypeSet,
};
use mtg_core::mana::{ColorSet, Mana};
use thiserror::Error;

use crate::ast::{
    Ability, Affects, Card, CdaCount, Cost, Effect, Filter, Kind, Mod, Recipient, Replacement,
    Replaces, SacCost, Sentence, Target, Trigger, TriggerObj,
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
    let mut c = Compiler {
        pool,
        card,
        lookup: by_name,
        groups: Vec::new(),
    };
    let def = c.card()?;
    Ok(pool.add(def))
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
        let mut b = Builder::new(&card.name, &card.cost.to_string())
            .supertypes(card.supertypes.iter().copied())
            .types(card.types.iter().copied())
            .subtypes(&card.subtypes.iter().map(String::as_str).collect::<Vec<_>>())
            .keywords(keywords)
            .text(&render::oracle_text(card, self.lookup));
        if let Some((p, t)) = card.pt {
            b = b.pt(p, t);
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
            .filter(|a| matches!(a, Ability::Spell { .. } | Ability::Modal(_)))
            .count();
        if spells > 1 {
            return self.err(
                "more than one `spell`/`choose one`; put the paragraphs in one `spell { }` block",
            );
        }
        for a in &card.abilities {
            if matches!(a, Ability::Keyword(_)) {
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
        }
    }

    fn targets(&mut self, ts: &[Target]) -> Vec<TargetSpec> {
        ts.iter().map(|t| self.target(t)).collect()
    }

    fn cost(&mut self, c: &Cost) -> CoreCost {
        CoreCost {
            mana: c.mana.clone(),
            discard_self: c.discard_self,
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
            Recipient::You => CoreRecipient::You,
            Recipient::EachOpponent => CoreRecipient::EachOpponent,
            Recipient::Source => CoreRecipient::Source,
            Recipient::TriggeringObject => CoreRecipient::TriggeringObject,
            Recipient::TriggeringPlayer => CoreRecipient::TriggeringPlayer,
            Recipient::DefendingPlayer => CoreRecipient::DefendingPlayer,
            Recipient::EachCreature => CoreRecipient::EachCreature,
            Recipient::EachPlayer => CoreRecipient::EachPlayer,
            Recipient::EachCreatureYouControl => CoreRecipient::EachCreatureYouControl,
            Recipient::EachPermanentYouControl => CoreRecipient::EachPermanentYouControl,
            Recipient::SameObject => CoreRecipient::SameObject,
            Recipient::CreatedObject => CoreRecipient::CreatedObject,
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
            Effect::ReturnToHand { what } => out.push(CoreEffect::ReturnToHand { what: r(*what) }),
            Effect::ReturnToBattlefield { what, counters } => {
                out.push(CoreEffect::ReturnToBattlefield {
                    what: r(*what),
                    counters: *counters,
                });
            }
            Effect::Regenerate { to } => out.push(CoreEffect::Regenerate { to: r(*to) }),
            Effect::Exile { what } => out.push(CoreEffect::Exile { what: r(*what) }),
            Effect::ExileAllGraveyards => out.push(CoreEffect::ExileAllGraveyards),
            Effect::Sacrifice { what } => out.push(CoreEffect::Sacrifice { what: r(*what) }),
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
            Effect::Surveil(n) => out.push(CoreEffect::Surveil { count: *n }),
            Effect::Discard { count, who } => out.push(CoreEffect::Discard {
                count: *count,
                who: who.map_or(CoreRecipient::You, r),
            }),
            Effect::Fight { a, b } => out.push(CoreEffect::Fight { a: r(*a), b: r(*b) }),
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
            } => {
                let subtypes = self.subtypes(subtypes);
                out.push(CoreEffect::Animate {
                    to: r(*to),
                    types: TypeSet::of(types.iter().copied()),
                    subtypes,
                    power: *power,
                    toughness: *toughness,
                });
            }
            Effect::YouMay(inner) => {
                let effects = self.sentences(inner)?;
                out.push(CoreEffect::YouMay(effects));
            }
            Effect::IfKicked(inner) => {
                let effects = self.sentences(inner)?;
                out.push(CoreEffect::IfKicked(effects));
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
            Trigger::Enters(o) => TriggerSpec::Enters(self.trigger_obj(o)),
            Trigger::Dies(o) => TriggerSpec::Dies(self.trigger_obj(o)),
            Trigger::BeginningOfStep { step, whose } => TriggerSpec::BeginningOfStep {
                step: *step,
                whose: *whose,
            },
            Trigger::Attacks(o) => TriggerSpec::Attacks(self.trigger_obj(o)),
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
            Trigger::BecomesTarget(o) => TriggerSpec::BecomesTarget(self.trigger_obj(o)),
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

    fn modification(&mut self, m: &Mod) -> Modification {
        match m {
            Mod::AddTypes(t) => Modification::AddTypes(TypeSet::of(t.iter().copied())),
            Mod::AddLandType(c) => Modification::AddLandType(*c),
            Mod::SetLandType(c) => Modification::SetLandType(*c),
            Mod::AddSubtype(s) => Modification::AddSubtype(self.pool.subtype(s)),
            Mod::Control => Modification::Control,
            Mod::AddColors(c) => Modification::AddColors(ColorSet::of(c.iter().copied())),
            Mod::SetColors(c) => Modification::SetColors(ColorSet::of(c.iter().copied())),
            Mod::AddKeywords(k) => Modification::AddKeywords(KeywordSet::of(k.iter().copied())),
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
            Ability::Enchant(t) => AbilityDef::Enchant(self.target(t)),
            Ability::Spell { targets, effects } => AbilityDef::Spell {
                targets: self.targets(targets),
                effects: self.sentences(effects)?,
            },
            Ability::Modal(modes) => {
                let mut out = Vec::new();
                for m in modes {
                    out.push(CoreMode {
                        targets: self.targets(&m.targets),
                        effects: self.sentences(&m.effects)?,
                        text: render::mode_text(self.card, m, self.lookup)
                            .trim_end_matches('.')
                            .to_string(),
                    });
                }
                AbilityDef::Modal(out)
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
                once_per_turn: cost.once,
                from_hand: *cycling,
                text: render::ability_text(self.card, a, self.lookup),
            }),
            Ability::Triggered {
                trigger,
                condition,
                targets,
                effects,
            } => AbilityDef::Triggered(TriggeredAbility {
                trigger: self.trigger(trigger),
                condition: *condition,
                targets: self.targets(targets),
                effects: self.sentences(effects)?,
            }),
            Ability::Replacement(r) => AbilityDef::Replacement(self.replacement(r)?),
            Ability::Static { filter, mods } => AbilityDef::Static(StaticAbility {
                affected: self.filter(filter),
                parts: mods.iter().map(|m| self.modification(m)).collect(),
            }),
            Ability::EntersAsCopy(f) => AbilityDef::EntersAsCopy(self.filter(f)),
        })
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
