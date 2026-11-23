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
    AbilityDef, AbilityId, Affects, CardRef, CounterKind, Keyword, ObjFilter, RedirectTo,
    Replacement, ReplacementAbility, Replaces, RuleKind, Who,
};
use crate::decision::{Choice, Decision, DecisionKind};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::state::{Chosen, GameState, LossReason, Target, Zone};

/// How a permanent enters the battlefield, as modified by replacement
/// effects (CR 614.1c, 614.12).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Enter {
    pub tapped: bool,
    pub counters: Option<(CounterKind, u32)>,
    /// Counters of a second kind ("two +1/+1 counters and a trample
    /// counter", Voidpouncer).
    pub more: Option<(CounterKind, u32)>,
    /// Riot's haste was chosen (CR 702.136a).
    pub haste: bool,
    /// "This creature enters prepared." (CR 722.3a).
    pub prepared: bool,
    /// The counters it enters with by its own rules (a planeswalker's
    /// loyalty, a Saga's lore counter, modular's +1/+1 counters, ...),
    /// doubled this many times and then `plus` added to each kind, by
    /// replacements of counters of any kind (Doubling Season, CR 122.6).
    pub intrinsic_doubled: u8,
    pub intrinsic_plus: u8,
}

impl Enter {
    /// `n` more counters of `kind`: with those of the same kind already,
    /// else in the first free slot (a third kind is not kept).
    pub(crate) fn add_counters(self, kind: CounterKind, n: u32) -> Self {
        match (self.counters, self.more) {
            (None, _) => Enter {
                counters: Some((kind, n)),
                ..self
            },
            (Some((k, m)), _) if k == kind => Enter {
                counters: Some((k, m + n)),
                ..self
            },
            (_, None) => Enter {
                more: Some((kind, n)),
                ..self
            },
            (_, Some((k, m))) if k == kind => Enter {
                more: Some((k, m + n)),
                ..self
            },
            _ => self,
        }
    }
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
    /// A player would lose life (not by paying it).
    LoseLife {
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
                | Proposed::LoseLife { amount: 0, .. }
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
            | Proposed::LoseLife { player: p, .. }
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

/// What a stored effect was locked to as it was created: the spell's
/// target (CR 611.2c) and the damage source its controller chose
/// (CR 609.7a). Static abilities have neither.
#[derive(Copy, Clone, Default)]
struct Lock {
    target: Option<Target>,
    chosen: Option<ObjId>,
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
    /// The event, then its player shuffles their library (a card shuffled
    /// into its owner's library instead, CR 701.20a).
    ThenShuffle(Proposed, PlayerId),
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
        match ev {
            Proposed::ZoneChange {
                obj,
                to: Zone::Battlefield,
                ..
            } => self.objects.get(*obj).is_some_and(|o| {
                self.flags_of(o)
                    .has(crate::card::DefFlags::MODIFIES_ENTERING_FLAG)
            }),
            // "If this card would be put into a graveyard from anywhere"
            // functions in every zone (CR 113.6b).
            Proposed::ZoneChange {
                obj,
                to: Zone::Graveyard,
                ..
            } => self
                .objects
                .get(*obj)
                .is_some_and(|o| self.flags_of(o).has_replacements()),
            _ => false,
        }
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
            // CR 616.1: the affected player chooses which applies next;
            // a permanent's own "enters tapped" and "enters with counters"
            // give the same result in either order ("This land enters
            // tapped with two depletion counters on it").
            let i = if candidates.len() == 1 || Self::commute(&candidates) {
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
                Outcome::ThenShuffle(e, p) => {
                    let mut a = applied.clone();
                    let new = self.propose_with(e, &mut a);
                    self.shuffle_library(p);
                    return new;
                }
            }
        }
    }

    /// The tokens to create when `n` tokens of `def` would be created under
    /// `p`'s control (CR 111.1, 614.1a): "twice that many of those tokens"
    /// (Parallel Lives), "those tokens plus a 1/1 green Frog creature
    /// token" (Quina). One event per effect: each applicable static
    /// ability of a permanent `p` controls applies once. Of the orders the
    /// player may choose (CR 616.1), the added tokens come first, so a
    /// doubling effect doubles them too.
    pub(super) fn tokens_to_create(
        &mut self,
        p: PlayerId,
        def: CardRef,
        n: u32,
    ) -> SmallVec<[(CardRef, u32); 2]> {
        let mut out: SmallVec<[(CardRef, u32); 2]> = smallvec![(def, n)];
        if self.replacement_sources == 0 || n == 0 {
            return out;
        }
        // (source, replacement, creature tokens only, artifact tokens only)
        let mut found: SmallVec<[(CardRef, Replacement, bool, bool); 2]> = SmallVec::new();
        let granted = self.grant_sources > 0;
        for id in &self.battlefield {
            let o = &self.objects[*id];
            if o.controller != p || (!granted && !self.flags_of(o).has_replacements()) {
                continue;
            }
            let Some(c) = self.characteristics(*id) else {
                continue;
            };
            for ability in c.abilities.iter().copied() {
                if let AbilityDef::Replacement(ReplacementAbility {
                    replaces:
                        Replaces::CreateTokens {
                            creature, artifact, ..
                        },
                    replacement,
                    ..
                }) = self.pool.ability(ability)
                {
                    found.push((o.def, *replacement, *creature, *artifact));
                }
            }
        }
        if found.is_empty() {
            return out;
        }
        let covers = |state: &GameState, d: CardRef, creature: bool, artifact: bool| {
            let types = state.pool.get(d).types;
            (!creature || types.contains(crate::card::CardType::Creature))
                && (!artifact || types.contains(crate::card::CardType::Artifact))
        };
        if !found.iter().any(|(_, _, c, a)| covers(self, def, *c, *a)) {
            return out;
        }
        // Tokens instead of others first, then added tokens, then doubling
        // (the player's order, CR 616.1, is fixed here).
        found.sort_by_key(|(_, r, _, _)| match r {
            Replacement::TokensInstead { .. } => 0,
            Replacement::Double => 2,
            _ => 1,
        });
        for (by, r, creature, artifact) in found {
            let total: u32 = out
                .iter()
                .filter(|(d, _)| covers(self, *d, creature, artifact))
                .map(|(_, k)| *k)
                .sum();
            if total == 0 {
                continue;
            }
            match r {
                Replacement::Double => {
                    for (d, k) in &mut out {
                        if covers(self, *d, creature, artifact) {
                            *k = k.saturating_mul(2);
                        }
                    }
                }
                Replacement::PlusToken { token, that_many } => {
                    out.push((token, if that_many { total } else { 1 }));
                }
                Replacement::TokensInstead { token } => {
                    for (d, _) in &mut out {
                        if covers(self, *d, creature, artifact) {
                            *d = token;
                        }
                    }
                }
                _ => continue,
            }
            self.emit(Event::Replaced { by });
        }
        out
    }

    /// Every replacement or prevention effect that applies to `ev` and has
    /// not yet applied, in a stable order: battlefield statics, the
    /// entering permanent's own, then stored effects.
    #[allow(clippy::too_many_lines)] // one block per place effects come from
    fn candidates(&self, ev: &Proposed, applied: &Applied) -> Vec<Candidate> {
        let mut out = Vec::new();
        if self.replacement_sources > 0 {
            // A permanent may have a replacement ability granted to it
            // (absorb from Lymph Sliver).
            let granted = self.grant_sources > 0;
            for id in &self.battlefield {
                let o = &self.objects[*id];
                if !granted && !self.flags_of(o).has_replacements() {
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
                    if self.applies(r, ev, *id, o.controller, Lock::default()) {
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
        // CR 113.6b: "if this card would be put into a graveyard from
        // anywhere" works off the battlefield too (on it, it was found
        // above).
        if let Proposed::ZoneChange {
            obj,
            to: Zone::Graveyard,
            ..
        } = ev
            && let Some(o) = self.objects.get(*obj)
            && o.zone != Zone::Battlefield
            && self.flags_of(o).has_replacements()
            && let Some(c) = self.characteristics(*obj)
        {
            for ability in c.abilities.iter().copied() {
                let AbilityDef::Replacement(r) = self.pool.ability(ability) else {
                    continue;
                };
                let key = Key::Ability {
                    source: *obj,
                    ability,
                };
                if matches!(r.replaces, Replaces::ThisToGraveyard) && !applied.contains(&key) {
                    out.push(Candidate {
                        key,
                        def: o.def,
                        source: *obj,
                        controller: o.owner,
                        ability: r.clone(),
                    });
                }
            }
        }
        for (i, e) in self.replacements.iter().enumerate() {
            let key = Key::Stored(i);
            if applied.contains(&key) {
                continue;
            }
            // An exhausted shield prevents nothing more (CR 615.7); a used
            // regeneration shield is gone (CR 701.15a); so is a redirection
            // of the next N damage once N have been redirected.
            if matches!(
                e.ability.replacement,
                Replacement::PreventNext(_)
                    | Replacement::PreventNextTime
                    | Replacement::DestroyNextTime
                    | Replacement::Regenerate
                    | Replacement::Redirect { next: Some(_), .. }
            ) && e.remaining == 0
            {
                continue;
            }
            let source = e.source.unwrap_or_default();
            let lock = Lock {
                target: e.target,
                chosen: e.chosen,
            };
            if self.applies(&e.ability, ev, source, e.controller, lock) {
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
    /// `lock` if stored by a spell or ability) applies to `ev`.
    #[allow(clippy::too_many_lines)] // one arm per replaced event
    fn applies(
        &self,
        r: &ReplacementAbility,
        ev: &Proposed,
        source: ObjId,
        controller: PlayerId,
        lock: Lock,
    ) -> bool {
        let whose_ok = |w: Who, p: PlayerId| match w {
            Who::Any => true,
            Who::You => p == controller,
            Who::Opponents => p != controller,
        };
        if let Some(c) = r.condition
            && !self.condition_now(self.pool.condition(c), controller, source)
        {
            return false;
        }
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
                        .is_none_or(|f| self.source_is(f, *src, source, controller, lock))
                    && self.affects(to, *target, source, controller, lock)
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
                    && (self.affects(what, *target, source, controller, lock)
                        || self.source_is(what, *src, source, controller, lock))
            }
            (Replaces::GainLife { whose }, Proposed::GainLife { player, .. })
            | (Replaces::LoseLife { whose }, Proposed::LoseLife { player, .. }) => {
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
                Replaces::ThisToGraveyard,
                Proposed::ZoneChange {
                    obj,
                    to: Zone::Graveyard,
                    ..
                },
            ) => *obj == source,
            (
                Replaces::FilteredToGraveyard { filter },
                Proposed::ZoneChange {
                    obj,
                    to: Zone::Graveyard,
                    ..
                },
            ) => self.objects.get(*obj).is_some_and(|o| {
                // A token isn't a card (CR 108.2).
                o.kind == crate::state::ObjKind::Card
                    && filter.matches(
                        *obj,
                        o.controller,
                        &self.base_characteristics(o),
                        Some(source),
                        Some(controller),
                    )
            }),
            (
                Replaces::PlusCounters { on },
                Proposed::AddCounters {
                    obj,
                    kind: CounterKind::PlusOnePlusOne,
                    n,
                },
            ) => *n > 0 && self.filter_matches(on, *obj, source, controller),
            // "If one or more counters would be put on a permanent you
            // control" (Doubling Season, Loading Zone): any kind.
            (Replaces::AnyCounters { on, by }, Proposed::AddCounters { obj, n, .. }) => {
                *n > 0
                    && match by {
                        crate::card::CountersBy::Any => true,
                        crate::card::CountersBy::Effect => self.effect_controller.is_some(),
                        crate::card::CountersBy::You => {
                            self.effect_controller
                                .or_else(|| self.objects.get(*obj).map(|o| o.controller))
                                == Some(controller)
                        }
                    }
                    && self.filter_matches(on, *obj, source, controller)
            }
            (
                Replaces::AnyCounters { on, by },
                Proposed::ZoneChange {
                    obj,
                    to: Zone::Battlefield,
                    controller: ctl,
                    enter,
                    ..
                },
            ) => {
                // CR 122.6: counters it enters with count as being put on
                // it, by the replacement effect that has it enter so (or
                // by its own rules: a planeswalker's loyalty).
                (enter.counters.is_some_and(|(_, n)| n > 0)
                    || self
                        .objects
                        .get(*obj)
                        .is_some_and(|o| self.enters_with_own_counters(o.def)))
                    && (*by != crate::card::CountersBy::You
                        || self.effect_controller.unwrap_or(*ctl) == controller)
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
            // "Each Dragon you control enters with an additional +1/+1
            // counter on it" (CR 614.1c).
            (
                Replaces::OthersEnter { what },
                Proposed::ZoneChange {
                    obj,
                    to: Zone::Battlefield,
                    controller: ctl,
                    ..
                },
            ) => {
                *obj != source
                    && self.objects.get(*obj).is_some_and(|o| {
                        o.zone != Zone::Battlefield
                            && what.matches(
                                *obj,
                                *ctl,
                                &self.printed(o.def),
                                Some(source),
                                Some(controller),
                            )
                    })
            }
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
            // Umbra armor (CR 702.89a) isn't regeneration: it applies to a
            // permanent that can't be regenerated, but not to one that is
            // indestructible (it wouldn't be destroyed).
            (Replaces::Destroy { to }, Proposed::Destroy { obj, .. })
                if r.replacement == Replacement::UmbraArmor =>
            {
                self.characteristics(*obj)
                    .is_some_and(|c| !c.has(Keyword::Indestructible))
                    && self.affects(to, Target::Object(*obj), source, controller, lock)
            }
            (
                Replaces::Destroy { to },
                Proposed::Destroy {
                    obj,
                    regenerate: true,
                },
            ) => self.affects(to, Target::Object(*obj), source, controller, lock),
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
                    // CR 700.4: a planeswalker dies too; a targeted one
                    // ("If that creature or planeswalker would die") counts.
                    && self.characteristics(*obj).is_some_and(|c| {
                        c.is_creature()
                            || (c.is_planeswalker() && matches!(to, Affects::Target(_)))
                    })
                    && self.affects(to, Target::Object(*obj), source, controller, lock)
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
        lock: Lock,
    ) -> bool {
        let locked = lock.target;
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
            Affects::OpponentsAnd(f) => match target {
                Target::Player(p) => p != controller,
                Target::Object(id) => self.filter_matches(f, id, source, controller),
            },
            Affects::Target(_) | Affects::Targets(..) | Affects::DamagedThisWay => {
                locked == Some(target)
            }
            Affects::OtherThanTarget(f, _) => {
                locked != Some(target)
                    && matches!(target, Target::Object(id) if self.filter_matches(f, id, source, controller))
            }
            // A chosen source is never a recipient.
            Affects::Chosen(_) => false,
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
        lock: Lock,
    ) -> bool {
        let locked = lock.target;
        match from {
            Affects::Any => true,
            Affects::This => src == source,
            Affects::You | Affects::Players(_) => false,
            Affects::Objects(f) | Affects::YouAnd(f) | Affects::OpponentsAnd(f) => {
                self.source_matches(f, src, source, controller)
            }
            Affects::Target(_) | Affects::Targets(..) | Affects::DamagedThisWay => {
                locked == Some(Target::Object(src))
            }
            Affects::OtherThanTarget(f, _) => {
                locked != Some(Target::Object(src))
                    && self.source_matches(f, src, source, controller)
            }
            // CR 609.7a: the source chosen as the effect was created.
            Affects::Chosen(_) => lock.chosen == Some(src),
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
        let f = &*self.with_source_chosen(f, source);
        self.characteristics(id)
            .is_some_and(|c| f.matches(id, o.controller, &c, Some(source), Some(controller)))
            && self.status_ok(f, id)
            // "a creature dealt damage by this creature this turn"
            // (Frostwielder): the source dealt it damage this turn.
            && (!f.status.damaged_by_source || self.damaged_by_this_turn.contains(&(source, id)))
    }

    /// `f` with "of the chosen color" filled in from what `source` chose as
    /// it entered (CR 607.2a: Prismatic Ward).
    pub(super) fn with_source_chosen<'f>(
        &self,
        f: &'f ObjFilter,
        source: ObjId,
    ) -> std::borrow::Cow<'f, ObjFilter> {
        if !f.chosen {
            return std::borrow::Cow::Borrowed(f);
        }
        let chosen = self
            .objects
            .get(source)
            .map(|o| o.chosen)
            .or_else(|| self.lki.get(&source).map(|l| l.object.chosen))
            .unwrap_or_default();
        std::borrow::Cow::Owned(f.with_chosen(chosen))
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
        let f = &*self.with_source_chosen(f, source);
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
        let unpreventable = self.rule_in_force(RuleKind::DamageCantBePrevented)
            || self.this_way.as_ref().is_some_and(|l| l.unpreventable);
        // "Prevent that damage and each opponent mills that many cards":
        // the replaced event's damage is the follow-up's "that much".
        let that_much = match &ev {
            Proposed::Damage { amount, .. } => *amount,
            _ => 0,
        };
        // The damaged permanent, "that creature" in what follows ("Put a
        // +1/+1 counter on that creature for each 1 damage prevented this
        // way", Vigor).
        let damaged = match &ev {
            Proposed::Damage {
                target: Target::Object(id),
                ..
            } => Some(*id),
            _ => None,
        };
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
                let prevented = match (c.key, c.ability.replacement) {
                    (Key::Stored(i), _) if !unpreventable => {
                        let p = amount.min(self.replacements[i].remaining);
                        self.replacements[i].remaining -= p;
                        p
                    }
                    // A static ability prevents N of each damage event, not
                    // a shield (absorb, CR 702.64a).
                    (Key::Ability { .. }, Replacement::PreventNext(crate::card::Num::N(n)))
                        if !unpreventable =>
                    {
                        amount.min(n)
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
            // "destroy that creature instead": the damage isn't dealt, and
            // the shield is used up (CR 614.1a).
            (
                Replacement::DestroyNextTime,
                Proposed::Damage {
                    target: Target::Object(id),
                    ..
                },
            ) => {
                if let Key::Stored(i) = c.key {
                    self.replacements[i].remaining = 0;
                }
                self.emit(Event::Replaced { by: c.def });
                self.destroy(id);
                Outcome::Nothing
            }
            (
                Replacement::PreventNextTime,
                Proposed::Damage {
                    source,
                    target,
                    amount,
                    ..
                },
            ) => {
                // CR 615.7: the shield applies to one damage event and is
                // used up, even where nothing could be prevented
                // (CR 615.12).
                if let Key::Stored(i) = c.key {
                    self.replacements[i].remaining = 0;
                }
                if unpreventable {
                    Outcome::Event(ev)
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
                Replacement::Redirect { to, next },
                Proposed::Damage {
                    source,
                    target,
                    amount,
                    combat,
                },
            ) => {
                let dest = match (to, c.key) {
                    (RedirectTo::This, _) => Some(Target::Object(c.source)),
                    (RedirectTo::Target(_), Key::Stored(i)) => self.replacements[i].redirect,
                    (RedirectTo::Target(_), Key::Ability { .. }) => None,
                };
                // CR 614.9: the effect does nothing if the new recipient has
                // left the battlefield, is no longer a creature or
                // planeswalker, or is a player who has left the game.
                match dest.filter(|d| self.can_be_redirected_to(*d)) {
                    None => Outcome::Event(ev),
                    Some(d) => {
                        let moved = match (next, c.key) {
                            (Some(_), Key::Stored(i)) => {
                                let m = amount.min(self.replacements[i].remaining);
                                self.replacements[i].remaining -= m;
                                m
                            }
                            _ => amount,
                        };
                        self.emit(Event::Redirected {
                            from: target,
                            to: d,
                            amount: moved,
                            by: c.def,
                        });
                        let redirected = Proposed::Damage {
                            source,
                            target: d,
                            amount: moved,
                            combat,
                        };
                        if moved == amount {
                            Outcome::Event(redirected)
                        } else {
                            // The rest is still dealt to the original
                            // recipient.
                            Outcome::Events(smallvec![
                                redirected,
                                Proposed::Damage {
                                    source,
                                    target,
                                    amount: amount - moved,
                                    combat,
                                }
                            ])
                        }
                    }
                }
            }
            (Replacement::UmbraArmor, Proposed::Destroy { obj, .. }) => {
                // CR 702.89a: remove all damage from it and destroy this
                // Aura instead.
                if let Some(o) = self.objects.get_mut(obj) {
                    o.damage = 0;
                    o.deathtouch_damage = false;
                }
                self.sba_touch();
                Outcome::Event(Proposed::Destroy {
                    obj: c.source,
                    regenerate: true,
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
            (
                Replacement::PlusN { n, .. },
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
                    amount: amount.saturating_add(n),
                    combat,
                })
            }
            (
                Replacement::PlusCount(n),
                Proposed::Damage {
                    source,
                    target,
                    amount,
                    combat,
                },
            ) => {
                let more = self.eval_num_of(n, c.source, c.controller);
                self.emit(Event::Replaced { by: c.def });
                Outcome::Event(Proposed::Damage {
                    source,
                    target,
                    amount: amount.saturating_add(more),
                    combat,
                })
            }
            // "twice that many +1/+1 counters are put on that creature
            // instead" (Branching Evolution), also as it enters (CR 122.6).
            (Replacement::Double, Proposed::AddCounters { obj, kind, n }) => {
                self.emit(Event::Replaced { by: c.def });
                Outcome::Event(Proposed::AddCounters {
                    obj,
                    kind,
                    n: n.saturating_mul(2),
                })
            }
            (
                Replacement::Double,
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
                        counters: enter.counters.map(|(k, n)| (k, n.saturating_mul(2))),
                        more: enter.more.map(|(k, n)| (k, n.saturating_mul(2))),
                        intrinsic_doubled: enter.intrinsic_doubled
                            + u8::from(matches!(c.ability.replaces, Replaces::AnyCounters { .. })),
                        ..enter
                    },
                })
            }
            // "they lose twice that much life instead".
            (Replacement::Double, Proposed::LoseLife { player, amount }) => {
                self.emit(Event::Replaced { by: c.def });
                Outcome::Event(Proposed::LoseLife {
                    player,
                    amount: amount.saturating_mul(2),
                })
            }
            (Replacement::Double, Proposed::GainLife { player, amount }) => {
                self.emit(Event::Replaced { by: c.def });
                Outcome::Event(Proposed::GainLife {
                    player,
                    amount: amount.saturating_mul(2),
                })
            }
            // "you gain that much life plus 1 instead" (Angel of Vitality).
            (Replacement::PlusOne, Proposed::GainLife { player, amount }) => {
                self.emit(Event::Replaced { by: c.def });
                Outcome::Event(Proposed::GainLife {
                    player,
                    amount: amount.saturating_add(1),
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
                        counters: enter.counters.map(|(k, n)| (k, n + 1)),
                        // "that many plus one of each of those kinds".
                        more: if matches!(c.ability.replaces, Replaces::AnyCounters { .. }) {
                            enter.more.map(|(k, n)| (k, n + 1))
                        } else {
                            enter.more
                        },
                        intrinsic_plus: enter.intrinsic_plus
                            + u8::from(matches!(c.ability.replaces, Replaces::AnyCounters { .. })),
                        ..enter
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
            // "put it on top of its owner's library instead" (Gravebane
            // Zombie) or "on the bottom" (Nissa's Chosen).
            (
                Replacement::IntoLibrary { top },
                Proposed::ZoneChange {
                    obj,
                    controller,
                    enter,
                    ..
                },
            ) => {
                self.emit(Event::Replaced { by: c.def });
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to: Zone::Library,
                    to_top: top,
                    controller,
                    enter,
                })
            }
            // "reveal this card and shuffle it into its owner's library
            // instead" (Darksteel Colossus).
            (
                Replacement::ShuffleIntoLibrary,
                Proposed::ZoneChange {
                    obj,
                    controller,
                    enter,
                    ..
                },
            ) => {
                self.emit(Event::Replaced { by: c.def });
                let owner = self.objects[obj].owner;
                Outcome::ThenShuffle(
                    Proposed::ZoneChange {
                        obj,
                        to: Zone::Library,
                        to_top: true,
                        controller,
                        enter,
                    },
                    owner,
                )
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
                let enter = if self.condition_now(self.pool.condition(cond), controller, obj) {
                    enter.add_counters(kind, n)
                } else {
                    enter
                };
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
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
                    enter: Enter { tapped, ..enter },
                })
            }
            (
                Replacement::EnterTappedUnlessPay(life),
                Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                },
            ) => {
                // CR 119.4: only a player with that much life can pay it.
                let def = self.objects[obj].def;
                let paid = self.player(controller).life >= i32::try_from(life).unwrap_or(i32::MAX)
                    && self.choose_mid(
                        controller,
                        DecisionKind::YouMay { def },
                        vec![Choice::Accept, Choice::Decline],
                    ) == 0;
                if paid {
                    self.change_life(controller, -i32::try_from(life).unwrap_or(i32::MAX));
                }
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter: Enter {
                        tapped: enter.tapped || !paid,
                        ..enter
                    },
                })
            }
            (
                Replacement::EnterTappedUnlessReveal { filter, or },
                Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                },
            ) => {
                // CR 701.20a: a matching card in the hand (not the land
                // itself, still there while it is played), judged as
                // printed; the engine picks the first such card.
                let f = self.pool.filter(filter).clone();
                let card = self.player(controller).hand.iter().copied().find(|&id| {
                    id != obj && {
                        let c = self.base_characteristics(&self.objects[id]);
                        f.matches(id, controller, &c, Some(obj), Some(controller))
                    }
                });
                let def = self.objects[obj].def;
                let revealed = card.is_some()
                    && self.choose_mid(
                        controller,
                        DecisionKind::YouMay { def },
                        vec![Choice::Accept, Choice::Decline],
                    ) == 0;
                if revealed && let Some(id) = card {
                    self.reveal_in_hand(id);
                }
                // "unless you revealed a Dragon card this way or you
                // control a Dragon".
                let untapped = revealed
                    || or.is_some_and(|c| {
                        self.condition_now(self.pool.condition(c), controller, obj)
                    });
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter: Enter {
                        tapped: enter.tapped || !untapped,
                        ..enter
                    },
                })
            }
            (
                Replacement::EnterPrepared,
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
                    prepared: true,
                    ..enter
                },
            }),
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
                    ..enter
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
                // X of the entering spell (Endless One, CR 107.3); a count
                // ("for each other creature you control", converge's "for
                // each color of mana spent to cast it") judged for the
                // entering object as it enters.
                // Another permanent's "Each other Angel you control enters
                // with ... for each Angel you already control" (Giada):
                // counted for that permanent, by its card's counts.
                let other = c.source != obj;
                let n = match n {
                    crate::card::Num::Count(_) => {
                        let ctx = EffectCtx {
                            source: if other { c.source } else { obj },
                            controller: if other { c.controller } else { controller },
                            targets: Vec::new(),
                            division: Vec::new(),
                            legal: Vec::new(),
                            trigger_obj: None,
                            trigger_player: None,
                            trigger_amount: 0,
                            created: None,
                            revealed: Vec::new(),
                            x: self.x_of(obj),
                            did: true,
                            chosen: Chosen::None,
                            couldnt: 0,
                            created_tokens: SmallVec::new(),
                            counts: other.then_some(c.def),
                        };
                        self.eval_num(n, &ctx)
                    }
                    n => n.eval(self.x_of(obj)),
                };
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter: enter.add_counters(kind, n),
                })
            }
            // CR 702.98a: "You may have this permanent enter with an
            // additional +1/+1 counter on it."
            // CR 702.136a: riot, "If you don't, it gains haste."
            (
                Replacement::EnterWithCountersMay { kind, n, haste },
                Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                },
            ) => {
                let def = self.objects[obj].def;
                let take = self.choose_mid(
                    controller,
                    DecisionKind::YouMay { def },
                    vec![Choice::Accept, Choice::Decline],
                ) == 0;
                let haste = enter.haste || (haste && !take);
                let enter = if take {
                    enter.add_counters(kind, n)
                } else {
                    enter
                };
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter: Enter { haste, ..enter },
                })
            }
            (
                Replacement::EnterWithCounterChoice { kinds, cond },
                Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                },
            ) => {
                let applies = cond
                    .is_none_or(|c| self.condition_now(self.pool.condition(c), controller, obj));
                let enter = if applies {
                    let def = self.objects[obj].def;
                    let i = self.choose_mid(
                        controller,
                        DecisionKind::ChooseOption { def },
                        vec![Choice::Mode(0), Choice::Mode(1)],
                    );
                    enter.add_counters(kinds[i.min(1)], 1)
                } else {
                    enter
                };
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                })
            }
            // CR 702.82a: devour, "you may sacrifice any number of
            // creatures. This permanent enters with N times that many +1/+1
            // counters on it."
            (
                Replacement::EnterDevour { kind, n },
                Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                },
            ) => {
                let mut cands: Vec<ObjId> = self
                    .battlefield
                    .iter()
                    .copied()
                    .filter(|id| {
                        *id != obj
                            && self.objects[*id].controller == controller
                            && self
                                .characteristics(*id)
                                .is_some_and(|c| c.types.contains(kind))
                    })
                    .collect();
                let eaten = self.choose_some(controller, &mut cands, Choice::Sacrifice);
                for id in &eaten {
                    self.sacrifice(*id);
                    self.enter_notes
                        .push((obj, crate::state::EnterNote::Devoured(*id)));
                }
                let m = n * u32::try_from(eaten.len()).unwrap_or(0);
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter: Enter {
                        counters: plus_counters(enter.counters, m),
                        ..enter
                    },
                })
            }
            // CR 702.38a: amplify, "reveal any number of cards from your
            // hand that share a creature type with it. This permanent
            // enters with N +1/+1 counters on it for each card revealed
            // this way."
            (
                Replacement::EnterAmplify(n),
                Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                },
            ) => {
                let me = self.base_characteristics(&self.objects[obj]);
                let mut cands: Vec<ObjId> = self
                    .player(controller)
                    .hand
                    .iter()
                    .copied()
                    .filter(|id| {
                        *id != obj && {
                            let c = self.base_characteristics(&self.objects[*id]);
                            c.subtypes
                                .iter()
                                .any(|t| t.is_creature_type() && me.has_subtype(*t))
                                || me
                                    .subtypes
                                    .iter()
                                    .any(|t| t.is_creature_type() && c.has_subtype(*t))
                        }
                    })
                    .collect();
                let shown = self.choose_some(controller, &mut cands, |card| Choice::Discard {
                    card,
                    def: crate::card::CardRef(0),
                });
                for id in &shown {
                    self.reveal_in_hand(*id);
                }
                let m = n * u32::try_from(shown.len()).unwrap_or(0);
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter: Enter {
                        counters: plus_counters(enter.counters, m),
                        ..enter
                    },
                })
            }
            // CR 702.104a: tribute, "As this creature enters, choose an
            // opponent. That player may put an additional N +1/+1 counters
            // on it as it enters."
            (
                Replacement::EnterTribute(n),
                Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                },
            ) => {
                let opponents: Vec<PlayerId> = self
                    .players_in_game()
                    .filter(|p| *p != controller)
                    .collect();
                let def = self.objects[obj].def;
                let chosen = match opponents.as_slice() {
                    [] => None,
                    [one] => Some(*one),
                    many => {
                        let options = many
                            .iter()
                            .map(|p| Choice::Target(crate::state::Target::Player(*p)))
                            .collect();
                        let k = self.choose_mid(
                            controller,
                            DecisionKind::ChooseOption { def },
                            options,
                        );
                        many.get(k).copied()
                    }
                };
                let paid = chosen.is_some_and(|p| {
                    self.choose_mid(
                        p,
                        DecisionKind::YouMay { def },
                        vec![Choice::Accept, Choice::Decline],
                    ) == 0
                });
                if paid {
                    self.enter_notes
                        .push((obj, crate::state::EnterNote::TributePaid));
                }
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter: Enter {
                        counters: if paid {
                            plus_counters(enter.counters, n)
                        } else {
                            enter.counters
                        },
                        ..enter
                    },
                })
            }
            // CR 702.155a: read ahead, the chapter to start at.
            (
                Replacement::EnterReadAhead,
                Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter,
                },
            ) => {
                let def = self.objects[obj].def;
                let last = self.pool.get(def).final_chapter;
                let k = if last > 1 {
                    let options = (0..usize::from(last)).map(Choice::Mode).collect();
                    self.choose_mid(controller, DecisionKind::ChooseOption { def }, options)
                } else {
                    0
                };
                // One more comes with every Saga (CR 714.3a).
                let more = u32::try_from(k).unwrap_or(0);
                Outcome::Event(Proposed::ZoneChange {
                    obj,
                    to,
                    to_top,
                    controller,
                    enter: Enter {
                        counters: match enter.counters {
                            _ if more == 0 => enter.counters,
                            Some((CounterKind::Lore, m)) => Some((CounterKind::Lore, m + more)),
                            Some(other) => Some(other),
                            None => Some((CounterKind::Lore, more)),
                        },
                        ..enter
                    },
                })
            }
            // `applies` only offers matching pairs.
            (_, ev) => Outcome::Event(ev),
        };
        if !c.ability.then.is_empty() {
            // CR 615.5: a prevention effect's additional effects see "the
            // damage prevented this way": what the event lost (a shield
            // prevents only part, CR 615.7; unpreventable damage, none,
            // CR 615.12).
            let that_much = match (c.ability.replacement, &outcome) {
                (
                    Replacement::Prevent
                    | Replacement::PreventNext(_)
                    | Replacement::PreventNextTime,
                    Outcome::Event(Proposed::Damage { amount, .. }),
                ) => that_much.saturating_sub(*amount),
                _ => that_much,
            };
            let mut ctx = EffectCtx {
                source: c.source,
                controller: c.controller,
                targets: Vec::new(),
                division: Vec::new(),
                legal: Vec::new(),
                trigger_obj: damaged,
                trigger_player: None,
                trigger_amount: that_much,
                created: None,
                revealed: Vec::new(),
                x: 0,
                did: true,
                chosen: Chosen::None,
                couldnt: 0,
                created_tokens: SmallVec::new(),
                counts: None,
            };
            for effect in &c.ability.then {
                self.apply_effect(&mut ctx, effect);
            }
        }
        outcome
    }

    /// Whether damage can be redirected to `t` (CR 614.9): a creature or
    /// planeswalker on the battlefield, or a player still in the game.
    fn can_be_redirected_to(&self, t: Target) -> bool {
        match t {
            Target::Player(p) => self.player(p).lost.is_none(),
            Target::Object(id) => {
                self.objects
                    .get(id)
                    .is_some_and(|o| o.zone == Zone::Battlefield)
                    && self.characteristics(id).is_some_and(|c| {
                        c.is_creature() || c.types.contains(crate::card::CardType::Planeswalker)
                    })
            }
        }
    }

    /// Whether these candidates are all plain "enters tapped" (also
    /// conditional: the condition reads no other replacement's result)
    /// and "enters with counters" effects, whose order changes nothing.
    fn commute(candidates: &[Candidate]) -> bool {
        candidates.iter().all(|c| {
            c.ability.then.is_empty()
                && matches!(c.ability.replaces, crate::card::Replaces::Enters)
                && matches!(
                    c.ability.replacement,
                    crate::card::Replacement::EnterTapped
                        | crate::card::Replacement::EnterTappedUnless(_)
                        | crate::card::Replacement::EnterPrepared
                        | crate::card::Replacement::EnterWithCounters { .. }
                        | crate::card::Replacement::EnterWithCountersIf { .. }
                        | crate::card::Replacement::EnterWithCountersMay { .. }
                )
        })
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

    /// "Any number of" `cands`, chosen one at a time as an event is
    /// applied (devour, amplify): the first option, the default, is to
    /// stop.
    fn choose_some(
        &mut self,
        player: PlayerId,
        cands: &mut Vec<ObjId>,
        option: impl Fn(ObjId) -> Choice,
    ) -> Vec<ObjId> {
        let mut out = Vec::new();
        while !cands.is_empty() {
            let mut options = vec![Choice::Done];
            options.extend(cands.iter().map(|id| match option(*id) {
                Choice::Discard { card, .. } => Choice::Discard {
                    card,
                    def: self.objects[card].def,
                },
                c => c,
            }));
            let k = self.choose_mid(player, DecisionKind::ChooseCard, options.clone());
            let (Choice::Sacrifice(id) | Choice::Discard { card: id, .. }) = options[k] else {
                break;
            };
            cands.retain(|c| *c != id);
            out.push(id);
        }
        out
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
        // "Your opponents can't win the game" (Platinum Angel).
        if self
            .players_in_game()
            .any(|p| p != winner && self.cant_lose(p))
        {
            return;
        }
        self.emit(Event::PlayerWon { player: winner });
        let losers: Vec<PlayerId> = self.players_in_game().filter(|p| *p != winner).collect();
        for p in losers {
            self.player_mut(p).lost = Some(LossReason::OpponentWon);
            self.emit(Event::PlayerLost {
                player: p,
                reason: LossReason::OpponentWon,
            });
        }
        self.cbump();
    }
}

/// `m` more +1/+1 counters on an entering permanent (devour, amplify,
/// tribute), with any it already enters with.
fn plus_counters(counters: Option<(CounterKind, u32)>, m: u32) -> Option<(CounterKind, u32)> {
    match counters {
        _ if m == 0 => counters,
        Some((k, c)) if k == CounterKind::PlusOnePlusOne => Some((k, c + m)),
        Some(other) => Some(other),
        None => Some((CounterKind::PlusOnePlusOne, m)),
    }
}
