//! Priority actions, casting (CR 601), mana payment, and resolution (CR 608).

use smallvec::SmallVec;

use crate::card::Modification;
use crate::card::{AbilityDef, Effect, Keyword, PlayerRef, TargetSpec};
use crate::decision::Choice;
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::mana::{Mana, ManaRequirement};
use crate::state::{Affected, ContinuousEffect, Duration, GameState, Target, Zone};

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
        options.push(Choice::Pass);
        options
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
        for id in self.permanents_of(p) {
            if self.objects[id].tapped {
                continue;
            }
            let Some(c) = self.characteristics(id) else {
                continue;
            };
            if c.is_land() {
                for s in &c.subtypes {
                    if let Some(color) = s.basic_color() {
                        out.push((id, Mana::Colored(color)));
                    }
                }
            }
            for a in &c.abilities {
                if let AbilityDef::TapForMana(m) = self.pool.ability(*a) {
                    out.push((id, *m));
                }
            }
        }
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
            let new = self.move_object(spell, Zone::Battlefield, true);
            self.objects[new].controller = obj.controller;
            if let Some(Target::Object(t)) = target {
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
            for effect in effects {
                self.apply_effect(spell, obj.controller, &obj.targets, &legal, effect);
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
                let new = self.move_object(spell, Zone::Battlefield, true);
                self.objects[new].controller = obj.controller;
            } else {
                self.move_object(spell, Zone::Graveyard, true);
            }
        }
    }

    pub(super) fn apply_effect(
        &mut self,
        source: ObjId,
        controller: PlayerId,
        targets: &[Target],
        legal: &[bool],
        effect: &Effect,
    ) {
        let target_of = |i: usize| -> Option<Target> {
            if legal.get(i).copied().unwrap_or(false) {
                targets.get(i).copied()
            } else {
                None
            }
        };
        match effect {
            Effect::DealDamage { amount, target } => {
                if let Some(t) = target_of(*target) {
                    self.deal_damage(source, t, *amount, false);
                }
            }
            Effect::Pump {
                power,
                toughness,
                target,
            } => {
                if let Some(Target::Object(id)) = target_of(*target) {
                    let timestamp = self.tick();
                    self.effects.push(ContinuousEffect {
                        source: Some(source),
                        timestamp,
                        duration: Duration::UntilEndOfTurn,
                        affected: Affected::Objects(smallvec::smallvec![id]),
                        parts: smallvec::smallvec![Modification::ModifyPt {
                            power: *power,
                            toughness: *toughness,
                        }],
                    });
                    self.bump();
                }
            }
            Effect::Destroy { target } => {
                if let Some(Target::Object(id)) = target_of(*target) {
                    self.destroy(id);
                }
            }
            Effect::GainLife { amount, player } => {
                let PlayerRef::You = player;
                self.change_life(controller, i32::try_from(*amount).unwrap_or(i32::MAX));
            }
            Effect::DrawCards { count, player } => {
                let PlayerRef::You = player;
                for _ in 0..*count {
                    self.draw(controller);
                }
            }
            Effect::AddCounters {
                kind,
                count,
                target,
            } => {
                if let Some(Target::Object(id)) = target_of(*target) {
                    self.add_counters(id, *kind, *count);
                }
            }
        }
    }
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
