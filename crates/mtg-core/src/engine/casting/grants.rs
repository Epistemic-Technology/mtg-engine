//! Abilities a permanent gives cards in a hand or graveyard (CR 113.6b;
//! `AbilityDef::CardGrant`): a way to cast them ("Each instant and sorcery
//! card in your graveyard has flashback.", "... in your hand have warp
//! {2}{R}.") or an activated ability ("Each Sliver card in each player's
//! hand has slivercycling {3}."), offered beside the cards' own.

use super::{CostBasis, Sources, Spend};
use crate::card::{
    AbilityDef, AbilityId, ActivatedAbility, ActivatedKeyword, CardGrant, GrantedWay, SacCost,
};
use crate::decision::Choice;
use crate::ids::{ObjId, PlayerId};
use crate::mana::ManaCost;
use crate::state::{AltCast, GameState, Zone};

/// A granted way's cost (`GrantedWay`): the card's mana cost, or one
/// written.
#[derive(Clone, Copy)]
pub(in crate::engine) enum GrantCost<'a> {
    Mana,
    Fixed(&'a ManaCost),
}

impl<'a> GrantCost<'a> {
    fn of(c: Option<&'a ManaCost>) -> Self {
        c.map_or(GrantCost::Mana, GrantCost::Fixed)
    }

    /// The cost written, if any.
    fn fixed(self) -> Option<&'a ManaCost> {
        match self {
            GrantCost::Mana => None,
            GrantCost::Fixed(c) => Some(c),
        }
    }
}

impl GameState {
    /// Whether a card grant could be in force: some card in the pool has
    /// one and a player-level static source is on the battlefield. Free
    /// in a game without such cards.
    #[inline]
    pub(in crate::engine) fn card_grants_live(&self) -> bool {
        self.pool.card_grants && self.player_static_sources > 0
    }

    /// The card grants in force: each with its source and the source's
    /// controller, its condition holding (CR 611.3a).
    fn card_grants_in_force(&self) -> impl Iterator<Item = (ObjId, PlayerId, &CardGrant)> + '_ {
        self.battlefield
            .iter()
            .filter(|id| self.flags_of(&self.objects[**id]).has_player_static())
            .flat_map(move |id| {
                let o = &self.objects[*id];
                self.pool
                    .get(o.def)
                    .ability_ids
                    .iter()
                    .filter_map(move |a| match self.pool.ability(*a) {
                        AbilityDef::CardGrant(g)
                            if g.condition.is_none_or(|c| {
                                self.condition_now(self.pool.condition(c), o.controller, *id)
                            }) =>
                        {
                            Some((*id, o.controller, &**g))
                        }
                        _ => None,
                    })
            })
    }

    /// What the grants in force give `card`: those for the zone it is in,
    /// or, for a spell, the zone it was cast from (its cost is judged as
    /// it is cast; CR 601.2f), matching its characteristics and owner.
    fn grants_of(&self, card: ObjId) -> impl Iterator<Item = &GrantedWay> + '_ {
        let o = self.objects.get(card);
        let zone = o.and_then(|o| {
            if o.zone == Zone::Stack {
                o.cast.cast_from()
            } else {
                Some(o.zone)
            }
        });
        let chars = o.map(|o| self.base_characteristics(o));
        self.card_grants_in_force()
            .filter(move |(src, who, g)| {
                let (Some(o), Some(zone), Some(c)) = (o, zone, chars.as_ref()) else {
                    return false;
                };
                g.zone == zone
                    && (g.players || o.owner == *who)
                    && g.cards.matches(card, o.owner, c, Some(*src), Some(*who))
            })
            .map(|(_, _, g)| &g.grant)
    }

    /// The flashback cost a grant gives `card`; with several, the one of
    /// least mana value (the player may choose any, CR 702.34a).
    pub(in crate::engine) fn granted_flashback(&self, card: ObjId) -> Option<GrantCost<'_>> {
        self.granted_cost(card, |g| match g {
            GrantedWay::Flashback(c) => Some(GrantCost::of(c.as_ref())),
            _ => None,
        })
    }

    /// The mayhem cost a grant gives `card`, as `granted_flashback`.
    pub(in crate::engine) fn granted_mayhem(&self, card: ObjId) -> Option<GrantCost<'_>> {
        self.granted_cost(card, |g| match g {
            GrantedWay::Mayhem(c) => Some(GrantCost::of(c.as_ref())),
            _ => None,
        })
    }

    /// The warp cost a grant gives `card` (CR 702.185a).
    fn granted_warp(&self, card: ObjId) -> Option<&ManaCost> {
        self.granted_cost(card, |g| match g {
            GrantedWay::Warp(c) => Some(GrantCost::Fixed(c)),
            _ => None,
        })
        .and_then(GrantCost::fixed)
    }

    fn granted_cost<'a>(
        &'a self,
        card: ObjId,
        pick: impl Fn(&'a GrantedWay) -> Option<GrantCost<'a>>,
    ) -> Option<GrantCost<'a>> {
        let own = self
            .objects
            .get(card)
            .map_or(0, |o| self.pool.get(o.def).mana_cost.mana_value());
        self.grants_of(card)
            .filter_map(pick)
            .min_by_key(|c| c.fixed().map_or(own, ManaCost::mana_value))
    }

    /// The cost a granted way pays instead of the mana cost (`cost_of`'s
    /// base): a granted flashback, mayhem, or warp cost written as mana.
    /// `None`: the way is the card's own, or its cost is the mana cost.
    #[cold]
    #[inline(never)]
    pub(in crate::engine) fn granted_way_cost(&self, b: &CostBasis) -> Option<&ManaCost> {
        if !self.card_grants_live() {
            return None;
        }
        let def = b.def;
        if b.flashback && def.flashback.is_none() && b.alt == AltCast::None {
            return self.granted_flashback(b.id).and_then(GrantCost::fixed);
        }
        match b.alt {
            AltCast::Mayhem if def.mayhem.is_none() => {
                self.granted_mayhem(b.id).and_then(GrantCost::fixed)
            }
            AltCast::Warp if def.warp.is_none() => self.granted_warp(b.id),
            _ => None,
        }
    }

    /// The activated abilities grants give `card` where it is.
    fn granted_abilities(&self, card: ObjId) -> impl Iterator<Item = AbilityId> + '_ {
        self.grants_of(card).filter_map(|g| match g {
            GrantedWay::Ability(a) => Some(*a),
            _ => None,
        })
    }

    /// The priority options grants give `p`'s cards (`priority_options`):
    /// casting a graveyard card with flashback or mayhem, a hand card for
    /// its warp cost, and the abilities activated from those zones.
    #[cold]
    #[inline(never)]
    pub(in crate::engine) fn card_grant_options(
        &self,
        p: PlayerId,
        sorcery_timing: bool,
        sources: &Sources,
        changers: &[ObjId],
        options: &mut Vec<Choice>,
    ) {
        let ps = self.player(p);
        for id in &ps.graveyard {
            let id = *id;
            let def = self.pool.get(self.objects[id].def);
            for a in self.granted_abilities(id) {
                if let AbilityDef::Activated(ab) = self.pool.ability(a)
                    && ab.from_graveyard
                {
                    self.graveyard_ability_options(p, id, a, ab, sorcery_timing, sources, options);
                }
            }
            if def.is_land() || options.contains(&Choice::Cast(id)) {
                continue;
            }
            // CR 702.34a: cast with flashback, for the granted cost.
            if def.flashback.is_none()
                && let Some(cost) = self.granted_flashback(id)
                && (cost.fixed().is_some() || !def.no_mana_cost)
                && self.granted_castable(
                    p,
                    &CostBasis {
                        flashback: true,
                        ..self.offer_basis(p, id, def, AltCast::None)
                    },
                    sorcery_timing,
                    sources,
                    changers,
                )
            {
                options.push(Choice::Cast(id));
                continue;
            }
            // Mayhem: a card its owner discarded this turn.
            if def.mayhem.is_none()
                && self.mayhem_discarded.contains(&id)
                && let Some(cost) = self.granted_mayhem(id)
                && (cost.fixed().is_some() || !def.no_mana_cost)
                && self.granted_castable(
                    p,
                    &self.offer_basis(p, id, def, AltCast::Mayhem),
                    sorcery_timing,
                    sources,
                    changers,
                )
            {
                options.push(Choice::Cast(id));
            }
        }
        for id in &ps.hand {
            let id = *id;
            let def = self.pool.get(self.objects[id].def);
            for a in self.granted_abilities(id) {
                if let AbilityDef::Activated(ab) = self.pool.ability(a)
                    && ab.from_hand
                {
                    self.hand_ability_options(p, id, a, ab, sorcery_timing, sources, options);
                }
            }
            // CR 702.185a: cast for the warp cost instead, with a
            // permanent spell's timing.
            if def.warp.is_none()
                && !def.is_land()
                && self.granted_warp(id).is_some()
                && !options.contains(&Choice::CastWarp(id))
                && (sorcery_timing || self.has_flash(p, def, id))
                && self.alt_payable(p, id, def, AltCast::Warp, sources, changers)
                && self.targets_available(p, def.unkicked_cast_targets(), Some(id))
            {
                options.push(Choice::CastWarp(id));
            }
        }
    }

    /// Whether `b`'s spell may be cast now: its timing, its cost
    /// (`basis_payable`), and its targets.
    fn granted_castable(
        &self,
        p: PlayerId,
        b: &CostBasis,
        sorcery_timing: bool,
        sources: &Sources,
        changers: &[ObjId],
    ) -> bool {
        let (def, id) = (b.def, b.id);
        (sorcery_timing || def.is_instant() || self.has_flash(p, def, id))
            && self.basis_payable(b, sources, changers)
            && ((!def.spree.is_empty() || !def.tiered.is_empty()) && def.modes().is_some()
                || self.way_targets_available(p, def, id))
    }

    /// The options to activate `ab` (ability `a`) of the card `id` in
    /// `p`'s hand (cycling, CR 702.29): mana, or life ("Cycling—Pay 2
    /// life.", CR 119.4); instant speed unless "Activate only as a
    /// sorcery" (transmute, CR 702.53a).
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub(in crate::engine) fn hand_ability_options(
        &self,
        p: PlayerId,
        id: ObjId,
        a: AbilityId,
        ab: &ActivatedAbility,
        sorcery_timing: bool,
        sources: &Sources,
        options: &mut Vec<Choice>,
    ) {
        if !(ab.from_hand
            && (!ab.sorcery_speed || sorcery_timing)
            && self.activation_allowed(ab, a, p, id)
            // "only once each turn" (forecast, CR 702.57b).
            && (ab.per_turn == 0
                || self
                    .activated_once
                    .iter()
                    .filter(|x| **x == (id, a))
                    .count()
                    < usize::from(ab.per_turn))
            && (ab.cost.life == 0
                || self.player(p).life >= i32::try_from(ab.cost.life).unwrap_or(i32::MAX))
            && self.payable_for(
                p,
                sources,
                &ab.cost.mana.requirements(0),
                Spend::Ability(
                    self.characteristics(id).as_ref(),
                    crate::card::SpendWays::of_ability(ab),
                ),
            )
            && self.targets_available(p, &ab.targets, Some(id)))
        {
            return;
        }
        // CR 702.49a: one option per unblocked attacker that could be
        // returned.
        if ab.keyword == Some(ActivatedKeyword::Ninjutsu) {
            for c in self.unblocked_attackers(p) {
                options.push(Choice::Activate {
                    source: id,
                    ability: a,
                    sacrifice: Some(c),
                    discard: None,
                });
            }
            return;
        }
        // "Cycling—Sacrifice a land." (Edge of Autumn): one option per
        // permanent that could be sacrificed, none without one.
        if let Some(SacCost::Filter(f)) = &ab.cost.sacrifice {
            let c = self.sacrifice_candidates(p, f, id);
            if c.len() < ab.cost.chosen_count() {
                return;
            }
            for s in c {
                options.push(Choice::Activate {
                    source: id,
                    ability: a,
                    sacrifice: Some(s),
                    discard: None,
                });
            }
            return;
        }
        options.push(Choice::Activate {
            source: id,
            ability: a,
            sacrifice: None,
            discard: None,
        });
    }

    /// The options to activate `ab` (ability `a`) of the card `id` in
    /// `p`'s graveyard (CR 702.84a and friends: unearth).
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub(in crate::engine) fn graveyard_ability_options(
        &self,
        p: PlayerId,
        id: ObjId,
        a: AbilityId,
        ab: &ActivatedAbility,
        sorcery_timing: bool,
        sources: &Sources,
        options: &mut Vec<Choice>,
    ) {
        if !(ab.from_graveyard
            && (!ab.sorcery_speed || sorcery_timing)
            && self.activation_allowed(ab, a, p, id)
            && self.payable_for(
                p,
                sources,
                &ab.cost.mana.requirements(0),
                Spend::Ability(
                    self.characteristics(id).as_ref(),
                    crate::card::SpendWays::of_ability(ab),
                ),
            )
            && self.targets_available(p, &ab.targets, Some(id)))
        {
            return;
        }
        // "Exile an instant or sorcery card from your graveyard" beside
        // it: other cards, enough.
        if let Some(g) = &ab.cost.exile_cards
            && !g.x
            && self.exile_candidates(p, g.filter.as_ref(), id).len() < g.n as usize
        {
            return;
        }
        // "{2}{B}, Tap an untapped creature you control: Return this card
        // from your graveyard ...".
        if ab.cost.taps_chosen() {
            for c in self.tap_cost_options(p, ab, id, sources) {
                options.push(Choice::Activate {
                    source: id,
                    ability: a,
                    sacrifice: c,
                    discard: None,
                });
            }
            return;
        }
        // "{2}, Sacrifice an artifact or creature: Return this card from
        // your graveyard ...": one option per permanent that could be
        // sacrificed, none without enough of them.
        if let Some(SacCost::Filter(f)) = &ab.cost.sacrifice {
            let c = self.sacrifice_candidates(p, f, id);
            if c.len() < ab.cost.chosen_count() {
                return;
            }
            for s in c {
                options.push(Choice::Activate {
                    source: id,
                    ability: a,
                    sacrifice: Some(s),
                    discard: None,
                });
            }
            return;
        }
        // "{2}{U}, Discard two cards: Return this card from your graveyard
        // ...": one option per card that could be discarded, none without
        // enough of them.
        if ab.cost.discard > 0 {
            let mut c = self.discard_candidates(p, ab.cost.discard_filter.as_ref(), id);
            if c.len() < ab.cost.discard as usize {
                return;
            }
            if ab.cost.discard_random {
                c.truncate(1);
            }
            for d in c {
                options.push(Choice::Activate {
                    source: id,
                    ability: a,
                    sacrifice: None,
                    discard: Some(d),
                });
            }
            return;
        }
        options.push(Choice::Activate {
            source: id,
            ability: a,
            sacrifice: None,
            discard: None,
        });
    }
}
