//! Priority actions, casting (CR 601), mana payment, and resolution (CR 608).

use smallvec::SmallVec;

use crate::card::{
    AbilityDef, AbilityId, Affects, Effect, Keyword, Modification, PtValue, Recipient, Replacement,
    Replaces, SacCost, TargetSpec,
};
use crate::decision::Choice;
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::mana::{Mana, ManaRequirement};
use crate::state::{
    Affected, ContinuousEffect, Duration, GameObject, GameState, ObjKind, ReplacementEffect,
    RuleEffect, StackAbility, Target, Zone,
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
            let targets_ok = def
                .cast_targets()
                .iter()
                .all(|s| !self.legal_targets(p, *s).is_empty());
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
            if !self.pool.get(o.def).has_activated() {
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
                if !ab
                    .targets
                    .iter()
                    .all(|s| !self.legal_targets(p, *s).is_empty())
                {
                    continue;
                }
                match &ab.cost.sacrifice {
                    None | Some(SacCost::This) => options.push(Choice::Activate {
                        source: id,
                        ability: *a,
                        sacrifice: None,
                    }),
                    Some(SacCost::Filter(f)) => {
                        for s in self.permanents_of(p) {
                            let sc = &self.objects[s];
                            let ok = self.characteristics(s).is_some_and(|c| {
                                f.matches(s, sc.controller, &c, Some(id), Some(p))
                            });
                            if ok {
                                options.push(Choice::Activate {
                                    source: id,
                                    ability: *a,
                                    sacrifice: Some(s),
                                });
                            }
                        }
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
    ) {
        self.consecutive_passes = 0;
        let def = self.objects[source].def;
        let timestamp = self.tick();
        let obj = self.objects.insert(GameObject {
            def,
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
        };
    }

    /// An ability object on the stack has its targets. An activated ability
    /// now pays its costs; a triggered one is done.
    pub(super) fn finish_ability_targets(&mut self, obj: ObjId, sacrifice: Option<ObjId>) {
        let activated = self.objects[obj]
            .ability
            .is_some_and(|sa| matches!(self.pool.ability(sa.ability), AbilityDef::Activated(_)));
        if activated {
            self.finish_activation(obj, sacrifice);
        } else {
            self.flow = crate::state::Flow::Priority;
        }
    }

    /// CR 602.2b, 601.2g–h: pay the ability's costs. The source is tapped
    /// first so that it is not also tapped for mana; sacrifices come last,
    /// so a "sacrifice ~" source is still there to be tapped.
    fn finish_activation(&mut self, obj: ObjId, sacrifice: Option<ObjId>) {
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
        match ab.cost.sacrifice {
            Some(SacCost::This) => self.sacrifice(sa.source),
            Some(SacCost::Filter(_)) => {
                if let Some(s) = sacrifice {
                    self.sacrifice(s);
                }
            }
            None => {}
        }
        self.emit(Event::Activated {
            obj,
            source: sa.source,
            def,
        });
        self.give_priority(controller);
    }

    pub(super) fn legal_targets(&self, caster: PlayerId, spec: TargetSpec) -> Vec<Target> {
        let mut out = Vec::new();
        for id in &self.battlefield {
            let t = Target::Object(*id);
            if self.is_legal_target(caster, spec, t) {
                out.push(t);
            }
        }
        for p in self.players_in_game() {
            let t = Target::Player(p);
            if self.is_legal_target(caster, spec, t) {
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
            let other_use = c.is_creature()
                || (self.activated_sources > 0 && self.pool.get(o.def).has_activated());
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
    pub(super) fn finish_cast(&mut self, spell: ObjId) {
        let obj = &self.objects[spell];
        let (caster, def_ref) = (obj.controller, obj.def);
        let targets: Vec<Target> = obj.targets.iter().copied().collect();
        let req = self.pool.get(def_ref).mana_cost.requirements(0);
        self.pay(caster, &req);
        self.emit(Event::Cast {
            player: caster,
            spell,
            def: def_ref,
            targets,
        });
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
            let legal = target.is_some_and(|t| self.is_legal_target(obj.controller, spec, t));
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
            self.bump();
            return;
        }
        if let Some((specs, effects)) = def.spell_ability() {
            // CR 608.2b: targets that are no longer legal are ignored; if all
            // are illegal the spell does not resolve.
            let legal: Vec<bool> = specs
                .iter()
                .zip(obj.targets.iter())
                .map(|(spec, t)| self.is_legal_target(obj.controller, *spec, *t))
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
            let ctx = EffectCtx {
                source: spell,
                controller: obj.controller,
                targets: obj.targets.to_vec(),
                legal,
                trigger_obj: None,
                trigger_player: None,
            };
            for effect in effects {
                self.apply_effect(&ctx, effect);
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
                self.put_onto_battlefield(spell, obj.controller);
            } else {
                self.move_object(spell, Zone::Graveyard, true);
            }
        }
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
        self.bump();
    }

    #[allow(clippy::too_many_lines)] // one arm per effect kind
    pub(super) fn apply_effect(&mut self, ctx: &EffectCtx, effect: &Effect) {
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
                    self.deal_damage(ctx.source, t, *amount, false);
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
                        self.gain_life(p, *amount);
                    }
                }
            }
            Effect::LoseLife { amount, who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.change_life(p, -i32::try_from(*amount).unwrap_or(i32::MAX));
                    }
                }
            }
            Effect::DrawCards { count, who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        for _ in 0..*count {
                            self.draw(p);
                        }
                    }
                }
            }
            Effect::AddCounters { kind, count, to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.add_counters(id, *kind, *count);
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
                        self.remove_counters(id, *kind, *count);
                    }
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
                        for _ in 0..*count {
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
                    self.bump();
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
        }
    }

    /// Store a replacement or prevention effect until end of turn (CR 614.3,
    /// 615.3). One aimed at a spell's target is locked to that target
    /// (CR 611.2c) and is not created if the target became illegal.
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
