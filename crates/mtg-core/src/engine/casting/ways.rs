//! What a spell costs (CR 601.2f), computed once for both the offer and
//! the payment: the offer describes the card it would cast where it is,
//! the payment the spell on the stack, and both read `cost_of`.

use super::{Sources, Spend, adjust_generic};
use crate::card::{AbilityDef, CardDef, Cost, Keyword, SacCost};
use crate::ids::{ObjId, PlayerId};
use crate::mana::ManaRequirement;
use crate::state::{AltCast, CastInfo, GameState};

/// What a spell's total cost is computed from (CR 601.2f): the way it is
/// cast and what has been chosen so far. Built from the card for an offer
/// (`offer_basis`: nothing chosen yet) and from the spell on the stack
/// for payment (`spell_basis`).
#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)] // one per fact payment reads
pub(in crate::engine) struct CostBasis<'a> {
    /// The caster.
    pub p: PlayerId,
    /// The card to be cast, or the spell being cast.
    pub id: ObjId,
    /// The spell's characteristics: the card's, or the face it is cast as.
    pub def: &'a CardDef,
    /// The alternative used (`AltCast`).
    pub alt: AltCast,
    /// Cast for its evoke cost (CR 702.74a).
    pub evoked: bool,
    /// Cast with flashback, or otherwise exiled as it leaves the stack.
    pub flashback: bool,
    /// Cast face down (CR 702.37c).
    pub face_down: bool,
    /// X (CR 107.3), 0 until chosen.
    pub x: u32,
    /// Paying the second of two additional costs (CR 601.2b).
    pub other: bool,
    /// The mode chosen; `None` before (no mode's cost is added).
    pub mode: Option<u8>,
    /// The optional costs chosen so far (kicker, buyback, offspring,
    /// replicate); none before the cast begins.
    pub cast: CastInfo,
    /// The spell is on the stack: its splices, targets, and entwine
    /// count; an offer judges a target cost by the targets that could be
    /// chosen instead.
    pub stack: bool,
    /// Generic mana the creature tapped for harmonize takes off, its
    /// power (CR 702.180a).
    pub less: u32,
    /// What the permission it is cast by adds (`CastRider`: "mana of any
    /// type can be spent to cast that spell").
    pub rider: crate::card::CastRider,
}

impl GameState {
    /// The basis of the spell `spell` as it is being cast, with X, the
    /// additional cost, and the mode fixed.
    pub(in crate::engine) fn spell_basis(
        &self,
        spell: ObjId,
        x: u32,
        other: bool,
        mode: u8,
    ) -> CostBasis<'_> {
        let o = &self.objects[spell];
        CostBasis {
            p: o.controller,
            id: spell,
            def: self.pool.get(o.def),
            alt: o.alt,
            evoked: o.cast.evoked(),
            flashback: o.cast.flashback(),
            face_down: self.pool.is_face_down(o.def),
            x,
            other,
            mode: Some(mode),
            cast: o.cast,
            stack: true,
            less: if o.alt == AltCast::Harmonize {
                self.harmonized
                    .iter()
                    .find(|(s, ..)| *s == spell)
                    .map_or(0, |h| h.2)
            } else {
                0
            },
            rider: match &self.exile_casts {
                Some(e) if e.riders => e
                    .rider_spells
                    .iter()
                    .find(|(s, _)| *s == spell)
                    .map_or_else(crate::card::CastRider::default, |r| r.1),
                _ => crate::card::CastRider::default(),
            },
        }
    }

    /// The rider of a permission `p` has to play the card `id` (CR
    /// 601.3), the most generous of several; none without one.
    #[cold]
    #[inline(never)]
    pub(in crate::engine) fn permission_rider(
        &self,
        p: PlayerId,
        id: ObjId,
    ) -> crate::card::CastRider {
        let Some(e) = &self.exile_casts else {
            return crate::card::CastRider::default();
        };
        let mut rider = e
            .playable
            .iter()
            .filter(|pe| pe.card == id && pe.player == p)
            .map(|pe| pe.rider)
            .chain(
                e.effect_riders
                    .iter()
                    .filter(|r| r.0 == p && r.1 == id)
                    .map(|r| r.2),
            )
            .reduce(|a, b| crate::card::CastRider {
                spend: a.spend.or(b.spend),
                more: a.more.min(b.more),
                exile: a.exile && b.exile,
                life: a.life && b.life,
            })
            .unwrap_or_default();
        // "Mana of any type can be spent to cast those spells." (a
        // permanent's permission to cast the cards exiled with it).
        let more = self.linked_permission(p, id);
        rider.spend = rider.spend.or(more.spend);
        rider
    }

    /// The riders of the permissions permanents `p` controls give to cast
    /// `id`, a card exiled with them (`AbilityDef::PlayExiled`, CR 607.2a),
    /// combined.
    #[cold]
    #[inline(never)]
    pub(in crate::engine) fn linked_permission(
        &self,
        p: PlayerId,
        id: ObjId,
    ) -> crate::card::PlayExiledMore {
        let mut out = crate::card::PlayExiledMore::default();
        for l in self.linked_exiles.iter().filter(|l| l.card == id) {
            let Some(s) = self.objects.get(l.source) else {
                continue;
            };
            if s.zone != crate::state::Zone::Battlefield || s.controller != p {
                continue;
            }
            for a in &self.pool.get(s.def).ability_ids {
                if let AbilityDef::PlayExiled { more, .. } = self.pool.ability(*a) {
                    out.spend = out.spend.or(more.spend);
                    out.finality |= more.finality;
                    out.flash |= more.flash;
                }
            }
        }
        out
    }

    /// CR 609.4b: how a static ability lets `b`'s caster spend mana on
    /// it ("You may spend mana as though it were mana of any color to cast
    /// planeswalker spells"; a `SpellGrant` of `changers`).
    #[cold]
    #[inline(never)]
    fn granted_spend(&self, b: &CostBasis, changers: &[ObjId]) -> crate::card::SpendAs {
        let mut spend = crate::card::SpendAs::Normal;
        for src in changers {
            for a in &self.pool.get(self.objects[*src].def).ability_ids {
                if let AbilityDef::CostChange(cc) = self.pool.ability(*a)
                    && let Some(g) = &cc.grant
                    && g.spend != crate::card::SpendAs::Normal
                    && self.cost_change_applies(cc, b.p, b.def, b.id, *src)
                {
                    spend = spend.or(g.spend);
                }
            }
        }
        spend
    }

    /// The basis of `p` casting the card `id` as `def` (the card's own
    /// definition or one of its faces) `alt`'s way, nothing chosen yet.
    #[allow(clippy::inline_always)] // measured: the priority listing's hot path
    #[inline(always)]
    pub(in crate::engine) fn offer_basis<'a>(
        &'a self,
        p: PlayerId,
        id: ObjId,
        def: &'a CardDef,
        alt: AltCast,
    ) -> CostBasis<'a> {
        CostBasis {
            p,
            id,
            def,
            alt,
            evoked: false,
            flashback: false,
            // The face-down 2/2's own definition as a card (random decks
            // hold it) costs {3} too, as payment charges it.
            face_down: def.mana_cost.is_empty()
                && def
                    .printed
                    .as_ref()
                    .is_some_and(|c| self.pool.is_face_down(c.def)),
            x: 0,
            other: false,
            mode: None,
            cast: CastInfo::default(),
            stack: false,
            less: 0,
            rider: match &self.exile_casts {
                Some(e) if e.riders => self.permission_rider(p, id),
                _ => crate::card::CastRider::default(),
            },
        }
    }

    /// CR 601.2f: the total mana cost of casting `b`'s way: the mana cost
    /// or the alternative cost that replaces it, the additional costs'
    /// mana, the optional costs and modes chosen, then cost increases and
    /// reductions (`changers`: `cost_changers`), a target's cost change,
    /// and harmonize's tapped creature. Offer and payment both use it.
    #[allow(clippy::inline_always)] // measured: the priority listing's hot path
    #[inline(always)]
    pub(in crate::engine) fn cost_of(&self, b: &CostBasis, changers: &[ObjId]) -> ManaRequirement {
        let def = b.def;
        let mut req = self.base_cost(b);
        // "A spell cast this way costs {2} more to cast."
        if b.rider.more > 0 {
            req.generic += u32::from(b.rider.more);
        }
        // What a plain cast from the hand doesn't have, out of line.
        if b.x > 0 || b.stack || b.mode.is_some() || b.other || !def.additional.mana.is_empty() {
            self.add_chosen_costs(b, &mut req);
        }
        if def.self_cost.is_some()
            || def.cond_cost.is_some()
            || !changers.is_empty()
            || self.next_spell_less[b.p.index().min(3)] > 0
            || !self.turn_spell_changes.is_empty()
        {
            adjust_generic(&mut req, self.cost_delta(b.p, def, b.id, changers));
        }
        if let Some(tc) = &def.target_cost {
            if b.stack {
                if self.objects[b.id]
                    .targets
                    .iter()
                    .any(|t| self.target_fits(&tc.1, *t, b.p, b.id))
                {
                    adjust_generic(&mut req, tc.0);
                    reduce_colors(&mut req, def.target_cost_colors);
                }
            } else if self.offer_target_cost(b, tc) {
                adjust_generic(&mut req, tc.0);
                reduce_colors(&mut req, def.target_cost_colors);
            }
        }
        // CR 702.180a: reduced by the tapped creature's power.
        if b.less > 0 {
            adjust_generic(&mut req, -i32::try_from(b.less).unwrap_or(i32::MAX));
        }
        // CR 609.4b: the total cost is paid with mana spent as though it
        // were of any color or type (after the cost is determined).
        let spend = if self.pool.spend_grants && !changers.is_empty() {
            b.rider.spend.or(self.granted_spend(b, changers))
        } else {
            b.rider.spend
        };
        spend.apply(&mut req);
        req
    }

    /// `cost_of`'s parts before the cost changes: X's color, the optional
    /// costs chosen, the mode's, and the additional costs' mana.
    #[inline(never)]
    fn add_chosen_costs(&self, b: &CostBasis, req: &mut ManaRequirement) {
        let (def, x) = (b.def, b.x);
        // CR 107.3: "Spend only black mana on X." X is then paid as that
        // many black symbols.
        if x > 0
            && req.generic >= x
            && let Some(c) = def
                .ability_ids
                .iter()
                .find_map(|a| match self.pool.ability(*a) {
                    AbilityDef::XManaColor(c) => Some(*c),
                    _ => None,
                })
        {
            req.generic -= x;
            req.colored[c as usize] += x;
        }
        // Nothing is chosen, spliced, targeted, or replicated before the
        // cast.
        if b.stack {
            self.add_optional_costs(b, req);
        }
        if let Some(mode) = b.mode {
            // CR 702.42a: every mode, for the entwine cost on top.
            if let Some(e) = &def.entwine
                && b.stack
                && self
                    .modes_of(b.id)
                    .and_then(|m| m.get(usize::from(mode)))
                    .is_some_and(|m| m.split.is_some())
            {
                req.add(&e.requirements(0));
            }
            // CR 702.120a: escalate, once for each mode beyond the first.
            if let Some(e) = &def.escalate {
                for _ in 1..(u32::from(mode) + 1).count_ones() {
                    req.add(&e.requirements(0));
                }
            }
            // CR 702.183a: the chosen tiered mode's cost.
            if let Some(c) = def.tiered.get(usize::from(mode)) {
                req.add(&c.requirements(0));
            }
            // CR 702.172a: the chosen spree modes' costs.
            if !def.spree.is_empty() {
                let bits = u32::from(mode) + 1;
                for (i, c) in def.spree.iter().enumerate() {
                    if bits & (1 << i) != 0 {
                        req.add(&c.requirements(0));
                    }
                }
            }
        }
        // "As an additional cost to cast this spell, sacrifice a creature
        // or pay {3}".
        if let Some(c) = def.additional_or.as_deref().filter(|_| b.other)
            && !c.mana.is_empty()
        {
            req.add(&c.mana.requirements(0));
        } else if !b.other && !def.additional.mana.is_empty() {
            req.add(&def.additional.mana.requirements(0));
        }
    }

    /// The optional costs the spell being cast has chosen (kicker,
    /// multikicker, buyback, offspring, CR 702.33, 702.27, 702.175), the
    /// splice costs, strive's, and replicate's (`add_splice_and_strive`).
    fn add_optional_costs(&self, b: &CostBasis, req: &mut ManaRequirement) {
        let def = b.def;
        if b.cast.kicked()
            && let Some(k) = &def.kicker
        {
            req.add(&k.requirements(0));
        }
        // CR 702.33c: the multikicker cost, once for each time it was paid.
        if let Some(k) = &def.multikicker {
            for _ in 0..b.cast.kicks() {
                req.add(&k.requirements(0));
            }
        }
        if b.cast.bought_back()
            && let Some(c) = &def.buyback
        {
            req.add(&c.requirements(0));
        }
        if b.cast.offspring()
            && let Some(c) = &def.offspring
        {
            req.add(&c.requirements(0));
        }
        // "As an additional cost to cast this spell, you may waterbend
        // {4}": its mana, when paid (CR 601.2b, 701.67a).
        if b.cast.optional_paid()
            && let Some(c) = def.optional_cost.as_deref()
            && !c.mana.is_empty()
        {
            req.add(&c.mana.requirements(0));
        }
        let targets = self.objects[b.id].targets.len();
        self.add_splice_and_strive(b.id, def, targets, req);
    }

    /// The definition of `b`'s card (`GameObject::card`), not the face
    /// it is cast as.
    fn card_def(&self, b: &CostBasis) -> &CardDef {
        self.pool.get(self.objects[b.id].card)
    }

    /// Whether an offer counts a target cost change (`CardDef::target_cost`)
    /// before targets are chosen: cheaper if a fitting target exists
    /// (targets are then held to what's payable, `target_payable`); dearer
    /// only if every legal target fits (Vanish into Eternity with only
    /// creatures to target costs {3} more). "If it targets" is any of the
    /// spell's targets (Titanic Brawl's creature you control is its first,
    /// Thing Swing's Hero too, but either may be the one).
    fn offer_target_cost(&self, b: &CostBasis, tc: &(i32, crate::card::ObjFilter)) -> bool {
        let fits = |t: &crate::state::Target| self.target_fits(&tc.1, *t, b.p, b.id);
        let specs = b.def.cast_targets();
        if tc.0 < 0 {
            specs
                .iter()
                .any(|s| self.legal_targets(b.p, *s, Some(b.id)).iter().any(fits))
        } else {
            specs.iter().any(|s| {
                !matches!(s, crate::card::TargetSpec::UpTo(_)) && {
                    let targets = self.legal_targets(b.p, *s, Some(b.id));
                    !targets.is_empty() && targets.iter().all(fits)
                }
            })
        }
    }

    /// The cost `b`'s way pays instead of or as the mana cost (CR 118.9),
    /// with X: a flashback cost, nothing for a spell cast without paying
    /// its mana cost, an alternative cost, or {3} face down.
    #[allow(clippy::inline_always)] // measured: the priority listing's hot path
    #[inline(always)]
    fn base_cost(&self, b: &CostBasis) -> ManaRequirement {
        let (def, x, alt) = (b.def, b.x, b.alt);
        let reqs = |c: &crate::mana::ManaCost| c.requirements(x);
        // Most casts: the mana cost (the last arm), at a glance.
        if matches!(alt, AltCast::None | AltCast::Hand) && !b.flashback && !b.evoked && !b.face_down
        {
            return reqs(&def.mana_cost);
        }
        self.alt_base_cost(b)
    }

    /// `base_cost` of a way that isn't the plain mana cost.
    #[inline(never)]
    fn alt_base_cost(&self, b: &CostBasis) -> ManaRequirement {
        let (def, x, alt) = (b.def, b.x, b.alt);
        let card = || self.card_def(b);
        let reqs = |c: &crate::mana::ManaCost| c.requirements(x);
        // CR 113.6b: a cost a permanent gives the card ("Each Lesson card
        // in your graveyard has flashback {1}.").
        if self.pool.card_grants
            && let Some(c) = self.granted_way_cost(b)
        {
            return reqs(c);
        }
        // CR 118.9: an alternative cost a permanent gives the spell; X is
        // 0 (CR 107.3b).
        if alt == AltCast::Granted {
            return self
                .granted_alt(b.p, def, b.id, &self.cost_changers(), b.stack)
                .map_or_else(|| reqs(&def.mana_cost), |g| g.2.mana.requirements(0));
        }
        match (&def.flashback, b.flashback) {
            (Some(fb), true) => reqs(&fb.mana),
            // Cast without paying its mana cost: X is 0 (CR 107.3b).
            _ if matches!(
                alt,
                AltCast::Free
                    | AltCast::Suspend
                    | AltCast::FreeExile
                    | AltCast::PayLife
                    | AltCast::DiscardCost
            ) =>
            {
                ManaRequirement::default()
            }
            (_, false) if alt == AltCast::Madness && def.madness.is_some() => {
                reqs(def.madness.as_ref().expect("checked"))
            }
            (_, false) if alt == AltCast::Miracle && def.miracle.is_some() => {
                reqs(def.miracle.as_ref().expect("checked"))
            }
            (_, false) if alt == AltCast::Foretell && def.foretell.is_some() => {
                reqs(def.foretell.as_ref().expect("checked"))
            }
            // Foretold by a grant (Dream Devourer): "its mana cost reduced
            // by {2}" (generic mana only).
            (_, false) if alt == AltCast::Foretell => {
                let mut r = reqs(&def.mana_cost);
                adjust_generic(&mut r, -2);
                r
            }
            (_, false) if alt == AltCast::Escape && def.escape.is_some() => {
                reqs(&def.escape.as_ref().expect("checked").0)
            }
            (_, false) if alt == AltCast::Mayhem && def.mayhem.is_some() => {
                reqs(def.mayhem.as_ref().expect("checked"))
            }
            (_, false) if alt == AltCast::Bestow && def.bestow.is_some() => {
                reqs(def.bestow.as_ref().expect("checked"))
            }
            (_, false) if alt == AltCast::Dash && def.dash.is_some() => {
                reqs(def.dash.as_ref().expect("checked"))
            }
            (_, false) if alt == AltCast::Warp && def.warp.is_some() => {
                reqs(def.warp.as_ref().expect("checked"))
            }
            // CR 701.65a: {2} rather than its mana cost (X is 0).
            (_, false) if alt == AltCast::Airbend => ManaRequirement {
                generic: 2,
                ..ManaRequirement::default()
            },
            (_, false) if alt == AltCast::Sneak && def.sneak.is_some() => {
                reqs(def.sneak.as_ref().expect("checked"))
            }
            (_, false) if alt == AltCast::Impending && def.impending.is_some() => {
                reqs(&def.impending.as_ref().expect("checked").1)
            }
            (_, false) if alt == AltCast::WebSlinging && def.web_slinging.is_some() => {
                reqs(def.web_slinging.as_ref().expect("checked"))
            }
            // CR 702.180a: cast from the graveyard (as with flashback, the
            // spell is exiled as it leaves the stack).
            _ if alt == AltCast::Harmonize => {
                reqs(def.harmonize.as_deref().unwrap_or(&def.mana_cost))
            }
            (_, false) if alt == AltCast::Overload && def.overload.is_some() => {
                reqs(def.overload.as_ref().expect("checked"))
            }
            // The face has no awaken of its own: the card's cost.
            (_, false) if alt == AltCast::Awaken && card().awaken.is_some() => {
                reqs(&card().awaken.as_ref().expect("checked").1)
            }
            // The back face has no disturb of its own: the card's cost.
            (_, false) if alt == AltCast::Disturb && card().disturb.is_some() => {
                reqs(card().disturb.as_ref().expect("checked"))
            }
            // The card's: a cleaved spell's face has no alternative cost of
            // its own (CR 702.148a), a prototyped one's is its mana cost.
            (_, false) if alt == AltCast::AltCost && card().alt_cost.is_some() => {
                reqs(&card().alt_cost.as_ref().expect("checked").cost.mana)
            }
            (_, false) if b.evoked && def.evoke.is_some() => {
                reqs(def.evoke.as_ref().expect("checked"))
            }
            // CR 702.37c: a face-down spell costs {3}.
            _ if b.face_down => self.face_down_cost(b.p),
            _ => reqs(&def.mana_cost),
        }
    }

    /// `basis_payable` for `p` casting the card `id` as `def` (the card's
    /// own definition or a face) `alt`'s way, nothing chosen yet.
    pub(in crate::engine) fn alt_payable(
        &self,
        p: PlayerId,
        id: ObjId,
        def: &CardDef,
        alt: AltCast,
        sources: &Sources,
        changers: &[ObjId],
    ) -> bool {
        self.basis_payable(&self.offer_basis(p, id, def, alt), sources, changers)
    }

    /// The sources payment would spend for `b`'s spell (`spell_sources`):
    /// restricted mana it may spend first (CR 106.6), then `sources`, then
    /// what convoke, improvise, delve, or an additional waterbend cost
    /// lets pay. `None`: just `sources`.
    #[allow(clippy::inline_always)] // measured: the priority listing's hot path
    #[inline(always)]
    pub(in crate::engine) fn basis_sources(
        &self,
        b: &CostBasis,
        sources: &Sources,
    ) -> Option<Sources> {
        let (p, def) = (b.p, b.def);
        let keywords = self.spell_keywords(p, def, b.id).0;
        let bend = Self::bend_cap(def, b.cast);
        let helped = Keyword::helps_pay(keywords) || bend > 0;
        let restricted = match (self.restricted_mana_possible(), &def.printed) {
            (false, _) | (_, None) => None,
            (true, Some(c)) => self.with_restricted(p, sources, Spend::Spell(c, b.id)),
        };
        if !helped && restricted.is_none() {
            return None;
        }
        let mut all = restricted.unwrap_or_else(|| sources.clone());
        if helped {
            all.extend(self.helper_sources(p, keywords, bend));
        }
        Some(all)
    }

    /// CR 601.2f-h: whether `p` could pay for casting `b`'s way now, from
    /// `sources` (`mana_sources`) as payment would: its total cost
    /// (`cost_of`) with either of two additional costs ("sacrifice a
    /// creature or pay {3}") or one that taps permanents the mana might
    /// need, the rest of its additional costs, an emerge sacrifice (CR
    /// 702.119a), and a spree or tiered spell's mode: one with its targets
    /// whose cost can be paid (CR 702.172a, 702.183a). The one check every
    /// way to cast a spell is offered by.
    #[allow(clippy::inline_always)] // measured: the priority listing's hot path
    #[inline(always)]
    pub(in crate::engine) fn basis_payable(
        &self,
        b: &CostBasis,
        sources: &Sources,
        changers: &[ObjId],
    ) -> bool {
        let (p, def, id) = (b.p, b.def, b.id);
        let all = self.basis_sources(b, sources);
        let srcs = all.as_ref().unwrap_or(sources);
        let ps = self.player(p);
        let req = self.cost_of(b, changers);
        // CR 118.9, 601.2h: an alternative cost that taps permanents
        // ("{3}{U}, Tap an untapped artifact you control", Zahid) leaves
        // enough of the mana untapped, as payment's choice of them
        // (`tap_first_options`) requires.
        if b.alt == AltCast::AltCost
            && let Some(alt) = self.card_def(b).alt_cost.as_deref()
            && alt.cost.taps_chosen()
            && !self.cost_option_payable(p, &alt.cost, id, srcs, req, false)
        {
            return false;
        }
        // CR 702.119a: emerge, reduced by the mana value of the creature
        // sacrificed: some creature must leave the rest payable without
        // its own mana.
        if b.alt == AltCast::AltCost
            && def.emerge
            && let Some(alt) = self.card_def(b).alt_cost.as_deref()
            && let Some(SacCost::Filter(f)) = &alt.cost.sacrifice
            && let Some(printed) = &def.printed
        {
            return !self
                .emerge_options(p, f, id, &req, srcs, Spend::Spell(printed, id))
                .is_empty()
                && self.additional_payable(p, &def.additional, id);
        }
        if def.additional_or.is_some() || def.additional.taps_chosen() {
            let option = |other: bool, c: &Cost| {
                let r = if other == b.other {
                    req
                } else {
                    self.cost_of(&CostBasis { other, ..*b }, changers)
                };
                self.cost_option_payable(p, c, id, srcs, r, false)
            };
            if !(option(false, &def.additional)
                || def
                    .additional_or
                    .as_deref()
                    .is_some_and(|c| option(true, c)))
            {
                return false;
            }
        } else if !self.payable(p, srcs, &ps.mana_pool, &req, ps.life, None)
            || !self.additional_payable(p, &def.additional, id)
        {
            return false;
        }
        if b.mode.is_none()
            && (!def.spree.is_empty() || !def.tiered.is_empty())
            && let Some(modes) = def.modes()
        {
            let spree = !def.spree.is_empty();
            let n = if spree {
                def.spree.len()
            } else {
                def.tiered.len()
            };
            return (0..n).any(|i| {
                let mode = if spree { (1usize << i) - 1 } else { i };
                let Ok(m) = u8::try_from(mode) else {
                    return false;
                };
                modes
                    .get(mode)
                    .is_some_and(|md| self.targets_available(p, &md.targets, Some(id)))
                    && self.payable(
                        p,
                        srcs,
                        &ps.mana_pool,
                        &self.cost_of(
                            &CostBasis {
                                mode: Some(m),
                                ..*b
                            },
                            changers,
                        ),
                        ps.life,
                        None,
                    )
            });
        }
        true
    }
}

/// CR 118.7b, 118.7c: one mana of each of `colors` off `req`'s colored
/// requirement of that color, or off its generic mana where it has none.
pub(in crate::engine) fn reduce_colors(
    req: &mut crate::mana::ManaRequirement,
    colors: crate::mana::ColorSet,
) {
    for c in colors.iter() {
        let i = c as usize;
        if req.colored[i] > 0 {
            req.colored[i] -= 1;
        } else {
            req.generic = req.generic.saturating_sub(1);
        }
    }
}
