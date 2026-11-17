//! Turns compiled definitions back into syntax trees: the migration path
//! from Rust-authored cards to the DSL, and a check that the DSL can
//! express every definition the engine holds.

use mtg_core::card::{
    AbilityDef, Affects as CoreAffects, CardDef, CardPool, CardType, CdaCount as CoreCdaCount,
    Cost as CoreCost, Effect as CoreEffect, KeywordSet, Modification, ObjFilter,
    Recipient as CoreRecipient, ReplacementAbility, Replaces as CoreReplaces,
    SacCost as CoreSacCost, Supertype, SupertypeSet, TargetSpec, TriggerObj as CoreTriggerObj,
    TriggerSpec, TypeSet,
};
use mtg_core::mana::{Color, ColorSet};

use crate::ast::{
    Ability, Affects, Card, CdaCount, Cost, Effect, Except, Filter, Kind, Mod, Mode, Recipient,
    Replacement, Replaces, SacCost, Sentence, Target, Trigger, TriggerObj,
};

const ALL_TYPES: [CardType; 9] = [
    CardType::Artifact,
    CardType::Battle,
    CardType::Creature,
    CardType::Enchantment,
    CardType::Instant,
    CardType::Kindred,
    CardType::Land,
    CardType::Planeswalker,
    CardType::Sorcery,
];

const ALL_SUPERTYPES: [Supertype; 4] = [
    Supertype::Basic,
    Supertype::Legendary,
    Supertype::Snow,
    Supertype::World,
];

fn types(t: TypeSet) -> Vec<CardType> {
    ALL_TYPES.into_iter().filter(|x| t.contains(*x)).collect()
}

fn supertypes(t: SupertypeSet) -> Vec<Supertype> {
    ALL_SUPERTYPES
        .into_iter()
        .filter(|x| t.contains(*x))
        .collect()
}

fn colors(c: ColorSet) -> Vec<Color> {
    c.iter().collect()
}

fn keywords(k: KeywordSet) -> Vec<mtg_core::card::Keyword> {
    k.iter().collect()
}

pub fn decompile(pool: &CardPool, def: &CardDef) -> Card {
    let d = Decompiler { pool };
    Card {
        kind: if def.token {
            Kind::Token
        } else if def.emblem {
            Kind::Emblem
        } else {
            Kind::Card
        },
        name: def.name.clone(),
        oracle_id: None,
        cost: def.mana_cost.clone(),
        color_indicator: def.color_indicator.map(colors),
        supertypes: supertypes(def.supertypes),
        types: types(def.types),
        subtypes: def.subtypes.clone(),
        pt: match (def.power, def.toughness) {
            (Some(p), Some(t)) => Some((p, t)),
            _ => None,
        },
        loyalty: (def.loyalty > 0).then_some(def.loyalty),
        keywords: keywords(def.keywords),
        additional: (def.additional != CoreCost::default()).then(|| d.cost(&def.additional)),
        abilities: def.abilities.iter().map(|a| d.ability(a)).collect(),
        except: def.copy_except.as_ref().map(|x| Except {
            subtypes: x.subtypes.clone(),
            keywords: keywords(x.keywords),
            abilities: x.abilities.iter().map(|a| d.ability(a)).collect(),
        }),
        mismatch: None,
        line: 0,
    }
}

struct Decompiler<'a> {
    pool: &'a CardPool,
}

impl Decompiler<'_> {
    fn filter(&self, f: &ObjFilter) -> Filter {
        Filter {
            who: f.who,
            types: types(f.types),
            not_types: types(f.not_types),
            supertypes: supertypes(f.supertypes),
            not_supertypes: supertypes(f.not_supertypes),
            colors: f.colors.map(colors),
            subtypes: f
                .subtypes
                .iter()
                .map(|s| self.pool.subtype_name(*s).to_string())
                .collect(),
            not_subtypes: f
                .not_subtypes
                .iter()
                .map(|s| self.pool.subtype_name(*s).to_string())
                .collect(),
            relation: f.relation,
            enchanted: f.enchanted,
            no_abilities: f.no_abilities,
        }
    }

    fn target(&self, t: TargetSpec) -> Target {
        match t {
            TargetSpec::CreatureWithSubtype(s) => {
                Target::CreatureOfSubtype(self.pool.subtype_name(s).to_string())
            }
            other => Target::Spec(other),
        }
    }

    fn cost(&self, c: &CoreCost) -> Cost {
        Cost {
            mana: c.mana.clone(),
            tap: c.tap,
            life: c.life,
            discard: c.discard,
            loyalty: (c.loyalty != 0).then_some(c.loyalty),
            sacrifice: c.sacrifice.as_ref().map(|s| match s {
                CoreSacCost::This => SacCost::This,
                CoreSacCost::Filter(f) => SacCost::Filter(self.filter(f)),
            }),
            sorcery: false,
        }
    }

    fn recipient(r: CoreRecipient) -> Recipient {
        match r {
            CoreRecipient::Target(i) => Recipient::Target(i),
            CoreRecipient::You => Recipient::You,
            CoreRecipient::EachOpponent => Recipient::EachOpponent,
            CoreRecipient::Source => Recipient::Source,
            CoreRecipient::TriggeringObject => Recipient::TriggeringObject,
            CoreRecipient::TriggeringPlayer => Recipient::TriggeringPlayer,
            CoreRecipient::DefendingPlayer => Recipient::DefendingPlayer,
            CoreRecipient::EachCreature => Recipient::EachCreature,
            CoreRecipient::EachPlayer => Recipient::EachPlayer,
            CoreRecipient::EachCreatureYouControl => Recipient::EachCreatureYouControl,
            CoreRecipient::EachPermanentYouControl => Recipient::EachPermanentYouControl,
            CoreRecipient::SameObject => Recipient::SameObject,
            CoreRecipient::CreatedObject => Recipient::CreatedObject,
        }
    }

    fn sentences(&self, effects: &[CoreEffect]) -> Vec<Sentence> {
        effects
            .iter()
            .map(|e| Sentence::new(vec![self.effect(e)]))
            .collect()
    }

    #[allow(clippy::too_many_lines)]
    fn effect(&self, e: &CoreEffect) -> Effect {
        let r = Self::recipient;
        match e {
            CoreEffect::DealDamage { amount, to } => Effect::DealDamage {
                amount: *amount,
                to: vec![r(*to)],
            },
            CoreEffect::Pump {
                power,
                toughness,
                to,
            } => Effect::Pump {
                power: *power,
                toughness: *toughness,
                to: r(*to),
            },
            CoreEffect::Destroy { to } => Effect::Destroy {
                to: vec![r(*to)],
                no_regen: false,
            },
            CoreEffect::DestroyNoRegen { to } => Effect::Destroy {
                to: vec![r(*to)],
                no_regen: true,
            },
            CoreEffect::GainLife { amount, who } => Effect::GainLife {
                amount: *amount,
                who: r(*who),
            },
            CoreEffect::LoseLife { amount, who } => Effect::LoseLife {
                amount: *amount,
                who: r(*who),
            },
            CoreEffect::DrawCards { count, who } => Effect::DrawCards {
                count: *count,
                who: (*who != CoreRecipient::You).then(|| r(*who)),
            },
            CoreEffect::Mill { count, who } => Effect::Mill {
                count: *count,
                who: r(*who),
            },
            CoreEffect::AddCounters { kind, count, to } => Effect::AddCounters {
                kind: *kind,
                count: *count,
                to: r(*to),
            },
            CoreEffect::RemoveCounters { kind, count, to } => Effect::RemoveCounters {
                kind: *kind,
                count: *count,
                to: r(*to),
            },
            CoreEffect::RemoveAllCounters { to } => Effect::RemoveAllCounters { to: r(*to) },
            CoreEffect::ReturnToHand { what } => Effect::ReturnToHand { what: r(*what) },
            CoreEffect::ReturnToBattlefield { what, counters } => Effect::ReturnToBattlefield {
                what: r(*what),
                counters: *counters,
            },
            CoreEffect::Regenerate { to } => Effect::Regenerate { to: r(*to) },
            CoreEffect::Exile { what } => Effect::Exile { what: r(*what) },
            CoreEffect::ExileAllGraveyards => Effect::ExileAllGraveyards,
            CoreEffect::Sacrifice { what } => Effect::Sacrifice { what: r(*what) },
            CoreEffect::Tap { to } => Effect::Tap { to: vec![r(*to)] },
            CoreEffect::Untap { to } => Effect::Untap { to: vec![r(*to)] },
            CoreEffect::Grant { keywords: k, to } => Effect::Grant {
                keywords: keywords(*k),
                to: r(*to),
            },
            CoreEffect::GainControl { of } => Effect::GainControl { of: r(*of) },
            CoreEffect::Attach { to } => Effect::Attach { to: r(*to) },
            CoreEffect::CreateToken { token, count, .. } => Effect::CreateToken {
                token: self.pool.get(*token).name.clone(),
                count: *count,
            },
            CoreEffect::CreateTokenCopy { of, except } => Effect::CreateTokenCopy {
                of: r(*of),
                except: *except,
            },
            CoreEffect::BecomeCopy { of, except } => Effect::BecomeCopy {
                of: r(*of),
                except: *except,
            },
            CoreEffect::CreateEmblem { emblem } => Effect::CreateEmblem {
                emblem: self.pool.get(*emblem).name.clone(),
            },
            CoreEffect::Animate {
                to,
                types: t,
                subtypes,
                power,
                toughness,
            } => Effect::Animate {
                to: r(*to),
                types: types(*t),
                subtypes: subtypes
                    .iter()
                    .map(|s| self.pool.subtype_name(*s).to_string())
                    .collect(),
                power: *power,
                toughness: *toughness,
            },
            CoreEffect::YouMay(inner) => Effect::YouMay(self.sentences(inner)),
            CoreEffect::Delayed { ability, about } => {
                let AbilityDef::Triggered(t) = self.pool.ability(*ability) else {
                    panic!("delayed ability is not triggered");
                };
                Effect::Delayed {
                    about: r(*about),
                    trigger: self.trigger(&t.trigger),
                    effects: self.sentences(&t.effects),
                    trigger_first: false,
                }
            }
            CoreEffect::CreateReplacement(rep) => Effect::CreateReplacement(self.replacement(rep)),
            CoreEffect::Rule(k) => Effect::Rule(*k),
        }
    }

    fn trigger_obj(&self, o: &CoreTriggerObj) -> TriggerObj {
        match o {
            CoreTriggerObj::This => TriggerObj::This,
            CoreTriggerObj::Filter(f) => TriggerObj::Filter(self.filter(f)),
        }
    }

    fn trigger(&self, t: &TriggerSpec) -> Trigger {
        match t {
            TriggerSpec::Enters(o) => Trigger::Enters(self.trigger_obj(o)),
            TriggerSpec::Dies(o) => Trigger::Dies(self.trigger_obj(o)),
            TriggerSpec::BeginningOfStep { step, whose } => Trigger::BeginningOfStep {
                step: *step,
                whose: *whose,
            },
            TriggerSpec::Attacks(o) => Trigger::Attacks(self.trigger_obj(o)),
            TriggerSpec::Blocks(o) => Trigger::Blocks(self.trigger_obj(o)),
            TriggerSpec::DealsCombatDamageToPlayer(o) => {
                Trigger::DealsCombatDamageToPlayer(self.trigger_obj(o))
            }
            TriggerSpec::GainsLife { whose } => Trigger::GainsLife { whose: *whose },
            TriggerSpec::CastsSpell {
                whose,
                noncreature,
                instant_or_sorcery,
            } => Trigger::CastsSpell {
                whose: *whose,
                noncreature: *noncreature,
                instant_or_sorcery: *instant_or_sorcery,
            },
            TriggerSpec::BecomesTarget(o) => Trigger::BecomesTarget(self.trigger_obj(o)),
        }
    }

    fn affects(&self, a: &CoreAffects) -> Affects {
        match a {
            CoreAffects::Any => Affects::Any,
            CoreAffects::This => Affects::This,
            CoreAffects::You => Affects::You,
            CoreAffects::Players(w) => Affects::Players(*w),
            CoreAffects::Objects(f) => Affects::Objects(self.filter(f)),
            CoreAffects::YouAnd(f) => Affects::YouAnd(self.filter(f)),
            CoreAffects::Target(i) => Affects::Target(*i),
        }
    }

    fn replacement(&self, r: &ReplacementAbility) -> Replacement {
        Replacement {
            event: match &r.replaces {
                CoreReplaces::Damage { from, to, combat } => Replaces::Damage {
                    from: from.as_ref().map(|f| self.filter(f)),
                    to: self.affects(to),
                    combat: *combat,
                },
                CoreReplaces::GainLife { whose } => Replaces::GainLife { whose: *whose },
                CoreReplaces::Draw {
                    whose,
                    library_empty,
                } => Replaces::Draw {
                    whose: *whose,
                    library_empty: *library_empty,
                },
                CoreReplaces::ToGraveyard {
                    whose,
                    from_battlefield,
                } => Replaces::ToGraveyard {
                    whose: *whose,
                    from_battlefield: *from_battlefield,
                },
                CoreReplaces::PlusCounters { on } => Replaces::PlusCounters {
                    on: self.filter(on),
                },
                CoreReplaces::Destroy { to } => Replaces::Destroy {
                    to: self.affects(to),
                },
                CoreReplaces::Enters => Replaces::Enters,
            },
            outcome: r.replacement,
            then: self.sentences(&r.then),
        }
    }

    fn modification(&self, m: &Modification) -> Mod {
        match m {
            Modification::AddTypes(t) => Mod::AddTypes(types(*t)),
            Modification::AddLandType(c) => Mod::AddLandType(*c),
            Modification::SetLandType(c) => Mod::SetLandType(*c),
            Modification::AddSubtype(s) => Mod::AddSubtype(self.pool.subtype_name(*s).to_string()),
            Modification::Control => Mod::Control,
            Modification::AddColors(c) => Mod::AddColors(colors(*c)),
            Modification::SetColors(c) => Mod::SetColors(colors(*c)),
            Modification::AddKeywords(k) => Mod::AddKeywords(keywords(*k)),
            Modification::RemoveAllAbilities => Mod::RemoveAllAbilities,
            Modification::SetPt { power, toughness } => Mod::SetPt {
                power: *power,
                toughness: *toughness,
            },
            Modification::ModifyPt { power, toughness } => Mod::ModifyPt {
                power: *power,
                toughness: *toughness,
            },
            Modification::SwitchPt => Mod::SwitchPt,
            Modification::CdaColors(c) => Mod::CdaColors(colors(*c)),
            Modification::CdaPt {
                count,
                plus_toughness,
            } => Mod::CdaPt {
                count: match count {
                    CoreCdaCount::CardTypesInGraveyards => CdaCount::CardTypesInGraveyards,
                    CoreCdaCount::CreatureCardsInGraveyards => CdaCount::CreatureCardsInGraveyards,
                    CoreCdaCount::Permanents(f) => CdaCount::Permanents(self.filter(f)),
                    CoreCdaCount::ControllerLife => CdaCount::ControllerLife,
                },
                plus_toughness: *plus_toughness,
            },
        }
    }

    fn ability(&self, a: &AbilityDef) -> Ability {
        match a {
            AbilityDef::TapForMana(m) => Ability::Mana(vec![*m]),
            AbilityDef::Enchant(t) => Ability::Enchant(self.target(*t)),
            AbilityDef::Spell { targets, effects } => Ability::Spell {
                targets: targets.iter().map(|t| self.target(*t)).collect(),
                effects: self.sentences(effects),
            },
            AbilityDef::Modal(modes) => Ability::Modal(
                modes
                    .iter()
                    .map(|m| Mode {
                        targets: m.targets.iter().map(|t| self.target(*t)).collect(),
                        effects: self.sentences(&m.effects),
                    })
                    .collect(),
            ),
            AbilityDef::Activated(act) => {
                let equip = act.sorcery_speed
                    && act.targets == [TargetSpec::CreatureYouControl]
                    && act.effects
                        == [CoreEffect::Attach {
                            to: CoreRecipient::Target(0),
                        }]
                    && !act.cost.tap
                    && act.cost.sacrifice.is_none();
                let mut cost = self.cost(&act.cost);
                cost.sorcery = act.sorcery_speed;
                Ability::Activated {
                    cost,
                    targets: act.targets.iter().map(|t| self.target(*t)).collect(),
                    effects: self.sentences(&act.effects),
                    equip,
                }
            }
            AbilityDef::Triggered(t) => Ability::Triggered {
                trigger: self.trigger(&t.trigger),
                condition: t.condition,
                targets: t.targets.iter().map(|x| self.target(*x)).collect(),
                effects: self.sentences(&t.effects),
            },
            AbilityDef::Replacement(r) => Ability::Replacement(self.replacement(r)),
            AbilityDef::Static(s) => Ability::Static {
                filter: self.filter(&s.affected),
                mods: s.parts.iter().map(|m| self.modification(m)).collect(),
            },
            AbilityDef::EntersAsCopy(f) => Ability::EntersAsCopy(self.filter(f)),
        }
    }
}
