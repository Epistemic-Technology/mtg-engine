//! Triggered abilities (CR 603): detection as events are emitted, placement
//! on the stack in APNAP order with the controller choosing among several,
//! and resolution.
//!
//! Detection reads abilities from current characteristics, so an ability
//! removed by Humility does not trigger. Leaves-the-battlefield triggers
//! look back in time (CR 603.10): they use last-known information for the
//! object that left and, during a batch of simultaneous zone changes
//! (`GameState::lookback`), the battlefield as it was before the batch.

use smallvec::SmallVec;

use crate::card::{AbilityDef, AbilityId, Condition, TriggerObj, TriggerSpec, Who};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::state::{
    CastInfo, Characteristics, Chosen, Flow, GameObject, GameState, ObjKind, PendingTrigger,
    StackAbility, Target, Zone,
};

use super::casting::EffectCtx;

/// A permanent whose triggered abilities may see an event.
struct Watcher {
    source: ObjId,
    controller: PlayerId,
    tapped: bool,
    chars: Characteristics,
}

impl GameState {
    // ----- detection ---------------------------------------------------------------

    /// CR 603.2: every triggered ability whose event this is triggers.
    #[inline(never)]
    pub(crate) fn detect_triggers(&mut self, event: &Event) {
        if !self.any_trigger_sources(event) {
            return;
        }
        let watchers = self.watchers(event);
        let event_amount = match event {
            Event::Damage { amount, .. } => *amount,
            _ => 0,
        };
        let mut fired: SmallVec<[PendingTrigger; 2]> = SmallVec::new();
        for w in &watchers {
            let mut abilities: SmallVec<[AbilityId; 8]> =
                w.chars.abilities.iter().copied().collect();
            for k in w.chars.keywords.iter() {
                if let Some(id) = self.pool.keyword_trigger(k) {
                    abilities.push(id);
                }
            }
            for ability in abilities {
                let AbilityDef::Triggered(t) = self.pool.ability(ability) else {
                    continue;
                };
                let Some((trigger_obj, trigger_player)) = self.fires(&t.trigger, event, w) else {
                    continue;
                };
                if let Some(cond) = t.condition
                    && !self.condition_holds_for(cond, w)
                {
                    continue;
                }
                // CR 603.2c: "one or more ... enter", once per batch.
                let batch = matches!(t.trigger, TriggerSpec::EntersBatch(_));
                if batch && self.join_batch(&mut fired, ability, w.source) {
                    continue;
                }
                fired.push(PendingTrigger {
                    controller: w.controller,
                    ability: StackAbility {
                        ability,
                        source: w.source,
                        def: w.chars.def,
                        trigger_obj,
                        trigger_player,
                        trigger_amount: if batch { 1 } else { event_amount },
                    },
                    timestamp: 0,
                });
            }
        }
        if matches!(event, Event::Cycled { .. }) {
            self.cycle_triggers(event, &mut fired);
        }
        // CR 702.62a: each suspended card of the active player's at their
        // upkeep.
        if let Event::StepBegin {
            step: crate::turn::Step::Upkeep,
        } = event
            && self.suspended_count > 0
        {
            self.suspend_triggers(&mut fired);
        }
        // CR 603.7: delayed triggered abilities, each once.
        if let Event::StepBegin { step } = event
            && !self.delayed_triggers.is_empty()
        {
            self.delayed_step_triggers(*step, &mut fired);
        }
        if let Event::ZoneChange {
            old,
            from: Zone::Battlefield,
            to,
            ..
        } = event
            && !self.delayed_triggers.is_empty()
        {
            self.delayed_leave_triggers(*old, *to, &mut fired);
        }
        for mut t in fired {
            t.timestamp = self.tick();
            if self.logging {
                self.log.push(Event::Triggered {
                    source: t.ability.source,
                    ability: t.ability.ability,
                    controller: t.controller,
                });
            }
            self.pending_triggers.push(t);
        }
    }

    /// Delayed triggered abilities waiting for the beginning of `step`
    /// (CR 603.7), each removed as it triggers.
    fn delayed_step_triggers(
        &mut self,
        step: crate::turn::Step,
        fired: &mut SmallVec<[PendingTrigger; 2]>,
    ) {
        let active = self.active_player;
        let mut i = 0;
        while i < self.delayed_triggers.len() {
            let dt = self.delayed_triggers[i];
            let at = match self.pool.ability(dt.ability.ability) {
                AbilityDef::Triggered(t) => match &t.trigger {
                    TriggerSpec::BeginningOfStep { step: s, whose } => {
                        *s == step
                            && match whose {
                                Who::Any => true,
                                Who::You => active == dt.controller,
                                Who::Opponents => active != dt.controller,
                            }
                    }
                    _ => false,
                },
                _ => false,
            };
            if at {
                self.delayed_triggers.remove(i);
                fired.push(PendingTrigger {
                    controller: dt.controller,
                    ability: dt.ability,
                    timestamp: 0,
                });
            } else {
                i += 1;
            }
        }
    }

    /// Delayed triggered abilities waiting for `old` to die or leave the
    /// battlefield ("When that creature dies this turn, ..."; CR 603.7),
    /// each removed as it triggers (CR 603.7c).
    #[cold]
    #[inline(never)]
    fn delayed_leave_triggers(
        &mut self,
        old: ObjId,
        to: Zone,
        fired: &mut SmallVec<[PendingTrigger; 2]>,
    ) {
        let mut i = 0;
        while i < self.delayed_triggers.len() {
            let dt = self.delayed_triggers[i];
            let at = dt.ability.trigger_obj == Some(old)
                && match self.pool.ability(dt.ability.ability) {
                    AbilityDef::Triggered(t) => match &t.trigger {
                        TriggerSpec::Dies(TriggerObj::This) => to == Zone::Graveyard,
                        TriggerSpec::Leaves(TriggerObj::This) => true,
                        _ => false,
                    },
                    _ => false,
                };
            if at {
                self.delayed_triggers.remove(i);
                fired.push(PendingTrigger {
                    controller: dt.controller,
                    ability: dt.ability,
                    timestamp: 0,
                });
            } else {
                i += 1;
            }
        }
    }

    /// Counts one more object into a waiting "one or more" trigger of
    /// this ability and source, if there is one (CR 603.2c: it triggers
    /// once for the objects entering together).
    #[cold]
    fn join_batch(
        &mut self,
        fired: &mut SmallVec<[PendingTrigger; 2]>,
        ability: AbilityId,
        source: ObjId,
    ) -> bool {
        match fired
            .iter_mut()
            .chain(self.pending_triggers.iter_mut())
            .find(|p| p.ability.ability == ability && p.ability.source == source)
        {
            Some(p) => {
                p.ability.trigger_amount += 1;
                true
            }
            None => false,
        }
    }

    /// CR 702.29d: "When you cycle this card" triggers from the card
    /// itself, now in the graveyard.
    #[cold]
    #[inline(never)]
    fn cycle_triggers(&self, event: &Event, fired: &mut SmallVec<[PendingTrigger; 2]>) {
        let Event::Cycled { player, card, def } = *event else {
            return;
        };
        if !self.pool.get(def).may_watch(event) {
            return;
        }
        for &ability in &self.pool.get(def).ability_ids {
            if let AbilityDef::Triggered(t) = self.pool.ability(ability)
                && t.trigger.cycles_this()
            {
                fired.push(PendingTrigger {
                    controller: player,
                    ability: StackAbility {
                        ability,
                        source: card,
                        def,
                        trigger_obj: Some(card),
                        trigger_player: Some(player),
                        trigger_amount: 0,
                    },
                    timestamp: 0,
                });
            }
        }
    }

    #[cold]
    #[inline(never)]
    fn suspend_triggers(&self, fired: &mut SmallVec<[PendingTrigger; 2]>) {
        let active = self.active_player;
        let ability = self.pool.builtin(crate::card::Builtin::SuspendUpkeep);
        for id in &self.player(active).exile {
            let o = &self.objects[*id];
            if o.alt == crate::state::AltCast::Suspended {
                fired.push(PendingTrigger {
                    controller: active,
                    ability: StackAbility {
                        ability,
                        source: *id,
                        def: o.def,
                        trigger_obj: None,
                        trigger_player: None,
                        trigger_amount: 0,
                    },
                    timestamp: 0,
                });
            }
        }
    }

    /// Cheap pre-check: could anything have a triggered ability that sees
    /// this event?
    fn any_trigger_sources(&self, event: &Event) -> bool {
        debug_assert_eq!(
            self.trigger_sources as usize,
            self.battlefield
                .iter()
                .filter(|id| self.flags_of(&self.objects[**id]).has_triggers())
                .count(),
            "trigger source count out of sync with the battlefield"
        );
        if self.trigger_sources > 0
            || !self.delayed_triggers.is_empty()
            || self.suspended_count > 0
            || self.grants_exist()
        {
            return true;
        }
        if let Some(lb) = &self.lookback
            && lb.iter().any(|l| {
                self.pool.get(l.characteristics.def).has_triggers()
                    || l.except
                        .is_some_and(|x| self.pool.get(x).except_flags.has_triggers())
            })
        {
            return true;
        }
        // The object that left may itself have a leaves-the-battlefield
        // trigger; a cycled card, a "when you cycle this card" trigger.
        matches!(event, Event::ZoneChange { def, .. } | Event::Cycled { def, .. }
            if self.pool.get(*def).has_triggers())
    }

    /// Permanents whose abilities see `event`, with the characteristics they
    /// had at the right moment.
    fn watchers(&self, event: &Event) -> Vec<Watcher> {
        let leaves = matches!(
            event,
            Event::ZoneChange {
                from: Zone::Battlefield,
                ..
            }
        );
        if leaves && let Some(lb) = &self.lookback {
            return lb
                .iter()
                .filter(|l| {
                    self.may_watch(l.characteristics.def, l.except, event) || self.grants_exist()
                })
                .map(|l| Watcher {
                    source: l.id,
                    controller: l.controller,
                    tapped: l.tapped,
                    chars: l.characteristics,
                })
                .collect();
        }
        let mut out: Vec<Watcher> = self
            .battlefield
            .iter()
            .filter_map(|id| {
                let o = &self.objects[*id];
                // Only objects whose printed abilities could see this kind
                // of event pay for a characteristics lookup (any object, once
                // abilities may have been granted).
                if !self.may_watch(o.def, o.except, event) && !self.grants_exist() {
                    return None;
                }
                Some(Watcher {
                    source: *id,
                    controller: o.controller,
                    tapped: o.tapped,
                    chars: self.characteristics(*id)?,
                })
            })
            .collect();
        if leaves
            && let Event::ZoneChange { old, .. } = event
            && let Some(l) = self.lki.get(old)
            && (self.may_watch(l.object.def, l.object.except, event) || self.grants_exist())
        {
            out.push(Watcher {
                source: *old,
                controller: l.object.controller,
                tapped: l.object.tapped,
                chars: l.characteristics,
            });
        }
        out
    }

    /// Whether `spec` on `w` triggers on `event`; if so, the triggering
    /// object and player.
    #[allow(clippy::too_many_lines)] // one arm per trigger kind
    fn fires(
        &self,
        spec: &TriggerSpec,
        event: &Event,
        w: &Watcher,
    ) -> Option<(Option<ObjId>, Option<PlayerId>)> {
        let whose_ok = |whose: Who, p: PlayerId| match whose {
            Who::Any => true,
            Who::You => p == w.controller,
            Who::Opponents => p != w.controller,
        };
        // "enchanted creature": only what the source (an Aura) is attached
        // to, as it last was.
        let enchants = |obj: ObjId| {
            self.objects
                .get(w.source)
                .map(|a| a.attached_to)
                .or_else(|| self.lki.get(&w.source).map(|l| l.object.attached_to))
                .flatten()
                == Some(obj)
        };
        // Does `obj` (currently on the battlefield) match?
        let filter_on_battlefield = |f: &crate::card::ObjFilter, obj: ObjId| -> Option<PlayerId> {
            let o = self.objects.get(obj)?;
            let c = self.characteristics(obj)?;
            (f.matches(obj, o.controller, &c, Some(w.source), Some(w.controller))
                && self.status_ok(f, obj)
                && (!f.enchanted || enchants(obj)))
            .then_some(o.controller)
        };
        let on_battlefield = |what: &TriggerObj, obj: ObjId| -> Option<PlayerId> {
            match what {
                TriggerObj::This => (obj == w.source)
                    .then(|| self.objects.get(obj).map(|o| o.controller))
                    .flatten(),
                TriggerObj::Filter(f) => filter_on_battlefield(f, obj),
            }
        };
        match (spec, event) {
            (
                TriggerSpec::Enters(what),
                Event::ZoneChange {
                    new,
                    to: Zone::Battlefield,
                    ..
                },
            ) => {
                let p = on_battlefield(what, *new)?;
                Some((Some(*new), Some(p)))
            }
            (
                TriggerSpec::Dies(what),
                Event::ZoneChange {
                    old,
                    from: Zone::Battlefield,
                    to: Zone::Graveyard,
                    ..
                },
            ) => {
                // Look back: the object as it last existed on the battlefield.
                let l = self.lki.get(old)?;
                let ok = match what {
                    TriggerObj::This => *old == w.source,
                    TriggerObj::Filter(f) => {
                        f.matches(
                            *old,
                            l.object.controller,
                            &l.characteristics,
                            Some(w.source),
                            Some(w.controller),
                        ) && (!f.enchanted || enchants(*old))
                    }
                };
                ok.then_some((Some(*old), Some(l.object.controller)))
            }
            (TriggerSpec::BeginningOfStep { step, whose }, Event::StepBegin { step: s }) => {
                (s == step && whose_ok(*whose, self.active_player))
                    .then_some((None, Some(self.active_player)))
            }
            (
                TriggerSpec::Leaves(what),
                Event::ZoneChange {
                    old,
                    from: Zone::Battlefield,
                    ..
                },
            ) => {
                let l = self.lki.get(old)?;
                let ok = match what {
                    TriggerObj::This => *old == w.source,
                    TriggerObj::Filter(f) => f.matches(
                        *old,
                        l.object.controller,
                        &l.characteristics,
                        Some(w.source),
                        Some(w.controller),
                    ),
                };
                ok.then_some((Some(*old), Some(l.object.controller)))
            }
            (
                TriggerSpec::Attacks(what) | TriggerSpec::AttacksOrBlocks(what),
                Event::Attack { attacker, .. },
            )
            | (
                TriggerSpec::BecomesBlocked(what) | TriggerSpec::BlocksOrBlocked(what),
                Event::Blocked { attacker },
            ) => {
                let p = on_battlefield(what, *attacker)?;
                Some((Some(*attacker), Some(p)))
            }
            (TriggerSpec::AttacksAlone(what), Event::Attack { attacker, .. }) => {
                // CR 506.5: the only creature declared as an attacker.
                let alone = self.combat.as_ref().is_some_and(|c| c.attackers.len() == 1);
                if !alone {
                    return None;
                }
                let p = on_battlefield(what, *attacker)?;
                Some((Some(*attacker), Some(p)))
            }
            (
                TriggerSpec::Blocks(what)
                | TriggerSpec::BlocksOrBlocked(what)
                | TriggerSpec::AttacksOrBlocks(what),
                Event::Block { blocker, attacker },
            ) => {
                // CR 509.3a: "whenever ~ blocks" triggers once even if it
                // blocks several creatures: on its first block.
                if self
                    .combat
                    .as_ref()
                    .and_then(|c| c.blocked_by(*blocker))
                    .is_some_and(|first| first != *attacker)
                {
                    return None;
                }
                let p = on_battlefield(what, *blocker)?;
                Some((Some(*blocker), Some(p)))
            }
            (
                TriggerSpec::DealsCombatDamageToPlayer(what),
                Event::Damage {
                    source,
                    target: Target::Player(p),
                    combat: true,
                    ..
                },
            ) => {
                on_battlefield(what, *source)?;
                Some((Some(*source), Some(*p)))
            }
            (TriggerSpec::GainsLife { whose }, Event::LifeChange { player, delta, .. }) => {
                (*delta > 0 && whose_ok(*whose, *player)).then_some((None, Some(*player)))
            }
            (TriggerSpec::BecomesTarget(what), Event::Targeted { target: obj, .. })
            | (TriggerSpec::BecomesTapped(what), Event::Tapped { obj })
            | (TriggerSpec::BecomesUntapped(what), Event::Untapped { obj })
            | (
                TriggerSpec::DealtDamage(what),
                Event::Damage {
                    target: Target::Object(obj),
                    ..
                },
            ) => {
                let p = on_battlefield(what, *obj)?;
                Some((Some(*obj), Some(p)))
            }
            (
                TriggerSpec::DealsDamage { what, to, combat },
                Event::Damage {
                    source,
                    target,
                    combat: c,
                    ..
                },
            ) => {
                if *combat && !*c {
                    return None;
                }
                // The source may have left the battlefield (it dealt the
                // damage as it last existed).
                let ok = match what {
                    TriggerObj::This => *source == w.source,
                    TriggerObj::Filter(_) => on_battlefield(what, *source).is_some(),
                };
                if !ok {
                    return None;
                }
                match (to, target) {
                    (
                        crate::card::DamageTo::Player | crate::card::DamageTo::Any,
                        Target::Player(p),
                    ) => Some((None, Some(*p))),
                    (crate::card::DamageTo::Opponent, Target::Player(p)) => {
                        (*p != w.controller).then_some((None, Some(*p)))
                    }
                    (
                        crate::card::DamageTo::Creature | crate::card::DamageTo::Any,
                        Target::Object(o),
                    ) => {
                        let obj = self.objects.get(*o)?;
                        let creature = self.characteristics(*o).is_some_and(|c| c.is_creature());
                        (creature || *to == crate::card::DamageTo::Any)
                            .then_some((Some(*o), Some(obj.controller)))
                    }
                    _ => None,
                }
            }
            (
                TriggerSpec::CastsFiltered { whose, filter },
                Event::Cast {
                    player, spell, def, ..
                },
            ) => {
                if !whose_ok(*whose, *player) {
                    return None;
                }
                let c = self
                    .characteristics(*spell)
                    .or_else(|| self.pool.get(*def).printed)?;
                (filter.matches(*spell, *player, &c, Some(w.source), Some(w.controller))
                    && self.status_ok(filter, *spell))
                .then_some((Some(*spell), Some(*player)))
            }
            (TriggerSpec::DrawsNth { whose, n }, Event::Draw { player, .. }) => {
                (whose_ok(*whose, *player) && self.drawn_this_turn[player.index().min(3)] == *n)
                    .then_some((None, Some(*player)))
            }
            (TriggerSpec::TurnedFaceUp, Event::TurnedFaceUp { obj }) => {
                (*obj == w.source).then_some((Some(*obj), Some(w.controller)))
            }
            (TriggerSpec::TargetedByOpponent, Event::Targeted { target, by }) => {
                let by_ctl = self.objects.get(*by)?.controller;
                (*target == w.source && by_ctl != w.controller).then_some((Some(*by), Some(by_ctl)))
            }
            (
                TriggerSpec::BlockPair {
                    what,
                    other,
                    blocks,
                    blocked,
                },
                Event::Block { blocker, attacker },
            ) => {
                // CR 509.3b, 509.3d: once per blocking pair (a blocker that
                // blocks several attackers makes one event per attacker);
                // the other creature is judged as it is now (CR 509.3f).
                let that = if *blocks && on_battlefield(what, *blocker).is_some() {
                    *attacker
                } else if *blocked && on_battlefield(what, *attacker).is_some() {
                    *blocker
                } else {
                    return None;
                };
                let o = self.objects.get(that)?;
                let c = self.characteristics(that)?;
                (other.matches(that, o.controller, &c, Some(w.source), Some(w.controller))
                    && self.status_ok(other, that))
                .then_some((Some(that), Some(o.controller)))
            }
            (TriggerSpec::BlockedBy(f), Event::Block { blocker, attacker }) => {
                if *attacker != w.source {
                    return None;
                }
                let o = self.objects.get(*blocker)?;
                let c = self.characteristics(*blocker)?;
                (f.matches(
                    *blocker,
                    o.controller,
                    &c,
                    Some(w.source),
                    Some(w.controller),
                ) && self.status_ok(f, *blocker))
                .then_some((Some(*blocker), Some(o.controller)))
            }
            (
                TriggerSpec::CastsSpell {
                    whose,
                    noncreature,
                    instant_or_sorcery,
                },
                Event::Cast {
                    player, spell, def, ..
                },
            ) => {
                let d = self.pool.get(*def);
                let ok = whose_ok(*whose, *player)
                    && (!*noncreature || !d.is_creature())
                    && (!*instant_or_sorcery || d.is_instant() || d.is_sorcery());
                ok.then_some((Some(*spell), Some(*player)))
            }
            (TriggerSpec::CastsNth { whose, nth }, Event::Cast { player, spell, .. }) => {
                let n = self.spells_this_turn[player.index().min(3)];
                (whose_ok(*whose, *player) && nth.holds(n, self.active_player == *player))
                    .then_some((Some(*spell), Some(*player)))
            }
            // CR 702.29a: cycling is discarding; a card cycled from the
            // hand is never one of the watching permanents.
            (
                TriggerSpec::Cycles {
                    this: false,
                    or_discard: false,
                    ..
                },
                Event::Cycled { player, card, .. },
            )
            | (
                TriggerSpec::Cycles {
                    this: false,
                    or_discard: true,
                    ..
                },
                Event::Discard { player, card },
            ) => (*player == w.controller).then_some((Some(*card), Some(*player))),
            (
                TriggerSpec::EntersBatch(f),
                Event::ZoneChange {
                    new,
                    to: Zone::Battlefield,
                    ..
                },
            ) => {
                let p = filter_on_battlefield(f, *new)?;
                Some((None, Some(p)))
            }
            (TriggerSpec::Either(pair), _) => self
                .fires(&pair.0, event, w)
                .or_else(|| self.fires(&pair.1, event, w)),
            _ => None,
        }
    }

    // ----- putting triggers on the stack -----------------------------------------------

    /// CR 603.3: before a player would receive priority, pending triggered
    /// abilities go on the stack in APNAP order, each controller choosing
    /// the order of their own (CR 603.3b). Returns whether anything was
    /// done; the caller re-enters the loop (which may yield a decision).
    pub(super) fn put_triggers_on_stack(&mut self) -> bool {
        if self.pending_triggers.is_empty() {
            return false;
        }
        let mut p = self.active_player;
        for _ in 0..self.num_players() {
            let mine: Vec<usize> = self.triggers_of(p);
            if mine.len() == 1 {
                self.put_trigger(mine[0]);
                return true;
            }
            if mine.len() > 1 {
                self.flow = Flow::OrderTriggers { player: p };
                return true;
            }
            p = self.next_player(p);
        }
        // Triggers of players who left the game are dropped.
        self.pending_triggers.clear();
        false
    }

    /// Indices into `pending_triggers` controlled by `p`.
    pub(super) fn triggers_of(&self, p: PlayerId) -> Vec<usize> {
        self.pending_triggers
            .iter()
            .enumerate()
            .filter(|(_, t)| t.controller == p && self.player(p).lost.is_none())
            .map(|(i, _)| i)
            .collect()
    }

    /// Put pending trigger `i` on the stack as an ability object; targets
    /// are then chosen (CR 603.3d). An ability that needs a target with no
    /// legal choice is removed instead.
    pub(super) fn put_trigger(&mut self, i: usize) {
        let t = self.pending_triggers.remove(i);
        let def = t.ability.def;
        let specs = self.pool.ability(t.ability.ability).targets().to_vec();
        // CR 603.3c: a modal triggered ability with no mode whose targets
        // can be chosen is removed.
        let modal =
            match self.pool.ability(t.ability.ability).effects() {
                [crate::card::Effect::Modes(modes)] => Some(modes.iter().any(|m| {
                    self.targets_available(t.controller, &m.targets, Some(t.ability.source))
                })),
                _ => None,
            };
        if modal == Some(false)
            || !self.targets_available(t.controller, &specs, Some(t.ability.source))
        {
            self.emit(Event::TriggerRemoved {
                source: t.ability.source,
                def,
            });
            self.flow = Flow::Priority;
            return;
        }
        let timestamp = self.tick();
        let obj = self.objects.insert(GameObject {
            def,
            card: def,
            except: None,
            cast: CastInfo::default(),
            kind: ObjKind::Ability,
            owner: t.controller,
            controller: t.controller,
            zone: Zone::Stack,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            controlled_since_turn_start: false,
            echo_due: false,
            skip_untap: false,
            chosen: Chosen::None,
            alt: crate::state::AltCast::None,
            timestamp,
            targets: SmallVec::new(),
            counters: SmallVec::new(),
            attached_to: None,
            ability: Some(t.ability),
        });
        self.stack.push(obj);
        self.consecutive_passes = 0;
        self.bump();
        self.emit(Event::TriggerOnStack {
            obj,
            source: t.ability.source,
            def,
        });
        self.flow = if modal.is_some() {
            Flow::ChoosingMode { spell: obj }
        } else if specs.is_empty() {
            Flow::Priority
        } else {
            Flow::Casting {
                spell: obj,
                next_target: 0,
                sacrifice: None,
                discard: None,
            }
        };
    }

    // ----- resolution --------------------------------------------------------------

    /// CR 608.2 for an ability object (triggered or activated) on top of
    /// the stack: check its intervening "if" (CR 603.4) and targets, apply
    /// its effects, and remove it (CR 608.2n).
    pub(super) fn resolve_ability(&mut self, id: ObjId) {
        let obj = self.objects[id].clone();
        let sa = obj.ability.expect("ability object");
        let condition = match self.pool.ability(sa.ability) {
            AbilityDef::Triggered(t) => t.condition,
            AbilityDef::Activated(_) => None,
            _ => unreachable!("only triggered and activated abilities use the stack"),
        };
        // A modal ability's targets and effects are its chosen mode's.
        let pool = std::sync::Arc::clone(&self.pool);
        let targets = crate::state::targets_in(&pool, &obj);
        let effects = crate::state::effects_in(&pool, &obj);
        let split = self.mode_split_of(id);
        if let Some(cond) = condition
            && !self.condition_holds(cond, sa.source)
        {
            self.emit(Event::TriggerRemoved {
                source: sa.source,
                def: obj.def,
            });
            self.remove_from_game(id);
            return;
        }
        let legal = self.targets_legal(obj.controller, targets, &obj.targets, Some(sa.source));
        if !legal.is_empty() && legal.iter().all(|l| !l) {
            self.emit(Event::Fizzle {
                spell: id,
                def: obj.def,
            });
            self.remove_from_game(id);
            return;
        }
        self.emit(Event::Resolve {
            spell: id,
            def: obj.def,
        });
        let mut ctx = EffectCtx {
            source: sa.source,
            controller: obj.controller,
            targets: obj.targets.to_vec(),
            division: self.division_of(id),
            legal,
            trigger_obj: sa.trigger_obj,
            trigger_player: sa.trigger_player,
            trigger_amount: sa.trigger_amount,
            created: None,
            x: 0,
            did: true,
            chosen: Chosen::None,
        };
        self.apply_effects(&mut ctx, effects, split);
        if self.objects.contains_key(id) {
            self.remove_from_game(id);
        }
    }

    /// An ability's intervening "if", evaluated against the current state.
    fn condition_holds(&self, cond: Condition, source: ObjId) -> bool {
        match cond {
            Condition::SourceUntapped => self
                .objects
                .get(source)
                .is_some_and(|o| o.zone == Zone::Battlefield && !o.tapped),
            Condition::NoCreatures => self.no_creatures(),
            Condition::Kicked => self.was_kicked(source),
            Condition::HadNoCounters(k) => self.had_no_counters(source, k),
            Condition::NoSpellsLastTurn => self.spells_last_turn.iter().all(|n| *n == 0),
            Condition::TwoSpellsLastTurn => self.spells_last_turn.iter().any(|n| *n >= 2),
            Condition::EchoDue => self.objects.get(source).is_some_and(|o| o.echo_due),
            Condition::Evoked => self
                .objects
                .get(source)
                .map(|o| o.cast.evoked())
                .or_else(|| self.lki.get(&source).map(|l| l.object.cast.evoked()))
                .unwrap_or(false),
            Condition::Static(i) => {
                let you = self
                    .objects
                    .get(source)
                    .map(|o| o.controller)
                    .or_else(|| self.lki.get(&source).map(|l| l.object.controller));
                you.is_some_and(|p| self.condition_now(self.pool.condition(i), p, source))
            }
        }
    }

    /// The same, as the ability would trigger, for a watcher whose state
    /// was captured with the event.
    fn condition_holds_for(&self, cond: Condition, w: &Watcher) -> bool {
        match cond {
            Condition::SourceUntapped => !w.tapped,
            Condition::NoCreatures => self.no_creatures(),
            Condition::Kicked => self.was_kicked(w.source),
            Condition::HadNoCounters(k) => self.had_no_counters(w.source, k),
            Condition::NoSpellsLastTurn => self.spells_last_turn.iter().all(|n| *n == 0),
            Condition::TwoSpellsLastTurn => self.spells_last_turn.iter().any(|n| *n >= 2),
            Condition::EchoDue => self.objects.get(w.source).is_some_and(|o| o.echo_due),
            Condition::Evoked => self.objects.get(w.source).is_some_and(|o| o.cast.evoked()),
            Condition::Static(i) => {
                self.condition_now(self.pool.condition(i), w.controller, w.source)
            }
        }
    }

    /// CR 702.33c: whether the object (or, once it has moved, the object it
    /// was) was kicked.
    pub(super) fn was_kicked(&self, id: ObjId) -> bool {
        self.objects
            .get(id)
            .map(|o| o.cast.kicked())
            .or_else(|| self.lki.get(&id).map(|l| l.object.cast.kicked()))
            .unwrap_or(false)
    }

    /// Persist and undying: the source, as it last existed on the
    /// battlefield, had no counters of this kind.
    fn had_no_counters(&self, id: ObjId, k: crate::card::CounterKind) -> bool {
        self.lki
            .get(&id)
            .map(|l| l.object.counters(k) == 0)
            .or_else(|| self.objects.get(id).map(|o| o.counters(k) == 0))
            .unwrap_or(false)
    }

    fn no_creatures(&self) -> bool {
        !self
            .battlefield
            .iter()
            .any(|id| self.is_creature_on_battlefield(*id))
    }

    /// Remove an object that ceases to exist (an ability leaving the stack).
    pub(super) fn remove_from_game(&mut self, id: ObjId) {
        if !self.divisions.is_empty() {
            self.divisions.retain(|(s, _)| *s != id);
        }
        self.remove_from_zone_list(id);
        self.objects.remove(id);
        self.bump();
    }
}
