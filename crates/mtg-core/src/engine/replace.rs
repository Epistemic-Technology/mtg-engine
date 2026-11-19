//! Replacement and prevention effects (CR 614–616): stage 1 of the event
//! pipeline (design §4.5). An event that is about to happen is a
//! [`Proposed`]; before it is performed, every applicable replacement or
//! prevention effect gets one chance to modify it (CR 614.5), the affected
//! player choosing the order when several apply (CR 616.1). The result is
//! zero or more events that happen instead (CR 614.6), each of which goes
//! through the same process for effects that have not yet applied
//! (CR 616.2).
//!
//! Effects come from three places: static abilities of permanents on the
//! battlefield, read from current characteristics (so Humility removes
//! Phantom Tiger's shield); the "enters with" abilities of the permanent
//! that is entering (CR 614.1c); and effects stored by resolved spells
//! (`GameState::replacements`: Fog, damage shields).
//!
//! Replacement and performance interleave: when one event becomes several
//! (draw two instead of one), each happens, with its own replacement
//! check, before the next is considered (CR 614.11a).
//!
//! The choice of CR 616.1 happens in the middle of applying an event, where
//! the engine cannot pause. `choose_mid` records the question and lets a
//! default stand; the `Engine` then rolls the action back and replays it
//! with the player's answer (see `Engine::advance`). If no snapshot was
//! taken because no choice looked possible, the default stands for good.

use smallvec::{SmallVec, smallvec};

use super::casting::EffectCtx;
use crate::card::{
    AbilityDef, AbilityId, Affects, CardRef, CounterKind, ObjFilter, Replacement,
    ReplacementAbility, Replaces, RuleKind, Who,
};
use crate::decision::{Choice, Decision, DecisionKind};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::state::{GameState, LossReason, Target, Zone};

/// How a permanent enters the battlefield, as modified by replacement
/// effects (CR 614.1c, 614.12).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Enter {
    pub tapped: bool,
    pub counters: Option<(CounterKind, u32)>,
}

/// An event that is about to happen and may still be replaced.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Proposed {
    Damage {
        source: ObjId,
        target: Target,
        amount: u32,
        combat: bool,
    },
    GainLife {
        player: PlayerId,
        amount: u32,
    },
    Draw {
        player: PlayerId,
    },
    ZoneChange {
        obj: ObjId,
        to: Zone,
        to_top: bool,
        /// Controller once on the battlefield (the owner elsewhere).
        controller: PlayerId,
        enter: Enter,
    },
    AddCounters {
        obj: ObjId,
        kind: CounterKind,
        n: u32,
    },
    /// `obj` would be destroyed (CR 701.7); `regenerate` is false for "it
    /// can't be regenerated".
    Destroy {
        obj: ObjId,
        regenerate: bool,
    },
}

impl Proposed {
    /// An event of nothing does not happen at all (CR 120.8, 614.7a).
    fn is_nothing(&self) -> bool {
        matches!(
            self,
            Proposed::Damage { amount: 0, .. }
                | Proposed::GainLife { amount: 0, .. }
                | Proposed::AddCounters { n: 0, .. }
        )
    }

    /// The player who chooses among several applicable effects (CR 616.1):
    /// the affected player, or the affected object's controller.
    fn affected_player(&self, state: &GameState) -> PlayerId {
        let controller_of = |id: ObjId| {
            state
                .objects
                .get(id)
                .map_or(state.active_player, |o| o.controller)
        };
        match self {
            Proposed::Damage {
                target: Target::Player(p),
                ..
            }
            | Proposed::GainLife { player: p, .. }
            | Proposed::Draw { player: p } => *p,
            Proposed::Damage {
                target: Target::Object(id),
                ..
            }
            | Proposed::AddCounters { obj: id, .. }
            | Proposed::Destroy { obj: id, .. } => controller_of(*id),
            Proposed::ZoneChange {
                obj, controller, ..
            } => {
                if state
                    .objects
                    .get(*obj)
                    .is_some_and(|o| o.zone == Zone::Battlefield)
                {
                    controller_of(*obj)
                } else {
                    *controller
                }
            }
        }
    }
}

/// Identity of a replacement effect, so that each applies once per event
/// (CR 614.5).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Key {
    Ability { source: ObjId, ability: AbilityId },
    Stored(usize),
}

/// A replacement effect that applies to the event under consideration.
struct Candidate {
    key: Key,
    def: CardRef,
    source: ObjId,
    controller: PlayerId,
    ability: ReplacementAbility,
}

/// What an applied effect made of the event.
enum Outcome {
    /// The event, modified; other effects may still apply.
    Event(Proposed),
    /// Several events instead (CR 614.6).
    Events(SmallVec<[Proposed; 2]>),
    /// The event does not happen.
    Nothing,
}

type Applied = SmallVec<[Key; 4]>;

impl GameState {
    // ----- the pipeline --------------------------------------------------------------

    /// Propose an event: replacement and prevention effects apply (stage 1),
    /// then whatever remains happens (stage 2, `perform`) and triggers are
    /// detected as each resulting event is emitted (stage 3). For zone
    /// changes, the new object's id.
    #[inline]
    pub(super) fn propose(&mut self, ev: Proposed) -> Option<ObjId> {
        if !self.replaceable(&ev) {
            return self.perform(ev);
        }
        self.propose_replaceable(ev)
    }

    #[inline(never)]
    fn propose_replaceable(&mut self, ev: Proposed) -> Option<ObjId> {
        let mut applied = Applied::new();
        self.propose_with(ev, &mut applied)
    }

    /// Cheap gate: could anything replace `ev`?
    #[inline]
    fn replaceable(&self, ev: &Proposed) -> bool {
        if self.replacement_sources > 0 || !self.replacements.is_empty() {
            return true;
        }
        matches!(ev, Proposed::ZoneChange { obj, to: Zone::Battlefield, .. }
            if self.objects.get(*obj).is_some_and(|o| self.flags_of(o).has(crate::card::DefFlags::MODIFIES_ENTERING_FLAG)))
    }

    fn propose_with(&mut self, mut ev: Proposed, applied: &mut Applied) -> Option<ObjId> {
        loop {
            if ev.is_nothing() {
                return None;
            }
            let candidates = self.candidates(&ev, applied);
            if candidates.is_empty() {
                return self.perform(ev);
            }
            // CR 616.1: the affected player chooses which applies next.
            let i = if candidates.len() == 1 {
                0
            } else {
                let player = ev.affected_player(self);
                self.choose_replacement(player, &candidates)
            };
            let chosen = &candidates[i];
            applied.push(chosen.key);
            match self.apply_replacement(chosen, ev) {
                Outcome::Event(e) => ev = e,
                Outcome::Events(events) => {
                    // Each happens in turn (CR 614.11a). Effects already
                    // applied get no second chance at the events that replace
                    // this one (CR 614.5); others may now apply (CR 616.2).
                    let mut last = None;
                    for e in events {
                        let mut a = applied.clone();
                        last = self.propose_with(e, &mut a).or(last);
                    }
                    return last;
                }
                Outcome::Nothing => return None,
            }
        }
    }

    /// Every replacement or prevention effect that applies to `ev` and has
    /// not yet applied, in a stable order: battlefield statics, the
    /// entering permanent's own, then stored effects.
    fn candidates(&self, ev: &Proposed, applied: &Applied) -> Vec<Candidate> {
        let mut out = Vec::new();
        if self.replacement_sources > 0 {
            for id in &self.battlefield {
                let o = &self.objects[*id];
                if !self.flags_of(o).has_replacements() {
                    continue;
                }
                let Some(c) = self.characteristics(*id) else {
                    continue;
                };
                for ability in c.abilities.iter().copied() {
                    let AbilityDef::Replacement(r) = self.pool.ability(ability) else {
                        continue;
                    };
                    let key = Key::Ability {
                        source: *id,
                        ability,
                    };
                    if applied.contains(&key) {
                        continue;
                    }
                    if self.applies(r, ev, *id, o.controller, None) {
                        out.push(Candidate {
                            key,
                            def: o.def,
                            source: *id,
                            controller: o.controller,
                            ability: r.clone(),
                        });
                    }
                }
            }
        }
        // CR 614.1c: how the permanent itself enters, judged as it would
        // exist on the battlefield (CR 614.12): under Humility a Phantom
        // Tiger has no "enters with" ability, under Blood Moon a Guildgate
        // does not enter tapped. The printed flag gates the look-ahead.
        if let Proposed::ZoneChange {
            obj,
            to: Zone::Battlefield,
            controller,
            ..
        } = ev
            && let Some(o) = self.objects.get(*obj)
        {
            let def = self.pool.get(o.def);
            if def.modifies_entering() {
                let c = crate::layers::compute_entering(self, *obj, *controller);
                for ability in c.abilities.iter().copied() {
                    let AbilityDef::Replacement(r) = self.pool.ability(ability) else {
                        continue;
                    };
                    if !matches!(r.replaces, Replaces::Enters) {
                        continue;
                    }
                    let key = Key::Ability {
                        source: *obj,
                        ability,
                    };
                    if !applied.contains(&key) {
                        out.push(Candidate {
                            key,
                            def: o.def,
                            source: *obj,
                            controller: *controller,
                            ability: r.clone(),
                        });
                    }
                }
            }
        }
        for (i, e) in self.replacements.iter().enumerate() {
            let key = Key::Stored(i);
            if applied.contains(&key) {
                continue;
            }
            // An exhausted shield prevents nothing more (CR 615.7); a used
            // regeneration shield is gone (CR 701.15a).
            if matches!(
                e.ability.replacement,
                Replacement::PreventNext(_) | Replacement::Regenerate
            ) && e.remaining == 0
            {
                continue;
            }
            let source = e.source.unwrap_or_default();
            if self.applies(&e.ability, ev, source, e.controller, e.target) {
                out.push(Candidate {
                    key,
                    def: e.def,
                    source,
                    controller: e.controller,
                    ability: e.ability.clone(),
                });
            }
        }
        out
    }

    /// Whether effect `r` of `source` (controlled by `controller`, locked to
    /// `locked` if from a targeted spell) applies to `ev`.
    #[allow(clippy::too_many_lines)] // one arm per replaced event
    fn applies(
        &self,
        r: &ReplacementAbility,
        ev: &Proposed,
        source: ObjId,
        controller: PlayerId,
        locked: Option<Target>,
    ) -> bool {
        let whose_ok = |w: Who, p: PlayerId| match w {
            Who::Any => true,
            Who::You => p == controller,
            Who::Opponents => p != controller,
        };
        match (&r.replaces, ev) {
            (
                Replaces::Damage { from, to, combat },
                Proposed::Damage {
                    source: src,
                    target,
                    combat: c,
                    ..
                },
            ) => {
                combat.is_none_or(|k| k == *c)
                    && from
                        .as_ref()
                        .is_none_or(|f| self.source_is(f, *src, source, controller, locked))
                    && self.affects(to, *target, source, controller, locked)
            }
            (
                Replaces::DamageToAndBy { what, combat },
                Proposed::Damage {
                    source: src,
                    target,
                    combat: c,
                    ..
                },
            ) => {
                combat.is_none_or(|k| k == *c)
                    && (self.affects(what, *target, source, controller, locked)
                        || self.source_is(what, *src, source, controller, locked))
            }
            (Replaces::GainLife { whose }, Proposed::GainLife { player, .. }) => {
                whose_ok(*whose, *player)
            }
            (
                Replaces::Draw {
                    whose,
                    library_empty,
                },
                Proposed::Draw { player },
            ) => {
                whose_ok(*whose, *player)
                    && (!*library_empty || self.player(*player).library.is_empty())
            }
            (
                Replaces::ToGraveyard {
                    whose,
                    from_battlefield,
                },
                Proposed::ZoneChange {
                    obj,
                    to: Zone::Graveyard,
                    ..
                },
            ) => self.objects.get(*obj).is_some_and(|o| {
                whose_ok(*whose, o.owner) && (!*from_battlefield || o.zone == Zone::Battlefield)
            }),
            (
                Replaces::PlusCounters { on },
                Proposed::AddCounters {
                    obj,
                    kind: CounterKind::PlusOnePlusOne,
                    n,
                },
            ) => *n > 0 && self.filter_matches(on, *obj, source, controller),
            (
                Replaces::PlusCounters { on },
                Proposed::ZoneChange {
                    obj,
                    to: Zone::Battlefield,
                    controller: ctl,
                    enter:
                        Enter {
                            counters: Some((CounterKind::PlusOnePlusOne, n)),
                            ..
                        },
                    ..
                },
            ) => {
                // CR 122.6: counters it enters with count as being put on it.
                *n > 0
                    && self.objects.get(*obj).is_some_and(|o| {
                        on.matches(
                            *obj,
                            *ctl,
                            &self.printed(o.def),
                            Some(source),
                            Some(controller),
                        )
                    })
            }
            (
                Replaces::Destroy { to },
                Proposed::Destroy {
                    obj,
                    regenerate: true,
                },
            ) => self.affects(to, Target::Object(*obj), source, controller, locked),
            (
                Replaces::Dies { to },
                Proposed::ZoneChange {
                    obj,
                    to: Zone::Graveyard,
                    ..
                },
            ) => {
                self.objects
                    .get(*obj)
                    .is_some_and(|o| o.zone == Zone::Battlefield)
                    && self.characteristics(*obj).is_some_and(|c| c.is_creature())
                    && self.affects(to, Target::Object(*obj), source, controller, locked)
            }
            // `Enters` effects apply only from the entering permanent itself
            // (handled in `candidates`), never from the battlefield.
            _ => false,
        }
    }

    /// Whether `target` is among what `to` covers.
    fn affects(
        &self,
        to: &Affects,
        target: Target,
        source: ObjId,
        controller: PlayerId,
        locked: Option<Target>,
    ) -> bool {
        let whose_ok = |w: Who, p: PlayerId| match w {
            Who::Any => true,
            Who::You => p == controller,
            Who::Opponents => p != controller,
        };
        match to {
            Affects::Any => true,
            Affects::This => target == Target::Object(source),
            Affects::You => target == Target::Player(controller),
            Affects::Players(w) => matches!(target, Target::Player(p) if whose_ok(*w, p)),
            Affects::Objects(f) => {
                matches!(target, Target::Object(id) if self.filter_matches(f, id, source, controller))
            }
            Affects::YouAnd(f) => {
                target == Target::Player(controller)
                    || matches!(target, Target::Object(id) if self.filter_matches(f, id, source, controller))
            }
            Affects::Target(_) => locked == Some(target),
            Affects::OtherThanTarget(f, _) => {
                locked != Some(target)
                    && matches!(target, Target::Object(id) if self.filter_matches(f, id, source, controller))
            }
        }
    }

    /// Whether `src`, the source of damage, is among what `from` covers
    /// (CR 609.7). Players never deal damage.
    fn source_is(
        &self,
        from: &Affects,
        src: ObjId,
        source: ObjId,
        controller: PlayerId,
        locked: Option<Target>,
    ) -> bool {
        match from {
            Affects::Any => true,
            Affects::This => src == source,
            Affects::You | Affects::Players(_) => false,
            Affects::Objects(f) | Affects::YouAnd(f) => {
                self.source_matches(f, src, source, controller)
            }
            Affects::Target(_) => locked == Some(Target::Object(src)),
            Affects::OtherThanTarget(f, _) => {
                locked != Some(Target::Object(src))
                    && self.source_matches(f, src, source, controller)
            }
        }
    }

    /// Whether battlefield object `id` matches `f`, judged for `source`.
    fn filter_matches(
        &self,
        f: &ObjFilter,
        id: ObjId,
        source: ObjId,
        controller: PlayerId,
    ) -> bool {
        let Some(o) = self.objects.get(id) else {
            return false;
        };
        if o.zone != Zone::Battlefield || (f.enchanted && !self.enchants(source, id)) {
            return false;
        }
        self.characteristics(id)
            .is_some_and(|c| f.matches(id, o.controller, &c, Some(source), Some(controller)))
            && self.status_ok(f, id)
    }

    /// Whether `source` (or its last known information) is attached to
    /// `id`: "enchanted creature".
    fn enchants(&self, source: ObjId, id: ObjId) -> bool {
        self.objects
            .get(source)
            .map(|a| a.attached_to)
            .or_else(|| self.lki.get(&source).map(|l| l.object.attached_to))
            .flatten()
            == Some(id)
    }

    /// Whether the source of damage matches `f` (CR 609.7b–c), using last
    /// known information for a source that has left the battlefield.
    fn source_matches(
        &self,
        f: &ObjFilter,
        src: ObjId,
        source: ObjId,
        controller: PlayerId,
    ) -> bool {
        let Some(c) = self.characteristics_or_lki(src) else {
            return false;
        };
        let Some(ctl) = self
            .objects
            .get(src)
            .map(|o| o.controller)
            .or_else(|| self.lki.get(&src).map(|l| l.object.controller))
        else {
            return false;
        };
        if f.enchanted && !self.enchants(source, src) {
            return false;
        }
        // State ("attacking", "unblocked") is judged while the source is on
        // the battlefield; one that has left has none.
        let on_battlefield = self
            .objects
            .get(src)
            .is_some_and(|o| o.zone == Zone::Battlefield);
        f.matches(src, ctl, &c, Some(source), Some(controller))
            && (!f.needs_state() || (on_battlefield && self.status_ok(f, src)))
    }

    /// Apply one effect to the event (CR 614.6, 615.6), then its additional
    /// effects (CR 615.5).
    #[allow(clippy::too_many_lines)] // one arm per (replacement, event) pair
    fn apply_replacement(&mut self, c: &Candidate, ev: Proposed) -> Outcome {
        let unpreventable = self.rule_in_force(RuleKind::DamageCantBePrevented);
        let outcome = match (c.ability.replacement, ev) {
            (
                Replacement::Prevent,
                Proposed::Damage {
                    source,
                    target,
                    amount,
                    combat,
                },
            ) => {
                // CR 615.12: unpreventable damage is still "applied to", but
                // nothing is prevented.
                if unpreventable {
                    Outcome::Event(Proposed::Damage {
                        source,
                        target,
                        amount,
                        combat,
                    })
                } else {
                    self.emit(Event::Prevented {
                        source,
                        target,
                        amount,
                        by: c.def,
                    });
                    Outcome::Nothing
                }
            }
            (
                Replacement::PreventNext(_),
                Proposed::Damage {
                    source,
                    target,
                    amount,
                    combat,
                },
            ) => {
                let prevented = match c.key {
                    Key::Stored(i) if !unpreventable => {
                        let p = amount.min(self.replacements[i].remaining);
                        self.replacements[i].remaining -= p;
                        p
                    }
                    _ => 0,
                };
                if prevented > 0 {
                    self.emit(Event::Prevented {
                        source,
                        target,
                        amount: prevented,
                        by: c.def,
                    });
                }
                Outcome::Event(Proposed::Damage {
                    source,
                    target,
                    amount: amount - prevented,
                    combat,
                })
            }
            (Replacement::Regenerate, Proposed::Destroy { obj, .. }) => {
                // CR 701.15a: remove all damage, tap, remove from combat.
                if let Key::Stored(i) = c.key {
                    self.replacements[i].remaining = 0;
                }
                if let Some(o) = self.objects.get_mut(obj) {
                    o.damage = 0;
                    o.deathtouch_damage = false;
                }
                self.tap(obj);
                self.remove_from_combat(obj);
                self.sba_touch();
                self.emit(Event::Regenerated { obj, by: c.def });
                Outcome::Nothing
            }
            (
                Replacement::RedirectToAttached,
                Proposed::Damage {
                    source,
                    target,
                    amount,
                    combat,
                },
            ) => {
                let to = self
                    .objects
                    .get(c.source)
                    .and_then(|aura| aura.attached_to)
                    .filter(|t| {
                        self.objects
                            .get(*t)
                            .is_some_and(|o| o.zone == Zone::Battlefield)
                    });
                match to {
                    Some(t) => {
                        self.emit(Event::Redirected {
                            from: target,
                            to: Target::Object(t),
                            amount,
                            by: c.def,
                        });
                        Outcome::Event(Proposed::Damage {
                            source,
                            target: Target::Object(t),
                            amount,
                            combat,
                        })
                    }
                    None => Outcome::Event(Proposed::Damage {
                        source,
                        target,
                        amount,
                        combat,
                    }),
                }
            }
            (Replacement::Prevent, Proposed::GainLife { .. }) => {
                self.emit(Event::Replaced { by: c.def });
                Outcome::Nothing
            }
            (
                Replacement::Double,
                Proposed::Damage {
                    source,
                    target,
                    amount,
                    combat,
                },
            ) => {
                self.emit(Event::Replaced { by: c.def });
                Outcome::Event(Proposed::Damage {
                    source,
                    target,
                    amount: amount.saturating_mul(2),
                    combat,
                })
            }
            (Replacement::Double, Proposed::GainLife { player, amount }) => {
                self.emit(Event::Replaced { by: c.def });
                Outcome::Event(Proposed::GainLife {
                    player,
                    amount: amount.saturating_mul(2),
                })
            }
            (Replacement::PlusOne, Proposed::AddCounters { obj, kind, n }) => {
                self.emit(Event::Replaced { by: c.def });
                Outcome::Event(Proposed::AddCounters {
                    obj,
                    kind,
                    n: n + 1,
                })
            }
            (
                Replacement::PlusOne,
                Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                },
            ) => {
                self.emit(Event::Replaced { by: c.def });
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter: Enter {
                        tapped: enter.tapped,
                        counters: enter.counters.map(|(k, n)| (k, n + 1)),
                    },
                })
            }
            (
                Replacement::Instead(zone),
                Proposed::ZoneChange {
                    obj,
                    to_top,
                    controller,
                    enter,
                    ..
                },
            ) => {
                self.emit(Event::Replaced { by: c.def });
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to: zone,
                    to_top,
                    controller,
                    enter,
                })
            }
            (Replacement::DrawTwo, Proposed::Draw { player }) => {
                self.emit(Event::Replaced { by: c.def });
                Outcome::Events(smallvec![
                    Proposed::Draw { player },
                    Proposed::Draw { player }
                ])
            }
            (Replacement::WinGame, Proposed::Draw { player }) => {
                self.emit(Event::Replaced { by: c.def });
                self.win_game(player);
                Outcome::Nothing
            }
            (
                Replacement::EnterWithCountersIf { cond, kind, n },
                Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                },
            ) => {
                let counters = if self.condition_now(self.pool.condition(cond), controller, obj) {
                    Some((kind, enter.counters.map_or(0, |(_, m)| m) + n))
                } else {
                    enter.counters
                };
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter: Enter {
                        tapped: enter.tapped,
                        counters,
                    },
                })
            }
            (
                Replacement::EnterTappedUnless(cond),
                Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                },
            ) => {
                let tapped =
                    enter.tapped || !self.condition_now(self.pool.condition(cond), controller, obj);
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter: Enter {
                        tapped,
                        counters: enter.counters,
                    },
                })
            }
            (
                Replacement::EnterTapped,
                Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                },
            ) => Outcome::Event(Proposed::ZoneChange {
                obj,
                to,
                to_top,
                controller,
                enter: Enter {
                    tapped: true,
                    counters: enter.counters,
                },
            }),
            (
                Replacement::EnterWithCounters { kind, n },
                Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                },
            ) => {
                // X of the entering spell (Endless One, CR 107.3).
                let n = n.eval(self.x_of(obj));
                let counters = match enter.counters {
                    Some((k, m)) if k == kind => Some((k, m + n)),
                    Some(other) => Some(other),
                    None => Some((kind, n)),
                };
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter: Enter {
                        tapped: enter.tapped,
                        counters,
                    },
                })
            }
            // `applies` only offers matching pairs.
            (_, ev) => Outcome::Event(ev),
        };
        if !c.ability.then.is_empty() {
            let mut ctx = EffectCtx {
                source: c.source,
                controller: c.controller,
                targets: Vec::new(),
                division: Vec::new(),
                legal: Vec::new(),
                trigger_obj: None,
                trigger_player: None,
                trigger_amount: 0,
                created: None,
                x: 0,
                did: true,
            };
            for effect in &c.ability.then {
                self.apply_effect(&mut ctx, effect);
            }
        }
        outcome
    }

    // ----- decisions in the middle of an event ------------------------------------

    /// CR 616.1: the affected player chooses which of several applicable
    /// effects applies next.
    fn choose_replacement(&mut self, player: PlayerId, candidates: &[Candidate]) -> usize {
        let options = candidates
            .iter()
            .enumerate()
            .map(|(index, c)| Choice::Replacement { index, def: c.def })
            .collect();
        self.choose_mid(player, DecisionKind::ChooseReplacement, options)
    }

    /// A decision needed while an event is being applied. On a replay the
    /// recorded answer is used; otherwise the question is recorded, the
    /// first option stands for now, and the `Engine` rolls back and asks.
    pub(super) fn choose_mid(
        &mut self,
        player: PlayerId,
        kind: DecisionKind,
        options: Vec<Choice>,
    ) -> usize {
        if let Some(a) = self.mid.answers.get(self.mid.next).copied() {
            self.mid.next += 1;
            return (a as usize).min(options.len() - 1);
        }
        if self.mid.pending.is_none() {
            self.mid.pending = Some(Decision {
                player,
                kind,
                options,
            });
        }
        0
    }

    /// CR 104.2b: an effect makes `winner` win; every opponent leaves the
    /// game.
    pub(super) fn win_game(&mut self, winner: PlayerId) {
        self.emit(Event::PlayerWon { player: winner });
        let losers: Vec<PlayerId> = self.players_in_game().filter(|p| *p != winner).collect();
        for p in losers {
            self.player_mut(p).lost = Some(LossReason::OpponentWon);
            self.emit(Event::PlayerLost {
                player: p,
                reason: LossReason::OpponentWon,
            });
        }
        self.bump();
    }
}
