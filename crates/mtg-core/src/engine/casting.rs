//! Priority actions, casting (CR 601), mana payment, and resolution (CR 608).

use smallvec::SmallVec;

use crate::card::{
    AbilityDef, AbilityId, Affects, Cost, CounterKind, Effect, Keyword, Layout, Modification,
    PtValue, Recipient, Replacement, Replaces, SacCost, SearchTo, TargetSpec,
};
use crate::decision::{Choice, DecisionKind};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::mana::{Color, Mana, ManaRequirement};
use crate::state::{
    Affected, AltCast, CastInfo, ContinuousEffect, DelayedTrigger, Duration, GameObject, GameState,
    ObjKind, ReplacementEffect, RuleEffect, StackAbility, Target, Zone,
};

impl GameState {
    // ----- priority actions --------------------------------------------------------

    #[allow(clippy::too_many_lines)] // one block per zone cast from
    pub(super) fn priority_options(&self, p: PlayerId) -> Vec<Choice> {
        let mut options = Vec::new();
        let sorcery_timing =
            p == self.active_player && self.step.is_main() && self.stack.is_empty();
        let ps = self.player(p);
        let sources = self.mana_sources(p);
        let pool = ps.mana_pool;
        let changers = self.cost_changers();
        for id in &ps.hand {
            let def = self.pool.get(self.objects[*id].def);
            // Abilities activated from the hand (cycling, CR 702.29) are
            // instant speed and cost only mana.
            if self.flags_of(&self.objects[*id]).has_activated() {
                for a in &def.ability_ids {
                    if let AbilityDef::Activated(ab) = self.pool.ability(*a)
                        && ab.from_hand
                        && payment_plan_from(
                            &sources,
                            &pool,
                            &ab.cost.mana.requirements(0),
                            ps.life,
                        )
                        .is_some()
                        && self.targets_available(p, &ab.targets, Some(*id))
                    {
                        options.push(Choice::Activate {
                            source: *id,
                            ability: *a,
                            sacrifice: None,
                            discard: None,
                        });
                    }
                }
            }
            if def.is_land() {
                if sorcery_timing && ps.lands_played_this_turn < 1 {
                    options.push(Choice::PlayLand(*id));
                }
                continue;
            }
            // CR 715.3, 709.3: an adventurer's spell or a split card's
            // half is cast as that face.
            if !def.faces.is_empty()
                && matches!(def.layout, Layout::Adventure | Layout::Omen | Layout::Split)
            {
                for (i, f) in def.faces.iter().enumerate() {
                    let face = self.pool.get(*f);
                    if self.castable(p, face, *id, sorcery_timing, &sources, &pool, &changers) {
                        options.push(Choice::CastFace {
                            card: *id,
                            face: u8::try_from(i).unwrap_or(0),
                        });
                    }
                }
                if def.layout == Layout::Split {
                    continue;
                }
            }
            if self.castable(p, def, *id, sorcery_timing, &sources, &pool, &changers)
                && self.undone_cast != Some((*id, self.version))
            {
                options.push(Choice::Cast(*id));
            }
            // CR 702.74a: cast for the evoke cost instead.
            if let Some(ev) = &def.evoke
                && sorcery_timing
                && payment_plan_from(&sources, &pool, &ev.requirements(0), ps.life).is_some()
                && self.targets_available(p, def.cast_targets(), Some(*id))
            {
                options.push(Choice::CastEvoke(*id));
            }
            // CR 702.37c: face down as a 2/2 creature spell for {3}.
            if def.morph.is_some() && sorcery_timing {
                self.face_down_option(p, *id, &sources, &mut options);
            }
            if def.suspend.is_some() || def.foretell.is_some() {
                self.exile_actions(p, *id, sorcery_timing, &sources, &mut options);
            }
        }
        // CR 116.2b: turning a face-down permanent face up.
        if self.face_down_count > 0 {
            self.face_up_options(p, &sources, &mut options);
        }
        // CR 715.3d: a card on an adventure may be cast from exile (its
        // main face).
        if ps.adventures_in_exile > 0 {
            for id in &ps.exile {
                let o = &self.objects[*id];
                // CR 702.143a: a foretold card, on a later turn, for its
                // foretell cost.
                if o.alt == AltCast::Foretold {
                    let def = self.pool.get(o.def);
                    let timing_ok =
                        sorcery_timing || def.is_instant() || def.keywords.contains(Keyword::Flash);
                    if timing_ok
                        && !self
                            .exile_casts
                            .as_ref()
                            .is_some_and(|e| e.foretold_this_turn.contains(id))
                        && self.exile_castable(p, *id, AltCast::Foretell)
                    {
                        options.push(Choice::Cast(*id));
                    }
                    continue;
                }
                if !o.cast.adventure() {
                    continue;
                }
                let def = self.pool.get(o.def);
                if self.castable(p, def, *id, sorcery_timing, &sources, &pool, &changers) {
                    options.push(Choice::Cast(*id));
                }
            }
        }
        // CR 702.34a: instants and sorceries with flashback may be cast
        // from the graveyard for their flashback cost.
        if ps.flashback_in_graveyard > 0 {
            for id in &ps.graveyard {
                let def = self.pool.get(self.objects[*id].def);
                // CR 702.84a and friends: abilities activated from the
                // graveyard (unearth).
                if def.graveyard_active() {
                    for a in &def.ability_ids {
                        if let AbilityDef::Activated(ab) = self.pool.ability(*a)
                            && ab.from_graveyard
                            && (!ab.sorcery_speed || sorcery_timing)
                            && payment_plan_from(
                                &sources,
                                &pool,
                                &ab.cost.mana.requirements(0),
                                ps.life,
                            )
                            .is_some()
                            && self.targets_available(p, &ab.targets, Some(*id))
                        {
                            options.push(Choice::Activate {
                                source: *id,
                                ability: *a,
                                sacrifice: None,
                                discard: None,
                            });
                        }
                    }
                }
                if def.flashback.is_none() {
                    if def.escape.is_some() && self.escape_castable(p, *id, sorcery_timing) {
                        options.push(Choice::Cast(*id));
                    }
                    continue;
                }
                let Some(fb) = &def.flashback else {
                    continue;
                };
                let instant_speed = def.is_instant() || def.keywords.contains(Keyword::Flash);
                if !(instant_speed || sorcery_timing) {
                    continue;
                }
                let mut req = fb.mana.requirements(0);
                adjust_generic(&mut req, self.cost_delta(p, def, *id, &changers));
                if payment_plan_from(&sources, &pool, &req, ps.life).is_none() {
                    continue;
                }
                if fb.life > 0 && ps.life < i32::try_from(fb.life).unwrap_or(i32::MAX) {
                    continue;
                }
                if !self.additional_payable(p, &def.additional, *id) {
                    continue;
                }
                let targets_ok = match def.modes() {
                    Some(modes) => modes
                        .iter()
                        .any(|m| self.targets_available(p, &m.targets, Some(*id))),
                    None => self.targets_available(p, def.cast_targets(), Some(*id)),
                };
                if targets_ok {
                    options.push(Choice::Cast(*id));
                }
            }
        }
        self.activation_options(p, sorcery_timing, &sources, &mut options);
        options.push(Choice::Pass);
        options
    }

    /// A variable amount's value as an effect happens: X from the spell, or
    /// one of the source card's counts (CR 107.3 for X; "the number of
    /// ..." judged from the effect's controller).
    pub(super) fn eval_num(&self, n: crate::card::Num, ctx: &EffectCtx) -> u32 {
        use crate::card::Num;
        let i = match n {
            Num::Count(i) => i as usize,
            Num::ThatMuch => return ctx.trigger_amount,
            other => return other.eval(ctx.x),
        };
        let def = self
            .objects
            .get(ctx.source)
            .map(|o| o.def)
            .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def));
        let pool = std::sync::Arc::clone(&self.pool);
        let Some(expr) = def.and_then(|d| pool.get(d).counts.get(i)) else {
            return 0;
        };
        self.eval_count(expr, ctx)
    }

    /// A count's value as an effect happens, judged from its controller.
    fn eval_count(&self, expr: &crate::card::CountExpr, ctx: &EffectCtx) -> u32 {
        use crate::card::CountExpr;
        let you = ctx.controller;
        let n = match expr {
            CountExpr::Times(k, inner) => return k.saturating_mul(self.eval_count(inner, ctx)),
            CountExpr::Permanents(f) => self
                .battlefield
                .iter()
                .filter(|id| {
                    let o = &self.objects[**id];
                    self.characteristics(**id).is_some_and(|c| {
                        f.matches(**id, o.controller, &c, Some(ctx.source), Some(you))
                    }) && self.status_ok(f, **id)
                })
                .count(),
            CountExpr::Domain => {
                let mut seen = crate::mana::ColorSet::default();
                for id in self.permanents_of(you) {
                    if let Some(c) = self.characteristics(id)
                        && c.is_land()
                    {
                        for s in &c.subtypes {
                            if let Some(col) = s.basic_color() {
                                seen = seen.with(col);
                            }
                        }
                    }
                }
                seen.iter().count()
            }
            CountExpr::CardsInHand => self.player(you).hand.len(),
            CountExpr::CardsInGraveyard(f) => self
                .player(you)
                .graveyard
                .iter()
                .filter(|id| {
                    f.as_ref().is_none_or(|f| {
                        let c = self.base_characteristics(&self.objects[**id]);
                        f.matches(**id, you, &c, None, Some(you)) && self.status_ok(f, **id)
                    })
                })
                .count(),
            CountExpr::SourcePower => {
                return self
                    .characteristics_or_lki(ctx.source)
                    .map_or(0, |c| u32::try_from(c.power().max(0)).unwrap_or(0));
            }
            CountExpr::TargetPower(t) => {
                return match ctx.targets.get(*t) {
                    Some(Target::Object(id)) => self
                        .characteristics_or_lki(*id)
                        .map_or(0, |c| u32::try_from(c.power().max(0)).unwrap_or(0)),
                    _ => 0,
                };
            }
            CountExpr::Life => return u32::try_from(self.player(you).life.max(0)).unwrap_or(0),
        };
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// Whether `p` could cast `def` (the card `id`, or one of its faces)
    /// now: timing, mana, additional costs, and targets (CR 601.2).
    #[inline]
    #[allow(clippy::too_many_arguments)] // the parts precomputed once per listing
    fn castable(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        id: ObjId,
        sorcery_timing: bool,
        sources: &Sources,
        pool: &crate::mana::ManaPool,
        changers: &[ObjId],
    ) -> bool {
        let instant_speed = def.is_instant() || def.keywords.contains(Keyword::Flash);
        if !(instant_speed || sorcery_timing) {
            return false;
        }
        let mut req = def.mana_cost.requirements(0);
        if def.self_cost.is_some() || def.cond_cost.is_some() || !changers.is_empty() {
            adjust_generic(&mut req, self.cost_delta(p, def, id, changers));
        }
        // Cheaper with a fitting target: castable if one exists (targets
        // are then held to what's payable).
        if let Some(tc) = &def.target_cost
            && tc.0 < 0
            && def.cast_targets().first().is_some_and(|spec| {
                self.legal_targets(p, *spec, Some(id))
                    .iter()
                    .any(|t| self.target_fits(&tc.1, *t, p, id))
            })
        {
            adjust_generic(&mut req, tc.0);
        }
        let helped = Keyword::helps_pay(def.keywords);
        let payable = if helped {
            let mut all = sources.clone();
            all.extend(self.helper_sources(p, def));
            payment_plan_from(&all, pool, &req, self.player(p).life).is_some()
        } else {
            payment_plan_from(sources, pool, &req, self.player(p).life).is_some()
        };
        if !payable {
            return false;
        }
        if !self.additional_payable(p, &def.additional, id) {
            return false;
        }
        // A modal spell needs some mode to have its targets (CR 601.2b).
        match def.modes() {
            Some(modes) => modes
                .iter()
                .any(|m| self.targets_available(p, &m.targets, Some(id))),
            None => self.targets_available(p, def.cast_targets(), Some(id)),
        }
    }

    #[cold]
    #[inline(never)]
    fn face_down_option(
        &self,
        p: PlayerId,
        id: ObjId,
        sources: &Sources,
        options: &mut Vec<Choice>,
    ) {
        let ps = self.player(p);
        if payment_plan_from(sources, &ps.mana_pool, &three(), ps.life).is_some() {
            options.push(Choice::CastFaceDown(id));
        }
    }

    #[cold]
    #[inline(never)]
    fn face_up_options(&self, p: PlayerId, sources: &Sources, options: &mut Vec<Choice>) {
        let ps = self.player(p);
        for id in self.permanents_of(p) {
            if let Some(cost) = self.face_up_cost(id)
                && payment_plan_from(sources, &ps.mana_pool, &cost.requirements(0), ps.life)
                    .is_some()
            {
                options.push(Choice::TurnFaceUp(id));
            }
        }
    }

    /// Suspend (CR 702.62a: whenever the card could be cast) and foretell
    /// (CR 702.143a: any time during its owner's turn) from the hand.
    #[cold]
    #[inline(never)]
    fn exile_actions(
        &self,
        p: PlayerId,
        id: ObjId,
        sorcery_timing: bool,
        sources: &Sources,
        options: &mut Vec<Choice>,
    ) {
        let ps = self.player(p);
        let def = self.pool.get(self.objects[id].def);
        if let Some((_, cost)) = def.suspend.as_deref()
            && (sorcery_timing || def.is_instant() || def.keywords.contains(Keyword::Flash))
            && payment_plan_from(sources, &ps.mana_pool, &cost.requirements(0), ps.life).is_some()
        {
            options.push(Choice::Suspend(id));
        }
        if def.foretell.is_some()
            && p == self.active_player
            && payment_plan_from(
                sources,
                &ps.mana_pool,
                &crate::mana::ManaCost::parse("{2}")
                    .expect("a mana cost")
                    .requirements(0),
                ps.life,
            )
            .is_some()
        {
            options.push(Choice::Foretell(id));
        }
    }

    /// What turning the face-down permanent `id` face up costs: its morph
    /// or disguise cost, or a manifested creature card's mana cost (CR
    /// 701.34a); `None` if it isn't face down or can't be turned up.
    pub(super) fn face_up_cost(&self, id: ObjId) -> Option<crate::mana::ManaCost> {
        let o = self.objects.get(id)?;
        if !self.pool.is_face_down(o.def) || o.zone != Zone::Battlefield {
            return None;
        }
        let card = self.pool.get(o.card);
        match card.morph.as_deref() {
            Some((cost, _)) => Some(cost.clone()),
            None if card.is_creature() => Some(card.mana_cost.clone()),
            None => None,
        }
    }

    /// Other untapped creatures `p` controls that could crew `vehicle`,
    /// with their power (CR 702.122a; summoning sickness doesn't matter).
    fn crew_candidates(&self, p: PlayerId, vehicle: ObjId) -> SmallVec<[(ObjId, u32); 4]> {
        self.creatures_of(p)
            .filter(|id| *id != vehicle && !self.objects[*id].tapped)
            .map(|id| {
                let power = self
                    .characteristics(id)
                    .map_or(0, |c| u32::try_from(c.power().max(0)).unwrap_or(0));
                (id, power)
            })
            .collect()
    }

    fn crew_power(&self, p: PlayerId, vehicle: ObjId) -> u32 {
        self.crew_candidates(p, vehicle)
            .iter()
            .map(|(_, n)| n)
            .sum()
    }

    /// The creatures that crew: the weakest one strong enough alone, or
    /// else the strongest until the power suffices.
    fn crew_choice(&self, p: PlayerId, vehicle: ObjId, need: u32) -> SmallVec<[ObjId; 4]> {
        let mut c = self.crew_candidates(p, vehicle);
        if let Some((id, _)) = c.iter().filter(|(_, n)| *n >= need).min_by_key(|(_, n)| *n) {
            return smallvec::smallvec![*id];
        }
        c.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let mut total = 0;
        let mut out = SmallVec::new();
        for (id, n) in c {
            if total >= need {
                break;
            }
            total += n;
            out.push(id);
        }
        out
    }

    /// Whether some permanent may have abilities its definition lacks.
    pub(super) fn grants_exist(&self) -> bool {
        self.grant_sources > 0 || self.eot_grants > 0
    }

    /// Convoke and improvise (CR 702.51, 702.126): untapped creatures (each
    /// paying one mana of its color, or generic) and untapped artifacts
    /// (generic) that may be tapped to help cast `def`; after mana
    /// sources, so they are used last.
    pub(super) fn helper_sources(&self, p: PlayerId, def: &crate::card::CardDef) -> Sources {
        let convoke = def.keywords.contains(Keyword::Convoke);
        let improvise = def.keywords.contains(Keyword::Improvise);
        let mut out = Sources::new();
        // CR 702.66a: each card exiled from the graveyard pays for {1}.
        if def.keywords.contains(Keyword::Delve) {
            for id in &self.player(p).graveyard {
                out.push((*id, Mana::Colorless));
            }
        }
        if !convoke && !improvise {
            return out;
        }
        for id in self.permanents_of(p) {
            if self.objects[id].tapped {
                continue;
            }
            let Some(c) = self.characteristics(id) else {
                continue;
            };
            if convoke && c.is_creature() {
                let mut any = false;
                for col in c.colors.iter() {
                    out.push((id, Mana::Colored(col)));
                    any = true;
                }
                if !any {
                    out.push((id, Mana::Colorless));
                }
            } else if improvise && c.types.contains(crate::card::CardType::Artifact) {
                out.push((id, Mana::Colorless));
            }
        }
        out
    }

    /// The sources that may pay for the spell `spell` being cast.
    fn spell_sources(&self, spell: ObjId) -> Sources {
        let o = &self.objects[spell];
        let mut s = self.mana_sources(o.controller);
        let def = self.pool.get(o.def);
        if Keyword::helps_pay(def.keywords) {
            s.extend(self.helper_sources(o.controller, def));
        }
        s
    }

    /// Permanents with a static ability that changes what spells cost.
    pub(super) fn cost_changers(&self) -> SmallVec<[ObjId; 2]> {
        if self.cost_change_sources == 0 {
            return SmallVec::new();
        }
        self.battlefield
            .iter()
            .copied()
            .filter(|id| self.flags_of(&self.objects[*id]).has_cost_change())
            .collect()
    }

    /// CR 601.2f: how much more (or, below zero, less) generic mana `def`
    /// costs `p` to cast as the card `id`, from its own text and from
    /// `changers`' static abilities.
    pub(super) fn cost_delta(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        id: ObjId,
        changers: &[ObjId],
    ) -> i32 {
        let mut d = 0i32;
        if let Some((delta, cond)) = def.cond_cost
            && self.condition_now(self.pool.condition(cond), p, id)
        {
            d += delta;
        }
        if let Some(sc) = &def.self_cost {
            let ctx = EffectCtx {
                source: id,
                controller: p,
                targets: Vec::new(),
                legal: Vec::new(),
                trigger_obj: None,
                trigger_player: None,
                trigger_amount: 0,
                created: None,
                x: 0,
                did: true,
            };
            let n = sc.per.saturating_mul(self.eval_num(sc.times, &ctx));
            d -= i32::try_from(n).unwrap_or(i32::MAX);
        }
        let Some(chars) = def.printed.as_ref() else {
            return d;
        };
        for src in changers {
            let o = &self.objects[*src];
            for a in &self.pool.get(o.def).ability_ids {
                if let AbilityDef::CostChange(cc) = self.pool.ability(*a) {
                    let whose = match cc.whose {
                        crate::card::Who::Any => true,
                        crate::card::Who::You => p == o.controller,
                        crate::card::Who::Opponents => p != o.controller,
                    };
                    if whose
                        && cc
                            .spells
                            .matches(id, p, chars, Some(*src), Some(o.controller))
                    {
                        d += cc.generic;
                    }
                }
            }
        }
        d
    }

    /// Cards in `p`'s hand that could be discarded for a cost of `source`
    /// with this filter ("discard a creature card").
    /// Whether `p` can pay a spell's additional costs (CR 601.2h): enough
    /// life (CR 119.4), other cards to discard, something to sacrifice.
    /// The spell's own card is never one of them, wherever it is cast from.
    fn additional_payable(&self, p: PlayerId, add: &Cost, spell: ObjId) -> bool {
        let ps = self.player(p);
        if add.life > 0 && ps.life < i32::try_from(add.life).unwrap_or(i32::MAX) {
            return false;
        }
        if add.discard > 0
            && self
                .discard_candidates(p, add.discard_filter.as_ref(), spell)
                .len()
                < add.discard as usize
        {
            return false;
        }
        if let Some(SacCost::Filter(f)) = &add.sacrifice
            && self.sacrifice_candidates(p, f, spell).is_empty()
        {
            return false;
        }
        true
    }

    pub(super) fn discard_candidates(
        &self,
        p: PlayerId,
        filter: Option<&crate::card::ObjFilter>,
        source: ObjId,
    ) -> Vec<ObjId> {
        self.player(p)
            .hand
            .iter()
            .copied()
            .filter(|id| {
                *id != source
                    && filter.is_none_or(|f| {
                        self.characteristics(*id)
                            .is_some_and(|c| f.matches(*id, p, &c, Some(source), Some(p)))
                            && self.status_ok(f, *id)
                    })
            })
            .collect()
    }

    /// Whether `spell` (being cast) has a kicker its caster could pay on
    /// top of its cost (CR 702.33b).
    pub(super) fn kicker_payable(&self, spell: ObjId) -> bool {
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let Some(k) = &def.kicker else {
            return false;
        };
        if o.cast.flashback() {
            return false;
        }
        let mut req = def.mana_cost.requirements(0);
        req.add(&k.requirements(0));
        adjust_generic(
            &mut req,
            self.cost_delta(o.controller, def, spell, &self.cost_changers()),
        );
        self.payment_plan(o.controller, &req).is_some()
    }

    /// Whether `spell` (being cast) has a buyback cost its caster could pay
    /// on top of its cost and any kicker (CR 702.27).
    pub(super) fn buyback_payable(&self, spell: ObjId) -> bool {
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let Some(b) = &def.buyback else {
            return false;
        };
        if o.cast.flashback() {
            return false;
        }
        let mut req = self.spell_requirements(spell, 0);
        req.add(&b.requirements(0));
        self.payment_plan(o.controller, &req).is_some()
    }

    /// The mana a spell being cast costs with X fixed: its mana cost (or
    /// its flashback cost, CR 702.34a) plus its kicker if kicked.
    pub(super) fn spell_requirements(&self, spell: ObjId, x: u32) -> ManaRequirement {
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let mut req = match (&def.flashback, o.cast.flashback()) {
            (Some(fb), true) => fb.mana.requirements(x),
            // Cast without paying its mana cost: X is 0 (CR 107.3b).
            _ if matches!(o.alt, AltCast::Free | AltCast::Suspend) => ManaRequirement::default(),
            (_, false) if o.alt == AltCast::Madness && def.madness.is_some() => {
                def.madness.as_ref().expect("checked").requirements(x)
            }
            (_, false) if o.alt == AltCast::Foretell && def.foretell.is_some() => {
                def.foretell.as_ref().expect("checked").requirements(x)
            }
            (_, false) if o.alt == AltCast::Escape && def.escape.is_some() => {
                def.escape.as_ref().expect("checked").0.requirements(x)
            }
            (_, false) if o.cast.evoked() && def.evoke.is_some() => {
                def.evoke.as_ref().expect("checked").requirements(x)
            }
            // CR 702.37c: a face-down spell costs {3}.
            _ if self.pool.is_face_down(o.def) => crate::mana::ManaCost::parse("{3}")
                .expect("a mana cost")
                .requirements(0),
            _ => def.mana_cost.requirements(x),
        };
        if o.cast.kicked()
            && let Some(k) = &def.kicker
        {
            req.add(&k.requirements(0));
        }
        if o.cast.bought_back()
            && let Some(b) = &def.buyback
        {
            req.add(&b.requirements(0));
        }
        // CR 702.42a: every mode, for the entwine cost on top.
        if let Some(e) = &def.entwine
            && self.mode_split_of(spell).is_some()
        {
            req.add(&e.requirements(0));
        }
        if def.self_cost.is_some() || def.cond_cost.is_some() || !self.cost_changers().is_empty() {
            adjust_generic(
                &mut req,
                self.cost_delta(o.controller, def, spell, &self.cost_changers()),
            );
        }
        if let Some(tc) = &def.target_cost
            && let Some(t) = o.targets.first()
            && self.target_fits(&tc.1, *t, o.controller, spell)
        {
            adjust_generic(&mut req, tc.0);
        }
        req
    }

    /// CR 702.138a: a card with escape in the graveyard, castable for its
    /// escape cost with enough other cards to exile.
    #[cold]
    #[inline(never)]
    fn escape_castable(&self, p: PlayerId, card: ObjId, sorcery_timing: bool) -> bool {
        let def = self.pool.get(self.objects[card].def);
        let Some((cost, n)) = def.escape.as_deref() else {
            return false;
        };
        let instant = def.is_instant() || def.keywords.contains(Keyword::Flash);
        if !(instant || sorcery_timing) || self.player(p).graveyard.len() <= *n as usize {
            return false;
        }
        let mut req = cost.requirements(0);
        adjust_generic(
            &mut req,
            self.cost_delta(p, def, card, &self.cost_changers()),
        );
        if self.payment_plan(p, &req).is_none() {
            return false;
        }
        match def.modes() {
            Some(modes) => modes
                .iter()
                .any(|m| self.targets_available(p, &m.targets, Some(card))),
            None => self.targets_available(p, def.cast_targets(), Some(card)),
        }
    }

    /// Whether `card` (exiled) could be cast `alt`'s way now: a nonland
    /// card with targets for some mode, whose cost (if any) is payable.
    fn exile_castable(&self, p: PlayerId, card: ObjId, alt: AltCast) -> bool {
        let def = self.pool.get(self.objects[card].def);
        if def.is_land() {
            return false;
        }
        let targets_ok = match def.modes() {
            Some(modes) => modes
                .iter()
                .any(|m| self.targets_available(p, &m.targets, Some(card))),
            None => self.targets_available(p, def.cast_targets(), Some(card)),
        };
        if !targets_ok {
            return false;
        }
        let cost = match alt {
            AltCast::Madness => def.madness.as_ref(),
            AltCast::Foretell => def.foretell.as_ref(),
            _ => return true,
        };
        let Some(cost) = cost else {
            return false;
        };
        let mut req = cost.requirements(0);
        adjust_generic(
            &mut req,
            self.cost_delta(p, def, card, &self.cost_changers()),
        );
        self.payment_plan(p, &req).is_some()
    }

    /// "You may cast it": if `card` can be cast `alt`'s way, its owner
    /// chooses; a yes casts it as the resolution finishes. Whether it will
    /// be cast.
    pub(super) fn offer_exile_cast(&mut self, card: ObjId, alt: AltCast) -> bool {
        let p = self.objects[card].owner;
        self.offer_exile_cast_by(card, alt, p)
    }

    pub(super) fn offer_exile_cast_by(&mut self, card: ObjId, alt: AltCast, p: PlayerId) -> bool {
        if !self.exile_castable(p, card, alt) {
            return false;
        }
        let k = self.choose_mid(
            p,
            DecisionKind::CastFromExile { card },
            vec![Choice::Accept, Choice::Decline],
        );
        if k == 0 {
            self.exile_casts
                .get_or_insert_with(Box::default)
                .pending
                .push((p, card, alt));
            true
        } else {
            false
        }
    }

    /// CR 702.85a: exile cards from the top of the library until a nonland
    /// card with lesser mana value; it may be cast free; the rest go to the
    /// bottom in a random order.
    /// Discover N (CR 701.57a) is the same with mana value N or less, and
    /// the card goes to the hand if not cast.
    fn cascade(&mut self, ctx: &EffectCtx, discover: Option<u32>) {
        let p = ctx.controller;
        let mv = if let Some(n) = discover {
            n + 1
        } else {
            let def = self
                .objects
                .get(ctx.source)
                .map(|o| o.def)
                .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def));
            let Some(def) = def else {
                return;
            };
            self.pool.get(def).mana_cost.mana_value()
        };
        let mut exiled: Vec<ObjId> = Vec::new();
        let mut hit = None;
        while let Some(top) = self.player(p).library.last().copied() {
            let new = self.move_object(top, Zone::Exile, true);
            let d = self.pool.get(self.objects[new].def);
            if !d.is_land() && d.mana_cost.mana_value() < mv {
                hit = Some(new);
                break;
            }
            exiled.push(new);
        }
        if let Some(h) = hit
            && !self.offer_exile_cast_by(h, AltCast::Free, p)
        {
            if discover.is_some() {
                self.move_object(h, Zone::Hand, false);
            } else {
                exiled.push(h);
            }
        }
        rand::seq::SliceRandom::shuffle(exiled.as_mut_slice(), &mut self.rng);
        for id in exiled {
            if self.objects.get(id).is_some_and(|o| o.zone == Zone::Exile) {
                self.move_object(id, Zone::Library, false);
            }
        }
    }

    /// CR 702.110a: the controller may sacrifice a creature; if they do,
    /// the source's "when this exploits a creature" abilities trigger.
    fn exploit(&mut self, ctx: &EffectCtx) {
        let p = ctx.controller;
        let mut options: Vec<Choice> = self.creatures_of(p).map(Choice::Sacrifice).collect();
        if options.is_empty() {
            return;
        }
        options.push(Choice::Decline);
        let k = self.choose_mid(p, DecisionKind::Edict, options.clone());
        let Choice::Sacrifice(victim) = options[k] else {
            return;
        };
        let def = self
            .objects
            .get(ctx.source)
            .map(|o| o.def)
            .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def));
        self.sacrifice(victim);
        let Some(def) = def else {
            return;
        };
        let pool = std::sync::Arc::clone(&self.pool);
        for a in &pool.get(def).ability_ids {
            if let AbilityDef::Triggered(t) = pool.ability(*a)
                && t.trigger == crate::card::TriggerSpec::Exploits
            {
                self.queue_builtin(p, *a, ctx.source, def);
            }
        }
    }

    /// CR 702.34a: a spell cast with flashback is exiled instead of going
    /// anywhere else as it leaves the stack.
    pub(super) fn leave_stack(&mut self, spell: ObjId) {
        let zone = if self.objects[spell].cast.flashback() {
            Zone::Exile
        } else {
            Zone::Graveyard
        };
        self.move_object(spell, zone, true);
    }

    /// CR 602: activated abilities of permanents `p` controls whose costs
    /// can be paid and whose targets exist. An ability with a "sacrifice a
    /// Goblin" cost is offered once per permanent that could be sacrificed.
    #[allow(clippy::too_many_lines)] // one check per cost kind
    fn activation_options(
        &self,
        p: PlayerId,
        sorcery_timing: bool,
        sources: &Sources,
        options: &mut Vec<Choice>,
    ) {
        let grants = self.grants_exist();
        if self.activated_sources == 0 && !grants {
            return;
        }
        let pool = self.player(p).mana_pool;
        let life = self.player(p).life;
        for id in self.permanents_of(p) {
            let o = &self.objects[id];
            if !grants && !self.flags_of(o).has_activated() {
                continue;
            }
            let Some(c) = self.characteristics(id) else {
                continue;
            };
            let sick = c.is_creature() && !o.controlled_since_turn_start && !c.has(Keyword::Haste);
            for a in &c.abilities {
                let AbilityDef::Activated(ab) = self.pool.ability(*a) else {
                    continue;
                };
                if ab.sorcery_speed && !sorcery_timing {
                    continue;
                }
                if ab.from_hand
                    || ab.from_graveyard
                    || (ab.per_turn > 0
                        && self
                            .activated_once
                            .iter()
                            .filter(|x| **x == (id, *a))
                            .count()
                            >= usize::from(ab.per_turn))
                {
                    continue;
                }
                // CR 302.6: a creature's {T} needs it to have been controlled
                // since the turn began.
                if ab.cost.tap && (o.tapped || sick) {
                    continue;
                }
                // A source tapped as a cost cannot also tap for mana.
                let payable = if ab.cost.tap {
                    let rest: Sources = sources.iter().filter(|(s, _)| *s != id).copied().collect();
                    payment_plan_from(&rest, &pool, &ab.cost.mana.requirements(0), life).is_some()
                } else {
                    payment_plan_from(sources, &pool, &ab.cost.mana.requirements(0), life).is_some()
                };
                if !payable {
                    continue;
                }
                if !self.targets_available(p, &ab.targets, Some(id)) {
                    continue;
                }
                if ab.cost.crew > 0 && self.crew_power(p, id) < ab.cost.crew {
                    continue;
                }
                if let Some((k, n)) = ab.cost.remove_counters
                    && o.counters(k) < n
                {
                    continue;
                }
                if ab.cost.energy > self.player(p).energy {
                    continue;
                }
                if ab.cost.life > 0
                    && self.player(p).life < i32::try_from(ab.cost.life).unwrap_or(i32::MAX)
                {
                    continue;
                }
                // CR 606.3: one loyalty ability per planeswalker per turn, at
                // sorcery speed; CR 606.5: a −N cost needs N loyalty.
                if ab.cost.loyalty != 0 {
                    if !sorcery_timing || self.loyalty_activated.contains(&id) {
                        continue;
                    }
                    let need = u32::try_from(-ab.cost.loyalty).unwrap_or(0);
                    if o.counters(CounterKind::Loyalty) < need {
                        continue;
                    }
                }
                // One option per way to pay the costs that need a choice.
                let sacrifices: Vec<Option<ObjId>> = match &ab.cost.sacrifice {
                    None | Some(SacCost::This) => vec![None],
                    Some(SacCost::Filter(f)) => self
                        .sacrifice_candidates(p, f, id)
                        .into_iter()
                        .map(Some)
                        .collect(),
                };
                let discards: Vec<Option<ObjId>> = if ab.cost.discard > 0 {
                    self.discard_candidates(p, ab.cost.discard_filter.as_ref(), id)
                        .into_iter()
                        .map(Some)
                        .collect()
                } else {
                    vec![None]
                };
                if discards.is_empty() {
                    continue;
                }
                for sacrifice in &sacrifices {
                    for discard in &discards {
                        options.push(Choice::Activate {
                            source: id,
                            ability: *a,
                            sacrifice: *sacrifice,
                            discard: *discard,
                        });
                    }
                }
            }
        }
    }

    /// CR 602.2a: announce an activated ability; it goes on the stack and
    /// its targets are chosen next (`Flow::Casting`), then costs are paid
    /// (`finish_activation`).
    pub(super) fn activate(
        &mut self,
        player: PlayerId,
        source: ObjId,
        ability: AbilityId,
        sacrifice: Option<ObjId>,
        discard: Option<ObjId>,
    ) {
        self.consecutive_passes = 0;
        let def = self.objects[source].def;
        if let AbilityDef::Activated(a) = self.pool.ability(ability) {
            if a.cost.loyalty != 0 {
                self.loyalty_activated.push(source);
            }
            if a.per_turn > 0 {
                self.activated_once.push((source, ability));
            }
        }
        let timestamp = self.tick();
        let obj = self.objects.insert(GameObject {
            def,
            card: def,
            except: None,
            cast: CastInfo::default(),
            kind: ObjKind::Ability,
            owner: player,
            controller: player,
            zone: Zone::Stack,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            controlled_since_turn_start: false,
            echo_due: false,
            skip_untap: false,
            alt: crate::state::AltCast::None,
            timestamp,
            targets: SmallVec::new(),
            counters: SmallVec::new(),
            attached_to: None,
            ability: Some(StackAbility {
                ability,
                source,
                def,
                trigger_obj: None,
                trigger_player: None,
                trigger_amount: 0,
            }),
        });
        self.stack.push(obj);
        self.bump();
        self.flow = crate::state::Flow::Casting {
            spell: obj,
            next_target: 0,
            sacrifice,
            discard,
        };
    }

    /// Permanents `p` could sacrifice for a "sacrifice a ..." cost of
    /// `source`.
    pub(super) fn sacrifice_candidates(
        &self,
        p: PlayerId,
        f: &crate::card::ObjFilter,
        source: ObjId,
    ) -> Vec<ObjId> {
        self.permanents_of(p)
            .filter(|s| {
                let sc = &self.objects[*s];
                self.characteristics(*s)
                    .is_some_and(|c| f.matches(*s, sc.controller, &c, Some(source), Some(p)))
                    && self.status_ok(f, *s)
            })
            .collect()
    }

    /// The values of X the caster can pay for (CR 601.2b, 601.2h), in
    /// increasing order.
    pub(super) fn x_options(&self, spell: ObjId) -> Vec<Choice> {
        const MAX_X: u32 = 30;
        if matches!(self.objects[spell].alt, AltCast::Free | AltCast::Suspend) {
            return vec![Choice::X(0)];
        }
        let caster = self.objects[spell].controller;
        let sources = self.spell_sources(spell);
        let pool = self.player(caster).mana_pool;
        (0..=MAX_X)
            .take_while(|x| {
                payment_plan_from(
                    &sources,
                    &pool,
                    &self.spell_requirements(spell, *x),
                    self.player(caster).life,
                )
                .is_some()
            })
            .map(Choice::X)
            .collect()
    }

    /// An ability object on the stack has its targets. An activated ability
    /// now pays its costs; a triggered one is done.
    pub(super) fn finish_ability_targets(
        &mut self,
        obj: ObjId,
        sacrifice: Option<ObjId>,
        discard: Option<ObjId>,
    ) {
        self.emit_targeted(obj);
        let activated = self.objects[obj]
            .ability
            .is_some_and(|sa| matches!(self.pool.ability(sa.ability), AbilityDef::Activated(_)));
        if activated {
            self.finish_activation(obj, sacrifice, discard);
        } else {
            self.flow = crate::state::Flow::Priority;
        }
    }

    /// Whether every one of `specs` has a legal target for `p` (for an
    /// ability of `source`).
    pub(super) fn targets_available(
        &self,
        p: PlayerId,
        specs: &[TargetSpec],
        source: Option<ObjId>,
    ) -> bool {
        // CR 115.3: "two target lands" needs two different lands.
        specs.iter().all(|s| {
            matches!(s, TargetSpec::UpTo(_))
                || self.legal_targets(p, *s, source).len()
                    >= specs.iter().filter(|x| *x == s).count()
        })
    }

    /// The permanents a stack object targets become its targets (CR 115.?):
    /// "becomes the target of a spell or ability" triggers see it.
    fn emit_targeted(&mut self, obj: ObjId) {
        let targets: SmallVec<[ObjId; 2]> = self.objects[obj]
            .targets
            .iter()
            .filter_map(|t| match t {
                Target::Object(id) => Some(*id),
                Target::Player(_) => None,
            })
            .collect();
        for target in targets {
            self.emit(Event::Targeted { target, by: obj });
        }
    }

    /// The modes of a modal spell being cast that have their targets
    /// (CR 601.2b).
    pub(super) fn mode_options(&self, spell: ObjId) -> Vec<Choice> {
        let o = &self.objects[spell];
        let Some(modes) = self.modes_of(spell) else {
            return Vec::new();
        };
        let source = Some(o.ability.map_or(spell, |a| a.source));
        let def = self.pool.get(o.def);
        modes
            .iter()
            .enumerate()
            .filter(|(_, m)| self.targets_available(o.controller, &m.targets, source))
            .filter(|(_, m)| {
                // Entwined, the spell costs more (CR 702.42a).
                m.split.is_none()
                    || o.kind == ObjKind::Ability
                    || def.entwine.as_ref().is_none_or(|e| {
                        let mut req = def.mana_cost.requirements(0);
                        req.add(&e.requirements(0));
                        adjust_generic(
                            &mut req,
                            self.cost_delta(o.controller, def, spell, &self.cost_changers()),
                        );
                        self.payment_plan(o.controller, &req).is_some()
                    })
            })
            .map(|(i, _)| Choice::Mode(i))
            .collect()
    }

    /// CR 602.2b, 601.2g–h: pay the ability's costs. The source is tapped
    /// first so that it is not also tapped for mana; sacrifices come last,
    /// so a "sacrifice ~" source is still there to be tapped.
    fn finish_activation(&mut self, obj: ObjId, sacrifice: Option<ObjId>, discard: Option<ObjId>) {
        let o = &self.objects[obj];
        let (controller, def) = (o.controller, o.def);
        let sa = o.ability.expect("ability object");
        let pool = std::sync::Arc::clone(&self.pool);
        let AbilityDef::Activated(ab) = pool.ability(sa.ability) else {
            unreachable!("checked by finish_ability_targets");
        };
        if ab.cost.tap {
            self.tap(sa.source);
        }
        self.pay(controller, &ab.cost.mana.requirements(0));
        if ab.cost.crew > 0 {
            for c in self.crew_choice(controller, sa.source, ab.cost.crew) {
                self.tap(c);
            }
        }
        if ab.cost.exile_self && self.objects[sa.source].zone == Zone::Graveyard {
            self.move_object(sa.source, Zone::Exile, false);
        }
        if ab.cost.discard_self && self.objects[sa.source].zone == Zone::Hand {
            self.discard_card(controller, sa.source);
        }
        self.pay_other_costs(controller, &ab.cost, sa.source, sacrifice, discard);
        self.emit(Event::Activated {
            obj,
            source: sa.source,
            def,
        });
        // CR 605.3b: a mana ability doesn't use the stack.
        if ab.is_mana_ability() {
            self.resolve_top();
        }
        self.give_priority(controller);
    }

    /// Whether `spell` could be paid for with `t` as its first target.
    pub(super) fn target_payable(&self, spell: ObjId, t: Target) -> bool {
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let Some(tc) = &def.target_cost else {
            return true;
        };
        let mut req = self.spell_requirements(spell, self.x_of(spell));
        if self.target_fits(&tc.1, t, o.controller, spell) {
            adjust_generic(&mut req, tc.0);
        }
        let sources = self.spell_sources(spell);
        let ps = self.player(o.controller);
        payment_plan_from(&sources, &ps.mana_pool, &req, ps.life).is_some()
    }

    /// Whether `t` matches a target-cost filter (a permanent's current
    /// characteristics and state).
    pub(super) fn target_fits(
        &self,
        f: &crate::card::ObjFilter,
        t: Target,
        p: PlayerId,
        source: ObjId,
    ) -> bool {
        let Target::Object(id) = t else {
            return false;
        };
        let Some(o) = self.objects.get(id) else {
            return false;
        };
        self.characteristics(id)
            .is_some_and(|c| f.matches(id, o.controller, &c, Some(source), Some(p)))
            && self.status_ok(f, id)
    }

    /// Legal targets for `spec` chosen by `caster`, for a spell or an
    /// ability of `source`.
    pub(super) fn legal_targets(
        &self,
        caster: PlayerId,
        spec: TargetSpec,
        source: Option<ObjId>,
    ) -> Vec<Target> {
        let spec = self.pool.base_spec(spec);
        let mut out = Vec::new();
        if spec.is_graveyard() {
            for p in self.players_in_game() {
                for id in &self.player(p).graveyard {
                    let t = Target::Object(*id);
                    // A spell cast from a graveyard is on the stack by the
                    // time it targets (CR 601.2a, 601.2c).
                    if Some(*id) != source && self.is_legal_target_for(caster, spec, t, source) {
                        out.push(t);
                    }
                }
            }
            return out;
        }
        if spec.is_spell() {
            for id in &self.stack {
                let t = Target::Object(*id);
                if Some(*id) != source && self.is_legal_target_for(caster, spec, t, source) {
                    out.push(t);
                }
            }
            return out;
        }
        for id in &self.battlefield {
            let t = Target::Object(*id);
            if self.is_legal_target_for(caster, spec, t, source) {
                out.push(t);
            }
        }
        for p in self.players_in_game() {
            let t = Target::Player(p);
            if self.is_legal_target_for(caster, spec, t, source) {
                out.push(t);
            }
        }
        out
    }

    /// Untapped mana sources `p` controls, as (object, mana produced): one
    /// entry per kind of mana a permanent can make. Basic land types give
    /// their mana intrinsically (CR 305.6); other mana abilities are listed
    /// on the card. Both are read from current characteristics, so a land
    /// turned into a Mountain taps for red.
    pub(super) fn mana_sources(&self, p: PlayerId) -> Sources {
        let mut out = Sources::new();
        // Sources with other uses (creatures, permanents with activated
        // abilities such as Mishra's Factory) are listed last so that the
        // canonical payment plan taps plain lands first.
        let mut later = Sources::new();
        let mut any_sacrificed = false;
        for id in self.permanents_of(p) {
            let o = &self.objects[id];
            if o.tapped {
                continue;
            }
            let Some(c) = self.characteristics(id) else {
                continue;
            };
            // CR 302.6: a creature's {T} abilities, mana abilities included,
            // need it to have been controlled since the turn began.
            if c.is_creature() && !o.controlled_since_turn_start && !c.has(Keyword::Haste) {
                continue;
            }
            let other_use =
                c.is_creature() || (self.activated_sources > 0 && self.flags_of(o).has_activated());
            let list = if other_use { &mut later } else { &mut out };
            if c.is_land() {
                for s in &c.subtypes {
                    if let Some(color) = s.basic_color() {
                        list.push((id, Mana::Colored(color)));
                    }
                }
            }
            for a in &c.abilities {
                match self.pool.ability(*a) {
                    AbilityDef::TapForMana(m) => list.push((id, *m)),
                    // Sources that sacrifice themselves go after the rest.
                    AbilityDef::SacForMana(_) => any_sacrificed = true,
                    _ => {}
                }
            }
        }
        out.extend(later);
        if any_sacrificed {
            for id in self.permanents_of(p) {
                let o = &self.objects[id];
                if o.tapped {
                    continue;
                }
                let Some(c) = self.characteristics(id) else {
                    continue;
                };
                if c.is_creature() && !o.controlled_since_turn_start && !c.has(Keyword::Haste) {
                    continue;
                }
                for a in &c.abilities {
                    if let AbilityDef::SacForMana(m) = self.pool.ability(*a) {
                        out.push((id, *m));
                    }
                }
            }
        }
        out
    }

    /// Whether `id` makes mana by sacrificing itself (and has no plain
    /// `{T}` mana ability the plan could have used instead). Read from the
    /// printed definition: no effect grants a sacrifice mana ability.
    fn sacrifices_for_mana(&self, id: ObjId) -> bool {
        let def = self.pool.get(self.objects[id].def);
        let mut sac = false;
        for a in &def.ability_ids {
            match self.pool.ability(*a) {
                AbilityDef::TapForMana(_) => return false,
                AbilityDef::SacForMana(_) => sac = true,
                _ => {}
            }
        }
        sac
    }

    /// A canonical payment plan for `p`; see `payment_plan_from`.
    pub(super) fn payment_plan(&self, p: PlayerId, req: &ManaRequirement) -> Option<Sources> {
        let ps = self.player(p);
        payment_plan_from(&self.mana_sources(p), &ps.mana_pool, req, ps.life)
    }

    /// CR 601.2g–h: activate mana abilities per the plan, then pay from the
    /// pool.
    /// The non-mana parts of a cost (CR 118.3): life, a sacrifice ("~" is
    /// `this`), a discard. The choices were made as the spell or ability was
    /// announced.
    fn pay_other_costs(
        &mut self,
        p: PlayerId,
        cost: &Cost,
        this: ObjId,
        sacrifice: Option<ObjId>,
        discard: Option<ObjId>,
    ) {
        if cost.life > 0 {
            self.change_life(p, -i32::try_from(cost.life).unwrap_or(i32::MAX));
        }
        if let Some((k, n)) = cost.remove_counters {
            self.remove_counters(this, k, n);
        }
        if cost.energy > 0 {
            let e = &mut self.player_mut(p).energy;
            *e = e.saturating_sub(cost.energy);
        }
        // CR 606.4: loyalty costs are counters put on or removed from the
        // planeswalker.
        if cost.loyalty > 0 {
            self.add_counters(this, CounterKind::Loyalty, cost.loyalty.unsigned_abs());
        } else if cost.loyalty < 0 {
            self.remove_counters(this, CounterKind::Loyalty, cost.loyalty.unsigned_abs());
        }
        match cost.sacrifice {
            Some(SacCost::This) => self.sacrifice(this),
            Some(SacCost::Filter(_)) => {
                if let Some(s) = sacrifice {
                    match cost.chosen {
                        crate::card::ChosenCost::Sacrifice => self.sacrifice(s),
                        crate::card::ChosenCost::Return => {
                            self.move_object(s, Zone::Hand, false);
                        }
                        crate::card::ChosenCost::Tap => self.tap(s),
                    }
                }
            }
            None => {}
        }
        if cost.discard > 0
            && let Some(c) = discard
        {
            self.discard_card(p, c);
            // "Discard two cards": the chosen card, then the rest in hand
            // order (the engine picks them).
            for _ in 1..cost.discard {
                let next = self.player(p).hand.iter().copied().find(|id| {
                    cost.discard_filter.as_ref().is_none_or(|f| {
                        let o = &self.objects[*id];
                        let ch = self.base_characteristics(o);
                        f.matches(*id, o.owner, &ch, None, Some(p))
                    })
                });
                let Some(n) = next else { break };
                self.discard_card(p, n);
            }
        }
    }

    pub(super) fn pay(&mut self, p: PlayerId, req: &ManaRequirement) {
        let sources = self.mana_sources(p);
        self.pay_from(p, req, &sources);
    }

    /// `pay` from these sources (a spell's, with convoke or improvise).
    fn pay_from(&mut self, p: PlayerId, req: &ManaRequirement, sources: &Sources) {
        let ps = self.player(p);
        let (plan, req, life) = resolve_payment(sources, &ps.mana_pool, req, ps.life)
            .expect("cast was offered only when payable");
        for (id, mana) in plan {
            // Delve's cards are exiled; everything else is tapped.
            if self
                .objects
                .get(id)
                .is_some_and(|o| o.zone == Zone::Graveyard)
            {
                self.move_object(id, Zone::Exile, true);
                self.player_mut(p).mana_pool.add(mana);
                continue;
            }
            self.tap(id);
            self.player_mut(p).mana_pool.add(mana);
            if self.sacrifices_for_mana(id) {
                self.sacrifice(id);
            }
        }
        let pool = self.player(p).mana_pool;
        let left = pool
            .payment_plan(&req)
            .expect("pool covers the cost after activating mana abilities");
        self.player_mut(p).mana_pool = left;
        // CR 107.4f: Phyrexian symbols the mana could not cover cost 2 life.
        if life > 0 {
            self.change_life(p, -i32::try_from(life).unwrap_or(i32::MAX));
        }
        self.bump();
    }

    /// CR 601.2f–i: pay and finish casting; caster gets priority.
    pub(super) fn finish_cast(
        &mut self,
        spell: ObjId,
        sacrifice: Option<ObjId>,
        discard: Option<ObjId>,
    ) {
        let obj = &self.objects[spell];
        let (caster, def_ref) = (obj.controller, obj.def);
        let x = self.x_of(spell);
        let flashback = obj.cast.flashback();
        let targets: Vec<Target> = obj.targets.iter().copied().collect();
        let pool = std::sync::Arc::clone(&self.pool);
        let def = pool.get(def_ref);
        let req = self.spell_requirements(spell, x);
        let sources = self.spell_sources(spell);
        self.pay_from(caster, &req, &sources);
        if let (Some(fb), true) = (&def.flashback, flashback) {
            self.pay_other_costs(caster, fb, spell, None, None);
        }
        // CR 702.138a: exile the other cards escape asks for (the oldest).
        if self.objects[spell].alt == AltCast::Escape
            && let Some(&(_, n)) = def.escape.as_deref()
        {
            let gone: Vec<ObjId> = self
                .player(caster)
                .graveyard
                .iter()
                .copied()
                .take(n as usize)
                .collect();
            for id in gone {
                self.move_object(id, Zone::Exile, true);
            }
        }
        self.pay_other_costs(caster, &def.additional, spell, sacrifice, discard);
        let n = &mut self.spells_this_turn[caster.index().min(3)];
        *n = n.saturating_add(1);
        self.emit(Event::Cast {
            player: caster,
            spell,
            def: def_ref,
            targets,
        });
        // CR 702.85a: cascade triggers as the spell is cast.
        if def.keywords.contains(Keyword::Cascade) {
            let ability = self.pool.builtin(crate::card::Builtin::Cascade);
            self.queue_builtin(caster, ability, spell, def_ref);
        }
        self.emit_targeted(spell);
        self.give_priority(caster);
    }

    // ----- resolution -------------------------------------------------------------

    /// CR 701.5: a countered spell goes to its owner's graveyard.
    pub(super) fn counter_spell(&mut self, id: ObjId) {
        let Some(o) = self.objects.get(id) else {
            return;
        };
        if o.zone != Zone::Stack || o.kind == ObjKind::Ability {
            return;
        }
        let def = o.def;
        self.emit(Event::Countered { spell: id, def });
        self.leave_stack(id);
    }

    /// Scry (CR 701.18) or surveil (CR 701.42) `n`: the player chooses
    /// which of the top `n` cards go down (to the bottom, or the graveyard),
    /// in their order; the rest stay on top in their order.
    pub(super) fn scry_or_surveil(&mut self, p: PlayerId, n: u32, graveyard: bool) {
        let lib = &self.player(p).library;
        let n = (n as usize).min(lib.len()).min(4);
        if n == 0 {
            return;
        }
        // Top of the library is the end of the vector.
        let top: Vec<ObjId> = lib[lib.len() - n..].iter().rev().copied().collect();
        let mut options: Vec<Choice> = Vec::new();
        for mask in 0..(1u32 << n) {
            let chosen: Vec<ObjId> = top
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, id)| *id)
                .collect();
            options.push(if graveyard {
                Choice::SurveilGraveyard(chosen)
            } else {
                Choice::ScryBottom(chosen)
            });
        }
        let kind = if graveyard {
            DecisionKind::Surveil {
                count: u32::try_from(n).unwrap_or(0),
            }
        } else {
            DecisionKind::Scry {
                count: u32::try_from(n).unwrap_or(0),
            }
        };
        let k = self.choose_mid(p, kind, options.clone());
        let down = match &options[k] {
            Choice::ScryBottom(ids) | Choice::SurveilGraveyard(ids) => ids.clone(),
            _ => Vec::new(),
        };
        for id in &down {
            if graveyard {
                self.move_object(*id, Zone::Graveyard, true);
            } else {
                let lib = &mut self.player_mut(p).library;
                lib.retain(|x| x != id);
                lib.insert(0, *id);
            }
        }
        self.emit(Event::Scried {
            player: p,
            looked: u32::try_from(n).unwrap_or(0),
            down: u32::try_from(down.len()).unwrap_or(0),
        });
        self.bump();
    }

    /// CR 608: resolve the top object of the stack.
    pub(super) fn resolve_top(&mut self) {
        let Some(spell) = self.stack.last().copied() else {
            return;
        };
        let obj = self.objects[spell].clone();
        if obj.kind == ObjKind::Ability {
            self.resolve_ability(spell);
            return;
        }
        let pool = std::sync::Arc::clone(&self.pool);
        let def = pool.get(obj.def);
        if let Some(spec) = def.enchant() {
            // CR 303.4f, 608.2b: an Aura spell whose target is illegal is
            // countered; otherwise it enters attached to the target.
            let target = obj.targets.first().copied();
            let legal = target
                .is_some_and(|t| self.is_legal_target_for(obj.controller, spec, t, Some(spell)));
            if !legal {
                self.emit(Event::Fizzle {
                    spell,
                    def: obj.def,
                });
                self.leave_stack(spell);
                return;
            }
            self.emit(Event::Resolve {
                spell,
                def: obj.def,
            });
            let new = self.put_onto_battlefield(spell, obj.controller);
            if let Some(Target::Object(t)) = target
                && self.objects.contains_key(new)
            {
                self.objects[new].attached_to = Some(t);
                self.emit(Event::Attached { obj: new, to: t });
            }
            self.cbump();
            return;
        }
        let parts: Option<(&[TargetSpec], &[Effect])> =
            (def.spell_ability().is_some() || def.modes().is_some()).then(|| {
                (
                    crate::state::targets_in(&pool, &obj),
                    crate::state::effects_in(&pool, &obj),
                )
            });
        if let Some((specs, effects)) = parts {
            // CR 608.2b: targets that are no longer legal are ignored; if all
            // are illegal the spell does not resolve.
            let legal: Vec<bool> = specs
                .iter()
                .zip(obj.targets.iter())
                .map(|(spec, t)| self.is_legal_target_for(obj.controller, *spec, *t, Some(spell)))
                .collect();
            // A spell whose optional targets were all declined has none.
            if !legal.is_empty() && legal.iter().all(|l| !l) {
                self.emit(Event::Fizzle {
                    spell,
                    def: obj.def,
                });
                self.leave_stack(spell);
                return;
            }
            self.emit(Event::Resolve {
                spell,
                def: obj.def,
            });
            let mut ctx = EffectCtx {
                source: spell,
                controller: obj.controller,
                targets: obj.targets.to_vec(),
                legal,
                trigger_obj: None,
                trigger_player: None,
                trigger_amount: 0,
                created: None,
                x: self.x_of(spell),
                did: true,
            };
            let split = self.mode_split_of(spell);
            self.apply_effects(&mut ctx, effects, split);
        } else {
            self.emit(Event::Resolve {
                spell,
                def: obj.def,
            });
        }
        if let Some(o) = self.objects.get(spell)
            && o.zone == Zone::Stack
        {
            if def.is_permanent_card() {
                self.choose_copy_on_entering(spell);
                self.put_onto_battlefield(spell, obj.controller);
            } else if obj.cast.adventure() {
                self.finish_adventure(spell);
            } else if obj.cast.bought_back() && !obj.cast.flashback() {
                // CR 702.27a: to its owner's hand as it resolves.
                self.move_object(spell, Zone::Hand, false);
            } else if obj.alt == AltCast::Hand && def.keywords.contains(Keyword::Rebound) {
                self.rebound(spell);
            } else {
                self.leave_stack(spell);
            }
        }
    }

    /// CR 702.88a: a rebound spell cast from hand is exiled as it resolves;
    /// at its owner's next upkeep it may be cast free.
    fn rebound(&mut self, spell: ObjId) {
        let controller = self.objects[spell].controller;
        let new = self.move_object(spell, Zone::Exile, true);
        let Some(o) = self.objects.get_mut(new) else {
            return;
        };
        o.alt = AltCast::Rebounding;
        let def = o.def;
        let ability = self.pool.builtin(crate::card::Builtin::Rebound);
        self.delayed_triggers.push(crate::state::DelayedTrigger {
            controller,
            ability: crate::state::StackAbility {
                ability,
                source: new,
                def,
                trigger_obj: None,
                trigger_player: None,
                trigger_amount: 0,
            },
        });
    }

    /// A resolved adventure goes on an adventure in exile (CR 715.4); a
    /// resolved omen is shuffled into its owner's library.
    fn finish_adventure(&mut self, spell: ObjId) {
        let o = &self.objects[spell];
        let owner = o.owner;
        if self.pool.get(o.card).layout == Layout::Omen {
            self.move_object(spell, Zone::Library, true);
            self.shuffle_library(owner);
            return;
        }
        let new = self.move_object(spell, Zone::Exile, true);
        self.objects[new].cast.set_adventure(true);
        self.player_mut(owner).adventures_in_exile += 1;
    }

    /// CR 707.?: "you may have ~ enter as a copy of ...". The controller
    /// chooses among the matching permanents (or nothing) as the spell
    /// resolves; the copy effect is set on the spell so that it enters with
    /// the copied values (and the copied card's "enters" effects, CR
    /// 614.12).
    fn choose_copy_on_entering(&mut self, spell: ObjId) {
        let o = &self.objects[spell];
        let (controller, card) = (o.controller, o.card);
        let Some(filter) = self.pool.get(card).enters_as_copy().cloned() else {
            return;
        };
        let candidates: Vec<ObjId> = self
            .battlefield
            .iter()
            .copied()
            .filter(|id| {
                let t = &self.objects[*id];
                self.characteristics(*id).is_some_and(|c| {
                    filter.matches(*id, t.controller, &c, Some(spell), Some(controller))
                }) && self.status_ok(&filter, *id)
            })
            .collect();
        if candidates.is_empty() {
            return;
        }
        let mut options: Vec<Choice> = candidates
            .iter()
            .map(|id| Choice::CopyOf {
                obj: *id,
                def: self.objects[*id].def,
            })
            .collect();
        options.push(Choice::NoCopy);
        let k = self.choose_mid(controller, DecisionKind::ChooseCopy, options);
        let Some(of) = candidates.get(k).copied() else {
            return;
        };
        let (def, except) = {
            let t = &self.objects[of];
            (t.def, t.except)
        };
        // The copying card's own exceptions (Phantasmal Image) replace
        // those of what it copies.
        let except = if self.pool.get(card).copy_except.is_some() {
            Some(card)
        } else {
            except
        };
        let s = &mut self.objects[spell];
        s.def = def;
        s.except = except;
        self.bump();
        self.emit(Event::BecameCopy {
            obj: spell,
            of: def,
        });
    }

    /// Who or what a `Recipient` denotes while `ctx`'s effects resolve.
    #[allow(clippy::too_many_lines)] // one arm per recipient kind
    fn recipients(&self, ctx: &EffectCtx, r: Recipient) -> SmallVec<[Target; 2]> {
        let mut out = SmallVec::new();
        match r {
            Recipient::Target(i) => {
                if ctx.legal.get(i).copied().unwrap_or(false)
                    && let Some(t) = ctx.targets.get(i)
                {
                    out.push(*t);
                }
            }
            Recipient::Targets(first, count) => {
                for i in usize::from(first)..usize::from(first) + usize::from(count) {
                    if ctx.legal.get(i).copied().unwrap_or(false)
                        && let Some(t) = ctx.targets.get(i)
                    {
                        out.push(*t);
                    }
                }
            }
            Recipient::ControllerOf(i) | Recipient::OwnerOf(i) => {
                if let Some(Target::Object(id)) = ctx.targets.get(i) {
                    let owner = matches!(r, Recipient::OwnerOf(_));
                    let p = self
                        .objects
                        .get(*id)
                        .map(|o| if owner { o.owner } else { o.controller })
                        .or_else(|| {
                            self.lki.get(id).map(|l| {
                                if owner {
                                    l.object.owner
                                } else {
                                    l.object.controller
                                }
                            })
                        });
                    if let Some(p) = p {
                        out.push(Target::Player(p));
                    }
                }
            }
            Recipient::You => out.push(Target::Player(ctx.controller)),
            Recipient::EachOpponent => {
                out.extend(
                    self.players_in_game()
                        .filter(|p| *p != ctx.controller)
                        .map(Target::Player),
                );
            }
            Recipient::Source => {
                if let Some(id) = self.current_object(ctx.source) {
                    out.push(Target::Object(id));
                }
            }
            Recipient::TriggeringObject => {
                if let Some(id) = ctx.trigger_obj.and_then(|o| self.current_object(o)) {
                    out.push(Target::Object(id));
                }
            }
            Recipient::TriggeringPlayer => {
                if let Some(p) = ctx.trigger_player {
                    out.push(Target::Player(p));
                }
            }
            Recipient::DefendingPlayer => {
                let attacked = ctx
                    .trigger_obj
                    .and_then(|a| self.combat.as_ref()?.attacker(a).map(|at| at.attacking));
                // If the attacker already left combat, the player it was
                // attacking is the only opponent in a two-player game.
                out.push(attacked.unwrap_or(Target::Player(self.opponent(ctx.controller))));
            }
            Recipient::EachCreature => {
                out.extend(
                    self.battlefield
                        .iter()
                        .copied()
                        .filter(|id| self.is_creature_on_battlefield(*id))
                        .map(Target::Object),
                );
            }
            Recipient::EachPlayer => out.extend(self.players_in_game().map(Target::Player)),
            Recipient::EachCreatureYouControl => {
                out.extend(self.creatures_of(ctx.controller).map(Target::Object));
            }
            Recipient::EachPermanentYouControl => {
                out.extend(self.permanents_of(ctx.controller).map(Target::Object));
            }
            Recipient::SameObject => {
                if let Some(id) = ctx.trigger_obj.filter(|id| self.objects.contains_key(*id)) {
                    out.push(Target::Object(id));
                }
            }
            Recipient::CreatedObject => {
                if let Some(id) = ctx.created.filter(|id| self.objects.contains_key(*id)) {
                    out.push(Target::Object(id));
                }
            }
            Recipient::Attached => {
                if let Some(t) = self
                    .current_object(ctx.source)
                    .and_then(|s| self.objects[s].attached_to)
                    .filter(|t| self.objects.contains_key(*t))
                {
                    out.push(Target::Object(t));
                }
            }
            Recipient::Group(f) => {
                let filter = self.pool.filter(f);
                let source = self.current_object(ctx.source);
                out.extend(
                    self.battlefield
                        .iter()
                        .copied()
                        .filter(|id| {
                            let o = &self.objects[*id];
                            self.characteristics(*id).is_some_and(|c| {
                                filter.matches(*id, o.controller, &c, source, Some(ctx.controller))
                            }) && self.status_ok(filter, *id)
                        })
                        .map(Target::Object),
                );
            }
        }
        out
    }

    /// Store an "until end of turn" effect on one object (CR 611.2a, 611.2c).
    fn effect_until_end_of_turn(
        &mut self,
        ctx: &EffectCtx,
        id: ObjId,
        parts: SmallVec<[Modification; 2]>,
    ) {
        let timestamp = self.tick();
        self.effects.push(ContinuousEffect {
            source: Some(ctx.source),
            controller: ctx.controller,
            timestamp,
            duration: Duration::UntilEndOfTurn,
            affected: Affected::Objects(smallvec::smallvec![id]),
            parts,
        });
        self.cbump();
    }

    #[allow(clippy::too_many_lines)] // one arm per effect kind
    pub(super) fn apply_effect(&mut self, ctx: &mut EffectCtx, effect: &Effect) {
        let on_battlefield = |st: &GameState, t: Target| match t {
            Target::Object(id) => st
                .objects
                .get(id)
                .is_some_and(|o| o.zone == Zone::Battlefield)
                .then_some(id),
            Target::Player(_) => None,
        };
        match effect {
            Effect::DealDamage { amount, to } => {
                for t in self.recipients(ctx, *to) {
                    self.deal_damage(ctx.source, t, self.eval_num(*amount, ctx), false);
                }
            }
            Effect::Pump {
                power,
                toughness,
                to,
            } => {
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    self.effect_until_end_of_turn(
                        ctx,
                        id,
                        smallvec::smallvec![Modification::ModifyPt {
                            power: *power,
                            toughness: *toughness,
                        }],
                    );
                }
            }
            Effect::Destroy { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.destroy(id);
                    }
                }
            }
            Effect::GainLife { amount, who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.gain_life(p, self.eval_num(*amount, ctx));
                    }
                }
            }
            Effect::LoseLife { amount, who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.change_life(
                            p,
                            -i32::try_from(self.eval_num(*amount, ctx)).unwrap_or(i32::MAX),
                        );
                    }
                }
            }
            Effect::DrawCards { count, who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        for _ in 0..self.eval_num(*count, ctx) {
                            self.draw(p);
                        }
                    }
                }
            }
            Effect::AddCounters { kind, count, to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.add_counters(id, *kind, self.eval_num(*count, ctx));
                    }
                }
            }
            Effect::ReturnToHand { what } => {
                for t in self.recipients(ctx, *what) {
                    let Target::Object(id) = t else {
                        continue;
                    };
                    let in_zone = self
                        .objects
                        .get(id)
                        .is_some_and(|o| matches!(o.zone, Zone::Battlefield | Zone::Graveyard));
                    if in_zone {
                        self.move_object(id, Zone::Hand, false);
                    }
                }
            }
            Effect::RemoveCounters { kind, count, to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.remove_counters(id, *kind, self.eval_num(*count, ctx));
                    }
                }
            }
            Effect::RemoveAllCounters { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.remove_all_counters(id);
                    }
                }
            }
            Effect::Regenerate { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.regenerate(ctx, id);
                    }
                }
            }
            Effect::DestroyNoRegen { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.destroy_with(id, false);
                    }
                }
            }
            Effect::Mill { count, who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.mill(p, self.eval_num(*count, ctx));
                    }
                }
            }
            Effect::CreateEmblem { emblem } => {
                self.create_emblem(*emblem, ctx.controller);
            }
            Effect::Counter { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Target::Object(id) = t {
                        self.counter_spell(id);
                    }
                }
            }
            Effect::CounterUnlessPays { to, mana } => {
                for t in self.recipients(ctx, *to) {
                    let Target::Object(id) = t else {
                        continue;
                    };
                    let Some(o) = self.objects.get(id) else {
                        continue;
                    };
                    if o.zone != Zone::Stack {
                        continue;
                    }
                    let (ctl, def) = (o.controller, o.def);
                    let req = mana.requirements(0);
                    let can_pay = self.payment_plan(ctl, &req).is_some();
                    let pays = can_pay
                        && self.choose_mid(
                            ctl,
                            DecisionKind::PayOrCounter { def },
                            vec![Choice::Accept, Choice::Decline],
                        ) == 0;
                    if pays {
                        self.pay(ctl, &req);
                    } else {
                        self.counter_spell(id);
                    }
                }
            }
            Effect::Scry { count } => {
                self.scry_or_surveil(ctx.controller, self.eval_num(*count, ctx), false);
            }
            Effect::Search { filter, count, to } => {
                let p = ctx.controller;
                let mut found = Vec::new();
                for _ in 0..self.eval_num(*count, ctx) {
                    // One option per distinct card (copies are alike).
                    let mut seen = Vec::new();
                    let mut options = Vec::new();
                    for id in self.player(p).library.iter().rev() {
                        if found.contains(id) {
                            continue;
                        }
                        let o = &self.objects[*id];
                        let c = self.base_characteristics(o);
                        if filter.matches(*id, p, &c, Some(ctx.source), Some(p))
                            && self.status_ok(filter, *id)
                            && !seen.contains(&o.def)
                        {
                            seen.push(o.def);
                            options.push(Choice::Target(Target::Object(*id)));
                        }
                    }
                    if options.is_empty() {
                        break;
                    }
                    options.push(Choice::Decline);
                    let k = self.choose_mid(p, DecisionKind::SearchLibrary, options.clone());
                    match options[k] {
                        Choice::Target(Target::Object(id)) => found.push(id),
                        _ => break,
                    }
                }
                let mut top = Vec::new();
                for id in found {
                    match to {
                        SearchTo::Hand => {
                            self.move_object(id, Zone::Hand, true);
                        }
                        SearchTo::Battlefield { tapped } => {
                            let new = self.put_onto_battlefield_with(
                                id,
                                p,
                                super::replace::Enter {
                                    tapped: *tapped,
                                    counters: None,
                                },
                            );
                            ctx.created = Some(new);
                        }
                        SearchTo::TopOfLibrary => top.push(id),
                    }
                }
                self.shuffle_library(p);
                for id in top {
                    let lib = &mut self.player_mut(p).library;
                    lib.retain(|x| *x != id);
                    lib.push(id);
                }
            }
            Effect::Transform { what } => {
                for t in self.recipients(ctx, *what) {
                    let Target::Object(id) = t else {
                        continue;
                    };
                    let Some(o) = self.objects.get(id) else {
                        continue;
                    };
                    if o.zone != Zone::Battlefield {
                        continue;
                    }
                    let d = self.pool.get(o.def);
                    if d.layout != Layout::Transform {
                        continue;
                    }
                    let Some(other) = d.faces.first().copied() else {
                        continue;
                    };
                    let legendary = d.is_legendary();
                    let flags = self.flags_of(&self.objects[id]);
                    self.count_sources(flags, legendary, false);
                    self.objects[id].def = other;
                    let flags = self.flags_of(&self.objects[id]);
                    self.count_sources(flags, self.pool.get(other).is_legendary(), true);
                    self.emit(Event::Transformed { obj: id });
                    self.cbump();
                }
            }
            Effect::AddMana(mana) => {
                let p = ctx.controller;
                for m in mana {
                    let m = if *m == Mana::Any {
                        let options: Vec<Choice> =
                            Color::ALL.into_iter().map(Choice::Color).collect();
                        let k = self.choose_mid(p, DecisionKind::ChooseColor, options);
                        Mana::Colored(Color::ALL[k.min(4)])
                    } else {
                        *m
                    };
                    self.player_mut(p).mana_pool.add(m);
                }
                self.bump();
            }
            Effect::AddManaOneOf(mana) => {
                let p = ctx.controller;
                let options: Vec<Choice> = mana
                    .iter()
                    .filter_map(|m| match m {
                        Mana::Colored(c) => Some(Choice::Color(*c)),
                        _ => None,
                    })
                    .collect();
                let m = if options.len() == mana.len() && !options.is_empty() {
                    let k = self.choose_mid(p, DecisionKind::ChooseColor, options.clone());
                    match options[k.min(options.len() - 1)] {
                        Choice::Color(c) => Mana::Colored(c),
                        _ => unreachable!("color options"),
                    }
                } else {
                    mana.first().copied().unwrap_or(Mana::Colorless)
                };
                self.player_mut(p).mana_pool.add(m);
                self.bump();
            }
            Effect::AddManaTimes { mana, count } => {
                let p = ctx.controller;
                let n = self.eval_num(*count, ctx);
                if n > 0 {
                    let m = if *mana == Mana::Any {
                        let options: Vec<Choice> =
                            Color::ALL.into_iter().map(Choice::Color).collect();
                        let k = self.choose_mid(p, DecisionKind::ChooseColor, options);
                        Mana::Colored(Color::ALL[k.min(4)])
                    } else {
                        *mana
                    };
                    for _ in 0..n.min(1000) {
                        self.player_mut(p).mana_pool.add(m);
                    }
                    self.bump();
                }
            }
            Effect::Surveil { count } => {
                self.scry_or_surveil(ctx.controller, self.eval_num(*count, ctx), true);
            }
            Effect::Discard { count, who } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    if self.player(p).hand.is_empty() {
                        ctx.did = false;
                    }
                    for _ in 0..self.eval_num(*count, ctx) {
                        let options: Vec<Choice> = self
                            .player(p)
                            .hand
                            .iter()
                            .map(|id| Choice::Discard {
                                card: *id,
                                def: self.objects[*id].def,
                            })
                            .collect();
                        if options.is_empty() {
                            break;
                        }
                        let k = self.choose_mid(p, DecisionKind::Discard, options.clone());
                        if let Choice::Discard { card, .. } = options[k] {
                            self.discard_card(p, card);
                        }
                    }
                }
            }
            Effect::Fight { a, b } => {
                let x = self.recipients(ctx, *a).first().copied();
                let y = self.recipients(ctx, *b).first().copied();
                if let (Some(x), Some(y)) = (
                    x.and_then(|t| on_battlefield(self, t)),
                    y.and_then(|t| on_battlefield(self, t)),
                ) && x != y
                {
                    let px = self.characteristics(x).map_or(0, |c| c.power.max(0));
                    let py = self.characteristics(y).map_or(0, |c| c.power.max(0));
                    self.deal_damage(x, Target::Object(y), u32::try_from(px).unwrap_or(0), false);
                    self.deal_damage(y, Target::Object(x), u32::try_from(py).unwrap_or(0), false);
                }
            }
            Effect::Bite { source, to } => {
                // CR 608.2b: an illegal target is unaffected. If the
                // creature is no longer a legal target it deals no damage;
                // if the recipient is illegal there is nothing to damage.
                let x = self
                    .recipients(ctx, *source)
                    .first()
                    .copied()
                    .and_then(|t| on_battlefield(self, t));
                let y = self.recipients(ctx, *to).first().copied();
                if let (Some(x), Some(y)) = (x, y) {
                    let p = self.characteristics(x).map_or(0, |c| c.power.max(0));
                    self.deal_damage(x, y, u32::try_from(p).unwrap_or(0), false);
                }
            }
            Effect::ExileAllGraveyards => {
                let cards: Vec<ObjId> = self
                    .players
                    .iter()
                    .flat_map(|p| p.graveyard.iter().copied())
                    .collect();
                for id in cards {
                    self.move_object(id, Zone::Exile, true);
                }
            }
            Effect::CreateReplacement(r) => self.create_replacement(ctx, r),
            Effect::Rule(kind) => {
                self.rules.push(RuleEffect {
                    kind: *kind,
                    duration: Duration::UntilEndOfTurn,
                });
                self.bump();
            }
            Effect::Tap { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.tap(id);
                    }
                }
            }
            Effect::TapOrUntap { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        // Accept taps, Decline untaps; asked as the source's
                        // choice.
                        let def = self
                            .objects
                            .get(ctx.source)
                            .map(|o| o.def)
                            .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
                            .unwrap_or(self.objects[id].def);
                        let k = self.choose_mid(
                            ctx.controller,
                            DecisionKind::YouMay { def },
                            vec![Choice::Accept, Choice::Decline],
                        );
                        if k == 0 {
                            self.tap(id);
                        } else {
                            self.untap(id);
                        }
                    }
                }
            }
            Effect::Untap { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.untap(id);
                    }
                }
            }
            Effect::Grant { keywords, to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::AddKeywords(*keywords)],
                        );
                    }
                }
            }
            Effect::GainControl { of } => {
                for t in self.recipients(ctx, *of) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::Control],
                        );
                        self.control_effects += 1;
                    }
                }
                // Later effects of the same spell ("untap it") see the new
                // controller.
                self.sync_control();
            }
            Effect::PumpVar {
                power,
                toughness,
                times,
                to,
            } => {
                let n = i32::try_from(self.eval_num(*times, ctx)).unwrap_or(i32::MAX);
                let (p, t) = (power.saturating_mul(n), toughness.saturating_mul(n));
                for r in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, r) {
                        self.effect_until_end_of_turn(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::ModifyPt {
                                power: p,
                                toughness: t
                            }],
                        );
                    }
                }
            }
            Effect::GrantAbility { ability, to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.eot_grants += 1;
                        self.effect_until_end_of_turn(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::AddAbility(*ability)],
                        );
                    }
                }
            }
            Effect::SkipNextUntap { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.objects[id].skip_untap = true;
                    }
                }
            }
            Effect::SwitchPt { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::SwitchPt],
                        );
                    }
                }
            }
            Effect::PutFromHand { filter, tapped } => {
                let p = ctx.controller;
                let mut options: Vec<Choice> = self
                    .player(p)
                    .hand
                    .iter()
                    .filter(|id| {
                        let o = &self.objects[**id];
                        let c = self.base_characteristics(o);
                        filter.matches(**id, p, &c, None, Some(p))
                    })
                    .map(|id| Choice::Discard {
                        card: *id,
                        def: self.objects[*id].def,
                    })
                    .collect();
                if !options.is_empty() {
                    options.push(Choice::Done);
                    let k = self.choose_mid(p, DecisionKind::PutFromHand, options.clone());
                    if let Some(Choice::Discard { card, .. }) = options.get(k) {
                        let card = *card;
                        let new = self.put_onto_battlefield(card, p);
                        if *tapped && self.objects.contains_key(new) {
                            self.tap(new);
                        }
                        ctx.created = Some(new);
                    }
                }
            }
            Effect::Strip { keywords, to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::RemoveKeywords(*keywords)],
                        );
                    }
                }
            }
            Effect::PutInLibrary { what, top } => {
                for t in self.recipients(ctx, *what) {
                    let Target::Object(id) = t else {
                        continue;
                    };
                    if !self.objects.get(id).is_some_and(|o| {
                        matches!(o.zone, Zone::Battlefield | Zone::Graveyard | Zone::Stack)
                    }) {
                        continue;
                    }
                    self.move_object(id, Zone::Library, *top);
                }
            }
            Effect::SacrificeChosen { who, filter, count }
            | Effect::ReturnChosen { who, filter, count } => {
                let bounce = matches!(effect, Effect::ReturnChosen { .. });
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    for _ in 0..self.eval_num(*count, ctx) {
                        let options: Vec<Choice> = self
                            .permanents_of(p)
                            .filter(|id| {
                                self.characteristics(*id).is_some_and(|c| {
                                    filter.matches(*id, p, &c, Some(ctx.source), Some(p))
                                }) && self.status_ok(filter, *id)
                            })
                            .map(Choice::Sacrifice)
                            .collect();
                        if options.is_empty() {
                            break;
                        }
                        let k = self.choose_mid(p, DecisionKind::Edict, options.clone());
                        if let Choice::Sacrifice(id) = options[k] {
                            if bounce {
                                self.move_object(id, Zone::Hand, false);
                            } else {
                                self.sacrifice(id);
                            }
                        }
                    }
                }
            }
            Effect::RevealHand { who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.emit(Event::RevealedHand { player: p });
                    }
                }
            }
            Effect::DiscardRandom { count, who } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    for _ in 0..self.eval_num(*count, ctx) {
                        let n = self.player(p).hand.len();
                        if n == 0 {
                            break;
                        }
                        let i = rand::Rng::random_range(&mut self.rng, 0..n);
                        let card = self.player(p).hand[i];
                        self.discard_card(p, card);
                    }
                }
            }
            Effect::DiscardChosen { who, filter } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    self.emit(Event::RevealedHand { player: p });
                    let options: Vec<Choice> = self
                        .player(p)
                        .hand
                        .iter()
                        .filter(|id| {
                            filter.as_ref().is_none_or(|f| {
                                let c = self.base_characteristics(&self.objects[**id]);
                                f.matches(**id, p, &c, None, None) && self.status_ok(f, **id)
                            })
                        })
                        .map(|id| Choice::Discard {
                            card: *id,
                            def: self.objects[*id].def,
                        })
                        .collect();
                    if options.is_empty() {
                        continue;
                    }
                    let k = self.choose_mid(ctx.controller, DecisionKind::Discard, options.clone());
                    if let Choice::Discard { card, .. } = options[k] {
                        self.discard_card(p, card);
                    }
                }
            }
            Effect::Sacrifice { what } => {
                for t in self.recipients(ctx, *what) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.sacrifice(id);
                    }
                }
            }
            Effect::CreateToken { token, count, who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        for _ in 0..self.eval_num(*count, ctx) {
                            // "then attach this Equipment to it".
                            ctx.created = Some(self.create_token(*token, p));
                        }
                    }
                }
            }
            Effect::Attach { to } => {
                let Some(source) = self
                    .current_object(ctx.source)
                    .filter(|s| self.objects[*s].zone == Zone::Battlefield)
                else {
                    return;
                };
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    // CR 701.3b: an attachment to an illegal object does
                    // nothing.
                    if id == source || !self.is_legal_attachment(TargetSpec::Creature, id) {
                        continue;
                    }
                    self.objects[source].attached_to = Some(id);
                    self.cbump();
                    self.emit(Event::Attached {
                        obj: source,
                        to: id,
                    });
                }
            }
            Effect::Animate {
                to,
                types,
                subtypes,
                power,
                toughness,
                colors,
            } => {
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    let mut parts: SmallVec<[Modification; 2]> =
                        smallvec::smallvec![Modification::AddTypes(*types)];
                    parts.extend(subtypes.iter().map(|s| Modification::AddSubtype(*s)));
                    if let Some(c) = colors {
                        parts.push(Modification::SetColors(*c));
                    }
                    parts.push(Modification::SetPt {
                        power: PtValue::Fixed(*power),
                        toughness: PtValue::Fixed(*toughness),
                    });
                    self.effect_until_end_of_turn(ctx, id, parts);
                }
            }
            Effect::BecomeColor { to, colors } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::SetColors(*colors)],
                        );
                    }
                }
            }
            Effect::BecomeLandType { to, land } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::SetLandType(*land)],
                        );
                    }
                }
            }
            Effect::IfCondition(cond, inner) => {
                let c = self.pool.condition(*cond).clone();
                if self.condition_now(&c, ctx.controller, ctx.source) {
                    for e in inner {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::IfKicked(inner) => {
                if self.was_kicked(ctx.source) {
                    for e in inner {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::SetBasePt {
                to,
                power,
                toughness,
            } => {
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    self.effect_until_end_of_turn(
                        ctx,
                        id,
                        smallvec::smallvec![Modification::SetPt {
                            power: PtValue::Fixed(*power),
                            toughness: PtValue::Fixed(*toughness),
                        }],
                    );
                }
            }
            Effect::YouMay(inner) => {
                let def = self
                    .objects
                    .get(ctx.source)
                    .map(|o| o.def)
                    .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
                    .expect("a resolving spell or its last known information exists");
                let k = self.choose_mid(
                    ctx.controller,
                    DecisionKind::YouMay { def },
                    vec![Choice::Accept, Choice::Decline],
                );
                ctx.did = k == 0;
                if k == 0 {
                    for e in inner {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::Pay { mana, life } => {
                let p = ctx.controller;
                let req = mana.requirements(0);
                let life_ok =
                    *life == 0 || self.player(p).life >= i32::try_from(*life).unwrap_or(i32::MAX);
                if life_ok && self.payment_plan(p, &req).is_some() {
                    self.pay(p, &req);
                    if *life > 0 {
                        self.change_life(p, -i32::try_from(*life).unwrap_or(i32::MAX));
                    }
                } else {
                    ctx.did = false;
                }
            }
            Effect::IfYouDo(inner) => {
                if ctx.did {
                    for e in inner {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::UnlessPays {
                payer,
                mana,
                life,
                effects,
            } => {
                let def = self
                    .objects
                    .get(ctx.source)
                    .map(|o| o.def)
                    .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
                    .expect("a resolving spell or its last known information exists");
                let payer = self
                    .recipients(ctx, *payer)
                    .into_iter()
                    .find_map(|t| match t {
                        Target::Player(p) => Some(p),
                        Target::Object(_) => None,
                    });
                let req = mana.requirements(0);
                let pays = payer.is_some_and(|p| {
                    let life_ok = *life == 0
                        || self.player(p).life >= i32::try_from(*life).unwrap_or(i32::MAX);
                    life_ok
                        && self.payment_plan(p, &req).is_some()
                        && self.choose_mid(
                            p,
                            DecisionKind::PayOrCounter { def },
                            vec![Choice::Accept, Choice::Decline],
                        ) == 0
                });
                if pays && let Some(p) = payer {
                    self.pay(p, &req);
                    if *life > 0 {
                        self.change_life(p, -i32::try_from(*life).unwrap_or(i32::MAX));
                    }
                } else {
                    for e in effects {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::Delayed { ability, about } => {
                let def = self
                    .objects
                    .get(ctx.source)
                    .map(|o| o.def)
                    .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
                    .expect("a resolving spell or its last known information exists");
                for t in self.recipients(ctx, *about) {
                    let Target::Object(id) = t else {
                        continue;
                    };
                    // "That card": the object it became after the effect
                    // that moved it (CR 400.7 exceptions).
                    let Some(id) = self.current_object(id) else {
                        continue;
                    };
                    self.delayed_triggers.push(DelayedTrigger {
                        controller: ctx.controller,
                        ability: StackAbility {
                            ability: *ability,
                            source: ctx.source,
                            def,
                            trigger_obj: Some(id),
                            trigger_player: None,
                            trigger_amount: 0,
                        },
                    });
                    self.emit(Event::DelayedTriggerCreated {
                        source: ctx.source,
                        def,
                        about: id,
                    });
                }
            }
            Effect::Exile { what } => {
                for t in self.recipients(ctx, *what) {
                    // A permanent, or a card in a graveyard ("exile target
                    // card from a graveyard").
                    // "Exile Flood of Recollection.": the resolving spell
                    // itself, instead of its graveyard.
                    let id = on_battlefield(self, t).or_else(|| match t {
                        Target::Object(id)
                            if self.objects.get(id).is_some_and(|o| {
                                o.zone == Zone::Graveyard
                                    || (o.zone == Zone::Stack && id == ctx.source)
                            }) =>
                        {
                            Some(id)
                        }
                        _ => None,
                    });
                    if let Some(id) = id {
                        let new = self.move_object(id, Zone::Exile, true);
                        ctx.created = Some(new);
                    }
                }
            }
            Effect::ReturnToBattlefield { what, counters } => {
                for t in self.recipients(ctx, *what) {
                    let Target::Object(id) = t else {
                        continue;
                    };
                    let Some(o) = self.objects.get(id) else {
                        continue;
                    };
                    if !matches!(o.zone, Zone::Exile | Zone::Graveyard) {
                        continue;
                    }
                    let owner = o.owner;
                    let new = self.put_onto_battlefield_with(
                        id,
                        owner,
                        super::replace::Enter {
                            tapped: false,
                            counters: *counters,
                        },
                    );
                    ctx.created = Some(new);
                }
            }
            Effect::BecomeCopy { of, except } => {
                let Some(source) = self
                    .current_object(ctx.source)
                    .filter(|s| self.objects[*s].zone == Zone::Battlefield)
                else {
                    return;
                };
                let x = except.then_some(self.objects[source].card);
                for t in self.recipients(ctx, *of) {
                    if let Some(id) = on_battlefield(self, t)
                        && id != source
                    {
                        self.become_copy(source, id, x);
                    }
                }
            }
            Effect::AddEnergy { count, who } => {
                let n = self.eval_num(*count, ctx);
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.player_mut(p).energy += n;
                        self.bump();
                    }
                }
            }
            Effect::PayEnergy(n) => {
                let p = ctx.controller;
                if self.player(p).energy >= *n {
                    self.player_mut(p).energy -= n;
                    self.bump();
                } else {
                    ctx.did = false;
                }
            }
            Effect::AddPoison { count, who } => {
                let n = self.eval_num(*count, ctx);
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t
                        && n > 0
                    {
                        self.player_mut(p).poison += n;
                        self.bump();
                        self.sba_touch();
                    }
                }
            }
            Effect::Proliferate => self.proliferate(ctx.controller),
            Effect::Explore { what } => {
                for t in self.recipients(ctx, *what) {
                    if let Target::Object(id) = t {
                        self.explore(ctx, id);
                    }
                }
            }
            Effect::Amass { count, token } => {
                let n = self.eval_num(*count, ctx);
                let p = ctx.controller;
                let army = self.pool.subtype_lookup("Army");
                let mut target = army.and_then(|a| {
                    self.permanents_of(p).find(|id| {
                        self.characteristics(*id)
                            .is_some_and(|c| c.is_creature() && c.has_subtype(a))
                    })
                });
                if target.is_none() {
                    let new = self.create_token(*token, p);
                    if self.objects.contains_key(new) {
                        target = Some(new);
                        ctx.created = Some(new);
                    }
                }
                if let Some(id) = target {
                    self.add_counters(id, CounterKind::PlusOnePlusOne, n);
                }
            }
            // Modes resolve through the chosen mode (`effects_of`); looking
            // at the top cards keeps their order.
            Effect::Modes(_) | Effect::LookAtTop { .. } => {}
            Effect::CastExiled(kind) => {
                let Some(card) = self
                    .current_object(ctx.source)
                    .filter(|c| self.objects[*c].zone == Zone::Exile)
                else {
                    return;
                };
                let alt = match kind {
                    crate::card::ExileCast::Madness => AltCast::Madness,
                    crate::card::ExileCast::Free => AltCast::Free,
                };
                self.objects[card].alt = AltCast::None;
                if !self.offer_exile_cast(card, alt) && *kind == crate::card::ExileCast::Madness {
                    self.move_object(card, Zone::Graveyard, true);
                }
            }
            Effect::SuspendTick => {
                let Some(card) = self
                    .current_object(ctx.source)
                    .filter(|c| self.objects[*c].zone == Zone::Exile)
                else {
                    return;
                };
                let o = &mut self.objects[card];
                if o.alt != AltCast::Suspended {
                    return;
                }
                o.remove_counters(CounterKind::Time, 1);
                if o.counters(CounterKind::Time) == 0 {
                    // CR 702.62a: as the last is removed, it may be cast;
                    // either way it's no longer suspended.
                    o.alt = AltCast::None;
                    self.suspended_count = self.suspended_count.saturating_sub(1);
                    self.offer_exile_cast(card, AltCast::Suspend);
                }
            }
            Effect::Cascade => self.cascade(ctx, None),
            Effect::Discover(n) => self.cascade(ctx, Some(*n)),
            Effect::Exploit => self.exploit(ctx),
            Effect::MoveSourceCounters { to } => {
                let n = self
                    .objects
                    .get(ctx.source)
                    .map(|o| o.counters(CounterKind::PlusOnePlusOne))
                    .or_else(|| {
                        self.lki
                            .get(&ctx.source)
                            .map(|l| l.object.counters(CounterKind::PlusOnePlusOne))
                    })
                    .unwrap_or(0);
                if n > 0 {
                    for t in self.recipients(ctx, *to) {
                        if let Some(id) = on_battlefield(self, t) {
                            self.add_counters(id, CounterKind::PlusOnePlusOne, n);
                        }
                    }
                }
            }
            Effect::CumulativeUpkeep(cost) => {
                let Some(id) = self
                    .current_object(ctx.source)
                    .filter(|s| self.objects[*s].zone == Zone::Battlefield)
                else {
                    return;
                };
                self.add_counters(id, CounterKind::Age, 1);
                let n = self.objects[id].counters(CounterKind::Age);
                let mut req = ManaRequirement::default();
                for _ in 0..n {
                    req.add(&cost.requirements(0));
                }
                let p = ctx.controller;
                let paid = self.payment_plan(p, &req).is_some()
                    && self.choose_mid(
                        p,
                        DecisionKind::YouMay {
                            def: self.objects[id].def,
                        },
                        vec![Choice::Accept, Choice::Decline],
                    ) == 0;
                if paid {
                    self.pay(p, &req);
                } else {
                    self.sacrifice(id);
                }
            }
            Effect::Adapt(n) => {
                if let Some(id) = self.current_object(ctx.source)
                    && self.objects[id].zone == Zone::Battlefield
                    && self.objects[id].counters(CounterKind::PlusOnePlusOne) == 0
                {
                    self.add_counters(id, CounterKind::PlusOnePlusOne, *n);
                }
            }
            Effect::Connive { what } => {
                for t in self.recipients(ctx, *what) {
                    let Target::Object(id) = t else { continue };
                    let p = self.objects[id].controller;
                    self.draw(p);
                    let options: Vec<Choice> = self
                        .player(p)
                        .hand
                        .iter()
                        .map(|c| Choice::Discard {
                            card: *c,
                            def: self.objects[*c].def,
                        })
                        .collect();
                    if options.is_empty() {
                        continue;
                    }
                    let k = self.choose_mid(p, DecisionKind::Discard, options.clone());
                    if let Choice::Discard { card, def } = options[k] {
                        let nonland = !self.pool.get(def).is_land();
                        self.discard_card(p, card);
                        if nonland && self.objects.contains_key(id) {
                            self.add_counters(id, CounterKind::PlusOnePlusOne, 1);
                        }
                    }
                }
            }
            Effect::Dig {
                count,
                filter,
                optional,
                all,
                rest_graveyard,
            } => {
                let p = ctx.controller;
                let n = self.eval_num(*count, ctx) as usize;
                let lib = &self.player(p).library;
                let top: Vec<ObjId> = lib.iter().rev().take(n).copied().collect();
                let matches = |s: &Self, id: ObjId| {
                    filter.as_ref().is_none_or(|f| {
                        let c = s.base_characteristics(&s.objects[id]);
                        f.matches(id, p, &c, Some(ctx.source), Some(p)) && s.status_ok(f, id)
                    })
                };
                let mut taken: Vec<ObjId> = Vec::new();
                if *all {
                    taken = top
                        .iter()
                        .copied()
                        .filter(|id| matches(self, *id))
                        .collect();
                } else {
                    let mut options: Vec<Choice> = top
                        .iter()
                        .copied()
                        .filter(|id| matches(self, *id))
                        .map(|id| Choice::Target(Target::Object(id)))
                        .collect();
                    if !options.is_empty() {
                        if *optional {
                            options.push(Choice::Decline);
                        }
                        let k = self.choose_mid(p, DecisionKind::SearchLibrary, options.clone());
                        if let Choice::Target(Target::Object(id)) = options[k] {
                            taken.push(id);
                        }
                    }
                }
                for id in &taken {
                    self.move_object(*id, Zone::Hand, false);
                }
                for id in top {
                    if taken.contains(&id)
                        || !self
                            .objects
                            .get(id)
                            .is_some_and(|o| o.zone == Zone::Library)
                    {
                        continue;
                    }
                    if *rest_graveyard {
                        self.move_object(id, Zone::Graveyard, false);
                    } else {
                        self.move_object(id, Zone::Library, false);
                    }
                }
            }
            Effect::Manifest => {
                let p = ctx.controller;
                if let Some(top) = self.player(p).library.last().copied() {
                    // Face down before it moves, so nothing sees the card's
                    // own characteristics enter (CR 701.34a).
                    self.objects[top].def = self.pool.face_down(false);
                    let new = self.put_onto_battlefield(top, p);
                    ctx.created = Some(new);
                }
            }
            Effect::ClearEcho => {
                if let Some(id) = self.current_object(ctx.source)
                    && let Some(o) = self.objects.get_mut(id)
                {
                    o.echo_due = false;
                }
            }
            Effect::CreateTokenCopy { of, except } => {
                let x = except.then(|| {
                    self.objects
                        .get(ctx.source)
                        .map(|o| o.card)
                        .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.card))
                        .expect("a resolving spell or its last known information exists")
                });
                for t in self.recipients(ctx, *of) {
                    if let Some(id) = on_battlefield(self, t) {
                        let new = self.create_token_copy(id, ctx.controller, x);
                        ctx.created = Some(new);
                    }
                }
            }
        }
    }

    /// Proliferate (CR 701.27) with the choices a player would make: one
    /// more of each kind of counter on their own permanents (+1/+1,
    /// loyalty), on opponents' permanents with -1/-1 counters, and on
    /// opponents with poison counters.
    fn proliferate(&mut self, p: PlayerId) {
        let ids: Vec<ObjId> = self.battlefield.clone();
        for id in ids {
            let o = &self.objects[id];
            let mine = o.controller == p;
            let kinds: SmallVec<[CounterKind; 2]> = o
                .counters
                .iter()
                .filter(|(k, n)| {
                    *n > 0
                        && match k {
                            CounterKind::MinusOneMinusOne | CounterKind::Stun => !mine,
                            _ => mine,
                        }
                })
                .map(|(k, _)| *k)
                .collect();
            for k in kinds {
                self.add_counters(id, k, 1);
            }
        }
        let opponents: SmallVec<[PlayerId; 2]> =
            self.players_in_game().filter(|q| *q != p).collect();
        for q in opponents {
            if self.player(q).poison > 0 {
                self.player_mut(q).poison += 1;
                self.bump();
                self.sba_touch();
            }
        }
    }

    /// The creature `id` explores (CR 701.44).
    fn explore(&mut self, ctx: &EffectCtx, id: ObjId) {
        let Some(o) = self.objects.get(id) else {
            return;
        };
        let p = o.controller;
        let Some(top) = self.player(p).library.last().copied() else {
            // CR 701.44b: nothing revealed is a nonland card.
            self.add_counters(id, CounterKind::PlusOnePlusOne, 1);
            return;
        };
        let land = self.pool.get(self.objects[top].def).is_land();
        let top_def = self.objects[top].def;
        self.emit(Event::Revealed {
            obj: top,
            def: top_def,
        });
        if land {
            self.move_object(top, Zone::Hand, false);
            return;
        }
        self.add_counters(id, CounterKind::PlusOnePlusOne, 1);
        let def = self.objects[top].def;
        let k = self.choose_mid(
            p,
            DecisionKind::YouMay { def },
            vec![Choice::Accept, Choice::Decline],
        );
        if k == 0
            && self
                .objects
                .get(top)
                .is_some_and(|o| o.zone == Zone::Library)
        {
            self.move_object(top, Zone::Graveyard, false);
        }
        let _ = ctx;
    }

    /// Apply a resolving object's effects in order. For a "both" mode
    /// (CR 700.2), the second mode's effects see their own targets.
    pub(super) fn apply_effects(
        &mut self,
        ctx: &mut EffectCtx,
        effects: &[Effect],
        split: Option<(usize, usize)>,
    ) {
        for (i, effect) in effects.iter().enumerate() {
            match split {
                Some((e, t)) if i >= e => {
                    let mut second = EffectCtx {
                        source: ctx.source,
                        controller: ctx.controller,
                        targets: ctx.targets.get(t..).unwrap_or(&[]).to_vec(),
                        legal: ctx.legal.get(t..).unwrap_or(&[]).to_vec(),
                        trigger_obj: ctx.trigger_obj,
                        trigger_player: ctx.trigger_player,
                        trigger_amount: ctx.trigger_amount,
                        created: ctx.created,
                        x: ctx.x,
                        did: ctx.did,
                    };
                    self.apply_effect(&mut second, effect);
                    ctx.created = second.created;
                    ctx.did = second.did;
                }
                _ => self.apply_effect(ctx, effect),
            }
        }
    }

    /// Store a replacement or prevention effect until end of turn (CR 614.3,
    /// 615.3). One aimed at a spell's target is locked to that target
    /// (CR 611.2c) and is not created if the target became illegal.
    /// A regeneration shield on `id` until end of turn (CR 701.15): a
    /// stored replacement of its destruction, used once.
    fn regenerate(&mut self, ctx: &EffectCtx, id: ObjId) {
        let def = self
            .objects
            .get(ctx.source)
            .map(|o| o.def)
            .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
            .expect("a resolving spell or its last known information exists");
        let timestamp = self.tick();
        self.replacements.push(ReplacementEffect {
            source: Some(ctx.source),
            def,
            controller: ctx.controller,
            timestamp,
            duration: Duration::UntilEndOfTurn,
            ability: crate::card::ReplacementAbility {
                replaces: Replaces::Destroy {
                    to: Affects::Target(0),
                },
                replacement: Replacement::Regenerate,
                then: Vec::new(),
            },
            target: Some(Target::Object(id)),
            remaining: 1,
        });
        self.bump();
    }

    fn create_replacement(&mut self, ctx: &EffectCtx, r: &crate::card::ReplacementAbility) {
        let target = match &r.replaces {
            Replaces::Damage {
                to: Affects::Target(i),
                ..
            }
            | Replaces::Dies {
                to: Affects::Target(i),
            } => {
                if !ctx.legal.get(*i).copied().unwrap_or(false) {
                    return;
                }
                ctx.targets.get(*i).copied()
            }
            _ => None,
        };
        let remaining = match r.replacement {
            Replacement::PreventNext(n) => n,
            _ => 0,
        };
        let def = self
            .objects
            .get(ctx.source)
            .map(|o| o.def)
            .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
            .expect("a resolving spell or its last known information exists");
        let timestamp = self.tick();
        self.replacements.push(ReplacementEffect {
            source: Some(ctx.source),
            def,
            controller: ctx.controller,
            timestamp,
            duration: Duration::UntilEndOfTurn,
            ability: r.clone(),
            target,
            remaining,
        });
        self.bump();
    }
}

/// What a resolving spell or ability knows while its effects apply.
pub(super) struct EffectCtx {
    /// The spell, or the ability's source (by the id it had when it
    /// triggered).
    pub source: ObjId,
    pub controller: PlayerId,
    pub targets: Vec<Target>,
    /// Per target: still legal on resolution (CR 608.2b).
    pub legal: Vec<bool>,
    pub trigger_obj: Option<ObjId>,
    pub trigger_player: Option<PlayerId>,
    /// The triggering event's amount, for "that much".
    pub trigger_amount: u32,
    /// The object the most recent effect of this resolution created or
    /// moved (a token, an exiled card), for "sacrifice it" clauses that
    /// follow (`Recipient::CreatedObject`).
    pub created: Option<ObjId>,
    /// X of the resolving spell (CR 107.3); 0 for abilities.
    pub x: u32,
    /// Whether the last optional action ("you may pay {1}") happened, for
    /// "If you do"; true until one is declined, so "if you do" after a
    /// mandatory action ("Sacrifice this creature. If you do, ...")
    /// follows it.
    pub did: bool,
}

/// A canonical payment plan: which sources to tap, and for what, so that
/// `pool` plus their mana covers `req`. With single-color sources a greedy
/// assignment (colored symbols first, then generic from whatever is left) is
/// exact; multi-type lands are assigned to the first color that needs them.
/// Mana sources, inline for typical board sizes.
pub(super) type Sources = SmallVec<[(ObjId, Mana); 12]>;

/// {3}: what a face-down spell costs (CR 702.37c).
fn three() -> ManaRequirement {
    ManaRequirement {
        generic: 3,
        ..ManaRequirement::default()
    }
}

/// Apply a cost change to the generic part (CR 601.2f): a reduction
/// takes off generic mana only, and never below zero.
fn adjust_generic(req: &mut ManaRequirement, delta: i32) {
    let g = i64::from(req.generic) + i64::from(delta);
    req.generic = u32::try_from(g.max(0)).unwrap_or(u32::MAX);
}

fn payment_plan_from(
    sources: &[(ObjId, Mana)],
    pool: &crate::mana::ManaPool,
    req: &ManaRequirement,
    life: i32,
) -> Option<Sources> {
    if req.is_plain() {
        return plain_payment_plan(sources, pool, req);
    }
    resolve_payment(sources, pool, req, life).map(|(plan, _, _)| plan)
}

/// The first expansion of `req` (hybrid and Phyrexian symbols resolved,
/// see `ManaRequirement::expansions`) that `sources` and `pool` can pay
/// with life to spare (CR 119.4): the plan, the plain requirement, and the
/// life to pay.
pub(super) fn resolve_payment(
    sources: &[(ObjId, Mana)],
    pool: &crate::mana::ManaPool,
    req: &ManaRequirement,
    life: i32,
) -> Option<(Sources, ManaRequirement, u32)> {
    if req.is_plain() {
        return plain_payment_plan(sources, pool, req).map(|p| (p, *req, 0));
    }
    req.expansions().into_iter().find_map(|(r, l)| {
        if i64::from(l) > i64::from(life) {
            return None;
        }
        plain_payment_plan(sources, pool, &r).map(|p| (p, r, l))
    })
}

fn plain_payment_plan(
    sources: &[(ObjId, Mana)],
    pool: &crate::mana::ManaPool,
    req: &ManaRequirement,
) -> Option<Sources> {
    let mut pool = *pool;
    let mut sources: Sources = SmallVec::from_slice(sources);
    let mut tapped = Sources::new();
    let take = |sources: &mut Sources, pos: usize| -> (ObjId, Mana) {
        let (id, mana) = sources[pos];
        sources.retain(|(s, _)| *s != id);
        (id, mana)
    };
    for (ci, need) in req.colored.iter().enumerate() {
        let mut need = *need;
        let from_pool = need.min(pool.colored[ci]);
        pool.colored[ci] -= from_pool;
        need -= from_pool;
        while need > 0 {
            let pos = sources
                .iter()
                .position(|(_, m)| matches!(m, Mana::Colored(c) if *c as usize == ci))
                .or_else(|| sources.iter().position(|(_, m)| *m == Mana::Any))?;
            let (id, mana) = take(&mut sources, pos);
            // An any-color source pays as the color needed.
            let mana = if mana == Mana::Any {
                Mana::Colored(crate::mana::Color::ALL[ci])
            } else {
                mana
            };
            tapped.push((id, mana));
            need -= 1;
        }
    }
    let mut need = req.colorless;
    let from_pool = need.min(pool.colorless);
    pool.colorless -= from_pool;
    need -= from_pool;
    while need > 0 {
        let pos = sources.iter().position(|(_, m)| *m == Mana::Colorless)?;
        tapped.push(take(&mut sources, pos));
        need -= 1;
    }
    let mut generic = req.generic;
    let from_pool = generic.min(pool.total());
    generic -= from_pool;
    while generic > 0 {
        if sources.is_empty() {
            return None;
        }
        let (id, mana) = take(&mut sources, 0);
        tapped.push((
            id,
            if mana == Mana::Any {
                Mana::Colorless
            } else {
                mana
            },
        ));
        generic -= 1;
    }
    Some(tapped)
}
