//! Turns compiled definitions back into syntax trees: the migration path
//! from Rust-authored cards to the DSL, and a check that the DSL can
//! express every definition the engine holds.

use mtg_core::card::{
    AbilityDef, Affects as CoreAffects, CardDef, CardPool, CardType, ChoiceKind as CoreChoiceKind,
    Cost as CoreCost, Effect as CoreEffect, KeywordSet, Modification, ObjFilter,
    Recipient as CoreRecipient, ReplacementAbility, Replaces as CoreReplaces,
    SacCost as CoreSacCost, StaticCondition, Supertype, SupertypeSet, TargetSpec,
    TriggerObj as CoreTriggerObj, TriggerSpec, TurnCount, TurnEvent, TurnWho, TypeSet,
};
use mtg_core::mana::{Color, ColorSet};

use crate::ast::{
    Ability, Affects, Card, ChoiceKind, Cost, Effect, Except, Filter, Kind, Mod, Mode, Recipient,
    Replacement, Replaces, Restrict, SacCost, Sentence, StaticCond, Target, Trigger, TriggerObj,
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

/// The card types of a set, in the card language's order.
fn card_types(t: TypeSet) -> Vec<CardType> {
    crate::render::CARD_TYPES
        .into_iter()
        .filter(|c| t.contains(*c))
        .collect()
}

/// A turn count's condition (`StaticCondition::Happened`) as the card
/// language words it.
fn happened(count: &TurnCount, cmp: mtg_core::card::Cmp, n: u32) -> StaticCond {
    use mtg_core::card::Cmp;
    match (count.kind, count.who, cmp, n) {
        (TurnEvent::LifeGained, TurnWho::You, Cmp::AtLeast, 1) => {
            StaticCond::GainedLifeThisTurn { or_lost: false }
        }
        (TurnEvent::LifeGained, _, _, n) => StaticCond::LifeGainedThisTurn(n),
        (TurnEvent::LifeLost, TurnWho::You, ..) => StaticCond::LostLifeThisTurn { gained: false },
        (TurnEvent::LifeLost, ..) => StaticCond::OpponentLostLifeThisTurn,
        (TurnEvent::Damaged, ..) => StaticCond::OpponentDamagedThisTurn,
        (TurnEvent::DamageTaken, _, _, n) => StaticCond::OpponentDamageThisTurn(n),
        (TurnEvent::NoncombatDamaged, ..) => {
            StaticCond::OpponentNoncombatDamagedThisTurn { was: false }
        }
        (TurnEvent::CreatureEntered, TurnWho::You, _, 1) => {
            StaticCond::CreatureEnteredThisTurn { other: count.other }
        }
        (TurnEvent::CreatureEntered, who, _, n) => StaticCond::CreaturesEnteredThisTurn {
            n: u8::try_from(n).unwrap_or(u8::MAX),
            opponent: who == TurnWho::Opponent,
        },
        (TurnEvent::NonlandEntered, _, _, n) => StaticCond::NonlandEnteredThisTurn(n),
        (TurnEvent::LandEntered, ..) => StaticCond::LandEnteredThisTurn,
        (TurnEvent::ArtifactEntered, ..) => StaticCond::ArtifactEnteredThisTurn,
        (TurnEvent::FaceDownEntered, ..) => StaticCond::FaceDownEnteredThisTurn,
        (TurnEvent::PermanentLeft, ..) => StaticCond::PermanentLeftThisTurn,
        (TurnEvent::Died, TurnWho::Anyone, _, 1) => StaticCond::CreatureDiedThisTurn,
        (TurnEvent::Died, who, _, n) => StaticCond::CreaturesDied {
            n,
            yours: who == TurnWho::You,
        },
        (TurnEvent::ToGraveyard, ..) => {
            StaticCond::PutIntoGraveyardThisTurn(card_types(count.types))
        }
        // Only as void's (an `Or` of both).
        (TurnEvent::NonlandLeft | TurnEvent::Warped, ..) => StaticCond::Void,
        (TurnEvent::CreatureLeft, ..) => unreachable!("only counted"),
        (TurnEvent::Drew, _, _, n) => StaticCond::CardsDrawnThisTurn(n),
        (TurnEvent::Discarded, ..) => StaticCond::DiscardedThisTurn,
        (TurnEvent::Descended, ..) => StaticCond::DescendedThisTurn,
        (TurnEvent::LeftGraveyard, ..) => StaticCond::LeftGraveyardThisTurn,
        (TurnEvent::CardToGraveyard, ..) => {
            StaticCond::CardToGraveyardThisTurn(card_types(count.types))
        }
        (TurnEvent::Sacrificed, ..) => StaticCond::SacrificedThisTurn(card_types(count.types)),
        (TurnEvent::Scried | TurnEvent::Surveilled, ..) => StaticCond::SurveilledThisTurn,
        (TurnEvent::Crime, ..) => StaticCond::CrimeThisTurn,
        (TurnEvent::PlusCountersOn, ..) => StaticCond::CounterPutThisTurn,
        (TurnEvent::PlusCountersBy, ..) => StaticCond::PutCounterThisTurn,
        (TurnEvent::CountersOnCreatureBy, ..) => StaticCond::PutAnyCounterThisTurn,
        (TurnEvent::Attacked, ..) => StaticCond::AttackedThisTurn,
        (TurnEvent::Equipped, ..) => unreachable!("only the first equip ability"),
        (TurnEvent::AssassinDamage, ..) => unreachable!("only freerunning's alternative cost"),
        (TurnEvent::Entered, ..) => unreachable!("only with a filter"),
        (TurnEvent::Cast, _, Cmp::Exactly, n) => StaticCond::SpellNumber(n + 1),
        (TurnEvent::Cast, ..) => StaticCond::NoSpellsCastThisTurn { from_hand: false },
        (TurnEvent::CastFromHand, ..) => StaticCond::NoSpellsCastThisTurn { from_hand: true },
    }
}

fn colors(c: ColorSet) -> Vec<Color> {
    c.iter().collect()
}

fn keywords(k: KeywordSet) -> Vec<mtg_core::card::Keyword> {
    k.iter()
        .filter(|k| {
            *k != mtg_core::card::Keyword::ProtectionFromFilter
                && *k != mtg_core::card::Keyword::HexproofFromFilter
        })
        .collect()
}

#[allow(clippy::too_many_lines)] // one field per line of the card
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
        display_name: None,
        layout: mtg_core::card::Layout::Normal,
        faces: Vec::new(),
        counts: Vec::new(),
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
        defense: (def.defense > 0).then_some(def.defense),
        keywords: keywords(def.keywords),
        additional: (def.additional != CoreCost::default()).then(|| d.cost(&def.additional)),
        additional_or: def.additional_or.as_deref().map(|c| d.cost(c)),
        optional_cost: def.optional_cost.as_deref().map(|c| d.cost(c)),
        alt_cost: def.alt_cost.as_deref().map(|a| {
            let mut cost = d.cost(&a.cost);
            cost.conditions = a
                .condition
                .map(|c| d.static_cond(pool.condition(c)))
                .into_iter()
                .collect();
            crate::ast::AltCost {
                cost,
                count: a.count,
                others_gain: a.others_gain,
                cond_late: false,
            }
        }),
        flash_if: def.flash_if.map(|c| d.static_cond(pool.condition(c))),
        flash_if_may: false,
        flash_if_named: false,
        flash_cleanup: def.flash_cleanup,
        pronoun: None,
        short_name: None,
        unkicked_targets: def.unkicked_targets.map(usize::from),
        kicked_drops: def.kicked_drops.map(usize::from),
        cast_only: def.cast_only.map(|c| d.static_cond(pool.condition(c))),
        cast_only_cant: false,
        kicker: def
            .kicker
            .as_deref()
            .filter(|_| def.kicker_cost.is_none())
            .cloned(),
        kicker_cost: def.kicker_cost.as_deref().map(|c| d.cost(c)),
        kicker_targets: def.kicker_targets,
        flashback: def
            .flashback
            .as_ref()
            .filter(|_| def.graveyard_cast.is_none())
            .map(|c| d.cost(c)),
        graveyard_alias: def.graveyard_alias.clone(),
        graveyard_cast: def.graveyard_cast.as_deref().map(|g| {
            let fb = def.flashback.as_deref().cloned().unwrap_or_default();
            let mut rest = fb.clone();
            rest.mana = mtg_core::mana::ManaCost::default();
            crate::ast::GraveyardCast {
                cond: g
                    .condition
                    .filter(|_| !g.max_speed)
                    .map(|c| d.static_cond(pool.condition(c))),
                as_long_as: g.as_long_as,
                max_speed: g.max_speed,
                mana: g.alt_mana.then(|| fb.mana.clone()),
                extra: (!g.alt_mana && rest != CoreCost::default()).then(|| d.cost(&rest)),
            }
        }),
        groups: pool.filters().iter().map(|f| d.filter(f)).collect(),
        // Unleash's second static ability comes back with its keyword line.
        abilities: def
            .abilities
            .iter()
            .filter(|a| **a != crate::compile::unleash_static())
            .map(|a| match a {
                // CR 702.172a: the single modes are the sets of one bit.
                mtg_core::card::AbilityDef::Modal(modes) if !def.tiered.is_empty() => {
                    crate::ast::Ability::Tiered(d.tiered(&def.tiered, modes))
                }
                mtg_core::card::AbilityDef::Modal(modes) if !def.spree.is_empty() => {
                    crate::ast::Ability::Spree(
                        def.spree
                            .iter()
                            .enumerate()
                            .filter_map(|(i, c)| {
                                modes.get((1 << i) - 1).map(|m| (c.clone(), d.mode(m)))
                            })
                            .collect(),
                    )
                }
                _ => d.ability(a),
            })
            .collect(),
        except: def.copy_except.as_ref().map(|x| Except {
            subtypes: x.subtypes.clone(),
            keywords: keywords(x.keywords),
            abilities: x.abilities.iter().map(|a| d.ability(a)).collect(),
            not_legendary: x.not_legendary,
            its_not: false,
            types: x.types.iter().collect(),
            pt: x.pt,
            colors: x.colors.iter().collect(),
            legendary: x.legendary,
            exactly: x.exactly,
            the_token: false,
            and_is: false,
        }),
        mismatch: None,
        line: 0,
    }
}

struct Decompiler<'a> {
    pool: &'a CardPool,
}

impl Decompiler<'_> {
    /// A characteristic-defining ability's count (`Modification::CdaPt`):
    /// the counts a static ability can judge (`layers::per_count`); others
    /// read as domain.
    fn cda_count(&self, c: &mtg_core::card::CountExpr) -> crate::ast::CountExpr {
        use crate::ast::CountExpr as A;
        use mtg_core::card::CountExpr as C;
        match c {
            C::Permanents(f) => A::Permanents(self.filter(f)),
            C::Life => A::Life,
            C::Speed => A::Speed,
            C::CardsInHand => A::CardsInHand,
            C::CardsInGraveyard(f) => A::CardsInGraveyard(f.as_ref().map(|f| self.filter(f))),
            C::CardsInAllGraveyards(f) => A::CardsInGraveyards {
                filter: f.as_ref().map(|f| self.filter(f)),
                each: false,
            },
            C::CardTypesInAllGraveyards => A::CardTypesInGraveyards,
            C::CardTypesInGraveyard => A::CardTypesInGraveyard,
            C::Plus(n, inner) => A::Plus {
                n: *n,
                word: false,
                inner: Box::new(self.cda_count(inner)),
            },
            C::Times(n, inner) => A::Times(*n, Box::new(self.cda_count(inner))),
            C::Divided { by, up, inner } => A::Divided {
                by: *by,
                up: *up,
                inner: Box::new(self.cda_count(inner)),
            },
            C::Sum(a, b) => A::Sum(Box::new(self.cda_count(a)), Box::new(self.cda_count(b))),
            _ => A::Domain,
        }
    }

    /// What restricted mana may be spent on (CR 106.6).
    fn mana_restriction(&self, r: &mtg_core::card::ManaRestriction) -> crate::ast::ManaRestriction {
        crate::ast::ManaRestriction {
            spells: r.spells.as_ref().map(|f| self.filter(f)),
            abilities: r.abilities,
            ability_source: r.ability_source.as_ref().map(|f| self.filter(f)),
            foretell: r.foretell,
            disturb: r.disturb,
            wording: r.wording,
            plural: r.plural,
            bare: r.bare,
            ways: r.ways,
            x_spells: r.x_spells.as_ref().map(|f| Box::new(self.filter(f))),
        }
    }

    #[allow(clippy::too_many_lines)] // one field per line
    fn filter(&self, f: &ObjFilter) -> Filter {
        Filter {
            who: f.who,
            types: types(f.types),
            not_types: types(f.not_types),
            supertypes: supertypes(f.supertypes),
            not_supertypes: supertypes(f.not_supertypes),
            colors: f.colors.map(colors),
            not_colors: colors(f.not_colors),
            keywords: keywords(f.keywords),
            not_keywords: keywords(f.not_keywords),
            all: false,
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
            enchanted_permanent: false,
            paired: f.paired.then_some(false),
            no_abilities: f.no_abilities,
            dont_control: false,
            your_opponents: false,
            they_control: false,
            per_opponent: false,
            per_player: false,
            defending: false,
            any_types: types(f.any_types),
            power: f.power,
            toughness: f.toughness,
            mana_value: f.mana_value,
            color_count: f.color_count,
            tapped: f.tapped,
            attacking: f.attacking,
            blocking: f.blocking,
            damaged: f.damaged,
            damaged_short: false,
            each: false,
            status: f.status,
            chosen: (f.chosen && f.status.equal_chosen.is_none())
                .then_some(crate::ast::OfChosen::ChosenType),
            same: f.same.map(|r| {
                use crate::ast::SameRef;
                use mtg_core::card::{SameOf, SameWhat};
                match (r.of, r.what) {
                    (SameOf::Source, SameWhat::NotName) => SameRef::NotNamedThis,
                    (SameOf::Source, SameWhat::LesserPower) => SameRef::LesserPower,
                    (SameOf::Source, SameWhat::ManaValueAtMostPower) => {
                        SameRef::ManaValueAtMostPower
                    }
                    (SameOf::Source, _) => SameRef::NamedThis,
                    (SameOf::Target(n), SameWhat::Color) => {
                        SameRef::SharesColorWith(usize::from(n))
                    }
                    (SameOf::Target(_), SameWhat::SharesType) => SameRef::ShareCardType,
                    (SameOf::Target(_), SameWhat::SharesCreatureType) => SameRef::ShareCreatureType,
                    (SameOf::Target(_), SameWhat::DifferentController) => {
                        SameRef::DifferentControllers
                    }
                    (SameOf::Target(_), SameWhat::SameController)
                        if f.who == Some(mtg_core::card::Who::Opponents) =>
                    {
                        SameRef::SameOpponent
                    }
                    (SameOf::Target(_), SameWhat::SameController) => SameRef::SameController,
                    (SameOf::Target(n), _) => SameRef::SameNameAs(usize::from(n)),
                }
            }),
            named_card: f.named.map(|(d, _)| self.pool.get(d).name.clone()),
            named_first: false,
            enchanted_late: false,
            equipped_late: false,
            attacking_late: false,
            attached_to: f.attached_to.map(|a| {
                use crate::ast::AttachedRef;
                use mtg_core::card::AttachedTo;
                match a {
                    AttachedTo::Source => AttachedRef::Source,
                    AttachedTo::SourceHost => AttachedRef::Host,
                    AttachedTo::TriggeringObject => AttachedRef::TriggeringObject,
                    AttachedTo::Target(n) => AttachedRef::Target(usize::from(n)),
                }
            }),
            combat_with: f.combat_with.map(|w| {
                use crate::ast::CombatRef;
                use mtg_core::card::CombatWith;
                match w {
                    CombatWith::Blocking => CombatRef::BlockingThis,
                    CombatWith::BlockedBy => CombatRef::BlockedByThis,
                    CombatWith::BlockingOrBlockedBy => CombatRef::BlockingOrBlockedByThis,
                }
            }),
            that_player: f.that_player,
            that_opponent: false,
            their: false,
            not_attached: f.not_attached,
            enchanted_player: f.enchanted_player,
            other_than_this: false,
            not_granter: f.not_granter,
            unchosen_wording: crate::ast::UnchosenWording::Other,
            and_or: false,
            and_list: false,
            that_are: false,
            all_word: false,
            article: false,
            control_late: false,
            subtype_neg_first: false,
            neg_before_color: false,
            plural: false,
            or: f.or.as_ref().map(|a| Box::new(self.filter(a))),
            alt_article: false,
            alt_card: false,
            targeting: f.targeting.as_ref().map(|t| {
                Box::new(crate::ast::Targeting {
                    you: t.you,
                    player: t.player,
                    filter: t.filter.as_ref().map(|g| self.filter(g)),
                })
            }),
        }
    }

    fn target(&self, t: TargetSpec) -> Target {
        match t {
            TargetSpec::CreatureWithSubtype(s) => {
                Target::CreatureOfSubtype(self.pool.subtype_name(s).to_string())
            }
            TargetSpec::Filtered(i) => Target::Filter(self.filter(self.pool.filter(i))),
            TargetSpec::SpellFiltered(i) => Target::Spell(self.filter(self.pool.filter(i))),
            TargetSpec::SpellOr(i) => Target::SpellOr(self.filter(self.pool.filter(i))),
            TargetSpec::GraveyardCard {
                filter,
                who,
                graveyards,
            } => Target::GraveyardCard {
                filter: self.filter(self.pool.filter(filter)),
                who,
                graveyards,
            },
            TargetSpec::UpTo(_) => Target::UpTo(Box::new(self.target(self.pool.base_spec(t)))),
            TargetSpec::Many(i) => {
                let m = self.pool.many(i);
                Target::Many {
                    target: Box::new(self.target(m.spec)),
                    min: m.min,
                    max: m.max,
                    total: m.total_mv,
                    different: m.different_names,
                }
            }
            other => Target::Spec(other),
        }
    }

    /// Tiered (CR 702.183): each mode with its cost.
    fn tiered(
        &self,
        costs: &[mtg_core::mana::ManaCost],
        modes: &[mtg_core::card::Mode],
    ) -> Vec<(mtg_core::mana::ManaCost, Mode)> {
        costs
            .iter()
            .zip(modes)
            .map(|(c, m)| (c.clone(), self.mode(m)))
            .collect()
    }

    fn mode(&self, m: &mtg_core::card::Mode) -> Mode {
        Mode {
            targets: m.targets.iter().map(|t| self.target(*t)).collect(),
            effects: self.sentences(&m.effects),
            name: m.name.clone(),
        }
    }

    /// "Choose two": only pairs; the first ones, (0, 1), (0, 2), ...,
    /// give every single mode. `None` for other modes.
    fn modal_two(&self, modes: &[mtg_core::card::Mode]) -> Option<Vec<Mode>> {
        if modes.is_empty() || !modes.iter().all(|m| m.split.is_some()) {
            return None;
        }
        let n = (2..=modes.len() + 1)
            .find(|n| n * (n - 1) / 2 == modes.len())
            .unwrap_or(2);
        let mut singles = Vec::new();
        for (i, m) in modes.iter().take(n - 1).enumerate() {
            let (e, t) = m
                .split
                .map_or((0, 0), |(e, t)| (usize::from(e), usize::from(t)));
            if i == 0 {
                singles.push(self.mode_part(m, ..e, ..t));
            }
            singles.push(self.mode_part(m, e.., t..));
        }
        Some(singles)
    }

    /// One mode of a "choose two" pair: some of its effects and targets.
    fn mode_part<E, T>(&self, m: &mtg_core::card::Mode, effects: E, targets: T) -> Mode
    where
        E: std::slice::SliceIndex<[CoreEffect], Output = [CoreEffect]>,
        T: std::slice::SliceIndex<[TargetSpec], Output = [TargetSpec]>,
    {
        Mode {
            targets: m.targets[targets].iter().map(|t| self.target(*t)).collect(),
            effects: self.sentences(&m.effects[effects]),
            name: None,
        }
    }

    fn cost(&self, c: &CoreCost) -> Cost {
        Cost {
            mana: c.mana.clone(),
            discard_self: c.discard_self,
            per_turn: 0,
            keyword: None,
            tap: c.tap,
            life: c.life,
            discard: c.discard,
            discard_x: c.discard_x,
            discard_random: c.discard_random,
            discard_filter: c.discard_filter.as_ref().map(|f| self.filter(f)),
            discard_exile: c.discard_exile,
            loyalty: (c.loyalty != 0).then_some(c.loyalty),
            sacrifice: c.sacrifice.as_ref().map(|s| match s {
                CoreSacCost::This => SacCost::This,
                CoreSacCost::Granter => SacCost::Granter,
                CoreSacCost::Filter(f) => SacCost::Filter(self.filter(f)),
            }),
            sorcery: false,
            from_graveyard: false,
            exile_self: c.exile_self,
            exile_hand: c.exile_hand,
            exile_this: c.exile_this,
            exile_this_joined: false,
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
            blight_put: c.blight_put,
            process: c.process,
            remove_from_creature: c.remove_from_creature,
            sacrifice_any: c.sacrifice_any,
            remove_x: c.remove_x,
            untap: c.untap,
            chosen: c.chosen,
            count: c.count,
            exile_cards: c.exile_cards.as_ref().map(|g| {
                Box::new(crate::ast::ExileCards {
                    n: g.n,
                    x: g.x,
                    filter: g.filter.as_ref().map(|f| self.filter(f)),
                    top: g.top,
                })
            }),
            order: Vec::new(),
            conditions: Vec::new(),
            once_ever: false,
            activators: None,
            restrict: Vec::new(),
            reduce: None,
            reveal_hand: c.reveal_hand,
            reveal_this: c.reveal_self,
            behold: c.behold.as_ref().map(|b| {
                Box::new(crate::ast::Behold {
                    filter: self.filter(&b.filter),
                    n: b.n,
                    exile: b.exile,
                    optional: false,
                    reveal: b.hand,
                    x: b.x,
                })
            }),
        }
    }

    #[allow(clippy::too_many_lines)] // one arm per condition
    fn static_cond(&self, c: &StaticCondition) -> StaticCond {
        let unowned = |f: &ObjFilter| {
            let mut f = self.filter(f);
            f.who = None;
            f
        };
        match c {
            StaticCondition::CardsInGraveyard(n) => StaticCond::CardsInGraveyard(*n),
            StaticCondition::TopCard(f) => StaticCond::TopCard(self.filter(f)),
            StaticCondition::CardTypesInGraveyard(n) => StaticCond::CardTypesInGraveyard(*n),
            StaticCondition::YouControl { filter, at_least } => StaticCond::YouControl {
                filter: unowned(filter),
                at_least: *at_least,
            },
            StaticCondition::YourTurn => StaticCond::YourTurn,
            StaticCondition::NotYourTurn => StaticCond::NotYourTurn,
            StaticCondition::AsCast(inner) => match &**inner {
                StaticCondition::YouControl { filter, .. } => {
                    StaticCond::ControlledAsCast(self.filter(filter))
                }
                _ => StaticCond::CastDuringMain,
            },
            StaticCondition::SourceAttacking => StaticCond::Attacking,
            StaticCondition::SourceEnchanted => StaticCond::Enchanted,
            StaticCondition::SourceEquipped => StaticCond::Equipped,
            StaticCondition::SourceMonstrous => StaticCond::Monstrous,
            StaticCondition::SourceSuspected(b) => StaticCond::Suspected(*b),
            StaticCondition::OpponentLifeAtMost(n) => StaticCond::OpponentLifeAtMost(*n),
            StaticCondition::MoreLifeThanOpponent => StaticCond::MoreLifeThanOpponent,
            StaticCondition::BasicLandTypes(n) => StaticCond::BasicLandTypes(*n),
            StaticCondition::LifeVsStarting { at_least, below } => StaticCond::LifeVsStarting {
                at_least: *at_least,
                below: *below,
                wording: if *below {
                    4
                } else if *at_least == 1 {
                    0
                } else if *at_least == 0 {
                    1
                } else {
                    3
                },
            },
            StaticCondition::YourLife { n, at_least } => StaticCond::YourLife {
                n: *n,
                at_least: *at_least,
            },
            StaticCondition::AnyPlayerLifeAtMost(n) => StaticCond::AnyPlayerLifeAtMost(*n),
            StaticCondition::OpponentPoisonAtLeast(n) => StaticCond::OpponentPoisonAtLeast(*n),
            StaticCondition::SourceCounters { kind, n, at_least } => StaticCond::SourceCounters {
                kind: *kind,
                n: *n,
                at_least: *at_least,
            },
            StaticCondition::DefenderControls { filter, at_least } => {
                StaticCond::DefenderControls {
                    filter: unowned(filter),
                    at_least: *at_least,
                }
            }
            StaticCondition::YouControlNone(f) => StaticCond::YouControlNone(unowned(f)),
            StaticCondition::YouControlAtMost { filter, n } => StaticCond::YouControlAtMost {
                filter: unowned(filter),
                n: *n,
            },
            StaticCondition::YouControlEach(fs) => {
                StaticCond::YouControlEach(fs.iter().map(&unowned).collect())
            }
            StaticCondition::OnBattlefield { filter, at_least } => StaticCond::OnBattlefield {
                filter: self.filter(filter),
                at_least: *at_least,
            },
            StaticCondition::DefenderPoisoned => StaticCond::DefenderPoisoned,
            StaticCondition::OthersInCombat {
                filter,
                at_least,
                also,
                block,
            } => StaticCond::OthersInCombat {
                filter: self.filter(filter),
                at_least: *at_least,
                also: *also,
                block: *block,
            },
            StaticCondition::MoreCreaturesThanOpponent { attacking } => {
                StaticCond::MoreCreaturesThanOpponent {
                    attacking: *attacking,
                }
            }
            StaticCondition::UnlockedDoors(n) => StaticCond::UnlockedDoors(*n),
            StaticCondition::SourceAttackedBattle => StaticCond::AttackedBattleThisTurn,
            StaticCondition::AttackedLastTurn => StaticCond::AttackedLastTurn,
            StaticCondition::ProwlDamage(_) => unreachable!("only prowl's alternative cost"),
            StaticCondition::Escaped => StaticCond::Escaped,
            StaticCondition::Kicked => StaticCond::Kicked,
            StaticCondition::Bargained => StaticCond::Bargained,
            StaticCondition::Teamwork => StaticCond::Teamwork,
            StaticCondition::Beheld => StaticCond::Beheld,
            StaticCondition::NoManaSpent => StaticCond::NoManaSpent,
            StaticCondition::XIs { n, at_least } => StaticCond::XIs {
                n: *n,
                at_least: *at_least,
            },
            StaticCondition::NotKicked => StaticCond::NotKicked,
            StaticCondition::ThatKicked => StaticCond::ThatKicked,
            StaticCondition::Impending => StaticCond::Impending,
            StaticCondition::And(a, b)
                if **a == StaticCondition::happened(TurnEvent::LifeLost, TurnWho::You)
                    && **b == StaticCondition::happened(TurnEvent::LifeGained, TurnWho::You) =>
            {
                StaticCond::LostLifeThisTurn { gained: true }
            }
            StaticCondition::And(a, b) => {
                StaticCond::And(Box::new(self.static_cond(a)), Box::new(self.static_cond(b)))
            }
            StaticCondition::OpponentMoreLands => StaticCond::OpponentMoreLands,
            StaticCondition::OpponentHasMore(w) => StaticCond::OpponentHasMore(*w),
            StaticCondition::PlayerHand { who, n, at_most } => StaticCond::PlayerHand {
                who: *who,
                n: *n,
                at_most: *at_most,
            },
            StaticCondition::PlayerMoreHand(w) => StaticCond::PlayerMoreHand(*w),
            StaticCondition::LibraryEmpty => StaticCond::LibraryEmpty,
            StaticCondition::NotYourMainPhase => StaticCond::NotYourMainPhase,
            StaticCondition::NotCastFrom(_) => StaticCond::NotCastFromHand,
            StaticCondition::WasCast => StaticCond::WasCast,
            StaticCondition::CameFrom { zone, wording } => StaticCond::CameFrom {
                zone: *zone,
                wording: *wording,
            },
            StaticCondition::TotalToughness(n) => StaticCond::TotalToughness(*n),
            StaticCondition::SneakPaid(p) => StaticCond::SneakPaid(*p),
            StaticCondition::ProwlPaid => StaticCond::ProwlPaid,
            StaticCondition::MoreVotes { .. } => unreachable!("a vote's words are the card's"),
            StaticCondition::WebSlung(p) => StaticCond::WebSlung(*p),
            StaticCondition::OptionalCostPaid => StaticCond::EvidenceCollected,
            StaticCondition::OffspringPaid => unreachable!("only offspring's trigger"),
            StaticCondition::Chose(_) => unreachable!("only a named option's ability"),
            StaticCondition::Saddled => StaticCond::Saddled,
            StaticCondition::GraveyardCards { filter, at_least } => StaticCond::GraveyardCards {
                filter: unowned(filter),
                at_least: *at_least,
                wording: crate::ast::GraveyardWording::ThereAre,
            },
            // Only "flash if you pay {2} more" negates sorcery timing.
            StaticCondition::Not(a) if **a == StaticCondition::SorceryTiming => {
                StaticCond::FlashPay(2)
            }
            StaticCondition::SorceryTiming => StaticCond::Not(Box::new(StaticCond::FlashPay(2))),
            StaticCondition::Not(a) => StaticCond::Not(Box::new(self.static_cond(a))),
            StaticCondition::Or(a, b)
                if **a == StaticCondition::happened(TurnEvent::Scried, TurnWho::You)
                    && **b == StaticCondition::happened(TurnEvent::Surveilled, TurnWho::You) =>
            {
                StaticCond::ScriedOrSurveilledThisTurn
            }
            StaticCondition::Or(a, b)
                if **a == StaticCondition::happened(TurnEvent::NonlandLeft, TurnWho::Anyone)
                    && **b == StaticCondition::happened(TurnEvent::Warped, TurnWho::Anyone) =>
            {
                StaticCond::Void
            }
            StaticCondition::Or(a, b)
                if **a == StaticCondition::happened(TurnEvent::LifeGained, TurnWho::You)
                    && **b == StaticCondition::happened(TurnEvent::LifeLost, TurnWho::You) =>
            {
                StaticCond::GainedLifeThisTurn { or_lost: true }
            }
            StaticCondition::Or(a, b) => {
                StaticCond::Or(Box::new(self.static_cond(a)), Box::new(self.static_cond(b)))
            }
            StaticCondition::OpponentGraveyard(n) => StaticCond::OpponentGraveyard(*n),
            StaticCondition::ManaValuesInGraveyard(n) => StaticCond::ManaValuesInGraveyard(*n),
            StaticCondition::OpponentControls { filter, at_least } => {
                StaticCond::OpponentControls {
                    filter: unowned(filter),
                    at_least: *at_least,
                }
            }
            StaticCondition::SourceIs(f) => StaticCond::Is(unowned(f)),
            StaticCondition::SourceModified => StaticCond::Modified,
            StaticCondition::SourceAttackingAlone => StaticCond::AttackingAlone,
            StaticCondition::SourceAttackedOrBlocked => StaticCond::AttackedOrBlocked,
            StaticCondition::AttackedTotalPower(n) => StaticCond::AttackedTotalPower(*n),
            StaticCondition::SourceBlocked => StaticCond::Blocked,
            StaticCondition::SourceBeingBlocked => StaticCond::BeingBlocked,
            StaticCondition::AttachedIs(f) => StaticCond::AttachedIs(unowned(f)),
            StaticCondition::CastFrom(z) => StaticCond::CastFrom(*z),
            StaticCondition::Night => StaticCond::Night,
            StaticCondition::YourMainPhase => StaticCond::YourMainPhase,
            StaticCondition::YourCombatPhase => StaticCond::YourCombatPhase,
            StaticCondition::FirstCombatPhase => StaticCond::FirstCombatPhase,
            StaticCondition::NthResolution(n) => StaticCond::NthResolution {
                n: *n,
                again: false,
            },
            StaticCondition::Happened { count, cmp, n } => match count.filter.as_deref() {
                Some(f) => match count.kind {
                    TurnEvent::Attacked if *n == 1 => StaticCond::AttackedWith(self.filter(f)),
                    TurnEvent::Attacked => StaticCond::AttackedWithAtLeast(self.filter(f), *n),
                    TurnEvent::Entered => StaticCond::EnteredWith {
                        filter: self.filter(f),
                        had: false,
                    },
                    _ => StaticCond::CastThisTurn {
                        filter: unowned(f),
                        at_least: *n,
                        other: count.other,
                    },
                },
                None => happened(count, *cmp, *n),
            },
            StaticCondition::MoreCardsInHand => StaticCond::MoreCardsInHand,
            StaticCondition::OpponentPoison(n) => StaticCond::OpponentPoison(*n),
            StaticCondition::OwnCardInExile => StaticCond::OwnCardInExile,
            StaticCondition::ExiledWith(n) => StaticCond::ExiledWith(*n),
            StaticCondition::TypesExiledWith(n) => StaticCond::TypesExiledWith(*n),
            StaticCondition::Monarch => StaticCond::Monarch,
            StaticCondition::CompletedDungeon => StaticCond::CompletedDungeon,
            StaticCondition::CityBlessing => StaticCond::CityBlessing,
            StaticCondition::EnduringStory => StaticCond::EnduringStory,
            StaticCondition::NoEnduringStory => StaticCond::NoEnduringStory,
            StaticCondition::MaxSpeed => StaticCond::MaxSpeed,
            StaticCondition::TotalPower(n) => StaticCond::TotalPower(*n),
            StaticCondition::DifferentPowers(n) => StaticCond::DifferentPowers(*n),
            StaticCondition::During(t) => StaticCond::During(*t),
            StaticCondition::ManaSpent {
                kind,
                at_least,
                that,
            } => StaticCond::ManaSpent {
                kind: *kind,
                at_least: *at_least,
                that: *that,
            },
            StaticCondition::ActivatedThisTurn(n) => StaticCond::ActivatedThisTurn(*n),
            StaticCondition::CardsInHand { n, at_most } => StaticCond::CardsInHand {
                n: *n,
                at_most: *at_most,
            },
            StaticCondition::NoCardInHand(f) => StaticCond::NoCardInHand(self.filter(f)),
            StaticCondition::EachHandEmpty => StaticCond::EachHandEmpty,
            StaticCondition::LibraryAtMost(n) => StaticCond::LibraryAtMost(*n),
        }
    }

    fn recipient(r: CoreRecipient) -> Recipient {
        match r {
            CoreRecipient::Target(i) => Recipient::Target(i),
            // A variable group's targets, to the end of the list.
            CoreRecipient::Targets(a, u8::MAX) => Recipient::Target(usize::from(a)),
            CoreRecipient::Targets(a, n) => Recipient::Targets(usize::from(a), usize::from(n)),
            CoreRecipient::You => Recipient::You,
            CoreRecipient::EachOpponent => Recipient::EachOpponent,
            CoreRecipient::Source => Recipient::Source,
            CoreRecipient::TriggeringObject => Recipient::TriggeringObject,
            CoreRecipient::TriggeringCard => Recipient::TriggeringCard,
            CoreRecipient::TriggeringController => Recipient::TriggeringController { that: false },
            CoreRecipient::TriggeringPlayer => Recipient::TriggeringPlayer,
            CoreRecipient::DefendingPlayer => Recipient::DefendingPlayer,
            CoreRecipient::ControllerOf(i) => Recipient::ControllerOf(i),
            CoreRecipient::OwnerOf(i) => Recipient::OwnerOf(i),
            CoreRecipient::SourceOwner => Recipient::SourceOwner,
            CoreRecipient::EachCreature => Recipient::EachCreature,
            CoreRecipient::EachPlayer => Recipient::EachPlayer,
            CoreRecipient::EachCreatureYouControl => Recipient::EachCreatureYouControl,
            CoreRecipient::EachPermanentYouControl => Recipient::EachPermanentYouControl,
            CoreRecipient::SameObject => Recipient::SameObject,
            CoreRecipient::CreatedObject => Recipient::CreatedObject,
            CoreRecipient::CreatedTokens => Recipient::CreatedTokens,
            CoreRecipient::Group(i) => Recipient::Group(i),
            CoreRecipient::RandomOf(i) => Recipient::RandomOf(i),
            CoreRecipient::ExiledWith(i) => Recipient::ExiledWith(i),
            CoreRecipient::OpponentsWithout(i) => Recipient::OpponentsWithout(i),
            CoreRecipient::DamagedThisWay => Recipient::DamagedThisWay,
            CoreRecipient::Rest => Recipient::Rest,
            CoreRecipient::Declined => Recipient::Declined(crate::ast::Declined {
                player: false,
                tense: crate::ast::DeclinedTense::Didnt,
                tail: false,
            }),
            CoreRecipient::TargetAndGroup(t, i) => Recipient::TargetAnd(t, i),
            CoreRecipient::AttachedAndSharing(i) => Recipient::AttachedAnd(i),
            CoreRecipient::Attached => Recipient::Attached,
            CoreRecipient::AttachedController => Recipient::AttachedController,
            CoreRecipient::BlockingIt => Recipient::BlockingIt,
            CoreRecipient::ExiledCard => Recipient::ExiledCard,
            CoreRecipient::Granter => Recipient::Granter,
            CoreRecipient::GranterController => Recipient::GranterController,
            CoreRecipient::ThoseObjects => Recipient::Those(crate::ast::Those::Them),
        }
    }

    fn sentences(&self, effects: &[CoreEffect]) -> Vec<Sentence> {
        let mut out = Vec::new();
        let mut sized = None;
        for e in effects {
            // CR 611.2b: one sentence with its duration.
            if let CoreEffect::Lasting { lasting, effects } = e {
                let mut s = Sentence::new(effects.iter().map(|e| self.effect(e)).collect());
                s.lasting = Some(*lasting);
                out.push(s);
                continue;
            }
            if let CoreEffect::Instead { cond, base, alt } = e {
                out.extend(self.sentences(base));
                let cond = match cond {
                    mtg_core::card::EffectCond::Kicked => crate::ast::InsteadCond::Kicked,
                    mtg_core::card::EffectCond::Static(i) => {
                        crate::ast::InsteadCond::Static(self.static_cond(self.pool.condition(*i)))
                    }
                    mtg_core::card::EffectCond::Target { n, filter } => {
                        crate::ast::InsteadCond::Target {
                            n: usize::from(*n),
                            that: true,
                            filter: self.filter(filter),
                        }
                    }
                    mtg_core::card::EffectCond::ThisWay(c) => {
                        crate::ast::InsteadCond::ThisWay(self.this_way_cond(c))
                    }
                };
                out.push(Sentence::new(vec![Effect::Instead {
                    cond,
                    wording: crate::ast::InsteadWording::Trail,
                    label: None,
                    effects: self.sentences(alt),
                }]));
                continue;
            }
            if let CoreEffect::FlipCoin { won } = e
                && !won.is_empty()
            {
                out.push(Sentence::new(vec![Effect::FlipCoin]));
                out.push(Sentence::new(vec![Effect::IfWonFlip(self.sentences(won))]));
                continue;
            }
            if let CoreEffect::Clash { won } = e
                && !won.is_empty()
            {
                out.push(Sentence::new(vec![Effect::Clash]));
                out.push(Sentence::new(vec![Effect::IfWonClash(self.sentences(won))]));
                continue;
            }
            // "an X/X ... token, where X is ...": X is fixed first.
            if let CoreEffect::TokenPtX(n) = e {
                sized = Some(*n);
                continue;
            }
            if let Some(n) = sized.take()
                && let CoreEffect::CreateToken { .. } = e
            {
                let mut c = self.effect(e);
                if let Effect::CreateToken { sized, .. } = &mut c {
                    *sized = Some(n);
                }
                out.push(Sentence::new(vec![c]));
                continue;
            }
            // "create a ... token that's tapped and attacking".
            if matches!(e, CoreEffect::CreatedAttack)
                && let Some(
                    Effect::CreateToken { attacking, .. }
                    | Effect::CreateTokenCopy { attacking, .. },
                ) = out
                    .last_mut()
                    .and_then(|s: &mut Sentence| s.effects.last_mut())
            {
                *attacking = true;
                continue;
            }
            // "... onto the battlefield tapped and attacking".
            if matches!(e, CoreEffect::EnteredAttack)
                && let Some(
                    Effect::ReturnToBattlefield { attacking, .. }
                    | Effect::PutFromHand { attacking, .. },
                ) = out
                    .last_mut()
                    .and_then(|s: &mut Sentence| s.effects.last_mut())
            {
                *attacking = true;
                continue;
            }
            out.push(Sentence::new(vec![self.effect(e)]));
        }
        out
    }

    #[allow(clippy::too_many_lines)]
    fn effect(&self, e: &CoreEffect) -> Effect {
        let r = Self::recipient;
        match e {
            CoreEffect::DealDamage { amount, to } => Effect::DealDamage {
                amount: *amount,
                to: vec![r(*to)],
                by: Recipient::Source,
                that_amount: false,
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
                joined: false,
            },
            CoreEffect::DestroyNoRegen { to } => Effect::Destroy {
                to: vec![r(*to)],
                no_regen: true,
                joined: false,
            },
            CoreEffect::GainLife { amount, who } => Effect::GainLife {
                amount: *amount,
                who: r(*who),
            },
            CoreEffect::LoseLife { amount, who } => Effect::LoseLife {
                amount: *amount,
                who: r(*who),
            },
            CoreEffect::LoseHalfLife { who } => Effect::LoseHalfLife { who: r(*who) },
            CoreEffect::LoseGame { who } => Effect::LoseGame { who: r(*who) },
            CoreEffect::DrawCards { count, who } => Effect::DrawCards {
                count: *count,
                who: (*who != CoreRecipient::You).then(|| r(*who)),
            },
            CoreEffect::Mill { count, who } => Effect::Mill {
                count: *count,
                who: (*who != CoreRecipient::You).then(|| r(*who)),
            },
            CoreEffect::AddCountersCapped {
                kind,
                count,
                to,
                cap,
            } => Effect::AddCountersCapped {
                kind: *kind,
                count: *count,
                to: r(*to),
                cap: *cap,
            },
            CoreEffect::AddCounters { kind, count, to } => Effect::AddCounters {
                kind: *kind,
                count: *count,
                to: r(*to),
            },
            CoreEffect::DealDamageDivided { amount, to } => Effect::DealDamageDivided {
                amount: *amount,
                to: r(*to),
            },
            CoreEffect::DistributeCounters { kind, count, to } => Effect::DistributeCounters {
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
            CoreEffect::RemoveCounterChosen { to, up_to } => Effect::RemoveCounterChosen {
                to: r(*to),
                up_to: *up_to,
            },
            CoreEffect::ReturnToHand { what } => Effect::ReturnToHand {
                what: vec![r(*what)],
                owner_hand: false,
                put: false,
            },
            CoreEffect::ReturnToBattlefield {
                what,
                counters,
                tapped,
                under_you,
                attached,
                more,
                if_creature,
                transformed,
            } => Effect::ReturnToBattlefield {
                what: r(*what),
                counters: *counters,
                tapped: *tapped,
                under_you: *under_you,
                attached: attached.map(r),
                put: false,
                attacking: false,
                additional: *if_creature,
                more: *more,
                if_creature: *if_creature,
                transformed: *transformed,
                front_up: false,
            },
            CoreEffect::Regenerate { to } => Effect::Regenerate { to: r(*to) },
            CoreEffect::Exile { what } | CoreEffect::ExileLinked { what, until: false } => {
                Effect::Exile {
                    what: vec![r(*what)],
                }
            }
            CoreEffect::ExileLinked { what, until: true } => Effect::ExileUntil { what: r(*what) },
            CoreEffect::ExileAllGraveyards => Effect::ExileAllGraveyards,
            CoreEffect::ExileGraveyard { who, filter } => Effect::ExileGraveyard {
                who: r(*who),
                filter: filter.as_ref().map(|f| self.filter(f)),
            },
            CoreEffect::Sacrifice { what } => Effect::Sacrifice { what: r(*what) },
            CoreEffect::SacrificeBy { who, what } => Effect::SacrificeBy {
                who: r(*who),
                what: r(*what),
            },
            CoreEffect::Tap { to } => Effect::Tap { to: vec![r(*to)] },
            CoreEffect::TapOrUntap { to } => Effect::TapOrUntap { to: r(*to) },
            CoreEffect::ExileSameName { what } => Effect::ExileSameName { what: r(*what) },
            CoreEffect::ExileChosenName {
                who,
                limit,
                per_hand,
            } => Effect::ExileChosenName {
                who: r(*who),
                limit: *limit,
                per_hand: per_hand.map(|p| match p {
                    mtg_core::card::PerHandExiled::Draw => crate::ast::PerHand::Draw,
                    mtg_core::card::PerHandExiled::Create(t) => {
                        crate::ast::PerHand::Create(self.pool.get(t).name.clone())
                    }
                }),
            },
            CoreEffect::BanChosenName { opponents, lands } => Effect::BanChosenName {
                opponents: opponents.then_some(true),
                lands: *lands,
                late: false,
            },
            CoreEffect::NextSpellLess(n) => Effect::NextSpellLess(*n),
            CoreEffect::NextSpellFree(f) => Effect::NextSpellFree(self.filter(f)),
            CoreEffect::NextSpellUncounterable => Effect::NextSpellUncounterable,
            CoreEffect::ExtraLandPlays(n) => Effect::ExtraLandPlays(*n),
            CoreEffect::FaceDownSpellsLess(n) => Effect::FaceDownSpellsLess(*n),
            CoreEffect::Goad { what } => Effect::Goad { what: r(*what) },
            CoreEffect::ExileOnLeave { what } => Effect::ExileOnLeave { what: r(*what) },
            CoreEffect::TurnFaceUp { what } => Effect::TurnFaceUp { what: r(*what) },
            CoreEffect::TurnFaceDown { what } => Effect::TurnFaceDown { what: r(*what) },
            CoreEffect::LookAtFaceDown { what } => Effect::LookAtFaceDown { what: r(*what) },
            CoreEffect::LookAtPlayer { who } => Effect::LookAtPlayer { who: r(*who) },
            CoreEffect::UntilYourNextTurn { to, parts }
                if parts.as_slice() == [Modification::AddKeywords(KeywordSet::DETAINED)] =>
            {
                Effect::Detain {
                    what: r(*to),
                    spelled: false,
                }
            }
            CoreEffect::Untap { to } => Effect::Untap { to: vec![r(*to)] },
            CoreEffect::RemoveFromCombat { what } => Effect::RemoveFromCombat { what: r(*what) },
            CoreEffect::BecomeBlocked { what } => Effect::BecomeBlocked { what: r(*what) },
            CoreEffect::Grant { keywords: k, to } => Effect::Grant {
                keywords: keywords(*k),
                to: r(*to),
                each: false,
                no_also: false,
                each_combat: false,
            },
            CoreEffect::GainControl { of, forever } => Effect::GainControl {
                of: r(*of),
                forever: *forever,
            },
            CoreEffect::Attach { what, to } => Effect::Attach {
                what: r(*what),
                to: r(*to),
            },
            CoreEffect::Unattach { what } => Effect::Unattach { what: r(*what) },
            CoreEffect::CreateToken {
                token,
                count,
                tapped,
                who,
                attached,
            } => Effect::CreateToken {
                token: self.pool.get(*token).name.clone(),
                count: *count,
                tapped: *tapped,
                attacking: false,
                who: (*who != CoreRecipient::You).then(|| r(*who)),
                it_has: false,
                attached: attached.map(r),
                sized: None,
            },
            CoreEffect::CreateTokenCopy {
                of,
                except,
                count,
                tapped,
            } => Effect::CreateTokenCopy {
                of: r(*of),
                except: *except,
                count: *count,
                tapped: *tapped,
                attacking: false,
                trailing: false,
            },
            CoreEffect::CastCopy { what, free } => Effect::CastCopy {
                what: r(*what),
                free: *free,
                wording: crate::ast::CopyCastWording::Sentences,
            },
            CoreEffect::CopySpells {
                what,
                times,
                new_targets,
                except,
            } => Effect::CopySpells {
                what: r(*what),
                times: *times,
                new_targets: *new_targets,
                except: *except,
                and_may: false,
            },
            CoreEffect::BecomeCopy {
                of,
                except,
                who,
                until,
            } => Effect::BecomeCopy {
                of: r(*of),
                except: *except,
                who: who.map_or(Recipient::Source, r),
                until: *until,
            },
            CoreEffect::CreateEmblem { emblem } => Effect::CreateEmblem {
                emblem: self.pool.get(*emblem).name.clone(),
            },
            CoreEffect::Counter { to, dest } => Effect::Counter {
                to: r(*to),
                dest: *dest,
            },
            CoreEffect::ChangeTarget { what } => Effect::ChangeTarget { what: r(*what) },
            CoreEffect::CounterUnlessPays { to, mana, x, dest } => Effect::CounterUnlessPays {
                to: r(*to),
                mana: mana.clone(),
                x: *x,
                dest: *dest,
            },
            CoreEffect::Scry {
                count,
                who: CoreRecipient::You,
            } => Effect::Scry(*count),
            CoreEffect::Scry { count, who } => Effect::ScryEach {
                who: Self::recipient(*who),
                count: *count,
                may: false,
            },
            CoreEffect::AddMana(m) => Effect::AddMana(m.clone()),
            CoreEffect::AddManaOneOf(m) => Effect::AddManaOneOf(m.clone()),
            CoreEffect::AddManaTimes { mana, count } => Effect::AddManaTimes {
                mana: *mana,
                count: *count,
            },
            CoreEffect::AddManaCombination {
                count,
                colors,
                different,
            } => Effect::AddManaCombination {
                count: *count,
                colors: *colors,
                different: *different,
                listed: false,
            },
            CoreEffect::AddManaTwoColors(n) => Effect::AddManaTwoColors(*n),
            CoreEffect::AddManaLandCould {
                whose,
                basic,
                any_type,
            } => Effect::AddManaLandCould {
                whose: *whose,
                basic: *basic,
                any_type: *any_type,
            },
            CoreEffect::AddManaCommander { rider } => Effect::AddManaCommander { rider: *rider },
            CoreEffect::PumpVar {
                power,
                toughness,
                times,
                to,
            } => Effect::PumpVar {
                power: *power,
                toughness: *toughness,
                times: *times,
                to: r(*to),
            },
            CoreEffect::PutInLibrary { what, place } => Effect::PutInLibrary {
                what: r(*what),
                place: *place,
                // A choice is the owner's to make, so it names them.
                wording: if matches!(
                    place,
                    mtg_core::card::LibraryPlace::TopOrBottom
                        | mtg_core::card::LibraryPlace::FromTopOrBottom(_)
                ) {
                    crate::ast::LibraryWording::Owner
                } else {
                    crate::ast::LibraryWording::Put
                },
            },
            CoreEffect::SacrificeChosen { who, filter, count } => Effect::SacrificeChosen {
                who: r(*who),
                count: *count,
                filter: self.filter(filter),
                and: None,
                choice_first: false,
            },
            CoreEffect::Strip { keywords: k, to } => Effect::Strip {
                keywords: keywords(*k),
                to: r(*to),
            },
            CoreEffect::RevealHand { who } => Effect::RevealHand { who: r(*who) },
            CoreEffect::RevealRandom { who } => Effect::RevealRandom { who: r(*who) },
            CoreEffect::DiscardRandom { count, who } => Effect::DiscardRandom {
                count: *count,
                who: Some(r(*who)),
                another: false,
            },
            CoreEffect::FlipCoin { .. } => Effect::FlipCoin,
            CoreEffect::Clash { .. } => Effect::Clash,
            CoreEffect::Vote { options } => {
                Effect::Vote((1..=*options).map(|i| format!("choice{i}")).collect())
            }
            CoreEffect::VoteReturn { filter } => Effect::VoteReturn(self.filter(filter)),
            CoreEffect::RollDie { sides, rows } => Effect::RollDie {
                sides: u32::from(*sides),
                rows: rows
                    .iter()
                    .map(|row| crate::ast::DieRow {
                        lo: u32::from(row.lo),
                        hi: (row.hi != u8::MAX).then_some(u32::from(row.hi)),
                        effects: self.sentences(&row.effects),
                    })
                    .collect(),
            },
            CoreEffect::PlayerChooses { who, filter } => Effect::PlayerChooses {
                who: r(*who),
                filter: self.filter(filter),
            },
            CoreEffect::ChooseFor(c) => {
                let each_type = c.slots.len() == 6;
                let (filter, count, types) = match c.slots.as_slice() {
                    [one] if !each_type => (
                        self.filter(&one.filter),
                        mtg_core::card::Num::N(u32::from(one.count)),
                        Vec::new(),
                    ),
                    slots => {
                        let mut base = slots.first().map(|s| s.filter.clone()).unwrap_or_default();
                        base.types = mtg_core::card::TypeSet::default();
                        let types = if each_type {
                            Vec::new()
                        } else {
                            slots.iter().flat_map(|s| types(s.filter.types)).collect()
                        };
                        (self.filter(&base), mtg_core::card::Num::N(1), types)
                    }
                };
                Effect::ChooseFor {
                    who: c.who.map_or(Recipient::EachPlayer, r),
                    by: if c.by_you {
                        crate::ast::ChooseBy::You
                    } else {
                        crate::ast::ChooseBy::Each
                    },
                    filter,
                    count,
                    up_to: c.up_to,
                    types,
                    each_type,
                    among: c.among.as_ref().map(|f| self.filter(f)),
                }
            }
            CoreEffect::CantBeTargeted { what, ban } => Effect::CantBeTargeted {
                what: r(*what),
                ban: *ban,
            },
            CoreEffect::PickTarget {
                first,
                by_controller,
                act,
                wording,
            } => Effect::PickTarget {
                first: usize::from(*first),
                by_controller: *by_controller,
                act: *act,
                wording: *wording,
            },
            CoreEffect::ExileChosen { who, filter, count } => Effect::ExileChosen {
                who: r(*who),
                count: *count,
                filter: self.filter(filter),
                graveyard: false,
            },
            CoreEffect::ExileFromHand {
                count,
                who,
                graveyard,
                face,
                all,
                filter,
            } => Effect::ExileFromHand {
                count: *count,
                who: r(*who),
                graveyard: *graveyard,
                face: *face,
                all: *all,
                filter: filter.as_ref().map(|f| self.filter(f)),
            },
            CoreEffect::ExileLibrary { who, keep, face } => Effect::ExileLibrary {
                who: match who {
                    CoreRecipient::You => None,
                    w => Some(r(*w)),
                },
                keep: *keep,
                face: *face,
            },
            CoreEffect::DrawDiscardDrawn { draw, discard } => Effect::DrawDiscardDrawn {
                draw: *draw,
                discard: *discard,
            },
            CoreEffect::ExileFromGraveyardOrHand { who, filter } => {
                Effect::ExileFromGraveyardOrHand {
                    who: r(*who),
                    filter: self.filter(filter),
                }
            }
            CoreEffect::DiscardChosen {
                who,
                filter,
                optional,
                exile,
                until,
                top,
                reveal,
            } => Effect::DiscardChosen {
                who: r(*who),
                filter: filter.as_ref().map(|f| self.filter(f)),
                reveal: *reveal,
                may: *optional,
                look: false,
                exile: exile.then_some(!*until),
                until: *until,
                and_choose: false,
                top: *top,
            },
            CoreEffect::Transform { what } => Effect::Transform { what: r(*what) },
            CoreEffect::Flip { what } => Effect::Flip { what: r(*what) },
            CoreEffect::Meld { partner, into } => Effect::Meld {
                partner: partner.to_string(),
                into: into.to_string(),
            },
            CoreEffect::Search {
                filter,
                count,
                to,
                who,
                may,
                reveal,
                graveyard,
                name,
                more,
            } => Effect::Search {
                graveyard: *graveyard,
                name: name.as_deref().map(str::to_string),
                filter: self.filter(filter),
                count: *count,
                up_to: *count != mtg_core::card::Num::N(1),
                to: *to,
                reveal: *reveal,
                that_card: false,
                or_card: crate::ast::OrCard::No,
                who: (*who != CoreRecipient::You).then(|| r(*who)),
                may: *may,
                old: false,
                those: false,
                more: more.as_ref().map(|m| {
                    Box::new(crate::ast::SearchMore {
                        slots: m
                            .slots
                            .iter()
                            .map(|(f, n)| (self.filter(f), n.as_deref().map(str::to_string)))
                            .collect(),
                        zones: match (m.hand, m.all) {
                            (false, _) => crate::ast::SearchZones::Library,
                            (true, false) => crate::ast::SearchZones::HandOr { comma: true },
                            (true, true) => crate::ast::SearchZones::HandAnd,
                        },
                        different_names: m.different_names,
                        split: m.split.as_ref().map(|(f, to)| (self.filter(f), *to)),
                        wording: crate::ast::SearchWording::Plain,
                        names: crate::ast::NamesWording::With,
                        exactly: false,
                        mana_value_last: false,
                        pick: m.pick.as_ref().map(|p| (p.n, p.by.map(r), p.chosen)),
                        counters: m.counters.map(|(k, n)| (k, n, false)),
                    })
                }),
            },
            CoreEffect::Surveil { count } => Effect::Surveil(*count),
            CoreEffect::Discard { count, who } => Effect::Discard {
                count: *count,
                who: (*who != CoreRecipient::You).then(|| r(*who)),
            },
            CoreEffect::HandToLibrary {
                count,
                who,
                bottom,
                shuffle,
            } => Effect::HandToLibrary {
                count: *count,
                who: (*who != CoreRecipient::You).then(|| r(*who)),
                bottom: *bottom,
                shuffle: *shuffle,
            },
            CoreEffect::ShuffleIntoLibrary {
                who,
                hand,
                both,
                filter,
            } => Effect::ShuffleZone {
                both: *both,
                who: (*who != CoreRecipient::You).then(|| r(*who)),
                of: None,
                hand: *hand,
                filter: filter.as_ref().map(|f| self.filter(f)),
                cards: *hand && filter.is_none(),
            },
            CoreEffect::Fight { a, b } => Effect::Fight { a: r(*a), b: r(*b) },
            CoreEffect::DamageFrom { source, amount, to } => Effect::DealDamage {
                amount: *amount,
                to: vec![r(*to)],
                by: match source {
                    CoreRecipient::TriggeringObject => Recipient::TriggeringIt,
                    other => r(*other),
                },
                that_amount: false,
            },
            CoreEffect::Bite { source, to, twice } => Effect::Bite {
                source: r(*source),
                to: r(*to),
                twice: *twice,
            },
            CoreEffect::CastFree {
                what,
                filter,
                max_mv,
                exile,
                play,
            } => Effect::CastFree {
                what: what.map(r),
                filter: filter.as_ref().map(|f| self.filter(f)),
                max: *max_mv,
                exile: *exile,
                play: *play,
            },
            CoreEffect::Animate {
                to,
                types: t,
                subtypes,
                pt: None,
                colors: None,
                keywords: kws,
                indefinite,
                loses_abilities: true,
                in_addition: false,
            } if t.is_empty() && subtypes.is_empty() && kws.is_empty() => {
                Effect::LoseAllAbilities {
                    to: r(*to),
                    indefinite: *indefinite,
                }
            }
            CoreEffect::Animate {
                to,
                types: t,
                subtypes,
                pt,
                colors: c,
                keywords: kws,
                indefinite,
                loses_abilities,
                in_addition,
            } => Effect::Animate {
                to: r(*to),
                types: types(*t),
                subtypes: subtypes
                    .iter()
                    .map(|s| self.pool.subtype_name(*s).to_string())
                    .collect(),
                pt: *pt,
                base: false,
                colors: c.map(colors).unwrap_or_default(),
                keywords: keywords(*kws),
                indefinite: *indefinite,
                loses_abilities: *loses_abilities,
                in_addition: *in_addition,
                is_word: false,
            },
            CoreEffect::BecomeColor {
                to,
                colors: c,
                indefinite,
            } => Effect::BecomeColor {
                to: r(*to),
                colors: colors(*c),
                indefinite: *indefinite,
            },
            CoreEffect::BecomeLandType {
                to,
                land,
                indefinite,
            } => Effect::BecomeLandType {
                to: r(*to),
                land: *land,
                indefinite: *indefinite,
            },
            CoreEffect::Choose(kind) => Effect::Choose(self.choice_kind(*kind)),
            CoreEffect::BecomeChosen { to, color } => Effect::BecomeChosen {
                to: r(*to),
                color: *color,
            },
            CoreEffect::ProtectionFromChosen { to } => Effect::ProtectionFromChosen { to: r(*to) },
            CoreEffect::OfChoice {
                to,
                kind,
                protection,
                in_addition,
            } => Effect::OfChoice {
                to: r(*to),
                kind: self.choice_kind(*kind),
                protection: *protection,
                in_addition: *in_addition,
            },
            CoreEffect::YouMay(inner) => Effect::YouMay(self.sentences(inner)),
            CoreEffect::AnyPlayerMay { opponents, effects } => Effect::AnyPlayerMay {
                opponents: *opponents,
                inner: self.sentences(effects),
            },
            CoreEffect::PlayerMay { who, effects } => Effect::PlayerMay {
                who: Self::recipient(*who),
                inner: self.sentences(effects),
            },
            CoreEffect::IfKicked(inner) => Effect::IfKicked(self.sentences(inner)),
            CoreEffect::KeepMana(inner) => Effect::KeepMana(self.sentences(inner)),
            CoreEffect::SpendRider {
                effects,
                spells,
                ability,
            } => {
                let AbilityDef::Triggered(t) = self.pool.ability(*ability) else {
                    panic!("a mana rider's ability is not triggered");
                };
                Effect::SpendRider {
                    inner: self.sentences(effects),
                    spells: self.filter(spells),
                    then: self.sentences(&t.effects),
                }
            }
            CoreEffect::SpendOnly {
                effects,
                restriction,
            } => Effect::SpendOnly {
                inner: self.sentences(effects),
                restriction: self.mana_restriction(restriction),
            },
            CoreEffect::EachPutsFromGraveyard { who, filter } => Effect::EachPutsFromGraveyard {
                who: Self::recipient(*who),
                filter: self.filter(filter),
            },
            CoreEffect::ForEachCouldnt(inner) => Effect::ForEachCouldnt {
                opponent: true,
                inner: self.sentences(inner),
            },
            CoreEffect::Instead { .. } => unreachable!("a sentence pair, in `sentences`"),
            CoreEffect::IfTarget { n, filter, then } => Effect::IfTarget(
                usize::from(*n),
                self.filter(filter),
                self.sentences(then),
                crate::ast::TargetCheck::default(),
            ),
            CoreEffect::IfCondition(i, inner) => Effect::IfCondition(
                self.static_cond(self.pool.condition(*i)),
                self.sentences(inner),
                crate::ast::IfWording::Lead,
            ),
            CoreEffect::IfYouDo(inner) => Effect::IfYouDo(self.sentences(inner)),
            CoreEffect::Otherwise(inner) => Effect::Otherwise(self.sentences(inner)),
            CoreEffect::Reflexive(ability) => {
                let AbilityDef::Triggered(t) = self.pool.ability(*ability) else {
                    panic!("reflexive ability is not triggered");
                };
                Effect::WhenYouDo {
                    targets: t.targets.iter().map(|t| self.target(*t)).collect(),
                    effects: self.sentences(&t.effects),
                }
            }
            CoreEffect::Waterbend(mana) => Effect::Waterbend(mana.clone()),
            CoreEffect::Pay { mana, life } => Effect::Pay {
                mana: mana.clone(),
                life: *life,
            },
            CoreEffect::UnlessYou { act, effects } => Effect::UnlessYou {
                act: Box::new(
                    act.first()
                        .map_or(Effect::ExileAllGraveyards, |e| self.effect(e)),
                ),
                effect: Box::new(
                    effects
                        .first()
                        .map_or(Effect::ExileAllGraveyards, |e| self.effect(e)),
                ),
            },
            CoreEffect::UnlessPaysCost {
                payer,
                alts,
                effects,
            } => Effect::UnlessPaysCost {
                payer: Self::recipient(*payer),
                alts: alts
                    .iter()
                    .map(|a| crate::ast::WardAlt {
                        cost: self.cost(&a.cost),
                        life: a.life_count,
                        poison: a.poison,
                        you: false,
                    })
                    .collect(),
                effect: Box::new(
                    effects
                        .first()
                        .map_or(Effect::ExileAllGraveyards, |e| self.effect(e)),
                ),
            },
            CoreEffect::MayOr { who, sacrifice, .. } => Effect::MayOr {
                who: Self::recipient(*who),
                sacrifice: sacrifice
                    .as_ref()
                    .map_or_else(Filter::default, |f| self.filter(f)),
                sacrifice_first: false,
            },
            CoreEffect::UnlessSacrifices {
                payer,
                filter,
                or_discard,
                effects,
            } => Effect::UnlessSacrifices {
                payer: Self::recipient(*payer),
                filter: self.filter(filter),
                or_discard: *or_discard,
                effect: Box::new(
                    effects
                        .first()
                        .map_or(Effect::ExileAllGraveyards, |e| self.effect(e)),
                ),
            },
            CoreEffect::UnlessPays {
                payer,
                mana,
                life,
                discard,
                waterbend,
                discard_random,
                both,
                effects,
            } => Effect::UnlessPays {
                payer: Self::recipient(*payer),
                mana: mana.clone(),
                life: *life,
                discard: *discard,
                waterbend: *waterbend,
                discard_random: *discard_random,
                both: *both,
                effect: Box::new(
                    effects
                        .first()
                        .map_or(Effect::ExileAllGraveyards, |e| self.effect(e)),
                ),
            },
            CoreEffect::SetBasePtVar { to, n } => Effect::SetBasePtVar { to: r(*to), n: *n },
            CoreEffect::SetBasePower { to, n } => Effect::SetBasePower {
                to: r(*to),
                n: *n,
                becomes: matches!(n, mtg_core::card::Num::Count(_)),
            },
            CoreEffect::SetBasePt {
                to,
                power,
                toughness,
            } => Effect::SetBasePt {
                to: r(*to),
                power: *power,
                toughness: *toughness,
            },
            CoreEffect::Delayed {
                ability,
                about,
                this_turn,
            } => {
                let AbilityDef::Triggered(t) = self.pool.ability(*ability) else {
                    panic!("delayed ability is not triggered");
                };
                Effect::Delayed {
                    about: r(*about),
                    trigger: self.trigger(&t.trigger),
                    effects: self.sentences(&t.effects),
                    trigger_first: *this_turn,
                    this_turn: *this_turn,
                    it: false,
                    closed: false,
                }
            }
            CoreEffect::CreateReplacement(rep) => Effect::CreateReplacement(self.replacement(rep)),
            CoreEffect::Rule(k) => Effect::Rule(*k),
            CoreEffect::DamageUnpreventable => Effect::DamageUnpreventable,
            CoreEffect::ExcessToController => Effect::ExcessToController,
            CoreEffect::PlayerRule { who, kind } => Effect::PlayerRule {
                who: Self::recipient(*who),
                kind: *kind,
            },
            CoreEffect::AddPoison { count, who } => Effect::AddPoison {
                count: *count,
                who: Self::recipient(*who),
            },
            CoreEffect::AddEnergy { count, who } => Effect::AddEnergy {
                count: *count,
                who: Self::recipient(*who),
            },
            CoreEffect::PayEnergy(n) => Effect::PayEnergy(*n),
            CoreEffect::Proliferate { times } => Effect::Proliferate {
                times: (*times != mtg_core::card::Num::N(1)).then_some(*times),
            },
            CoreEffect::BecomeStage(_) => {
                unreachable!("a Class's level or a Case's to-solve line, never decompiled")
            }
            CoreEffect::RingTempts => Effect::RingTempts,
            CoreEffect::BecomeMonarch { .. } => Effect::BecomeMonarch,
            CoreEffect::TakeInitiative { .. } => Effect::TakeInitiative,
            CoreEffect::Venture { undercity } => Effect::Venture {
                undercity: *undercity,
            },
            CoreEffect::IncreaseSpeed
            | CoreEffect::UntilYourNextTurn { .. }
            | CoreEffect::PunishEach { .. }
            | CoreEffect::DrawCastOne { .. }
            | CoreEffect::CastCipherCopy
            | CoreEffect::CastParadigmCopy
            | CoreEffect::RevealPutCreature { .. } => {
                unreachable!("built-in abilities and dungeon rooms only")
            }
            CoreEffect::Ascend => unreachable!("ascend is decompiled as a keyword line"),
            CoreEffect::ExileHaunting => unreachable!("haunt is decompiled as a keyword line"),
            CoreEffect::Champion(_) => unreachable!("champion is decompiled as a keyword line"),
            CoreEffect::Lasting { .. } => unreachable!("a sentence's duration, by `sentences`"),
            CoreEffect::Explore { what, times } => Effect::Explore {
                what: Self::recipient(*what),
                twice: false,
                times: (*times != mtg_core::card::Num::N(1)).then_some(*times),
            },
            CoreEffect::PhaseOut { what } => Effect::PhaseOut {
                what: Self::recipient(*what),
            },
            CoreEffect::Endure { what, count, .. } => Effect::Endure {
                what: Self::recipient(*what),
                count: *count,
            },
            CoreEffect::Amass { count, token } => Effect::Amass {
                kind: format!("{}s", self.pool.get(*token).name.trim_end_matches(" Army")),
                count: *count,
            },
            CoreEffect::EmpowerJace { count, .. } => Effect::EmpowerJace { count: *count },
            CoreEffect::ClearEcho => Effect::ClearEcho,
            CoreEffect::Manifest => Effect::Manifest,
            CoreEffect::ManifestDread => Effect::ManifestDread,
            CoreEffect::Blight(n) => Effect::Blight(*n),
            CoreEffect::CollectEvidence(n) => Effect::CollectEvidence(*n),
            CoreEffect::Forage => Effect::Forage,
            CoreEffect::CastExiled(_)
            | CoreEffect::SuspendTick
            | CoreEffect::CopySpell
            | CoreEffect::WarpExile
            | CoreEffect::Ripple
            | CoreEffect::Cascade
            | CoreEffect::Exploit
            | CoreEffect::EnterAttacking
            | CoreEffect::CreatedAttack
            | CoreEffect::TokenPtX(_)
            | CoreEffect::EnteredAttack
            | CoreEffect::ReturnTransformed
            | CoreEffect::BecomeSaddled => {
                unreachable!("built-in abilities only")
            }
            CoreEffect::Adapt(n) => Effect::Adapt(*n),
            CoreEffect::Bolster(n) => Effect::Bolster(*n),
            CoreEffect::Monstrosity(n) => Effect::Monstrosity(*n),
            CoreEffect::Suspect { what, on } => Effect::Suspect {
                what: r(*what),
                on: *on,
            },
            CoreEffect::BlightBy { who, n } => Effect::BlightBy {
                who: r(*who),
                n: *n,
            },
            CoreEffect::Firebend(n) => Effect::Firebend(*n),
            CoreEffect::DoublePt { to, toughness } => Effect::DoublePt {
                to: r(*to),
                toughness: *toughness,
                of: false,
            },
            CoreEffect::DoubleCounters { kind, to } => Effect::DoubleCounters {
                kind: *kind,
                to: r(*to),
            },
            CoreEffect::DoubleLife { who } => Effect::DoubleLife { who: r(*who) },
            CoreEffect::Earthbend { count, to } => Effect::Earthbend {
                count: *count,
                to: r(*to),
            },
            CoreEffect::Populate => Effect::Populate,
            CoreEffect::EndTurn => Effect::EndTurn,
            CoreEffect::Recruit { .. } => Effect::Recruit { you: false },
            CoreEffect::GraveyardToBattlefield { filter, tapped } => {
                Effect::GraveyardToBattlefield {
                    filter: self.filter(filter),
                    tapped: *tapped,
                }
            }
            CoreEffect::CloakTop { .. } => Effect::CloakTop,
            CoreEffect::ExchangeControl { a, b } => Effect::ExchangeControl {
                what: std::iter::once(*a).chain(*b).map(r).collect(),
            },
            CoreEffect::ExchangeLife { with } => Effect::ExchangeLife { with: r(*with) },
            CoreEffect::GiveControl { of, to } => Effect::GiveControl {
                of: r(*of),
                to: r(*to),
            },
            CoreEffect::Airbend { what } => Effect::Airbend {
                what: vec![r(*what)],
            },
            CoreEffect::Incubate { count, .. } => Effect::Incubate {
                count: *count,
                you: false,
            },
            CoreEffect::Hideaway { count } => Effect::DigFaceDown {
                count: *count,
                then: false,
                them: true,
                any_order: false,
            },
            CoreEffect::Fabricate { .. } | CoreEffect::BecomeRenowned => {
                unreachable!("decompiled as a keyword line")
            }
            CoreEffect::ExileTop { who, count, face } => Effect::ExileTop {
                count: *count,
                who: match who {
                    CoreRecipient::You => None,
                    w => Some(Self::recipient(*w)),
                },
                face: *face,
            },
            CoreEffect::Discover(n) => Effect::Discover(*n),
            CoreEffect::Soulbond { triggering } => Effect::Soulbond {
                triggering: *triggering,
            },
            CoreEffect::MoveSourceCounters { to } => Effect::MoveSourceCounters {
                to: Self::recipient(*to),
            },
            CoreEffect::CumulativeUpkeep(c) => Effect::CumulativeUpkeep(c.clone()),
            CoreEffect::BecomePrepared { what } => Effect::BecomePrepared { what: r(*what) },
            CoreEffect::BecomeSaddledTarget { what } => {
                Effect::BecomeSaddledTarget { what: r(*what) }
            }
            CoreEffect::Connive { what } => Effect::Connive {
                what: Self::recipient(*what),
            },
            CoreEffect::LookAtTop { count, who, looker } => Effect::LookAtTop {
                count: *count,
                of: match r(*who) {
                    Recipient::You => None,
                    w => Some(w),
                },
                by: match r(*looker) {
                    Recipient::You => None,
                    w => Some(w),
                },
                only: false,
            },
            CoreEffect::RevealTop { who, look } => Effect::RevealTop {
                who: match r(*who) {
                    Recipient::You => None,
                    w => Some(w),
                },
                look: *look,
                of: false,
            },
            CoreEffect::Piles {
                count,
                reveal,
                face_down,
                separator,
                opponent_chooses,
                form,
            } => Effect::Piles {
                count: *count,
                reveal: *reveal,
                face_down: *face_down,
                separator: separator.map(r),
                opponent_chooses: *opponent_chooses,
                the_piles: false,
                form: *form,
            },
            CoreEffect::DigSplit {
                count,
                of,
                reveal,
                opponent,
                piles,
                rest,
            } => Effect::DigSplit {
                count: *count,
                of: match r(*of) {
                    Recipient::You => None,
                    w => Some(w),
                },
                reveal: *reveal,
                opponent: *opponent,
                piles: piles.clone(),
                rest: *rest,
                then: false,
            },
            CoreEffect::ShuffleLibrary { who } => Effect::Shuffle {
                who: r(*who),
                library: false,
                have: false,
            },
            CoreEffect::MoveCounters {
                from,
                to,
                kind,
                count,
            } => Effect::MoveCounters {
                from: r(*from),
                to: r(*to),
                kind: *kind,
                count: *count,
            },
            CoreEffect::ExileUntilCard { who, filter } => Effect::ExileUntilCard {
                who: match who {
                    CoreRecipient::You => None,
                    w => Some(Self::recipient(*w)),
                },
                filter: self.filter(filter),
            },
            CoreEffect::MayCastThisFromExile => Effect::MayCastThisFromExile,
            CoreEffect::MayPlayCard {
                until,
                cast,
                free: false,
                what: Some(w),
                adventure,
                rider,
                owner,
                more_opponents,
            } => Effect::MayPlayTarget {
                what: r(*w),
                until: *until,
                cast: *cast,
                adventure: *adventure,
                owner: *owner,
                remains: false,
                rider: ast_rider(*rider, *more_opponents),
            },
            CoreEffect::MayPlayCard {
                until,
                cast,
                free,
                rider,
                owner,
                more_opponents,
                ..
            } => Effect::MayPlayCard {
                until: *until,
                cast: *cast,
                free: *free,
                first: false,
                it: false,
                from_exile: false,
                remains: false,
                owner: *owner,
                rider: ast_rider(*rider, *more_opponents),
            },
            CoreEffect::RevealUntil { who, filter } => Effect::RevealUntil {
                who: match r(*who) {
                    Recipient::You => None,
                    w => Some(w),
                },
                filter: self.filter(filter),
            },
            CoreEffect::IfCard {
                filter,
                negated,
                shares_type,
                revealed,
                then,
            } => Effect::IfCard {
                filter: self.filter(filter),
                negated: *negated,
                card: true,
                that: false,
                shares_type: *shares_type,
                revealed: *revealed,
                then: self.sentences(then),
            },
            CoreEffect::RevealCard => Effect::RevealIt { that: false },
            CoreEffect::DrawReveal => Effect::DrawReveal,
            CoreEffect::PutCard { what, to, random } => Effect::PutCard {
                random: *random,
                rest: None,
                what: match what {
                    mtg_core::card::CardSet::It => crate::ast::CardWords::It,
                    mtg_core::card::CardSet::Revealed => crate::ast::CardWords::AllRevealed,
                    mtg_core::card::CardSet::OtherRevealed => crate::ast::CardWords::OtherRevealed,
                    mtg_core::card::CardSet::ItUnlessCast => {
                        crate::ast::CardWords::ExiledUnlessCast
                    }
                },
                to: *to,
                who: None,
                that_player: false,
            },
            CoreEffect::ExilePlayable {
                count,
                until,
                choose,
                from,
                face_down,
                cond,
                rider,
            } => Effect::ExilePlayable {
                count: *count,
                until: *until,
                first: *until != mtg_core::card::PlayUntil::YourNextEndStep,
                it: false,
                choose: *choose,
                choose_one: false,
                from: from.map(Self::recipient),
                possessive: false,
                this_way: false,
                remains: false,
                face_down: *face_down,
                look_first: false,
                look_those: false,
                cond: cond.map(|c| self.static_cond(self.pool.condition(c))),
                rider: ast_rider(*rider, false),
            },
            CoreEffect::Dig {
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
            } => Effect::Dig {
                count: *count,
                filter: filter.as_ref().map(|f| self.filter(f)),
                optional: *optional,
                all: *all,
                rest_graveyard: *rest_graveyard,
                reveal_top: false,
                revealing: *optional && onto.is_none(),
                take: *take,
                also: also.as_ref().map(|f| self.filter(f)),
                random: *random,
                then: false,
                top: *top,
                onto: *onto,
                that_card: false,
                short: false,
                reword: false,
                rest_revealed: false,
            },
            CoreEffect::Modes(modes) => match self.modal_two(modes) {
                Some(singles) => Effect::ModesTwo(singles),
                None => Effect::Modes(modes.iter().map(|m| self.mode(m)).collect()),
            },
            CoreEffect::GrantAbility { ability, to } => Effect::GrantAbility {
                ability: Box::new(self.ability(self.pool.ability(*ability))),
                to: Self::recipient(*to),
            },
            CoreEffect::ReturnChosen { filter, count, who } => {
                let mut f = self.filter(filter);
                f.who = Some(mtg_core::card::Who::You);
                Effect::ReturnChosen {
                    count: *count,
                    filter: f,
                    who: (*who != CoreRecipient::You).then(|| Self::recipient(*who)),
                }
            }
            CoreEffect::CantBeRegenerated { to } => Effect::CantBeRegenerated {
                to: Self::recipient(*to),
            },
            CoreEffect::SkipNext { who, what } => Effect::SkipNext {
                who: r(*who),
                what: *what,
            },
            CoreEffect::AssignsNoCombatDamage { to } => Effect::AssignsNoCombatDamage {
                to: Self::recipient(*to),
            },
            CoreEffect::ExtraTurns { who, n } => Effect::ExtraTurns {
                who: r(*who),
                n: *n,
            },
            CoreEffect::AdditionalPhases { combats, main } => Effect::AdditionalPhases {
                combats: *combats,
                main: *main,
                leading: true,
                main_phase: *main,
            },
            CoreEffect::SkipNextUntap { to } => Effect::SkipNextUntap {
                to: Self::recipient(*to),
                steps: false,
            },
            CoreEffect::BlocksIfAble { blocker, attacker } => Effect::BlocksIfAble {
                blocker: Self::recipient(*blocker),
                attacker: Self::recipient(*attacker),
            },
            CoreEffect::CantBlockThis { blocker, attacker } => Effect::CantBlockThis {
                blocker: Self::recipient(*blocker),
                attacker: Self::recipient(*attacker),
            },
            CoreEffect::GrantFlashback { to, harmonize } => Effect::GrantFlashback {
                to: Self::recipient(*to),
                that_card: false,
                harmonize: *harmonize,
            },
            CoreEffect::GrantGraveyardCast(f) => Effect::GrantGraveyardCast(self.filter(f)),
            CoreEffect::TopPermission(t) => {
                Effect::TopPermission(Box::new(crate::ast::TopPlayAst {
                    lands: t.lands,
                    spells: t.spells.as_ref().map(|f| self.filter(f)),
                    more: crate::ast::TopMore {
                        look: t.look,
                        life: t.life,
                        ..Default::default()
                    },
                }))
            }
            CoreEffect::SwitchPt { to } => Effect::SwitchPt {
                to: Self::recipient(*to),
            },
            CoreEffect::CreatureTypes { to, all } => Effect::CreatureTypes {
                to: Self::recipient(*to),
                all: *all,
            },
            CoreEffect::PutFromHand { filter, tapped } => Effect::PutFromHand {
                filter: self.filter(filter),
                tapped: *tapped,
                also: None,
                attacking: false,
            },
            CoreEffect::IfThisWay { cond, then } => {
                Effect::IfThisWay(self.this_way_cond(cond), self.sentences(then))
            }
            CoreEffect::IfExcessDamage(then) => Effect::IfExcessDamage(self.sentences(then), false),
            CoreEffect::ChooseAct {
                act,
                filter,
                count,
                up_to,
            } => Effect::ChooseAct {
                act: *act,
                count: *count,
                up_to: *up_to,
                filter: self.filter(filter),
                among_them: false,
                among_those: false,
                this_way: false,
                returns: false,
            },
            CoreEffect::OneOf(options) => {
                // "gains your choice of lifelink or indestructible".
                let granted: Option<Vec<_>> = options
                    .iter()
                    .map(|o| match o {
                        CoreEffect::Grant { keywords: k, to } if keywords(*k).len() == 1 => {
                            Some((keywords(*k)[0], *to))
                        }
                        _ => None,
                    })
                    .collect();
                match granted {
                    Some(g) if !g.is_empty() && g.iter().all(|(_, t)| *t == g[0].1) => {
                        Effect::GrantChoice {
                            to: r(g[0].1),
                            keywords: g.iter().map(|(k, _)| *k).collect(),
                        }
                    }
                    _ => Effect::OneOf(options.iter().map(|o| self.effect(o)).collect()),
                }
            }
            CoreEffect::DiscardUnless { who, count, filter } => Effect::DiscardUnless {
                who: Some(r(*who)),
                count: *count,
                filter: self.filter(filter),
            },
            CoreEffect::DiscardAll { who, filter, one } => Effect::DiscardAll {
                who: Some(r(*who)),
                filter: filter.as_ref().map(|f| self.filter(f)),
                all_cards: false,
                one: *one,
            },
        }
    }

    fn this_way_cond(&self, c: &mtg_core::card::ThisWayCond) -> crate::ast::ThisWayCond {
        use mtg_core::card::ThisWay;
        crate::ast::ThisWayCond {
            verb: c.verb,
            filter: c.filter.as_ref().map(|f| self.filter(f)),
            card: matches!(
                c.verb,
                ThisWay::Discarded | ThisWay::Milled | ThisWay::PutInHand
            ),
            negated: c.negated,
            wording: if c.by_you {
                crate::ast::ThisWayWording::You
            } else {
                crate::ast::ThisWayWording::Was
            },
        }
    }

    fn trigger_obj(&self, o: &CoreTriggerObj) -> TriggerObj {
        match o {
            CoreTriggerObj::This => TriggerObj::This,
            CoreTriggerObj::Filter(f) => TriggerObj::Filter(self.filter(f)),
        }
    }

    #[allow(clippy::too_many_lines)] // one arm per trigger kind
    fn trigger(&self, t: &TriggerSpec) -> Trigger {
        match t {
            TriggerSpec::Enters(o) => Trigger::Enters(self.trigger_obj(o)),
            TriggerSpec::Dies(o) => Trigger::Dies(self.trigger_obj(o)),
            TriggerSpec::BeginningOfStep { step, whose } => Trigger::BeginningOfStep {
                step: *step,
                whose: *whose,
            },
            TriggerSpec::AttachedControllersStep(step) => Trigger::AttachedStep(*step),
            TriggerSpec::State(i) => Trigger::State(self.static_cond(self.pool.condition(*i))),
            TriggerSpec::Attacks(o) => Trigger::Attacks(self.trigger_obj(o)),
            TriggerSpec::AttacksFirstTime(o) => Trigger::AttacksFirstTime(self.trigger_obj(o)),
            TriggerSpec::AttacksBattle(o) => Trigger::AttacksBattle(self.trigger_obj(o)),
            TriggerSpec::UnlocksDoor { .. } => Trigger::UnlocksDoor,
            TriggerSpec::BecomesStage(k) => Trigger::BecomesLevel(k + 1),
            TriggerSpec::Chapter(mask) => {
                Trigger::Chapter((1..=15u8).filter(|n| mask & (1 << n) != 0).collect())
            }
            TriggerSpec::FullyUnlocks { .. } => Trigger::FullyUnlocks,
            TriggerSpec::DealsCombatDamageToPlayerOrBattle(o) => {
                Trigger::DealsCombatDamageToPlayerOrBattle(self.trigger_obj(o))
            }
            TriggerSpec::DealsCombatDamageToPlayerOrPlaneswalker(o) => {
                Trigger::DealsCombatDamageToPlayerOrPlaneswalker(self.trigger_obj(o))
            }
            TriggerSpec::AttacksAlone(o) => Trigger::AttacksAlone(self.trigger_obj(o)),
            TriggerSpec::PlayerAttacksWith { whose, min } => Trigger::PlayerAttacksWith {
                whose: *whose,
                min: *min,
            },
            TriggerSpec::AttacksWith { this, min } => Trigger::AttacksWith {
                this: *this,
                min: *min,
            },
            TriggerSpec::AttacksUnblocked(o) => Trigger::AttacksUnblocked(self.trigger_obj(o)),
            TriggerSpec::BecomesBlocked(o) => Trigger::BecomesBlocked(self.trigger_obj(o)),
            TriggerSpec::BlocksOrBlocked(o) => Trigger::BlocksOrBlocked(self.trigger_obj(o)),
            TriggerSpec::AttacksOrBlocks(o) => Trigger::AttacksOrBlocks(self.trigger_obj(o)),
            TriggerSpec::Leaves(o) => Trigger::Leaves(self.trigger_obj(o)),
            TriggerSpec::LeavesWithoutDying(o) => Trigger::LeavesWithoutDying(self.trigger_obj(o)),
            TriggerSpec::ReturnedToHand(o) => Trigger::ReturnedToHand(self.trigger_obj(o)),
            TriggerSpec::DiesOrExiled(o) => Trigger::DiesOrExiled(self.trigger_obj(o)),
            TriggerSpec::Blocks(o) => Trigger::Blocks(self.trigger_obj(o)),
            TriggerSpec::BlockPair {
                what,
                other,
                blocks,
                blocked,
                any,
            } => Trigger::BlockPair {
                obj: self.trigger_obj(what),
                other: Box::new(self.filter(other)),
                blocks: *blocks,
                blocked: *blocked,
                any: *any,
            },
            TriggerSpec::DealsCombatDamageToPlayer(o) => {
                Trigger::DealsCombatDamageToPlayer(self.trigger_obj(o))
            }
            TriggerSpec::GainsLife { whose } => Trigger::GainsLife { whose: *whose },
            TriggerSpec::LosesLife { whose } => Trigger::LosesLife { whose: *whose },
            TriggerSpec::GainsLifeFirst { whose } => Trigger::GainsLifeFirst { whose: *whose },
            TriggerSpec::RollsDice { whose } => Trigger::RollsDice { whose: *whose },
            TriggerSpec::DayNight => Trigger::DayNight,
            TriggerSpec::Sacrificed { what, whose } => Trigger::Sacrificed {
                obj: self.trigger_obj(what),
                whose: *whose,
            },
            TriggerSpec::WinsFlip { whose } => Trigger::WinsFlip { whose: *whose },
            TriggerSpec::CastsSpell {
                whose,
                noncreature,
                instant_or_sorcery,
                or_copy,
            } => Trigger::CastsSpell {
                whose: *whose,
                noncreature: *noncreature,
                instant_or_sorcery: *instant_or_sorcery,
                or_copy: *or_copy,
            },
            TriggerSpec::BecomesTarget(o) => Trigger::BecomesTarget(self.trigger_obj(o)),
            TriggerSpec::TargetedBy {
                what,
                spells,
                whose,
            } => Trigger::TargetedBy {
                obj: self.trigger_obj(what),
                spells: *spells,
                whose: *whose,
                whenever: true,
            },
            TriggerSpec::TargetedByOpponent => Trigger::TargetedByOpponent,
            TriggerSpec::TargetedFirstTime => Trigger::TargetedFirstTime,
            TriggerSpec::TargetedByYouFirstTime => Trigger::TargetedByYouFirstTime,
            TriggerSpec::TurnedFaceUp(o) => Trigger::TurnedFaceUp(self.trigger_obj(o)),
            TriggerSpec::Exploits => Trigger::Exploits,
            TriggerSpec::BecomesPlotted => Trigger::BecomesPlotted,
            TriggerSpec::Foretells => Trigger::Foretells,
            TriggerSpec::Reflexive => {
                unreachable!("a reflexive trigger is decompiled with its effect")
            }
            TriggerSpec::DrawsNth { whose, n } => Trigger::DrawsNth {
                whose: *whose,
                n: *n,
                in_a_turn: false,
            },
            TriggerSpec::BecomesTapped(o) => Trigger::BecomesTapped(self.trigger_obj(o)),
            TriggerSpec::TapsForMana { what, whose } => Trigger::TapsForMana {
                what: self.trigger_obj(what),
                whose: *whose,
            },
            TriggerSpec::BecomesAttachedTo(f) => Trigger::BecomesAttachedTo(self.filter(f)),
            TriggerSpec::BecomesUntapped(o) => Trigger::BecomesUntapped(self.trigger_obj(o)),
            TriggerSpec::CountersPut { what, kind } => Trigger::CountersPut {
                obj: self.trigger_obj(what),
                kind: *kind,
            },
            TriggerSpec::NthCounterPut { kind, n } => Trigger::NthCounterPut { kind: *kind, n: *n },
            TriggerSpec::CountersRemoved {
                what,
                kind,
                one_or_more,
            } => Trigger::CountersRemoved {
                obj: self.trigger_obj(what),
                kind: *kind,
                one_or_more: *one_or_more,
            },
            TriggerSpec::YouPutCounters { what, kind } => Trigger::YouPutCounters {
                obj: self.trigger_obj(what),
                kind: *kind,
            },
            TriggerSpec::DealtDamage(o) => Trigger::DealtDamage(self.trigger_obj(o)),
            TriggerSpec::DealtDamageAtLeast(o, n) => {
                Trigger::DealtDamageAtLeast(self.trigger_obj(o), *n)
            }
            TriggerSpec::DealtCombatDamage(o) => Trigger::DealtCombatDamage(self.trigger_obj(o)),
            TriggerSpec::DealsDamage { what, to, combat } => Trigger::DealsDamage {
                obj: self.trigger_obj(what),
                to: *to,
                combat: *combat,
            },
            TriggerSpec::DealsDamageTo { what, filter } => Trigger::DealsDamageTo {
                obj: self.trigger_obj(what),
                filter: Box::new(self.filter(filter)),
            },
            TriggerSpec::CastsFiltered { whose, filter } => Trigger::CastsFiltered {
                whose: *whose,
                filter: self.filter(filter),
            },
            TriggerSpec::BlockedBy(f) => {
                Trigger::BecomesBlocked(TriggerObj::Filter(self.filter(f)))
            }
            TriggerSpec::Cycles {
                this,
                another,
                or_discard,
            } => Trigger::Cycles {
                this: *this,
                another: *another,
                or_discard: *or_discard,
            },
            TriggerSpec::CastsNth { whose, nth } => Trigger::CastsNth {
                whose: *whose,
                nth: *nth,
            },
            TriggerSpec::EntersBatch(f) => Trigger::EntersBatch(self.filter(f)),
            TriggerSpec::LeavesBatch { filter, dies } => Trigger::LeavesBatch {
                filter: self.filter(filter),
                dies: *dies,
            },
            TriggerSpec::Draws { whose } => Trigger::Draws { whose: *whose },
            TriggerSpec::DiscardsThis => Trigger::DiscardsThis,
            TriggerSpec::Discards { whose, filter } => Trigger::Discards {
                whose: *whose,
                filter: self.filter(filter),
            },
            TriggerSpec::LeavesGraveyard(f) => Trigger::LeavesGraveyard(self.filter(f)),
            TriggerSpec::PutIntoGraveyard(f) => Trigger::PutIntoGraveyard(self.filter(f)),
            TriggerSpec::ThisToGraveyard => Trigger::ThisToGraveyard,
            TriggerSpec::PutIntoGraveyardFromBattlefield(f) => {
                Trigger::PutIntoGraveyardFromBattlefield(self.filter(f))
            }
            TriggerSpec::EntersFromGraveyard(f) => Trigger::EntersFromGraveyard(self.filter(f)),
            TriggerSpec::EntersFrom { what, zone, yours } => Trigger::EntersFrom {
                obj: self.trigger_obj(what),
                zone: *zone,
                yours: *yours,
            },
            TriggerSpec::Expends(n) => Trigger::Expends(*n),
            TriggerSpec::CollectsEvidence { .. } => Trigger::CollectsEvidence,
            TriggerSpec::Forages { .. } => Trigger::Forages,
            TriggerSpec::CreaturesDealDamage(f) => Trigger::CreaturesDealDamage(self.filter(f)),
            TriggerSpec::CreaturesDealCombatDamage(f) => {
                Trigger::CreaturesDealCombatDamage(self.filter(f))
            }
            TriggerSpec::NoncombatDamageToOpponent { yours, batch } => {
                Trigger::NoncombatDamageToOpponent {
                    yours: *yours,
                    batch: *batch,
                }
            }
            TriggerSpec::OpponentGainsControl => Trigger::OpponentGainsControl,
            TriggerSpec::CommitsCrime { your_turn } => Trigger::CommitsCrime {
                your_turn: *your_turn,
            },
            TriggerSpec::CastThis => Trigger::CastThis { or_cycle: false },
            TriggerSpec::Next(inner) => Trigger::Next(Box::new(self.trigger(inner))),
            TriggerSpec::Batch(inner) => Trigger::Batch(Box::new(self.trigger(inner))),
            TriggerSpec::Each(inner) => Trigger::Each(Box::new(self.trigger(inner))),
            TriggerSpec::OnNextTurn(inner) => Trigger::OnNextTurn(Box::new(self.trigger(inner))),
            TriggerSpec::Sacrifices(f) => Trigger::Sacrifices(self.filter(f)),
            TriggerSpec::DiscardsBatch { whose } => Trigger::DiscardsBatch { whose: *whose },
            TriggerSpec::SacrificesBatch(f) => Trigger::SacrificesBatch(self.filter(f)),
            TriggerSpec::AttacksWithFilter { filter, player } => Trigger::AttacksWithFilter {
                filter: self.filter(filter),
                player: *player,
            },
            TriggerSpec::GainsOrLosesLife { your_turn } => Trigger::GainsOrLosesLife {
                your_turn: *your_turn,
            },
            TriggerSpec::ScriesOrSurveils => Trigger::ScriesOrSurveils,
            TriggerSpec::Scries => Trigger::Scries,
            TriggerSpec::Explores(o) => Trigger::Explores(self.trigger_obj(o)),
            TriggerSpec::CrewsOrSaddles(o) => Trigger::CrewsOrSaddles(self.trigger_obj(o)),
            TriggerSpec::BecomesSaddled(o) => Trigger::BecomesSaddled(self.trigger_obj(o)),
            TriggerSpec::YouTap(f) => Trigger::YouTap(self.filter(f)),
            TriggerSpec::Activates { whose, kind } => Trigger::Activates {
                whose: *whose,
                kind: *kind,
            },
            TriggerSpec::Transforms => Trigger::Transforms,
            // "When this creature enters or transforms into ~".
            TriggerSpec::Either(pair)
                if pair.0 == TriggerSpec::Enters(CoreTriggerObj::This)
                    && pair.1 == TriggerSpec::Transforms =>
            {
                Trigger::Or {
                    first: Box::new(self.trigger(&pair.0)),
                    second: Box::new(Trigger::Transforms),
                    whenever: true,
                }
            }
            // "When you cast or cycle ~".
            TriggerSpec::Either(pair)
                if pair.0 == TriggerSpec::CastThis && pair.1.cycles_this() =>
            {
                Trigger::CastThis { or_cycle: true }
            }
            // "When this creature enters or dies" and kin.
            TriggerSpec::Either(pair)
                if pair.0 == TriggerSpec::Enters(CoreTriggerObj::This)
                    && matches!(
                        pair.1,
                        TriggerSpec::Dies(CoreTriggerObj::This)
                            | TriggerSpec::Leaves(CoreTriggerObj::This)
                            | TriggerSpec::Attacks(CoreTriggerObj::This)
                            | TriggerSpec::TurnedFaceUp(CoreTriggerObj::This)
                            | TriggerSpec::BecomesTapped(CoreTriggerObj::This)
                    ) =>
            {
                Trigger::Or {
                    first: Box::new(self.trigger(&pair.0)),
                    second: Box::new(self.trigger(&pair.1)),
                    whenever: matches!(
                        pair.1,
                        TriggerSpec::Attacks(_) | TriggerSpec::BecomesTapped(_)
                    ),
                }
            }
            // "When enchanted creature becomes tapped or is dealt damage".
            TriggerSpec::Either(pair)
                if matches!(
                    (&pair.0, &pair.1),
                    (TriggerSpec::BecomesTapped(a), TriggerSpec::DealtDamage(b)) if a == b
                ) =>
            {
                Trigger::Or {
                    first: Box::new(self.trigger(&pair.0)),
                    second: Box::new(self.trigger(&pair.1)),
                    whenever: false,
                }
            }
            TriggerSpec::Either(pair) => Trigger::Either(
                Box::new(self.trigger(&pair.0)),
                Box::new(self.trigger(&pair.1)),
            ),
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
            CoreAffects::OpponentsAnd(f) => Affects::OpponentsAnd(self.filter(f)),
            CoreAffects::Target(i) => Affects::Target(*i),
            CoreAffects::Targets(a, u8::MAX) => Affects::Target(usize::from(*a)),
            CoreAffects::Targets(a, n) => Affects::Targets(usize::from(*a), usize::from(*n)),
            CoreAffects::OtherThanTarget(f, i) => Affects::OtherThanTarget(self.filter(f), *i),
            CoreAffects::Chosen(f) => Affects::Chosen(self.filter(f)),
            CoreAffects::DamagedThisWay => Affects::DamagedThisWay,
            CoreAffects::Except(f) => Affects::Except(self.filter(f)),
        }
    }

    fn replacement(&self, r: &ReplacementAbility) -> Replacement {
        Replacement {
            event: match &r.replaces {
                CoreReplaces::Damage { from, to, combat } => Replaces::Damage {
                    from: from.as_ref().map(|f| self.affects(f)),
                    to: self.affects(to),
                    combat: *combat,
                    wording: crate::ast::DamageWording::Dealt,
                },
                CoreReplaces::DamageToAndBy { what, combat } => Replaces::DamageToAndBy {
                    what: self.affects(what),
                    combat: *combat,
                },
                CoreReplaces::GainLife { whose } => Replaces::GainLife { whose: *whose },
                CoreReplaces::LoseLife { whose } => Replaces::LoseLife {
                    whose: *whose,
                    your_turn: false,
                },
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
                CoreReplaces::ThisToGraveyard => Replaces::ThisToGraveyard,
                CoreReplaces::FilteredToGraveyard { filter } => {
                    Replaces::FilteredToGraveyard(self.filter(filter))
                }
                CoreReplaces::CreateTokens {
                    creature,
                    effect,
                    artifact,
                    you,
                } => Replaces::CreateTokens {
                    creature: *creature,
                    effect: *effect,
                    artifact: *artifact,
                    you: *you,
                },
                CoreReplaces::PlusCounters { on } => Replaces::PlusCounters {
                    on: self.filter(on),
                },
                CoreReplaces::AnyCounters { on, by } => Replaces::AnyCounters {
                    on: self.filter(on),
                    by: *by,
                },
                CoreReplaces::Dies { to } => Replaces::Dies {
                    to: self.affects(to),
                },
                CoreReplaces::Destroy { to } => Replaces::Destroy {
                    to: self.affects(to),
                },
                CoreReplaces::Enters => Replaces::Enters,
                CoreReplaces::OthersEnter { what } => Replaces::OthersEnter {
                    what: self.filter(what),
                    each: true,
                },
            },
            outcome: r.replacement,
            then: self.sentences(&r.then),
            token: match r.replacement {
                mtg_core::card::Replacement::PlusToken { token, .. }
                | mtg_core::card::Replacement::TokensInstead { token } => {
                    Some(self.pool.get(token).name.clone())
                }
                _ => None,
            },
            wording: if r.joined {
                crate::ast::ThenWording::Joined
            } else {
                crate::ast::ThenWording::Sentence
            },
            cond: None,
        }
    }

    fn modification(&self, m: &Modification) -> Mod {
        match m {
            Modification::AddAbility(a) => {
                Mod::AddAbility(Box::new(self.ability(self.pool.ability(*a))))
            }
            // `BecomeTypes`: only `Effect::Animate` makes it, never a static
            // ability.
            Modification::BecomeTypes(t) | Modification::AddTypes(t) => Mod::AddTypes(types(*t)),
            Modification::AddLandType(c) => Mod::AddLandType(*c),
            Modification::SetLandType(c) => Mod::SetLandType(*c),
            Modification::ChosenLandType => Mod::ChosenLandType,
            Modification::AddChosenType => Mod::AddChosenType { creature: false },
            Modification::AddSubtype(s) => Mod::AddSubtype(self.pool.subtype_name(*s).to_string()),
            Modification::AddSupertype(s) => Mod::AddSupertype(*s),
            Modification::SetCreatureType(s) => {
                Mod::SetSubtype(self.pool.subtype_name(*s).to_string())
            }
            Modification::Control => Mod::Control,
            Modification::AddColors(c) => Mod::AddColors(colors(*c)),
            Modification::SetColors(c) => Mod::SetColors(colors(*c)),
            Modification::ChosenColor => Mod::ChosenColor,
            Modification::ProtectionFromChosen => Mod::ProtectionFromChosen,
            Modification::AddKeywords(k) => Mod::AddKeywords(keywords(*k)),
            Modification::RemoveKeywords(k) => Mod::LosesKeywords(keywords(*k)),
            Modification::RemoveAllAbilities => Mod::RemoveAllAbilities,
            Modification::AllCreatureTypes => Mod::AllCreatureTypes { every: true },
            Modification::NoCreatureTypes => Mod::NoCreatureTypes,
            Modification::SetTypes(t) => Mod::SetTypes(types(*t)),
            Modification::SetPt { power, toughness } => Mod::SetPt {
                power: *power,
                toughness: *toughness,
            },
            Modification::ModifyPtPer {
                power, toughness, ..
            }
            | Modification::ModifyPt { power, toughness } => Mod::ModifyPt {
                power: *power,
                toughness: *toughness,
            },
            Modification::SwitchPt => Mod::SwitchPt,
            Modification::AddActivatedOf(from) => Mod::ActivatedOf {
                from: match &**from {
                    mtg_core::card::CountExpr::ExiledWith(f) => {
                        crate::ast::CountExpr::ExiledWith(f.as_ref().map(|f| self.filter(f)))
                    }
                    other => self.cda_count(other),
                },
                word: crate::ast::GroupWord::All,
                while_here: false,
            },
            Modification::CdaColors(c) => Mod::CdaColors(colors(*c)),
            Modification::CdaPt {
                count,
                sets,
                plus_toughness,
                ..
            } => Mod::CdaPt {
                count: self.cda_count(count),
                sets: *sets,
                plus_toughness: *plus_toughness,
                noun: false,
            },
        }
    }

    #[allow(clippy::too_many_lines)] // one arm per ability kind
    fn ability(&self, a: &AbilityDef) -> Ability {
        match a {
            AbilityDef::TapForMana(m) => Ability::Mana(vec![*m]),
            AbilityDef::ManaTrigger { what, effects } => Ability::Triggered {
                limit: mtg_core::card::TurnLimit::None,
                trigger: Trigger::TappedForMana(TriggerObj::Filter(self.filter(what))),
                condition: None,
                targets: Vec::new(),
                effects: self.sentences(effects),
                from_graveyard: false,
            },
            AbilityDef::Protection(f) => Ability::Protection(self.filter(f)),
            AbilityDef::Landwalk(f) => Ability::Landwalk(self.filter(f)),
            AbilityDef::HexproofFrom(f) => Ability::HexproofFrom(self.filter(f)),
            AbilityDef::OpeningHand => Ability::OpeningHand,
            AbilityDef::OpeningHandReveal(id) => match self.ability(self.pool.ability(*id)) {
                Ability::Triggered {
                    trigger: crate::ast::Trigger::BeginningOfStep { step, whose },
                    effects,
                    ..
                } => Ability::OpeningHandReveal {
                    step,
                    whose,
                    after: false,
                    effects,
                },
                _ => unreachable!("an opening hand reveal's delayed trigger is a step trigger"),
            },
            AbilityDef::TriggerAgain {
                filter,
                entering,
                battlefield,
                cause,
                suppress,
                ..
            } => Ability::TriggerAgain {
                filter: self.filter(filter),
                entering: entering.as_ref().map(|f| self.filter(f)),
                battlefield: *battlefield,
                cause: *cause,
                suppress: *suppress,
            },
            AbilityDef::PlayerStatic {
                kind:
                    mtg_core::card::PlayerStaticKind::ActivationCost {
                        sources,
                        by,
                        more,
                        min_one,
                        class,
                        activator,
                    },
                ..
            } => Ability::ActivationCost {
                sources: match sources {
                    mtg_core::card::AbilitySources::Filter(f) => {
                        crate::ast::ActivationSources::Filter(Box::new(
                            self.filter(self.pool.filter(*f)),
                        ))
                    }
                    mtg_core::card::AbilitySources::Any => crate::ast::ActivationSources::Any,
                    mtg_core::card::AbilitySources::ChosenName => {
                        crate::ast::ActivationSources::ChosenName
                    }
                },
                by: *by,
                more: *more,
                min_one: *min_one,
                class: *class,
                activator: *activator,
            },
            AbilityDef::PlayerStatic {
                kind: mtg_core::card::PlayerStaticKind::CantCast { spells },
                ..
            } => Ability::CantBeCast(self.filter(self.pool.filter(*spells))),
            AbilityDef::PlayerStatic {
                kind:
                    mtg_core::card::PlayerStaticKind::CantAttackYou {
                        attackers,
                        planeswalkers,
                    },
                ..
            } => Ability::CantAttackYou {
                filter: self.filter(self.pool.filter(*attackers)),
                planeswalkers: *planeswalkers,
            },
            AbilityDef::PlayerStatic { kind, condition } => Ability::PlayerStatic {
                kind: *kind,
                cond: condition.map(|c| self.static_cond(self.pool.condition(c))),
            },
            AbilityDef::AttackUnless {
                condition,
                attack,
                block,
                negated,
            } => Ability::AttackUnless {
                cond: self.static_cond(self.pool.condition(*condition)),
                attack: *attack,
                block: *block,
                negated: *negated,
            },
            AbilityDef::Toxic(n) => Ability::Mechanic(crate::ast::Mechanic::Toxic(*n)),
            AbilityDef::CombatTax(t) => Ability::CombatTax {
                scope: t.scope,
                generic: t.generic,
                per: t.per,
                cond: t
                    .condition
                    .map(|c| self.static_cond(self.pool.condition(c))),
            },
            AbilityDef::HandSize { whose, delta, set } => Ability::HandSize {
                whose: *whose,
                delta: *delta,
                set: *set,
            },
            AbilityDef::ExtraVote { optional } => Ability::ExtraVote {
                optional: *optional,
            },
            // The enters-with half of "enters or is turned face up" stays a
            // replacement of its own here.
            AbilityDef::AsTurnedFaceUp { kind, n } => Ability::AsTurnedFaceUp {
                enters: false,
                kind: *kind,
                n: *n,
            },
            AbilityDef::CardGrant(g) => Ability::CardGrant {
                filter: self.filter(&g.cards),
                zone: g.zone,
                players: g.players,
                each: g.each,
                grant: match &g.grant {
                    mtg_core::card::GrantedWay::Flashback(c) => {
                        crate::ast::Granted::Flashback(c.clone())
                    }
                    mtg_core::card::GrantedWay::Mayhem(c) => crate::ast::Granted::Mayhem(c.clone()),
                    mtg_core::card::GrantedWay::Warp(c) => crate::ast::Granted::Warp(c.clone()),
                    mtg_core::card::GrantedWay::Miracle(c) => {
                        crate::ast::Granted::Miracle(c.clone())
                    }
                    mtg_core::card::GrantedWay::Retrace => crate::ast::Granted::Retrace,
                    mtg_core::card::GrantedWay::Foretell => crate::ast::Granted::Foretell,
                    mtg_core::card::GrantedWay::Sneak(c) => crate::ast::Granted::Sneak(c.clone()),
                    mtg_core::card::GrantedWay::JumpStart => crate::ast::Granted::JumpStart,
                    mtg_core::card::GrantedWay::Ability(a) => {
                        crate::ast::Granted::Ability(Box::new(self.ability(self.pool.ability(*a))))
                    }
                },
                cond: g
                    .condition
                    .map(|c| self.static_cond(self.pool.condition(c))),
            },
            AbilityDef::CastFree {
                spells,
                from_hand,
                max,
                limit,
            } => Ability::CastFree {
                spells: self.filter(spells),
                from_hand: *from_hand,
                max: max.as_ref().map(|c| self.cda_count(c)),
                limit: *limit,
            },
            AbilityDef::PlayFromTop {
                lands,
                spells,
                once,
                more,
            } => Ability::PlayFromTop {
                lands: *lands,
                spells: spells.as_ref().map(|f| self.filter(f)),
                once: *once,
                more: crate::ast::TopMore {
                    look: more.look,
                    cond: more
                        .condition
                        .map(|c| self.static_cond(self.pool.condition(c))),
                    flash: more.flash,
                    life: more.life,
                    wording: crate::ast::TopWording::Split,
                },
            },
            AbilityDef::PlayExiled {
                lands,
                spells,
                your_turn,
                owned,
                look,
                more,
            } => Ability::PlayExiled {
                lands: *lands,
                spells: spells.as_ref().map(|f| self.filter(f)),
                your_turn: *your_turn,
                owned: *owned,
                look: *look,
                more: crate::ast::PlayExiledMore {
                    any_type: more.spend != mtg_core::card::SpendAs::Normal,
                    flash: more.flash,
                    finality: more.finality,
                    the_card: false,
                    condition: more
                        .condition
                        .map(|c| Box::new(self.static_cond(self.pool.condition(c)))),
                },
            },
            AbilityDef::TopRevealed { players, hands } => Ability::TopRevealed {
                players: *players,
                hands: *hands,
            },
            AbilityDef::MayLook {
                top,
                face_down,
                opponents,
            } => Ability::MayLook {
                top: *top,
                face_down: *face_down,
                opponents: *opponents,
            },
            AbilityDef::CrewsAs {
                saddles,
                toughness,
                plus,
            } => Ability::CrewsAs {
                saddles: *saddles,
                toughness: *toughness,
                plus: *plus,
            },
            AbilityDef::CostChange(c) if c.grant.is_some() && c.generic == 0 => {
                let g = c.grant.as_deref().expect("checked");
                Ability::SpellsHave {
                    filter: self.filter(&c.spells),
                    from: c.from,
                    whose: c.whose,
                    keywords: keywords(g.keywords),
                    storm: g.storm,
                    affinity: g.affinity.as_ref().map(|f| self.filter(f)),
                    uncounterable: g.uncounterable,
                    spend: (g.spend != mtg_core::card::SpendAs::Normal).then_some((
                        g.spend,
                        false,
                        g.spend_abilities,
                    )),
                }
            }
            AbilityDef::CostChange(c) => Ability::CostChange {
                filter: self.filter(&c.spells),
                whose: c.whose,
                generic: c.generic,
                cond: c.cond.map(|i| self.static_cond(self.pool.condition(i))),
                per: c.per,
                first: c.first,
                flash: c
                    .grant
                    .as_ref()
                    .is_some_and(|g| g.keywords.contains(mtg_core::card::Keyword::Flash)),
                from: c.from,
            },
            AbilityDef::SacForMana(m) => Ability::SacMana(*m),
            AbilityDef::RestrictedMana { mana, restriction } => Ability::RestrictedMana {
                mana: *mana,
                restriction: self.mana_restriction(restriction),
            },
            AbilityDef::XManaColor(c) => Ability::XManaColor(*c),
            AbilityDef::BlockRule(r) => Ability::BlockRule(match r {
                mtg_core::card::BlockRule::NotBy(f) => crate::ast::BlockRule::NotBy(self.filter(f)),
                mtg_core::card::BlockRule::OnlyBy(f) => {
                    crate::ast::BlockRule::OnlyBy(self.filter(f))
                }
                mtg_core::card::BlockRule::AtMostOne => crate::ast::BlockRule::AtMostOne,
                mtg_core::card::BlockRule::NotByLesserPower => {
                    crate::ast::BlockRule::NotByLesserPower
                }
                mtg_core::card::BlockRule::BlocksOnly(f) => {
                    crate::ast::BlockRule::BlocksOnly(self.filter(f))
                }
                mtg_core::card::BlockRule::CantBlock(f) => {
                    crate::ast::BlockRule::CantBlock(self.filter(f))
                }
                mtg_core::card::BlockRule::MinBlockers(n) => crate::ast::BlockRule::MinBlockers(*n),
                mtg_core::card::BlockRule::BlocksShadow { had } => {
                    crate::ast::BlockRule::BlocksShadow { had: *had }
                }
                mtg_core::card::BlockRule::BlocksLandwalk => crate::ast::BlockRule::BlocksLandwalk,
                mtg_core::card::BlockRule::BlocksAdditional(n) => {
                    crate::ast::BlockRule::BlocksAdditional(*n)
                }
            }),
            AbilityDef::Enchant(t) => Ability::Enchant(self.target(*t)),
            AbilityDef::Spell { targets, effects } => Ability::Spell {
                targets: targets.iter().map(|t| self.target(*t)).collect(),
                effects: self.sentences(effects),
            },
            AbilityDef::Modal(modes) => {
                // "Choose two": only pairs; the first ones, (0, 1), (0, 2),
                // ..., give every single mode.
                if let Some(singles) = self.modal_two(modes) {
                    return Ability::ModalTwo(singles);
                }
                let both = modes.iter().any(|m| m.split.is_some());
                let modes: Vec<Mode> = modes
                    .iter()
                    .filter(|m| m.split.is_none())
                    .map(|m| self.mode(m))
                    .collect();
                if both {
                    Ability::ModalBoth(modes)
                } else {
                    Ability::Modal(modes)
                }
            }
            AbilityDef::Activated(act) => {
                // "Equip Knight {1}": a filter of creatures of one type you
                // control.
                let equip_target = match act.targets.as_slice() {
                    [TargetSpec::CreatureYouControl] => true,
                    [TargetSpec::Filtered(f)] => {
                        let f = self.pool.filter(*f);
                        f.who == Some(mtg_core::card::Who::You)
                            && f.types == TypeSet::of([mtg_core::card::CardType::Creature])
                            && f.subtypes.len() == 1
                    }
                    _ => false,
                };
                let equip = act.sorcery_speed
                    && equip_target
                    && act.effects
                        == [CoreEffect::Attach {
                            what: CoreRecipient::Source,
                            to: CoreRecipient::Target(0),
                        }]
                    && !act.cost.tap
                    && act.cost.sacrifice.is_none();
                let mut cost = self.cost(&act.cost);
                cost.sorcery = act.sorcery_speed;
                cost.per_turn = act.per_turn;
                cost.conditions = act.conditions.iter().map(|c| self.static_cond(c)).collect();
                cost.restrict = (0..cost.conditions.len()).map(Restrict::Cond).collect();
                if cost.sorcery && !cost.restrict.is_empty() {
                    cost.restrict.push(Restrict::Sorcery);
                }
                if cost.per_turn == 1 && !cost.restrict.is_empty() {
                    cost.restrict.push(Restrict::Once);
                }
                if act.once_ever {
                    cost.once_ever = true;
                    cost.restrict.push(Restrict::OnceEver);
                }
                cost.keyword = act.keyword;
                cost.activators = act.activators;
                Ability::Activated {
                    cost,
                    targets: act.targets.iter().map(|t| self.target(*t)).collect(),
                    effects: self.sentences(&act.effects),
                    equip,
                    cycling: act.cycling,
                }
            }
            // Keyword lines that compile to a trigger of their own.
            AbilityDef::Triggered(t)
                if matches!(t.effects.as_slice(), [CoreEffect::Fabricate { .. }]) =>
            {
                let [CoreEffect::Fabricate { count, .. }] = t.effects.as_slice() else {
                    unreachable!("matched above")
                };
                Ability::Mechanic(crate::ast::Mechanic::Fabricate(*count))
            }
            AbilityDef::Triggered(t)
                if matches!(t.effects.as_slice(), [CoreEffect::Hideaway { .. }]) =>
            {
                let [CoreEffect::Hideaway { count }] = t.effects.as_slice() else {
                    unreachable!("matched above")
                };
                Ability::Mechanic(crate::ast::Mechanic::Hideaway(*count))
            }
            AbilityDef::Triggered(t)
                if matches!(
                    t.effects.as_slice(),
                    [CoreEffect::ExileTop {
                        who: CoreRecipient::TriggeringPlayer,
                        count: mtg_core::card::Num::N(1),
                        face: mtg_core::card::ExileFace::Up,
                    }]
                ) && matches!(
                    t.trigger,
                    TriggerSpec::DealsCombatDamageToPlayer(mtg_core::card::TriggerObj::This)
                ) =>
            {
                Ability::Mechanic(crate::ast::Mechanic::Ingest)
            }
            AbilityDef::Triggered(t)
                if t.condition == Some(mtg_core::card::Condition::NotRenowned) =>
            {
                let n = match t.effects.first() {
                    Some(CoreEffect::AddCounters {
                        count: mtg_core::card::Num::N(n),
                        ..
                    }) => *n,
                    _ => 1,
                };
                Ability::Mechanic(crate::ast::Mechanic::Renown(n))
            }
            AbilityDef::Triggered(t) if t.condition == Some(mtg_core::card::Condition::Evolves) => {
                Ability::Mechanic(crate::ast::Mechanic::Evolve)
            }
            AbilityDef::Triggered(t)
                if t.condition == Some(mtg_core::card::Condition::Increments) =>
            {
                Ability::Mechanic(crate::ast::Mechanic::Increment)
            }
            AbilityDef::Replacement(ReplacementAbility {
                replacement: mtg_core::card::Replacement::EnterWithCountersMay { haste, .. },
                ..
            }) => Ability::Mechanic(if *haste {
                crate::ast::Mechanic::Riot
            } else {
                crate::ast::Mechanic::Unleash
            }),
            AbilityDef::Triggered(t) => Ability::Triggered {
                // "for the first time each turn" carries its own limit.
                limit: if t.trigger == TriggerSpec::TargetedFirstTime {
                    mtg_core::card::TurnLimit::None
                } else {
                    t.limit
                },
                trigger: self.trigger(&t.trigger),
                // Meld's condition is the effect's (`Effect::Meld`).
                condition: t
                    .condition
                    .filter(|c| *c != mtg_core::card::Condition::MeldPair)
                    .map(|c| match c {
                        mtg_core::card::Condition::Static(i) => {
                            crate::ast::TrigCond::Static(self.static_cond(self.pool.condition(i)))
                        }
                        mtg_core::card::Condition::While(i)
                        | mtg_core::card::Condition::WhileSecond(i) => {
                            crate::ast::TrigCond::While(self.static_cond(self.pool.condition(i)))
                        }
                        other => crate::ast::TrigCond::Core(other),
                    }),
                targets: t.targets.iter().map(|x| self.target(*x)).collect(),
                effects: self.sentences(&t.effects),
                from_graveyard: t.from_graveyard,
            },
            AbilityDef::Replacement(ReplacementAbility {
                replacement: mtg_core::card::Replacement::EnterWithCounterChoice { kinds, cond },
                ..
            }) => Ability::Mechanic(crate::ast::Mechanic::CounterChoice(*kinds, cond.is_some())),
            AbilityDef::Replacement(ReplacementAbility {
                replacement: mtg_core::card::Replacement::EnterTappedUnless(i),
                ..
            }) => match self.pool.condition(*i) {
                StaticCondition::Not(c) => Ability::EntersTappedIf(self.static_cond(c)),
                c => Ability::EntersTappedUnless(self.static_cond(c)),
            },
            AbilityDef::Replacement(ReplacementAbility {
                replacement: mtg_core::card::Replacement::EnterTappedUnlessReveal { filter, or },
                ..
            }) => Ability::EntersTappedUnlessReveal {
                filter: self.filter(self.pool.filter(*filter)),
                or: or.map(|c| self.static_cond(self.pool.condition(c))),
            },
            AbilityDef::Replacement(ReplacementAbility {
                replacement: mtg_core::card::Replacement::EnterWithCountersIf { n, cond, kind },
                ..
            }) => match self.pool.condition(*cond) {
                StaticCondition::Escaped => {
                    Ability::Mechanic(crate::ast::Mechanic::EscapesWith(*n))
                }
                // The keywords come back as a separate conditional static.
                StaticCondition::Kicked if *kind == mtg_core::card::CounterKind::PlusOnePlusOne => {
                    Ability::Mechanic(crate::ast::Mechanic::KickedWith(*n, None, Vec::new()))
                }
                c if *c == StaticCondition::happened(TurnEvent::Damaged, TurnWho::Opponent)
                    && *kind == mtg_core::card::CounterKind::PlusOnePlusOne =>
                {
                    Ability::Mechanic(crate::ast::Mechanic::Bloodthirst(*n))
                }
                c => Ability::EntersWithIf {
                    n: *n,
                    kind: *kind,
                    cond: self.static_cond(c),
                    first: false,
                    more: None,
                },
            },
            AbilityDef::Replacement(r) => Ability::Replacement(self.replacement(r)),
            AbilityDef::Static(s) => Ability::Static {
                filter: self.filter(&s.affected),
                mods: s.parts.iter().map(|m| self.modification(m)).collect(),
                condition: s.condition.as_ref().map(|c| self.static_cond(c)),
                cond_first: false,
                cond_if: false,
                otherwise: Vec::new(),
            },
            AbilityDef::EntersAsCopy(f, way) => Ability::EntersAsCopy(self.filter(f), *way),
            AbilityDef::AsEntersChoose(k) => Ability::AsEntersChoose {
                kind: self.choice_kind(*k),
                joined: false,
            },
        }
    }

    fn choice_kind(&self, k: CoreChoiceKind) -> ChoiceKind {
        match k {
            CoreChoiceKind::Color { except } => ChoiceKind::Color { except },
            CoreChoiceKind::Colors => ChoiceKind::Colors,
            CoreChoiceKind::CreatureType { except } => ChoiceKind::CreatureType {
                except: except.map(|s| self.pool.subtype_name(s).to_string()),
            },
            CoreChoiceKind::LandType(set) => ChoiceKind::LandType(colors(set)),
            CoreChoiceKind::Named(n) => {
                ChoiceKind::Named((1..=n).map(|i| format!("Option {i}")).collect())
            }
            CoreChoiceKind::CardName { nonland, look } => ChoiceKind::CardName { nonland, look },
            CoreChoiceKind::CardNameOf(k) => ChoiceKind::CardNameOf(k),
            CoreChoiceKind::Parity => ChoiceKind::Parity,
            CoreChoiceKind::CreatureTypeIn { types, len } => ChoiceKind::CreatureTypeIn(
                types[..usize::from(len)]
                    .iter()
                    .map(|s| self.pool.subtype_name(*s).to_string())
                    .collect(),
            ),
            CoreChoiceKind::Number { min, max } => ChoiceKind::Number { min, max },
        }
    }
}

/// The engine's `CastRider` with the default wording; `None` for none.
fn ast_rider(r: mtg_core::card::CastRider, more_opponents: bool) -> Option<crate::ast::Rider> {
    (!r.is_none()).then_some(crate::ast::Rider {
        spend: r.spend,
        wording: crate::ast::SpendWording::CanBeSpent,
        more: r.more,
        more_opponents,
        exile: r.exile,
    })
}
