//! Priority actions, casting (CR 601), mana payment, and resolution (CR 608).

use smallvec::SmallVec;

use crate::card::{
    AbilityDef, AbilityId, Affects, Cost, CounterKind, Effect, Keyword, Modification, PtValue,
    Recipient, Replacement, Replaces, SacCost, TargetSpec,
};
use crate::decision::{Choice, DecisionKind};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::mana::{Mana, ManaRequirement};
use crate::state::{
    Affected, ContinuousEffect, DelayedTrigger, Duration, GameObject, GameState, ObjKind,
    ReplacementEffect, RuleEffect, StackAbility, Target, Zone,
};

impl GameState {
    // ----- priority actions --------------------------------------------------------

    pub(super) fn priority_options(&self, p: PlayerId) -> Vec<Choice> {
        let mut options = Vec::new();
        let sorcery_timing =
            p == self.active_player && self.step.is_main() && self.stack.is_empty();
        let ps = self.player(p);
        let sources = self.mana_sources(p);
        let pool = ps.mana_pool;
        for id in &ps.hand {
            let def = self.pool.get(self.objects[*id].def);
            if def.is_land() {
                if sorcery_timing && ps.lands_played_this_turn < 1 {
                    options.push(Choice::PlayLand(*id));
                }
                continue;
            }
            let instant_speed = def.is_instant() || def.keywords.contains(Keyword::Flash);
            if !(instant_speed || sorcery_timing) {
                continue;
            }
            if payment_plan_from(&sources, &pool, &def.mana_cost.requirements(0)).is_none() {
                continue;
            }
            // Additional costs must be payable (CR 601.2h): enough life
            // (CR 119.4), another card to discard, something to sacrifice.
            let add = &def.additional;
            if add.life > 0 && ps.life < i32::try_from(add.life).unwrap_or(i32::MAX) {
                continue;
            }
            if add.discard > 0 && ps.hand.len() <= add.discard as usize {
                continue;
            }
            if let Some(SacCost::Filter(f)) = &add.sacrifice
                && self.sacrifice_candidates(p, f, *id).is_empty()
            {
                continue;
            }
            // A modal spell needs some mode to have its targets (CR 601.2b).
            let targets_ok = match def.modes() {
                Some(modes) => modes
                    .iter()
                    .any(|m| self.targets_available(p, &m.targets, None)),
                None => self.targets_available(p, def.cast_targets(), None),
            };
            if targets_ok {
                options.push(Choice::Cast(*id));
            }
        }
        self.activation_options(p, sorcery_timing, &sources, &mut options);
        options.push(Choice::Pass);
        options
    }

    /// CR 602: activated abilities of permanents `p` controls whose costs
    /// can be paid and whose targets exist. An ability with a "sacrifice a
    /// Goblin" cost is offered once per permanent that could be sacrificed.
    fn activation_options(
        &self,
        p: PlayerId,
        sorcery_timing: bool,
        sources: &Sources,
        options: &mut Vec<Choice>,
    ) {
        if self.activated_sources == 0 {
            return;
        }
        let pool = self.player(p).mana_pool;
        for id in self.permanents_of(p) {
            let o = &self.objects[id];
            if !self.flags_of(o).has_activated() {
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
                // CR 302.6: a creature's {T} needs it to have been controlled
                // since the turn began.
                if ab.cost.tap && (o.tapped || sick) {
                    continue;
                }
                // A source tapped as a cost cannot also tap for mana.
                let payable = if ab.cost.tap {
                    let rest: Sources = sources.iter().filter(|(s, _)| *s != id).copied().collect();
                    payment_plan_from(&rest, &pool, &ab.cost.mana.requirements(0)).is_some()
                } else {
                    payment_plan_from(sources, &pool, &ab.cost.mana.requirements(0)).is_some()
                };
                if !payable {
                    continue;
                }
                if !self.targets_available(p, &ab.targets, Some(id)) {
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
                    self.player(p).hand.iter().copied().map(Some).collect()
                } else {
                    vec![None]
                };
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
        if matches!(self.pool.ability(ability), AbilityDef::Activated(a) if a.cost.loyalty != 0) {
            self.loyalty_activated.push(source);
        }
        let timestamp = self.tick();
        let obj = self.objects.insert(GameObject {
            def,
            card: def,
            except: None,
            mode: 0,
            kind: ObjKind::Ability,
            owner: player,
            controller: player,
            zone: Zone::Stack,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            controlled_since_turn_start: false,
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
            })
            .collect()
    }

    /// The values of X the caster can pay for (CR 601.2b, 601.2h), in
    /// increasing order.
    pub(super) fn x_options(&self, spell: ObjId) -> Vec<Choice> {
        const MAX_X: u32 = 30;
        let o = &self.objects[spell];
        let caster = o.controller;
        let cost = &self.pool.get(o.def).mana_cost;
        let sources = self.mana_sources(caster);
        let pool = self.player(caster).mana_pool;
        (0..=MAX_X)
            .take_while(|x| payment_plan_from(&sources, &pool, &cost.requirements(*x)).is_some())
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
    fn targets_available(&self, p: PlayerId, specs: &[TargetSpec], source: Option<ObjId>) -> bool {
        specs
            .iter()
            .all(|s| !self.legal_targets(p, *s, source).is_empty())
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
        let Some(modes) = self.pool.get(o.def).modes() else {
            return Vec::new();
        };
        modes
            .iter()
            .enumerate()
            .filter(|(_, m)| self.targets_available(o.controller, &m.targets, None))
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
        let AbilityDef::Activated(ab) = self.pool.ability(sa.ability).clone() else {
            unreachable!("checked by finish_ability_targets");
        };
        if ab.cost.tap {
            self.tap(sa.source);
        }
        self.pay(controller, &ab.cost.mana.requirements(0));
        self.pay_other_costs(controller, &ab.cost, sa.source, sacrifice, discard);
        self.emit(Event::Activated {
            obj,
            source: sa.source,
            def,
        });
        self.give_priority(controller);
    }

    /// Legal targets for `spec` chosen by `caster`, for a spell or an
    /// ability of `source`.
    pub(super) fn legal_targets(
        &self,
        caster: PlayerId,
        spec: TargetSpec,
        source: Option<ObjId>,
    ) -> Vec<Target> {
        let mut out = Vec::new();
        if spec == TargetSpec::CreatureCardInYourGraveyard {
            for id in &self.player(caster).graveyard {
                let t = Target::Object(*id);
                if self.is_legal_target_for(caster, spec, t, source) {
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
                if let AbilityDef::TapForMana(m) = self.pool.ability(*a) {
                    list.push((id, *m));
                }
            }
        }
        out.extend(later);
        out
    }

    /// A canonical payment plan for `p`; see `payment_plan_from`.
    pub(super) fn payment_plan(&self, p: PlayerId, req: &ManaRequirement) -> Option<Sources> {
        payment_plan_from(&self.mana_sources(p), &self.player(p).mana_pool, req)
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
                    self.sacrifice(s);
                }
            }
            None => {}
        }
        if cost.discard > 0
            && let Some(c) = discard
        {
            self.discard_card(p, c);
        }
    }

    pub(super) fn pay(&mut self, p: PlayerId, req: &ManaRequirement) {
        let plan = self
            .payment_plan(p, req)
            .expect("cast was offered only when payable");
        for (id, mana) in plan {
            self.tap(id);
            self.player_mut(p).mana_pool.add(mana);
        }
        let pool = self.player(p).mana_pool;
        let left = pool
            .payment_plan(req)
            .expect("pool covers the cost after activating mana abilities");
        self.player_mut(p).mana_pool = left;
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
        let targets: Vec<Target> = obj.targets.iter().copied().collect();
        let def = self.pool.get(def_ref).clone();
        self.pay(caster, &def.mana_cost.requirements(x));
        self.pay_other_costs(caster, &def.additional, spell, sacrifice, discard);
        self.emit(Event::Cast {
            player: caster,
            spell,
            def: def_ref,
            targets,
        });
        self.emit_targeted(spell);
        self.give_priority(caster);
    }

    // ----- resolution -------------------------------------------------------------

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
        let def = self.pool.get(obj.def).clone();
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
                self.move_object(spell, Zone::Graveyard, true);
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
        let parts: Option<(Vec<TargetSpec>, Vec<Effect>)> =
            (def.spell_ability().is_some() || def.modes().is_some()).then(|| {
                (
                    self.targets_of(spell).to_vec(),
                    self.effects_of(spell).to_vec(),
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
            if !specs.is_empty() && legal.iter().all(|l| !l) {
                self.emit(Event::Fizzle {
                    spell,
                    def: obj.def,
                });
                self.move_object(spell, Zone::Graveyard, true);
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
                created: None,
                x: self.x_of(spell),
            };
            for effect in &effects {
                self.apply_effect(&mut ctx, effect);
            }
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
            } else {
                self.move_object(spell, Zone::Graveyard, true);
            }
        }
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
                })
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
                    self.deal_damage(ctx.source, t, amount.eval(ctx.x), false);
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
                        self.gain_life(p, amount.eval(ctx.x));
                    }
                }
            }
            Effect::LoseLife { amount, who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.change_life(p, -i32::try_from(amount.eval(ctx.x)).unwrap_or(i32::MAX));
                    }
                }
            }
            Effect::DrawCards { count, who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        for _ in 0..count.eval(ctx.x) {
                            self.draw(p);
                        }
                    }
                }
            }
            Effect::AddCounters { kind, count, to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.add_counters(id, *kind, count.eval(ctx.x));
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
                        self.remove_counters(id, *kind, count.eval(ctx.x));
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
                        self.mill(p, count.eval(ctx.x));
                    }
                }
            }
            Effect::CreateEmblem { emblem } => {
                self.create_emblem(*emblem, ctx.controller);
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
                        for _ in 0..count.eval(ctx.x) {
                            self.create_token(*token, p);
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
            } => {
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    let mut parts: SmallVec<[Modification; 2]> =
                        smallvec::smallvec![Modification::AddTypes(*types)];
                    parts.extend(subtypes.iter().map(|s| Modification::AddSubtype(*s)));
                    parts.push(Modification::SetPt {
                        power: PtValue::Fixed(*power),
                        toughness: PtValue::Fixed(*toughness),
                    });
                    self.effect_until_end_of_turn(ctx, id, parts);
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
                if k == 0 {
                    for e in inner {
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
                    if let Some(id) = on_battlefield(self, t) {
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
    /// The object the most recent effect of this resolution created or
    /// moved (a token, an exiled card), for "sacrifice it" clauses that
    /// follow (`Recipient::CreatedObject`).
    pub created: Option<ObjId>,
    /// X of the resolving spell (CR 107.3); 0 for abilities.
    pub x: u32,
}

/// A canonical payment plan: which sources to tap, and for what, so that
/// `pool` plus their mana covers `req`. With single-color sources a greedy
/// assignment (colored symbols first, then generic from whatever is left) is
/// exact; multi-type lands are assigned to the first color that needs them.
/// Mana sources, inline for typical board sizes.
pub(super) type Sources = SmallVec<[(ObjId, Mana); 12]>;

fn payment_plan_from(
    sources: &[(ObjId, Mana)],
    pool: &crate::mana::ManaPool,
    req: &ManaRequirement,
) -> Option<Sources> {
    let mut pool = *pool;
    let mut sources: Sources = SmallVec::from_slice(sources);
    let mut tapped = Sources::new();
    let mut take = |sources: &mut Sources, pos: usize| {
        let (id, mana) = sources[pos];
        sources.retain(|(s, _)| *s != id);
        tapped.push((id, mana));
    };
    for (ci, need) in req.colored.iter().enumerate() {
        let mut need = *need;
        let from_pool = need.min(pool.colored[ci]);
        pool.colored[ci] -= from_pool;
        need -= from_pool;
        while need > 0 {
            let pos = sources
                .iter()
                .position(|(_, m)| matches!(m, Mana::Colored(c) if *c as usize == ci))?;
            take(&mut sources, pos);
            need -= 1;
        }
    }
    let mut need = req.colorless;
    let from_pool = need.min(pool.colorless);
    pool.colorless -= from_pool;
    need -= from_pool;
    while need > 0 {
        let pos = sources.iter().position(|(_, m)| *m == Mana::Colorless)?;
        take(&mut sources, pos);
        need -= 1;
    }
    let mut generic = req.generic;
    let from_pool = generic.min(pool.total());
    generic -= from_pool;
    while generic > 0 {
        if sources.is_empty() {
            return None;
        }
        take(&mut sources, 0);
        generic -= 1;
    }
    Some(tapped)
}
