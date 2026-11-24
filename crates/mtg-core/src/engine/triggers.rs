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

use crate::card::{AbilityDef, AbilityId, Condition, KeywordSet, TriggerObj, TriggerSpec, Who};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::state::{
    CastInfo, Characteristics, Chosen, Flow, GameObject, GameState, ObjKind, PendingTrigger,
    StackAbility, Target, Zone,
};

use super::casting::EffectCtx;

/// A permanent whose triggered abilities may see an event, or a card in a
/// graveyard whose graveyard abilities may (CR 113.6k).
struct Watcher {
    source: ObjId,
    controller: PlayerId,
    tapped: bool,
    chars: Characteristics,
    /// A card in a graveyard: only its abilities that work there trigger,
    /// and only those (CR 113.6).
    in_graveyard: bool,
}

impl GameState {
    // ----- detection ---------------------------------------------------------------

    /// CR 603.2: every triggered ability whose event this is triggers.
    #[inline(never)]
    #[allow(clippy::too_many_lines)] // the batching rules
    pub(crate) fn detect_triggers(&mut self, event: &Event) {
        if !self.any_trigger_sources(event) {
            return;
        }
        let watchers = self.watchers(event);
        let event_amount = match event {
            Event::Damage { amount, .. } => *amount,
            Event::CountersAdded { n, .. } | Event::CountersRemoved { n, .. } => *n,
            // "you gain that much life" after a player loses or gains life.
            Event::LifeChange { delta, .. } => delta.unsigned_abs(),
            // "discover again for the same value".
            Event::Discovered { value, .. } => *value,
            // "If eight or more mana was spent to cast that spell, ... that
            // much damage": the mana spent to cast it (CR 601.2h), recorded
            // only while something watches it.
            Event::Cast { spell, .. } if self.spent_watchers > 0 => {
                self.spent_on(*spell).count(crate::card::SpentKind::Total)
            }
            _ => 0,
        };
        let mut fired: SmallVec<[PendingTrigger; 2]> = SmallVec::new();
        // Indexes into `fired` of abilities naming their granter, and it.
        let mut granted: SmallVec<[(usize, ObjId); 2]> = SmallVec::new();
        for w in &watchers {
            let mut abilities: SmallVec<[AbilityId; 8]> =
                w.chars.abilities.iter().copied().collect();
            // Only the keywords that stand for triggered abilities have
            // one (`CardPool::keyword_trigger`).
            if w.chars.keywords.intersects(KeywordSet::TRIGGERED) {
                for k in w.chars.keywords.iter() {
                    if let Some(id) = self.pool.keyword_trigger(k) {
                        abilities.push(id);
                    }
                }
            }
            for ability in abilities {
                let t = match self.pool.ability(ability) {
                    AbilityDef::Triggered(t) if t.from_graveyard == w.in_graveyard => t,
                    _ => continue,
                };
                let Some((trigger_obj, trigger_player)) = self.fires(&t.trigger, event, w) else {
                    continue;
                };
                if let Some(cond) = t.condition
                    && !(matches!(cond, Condition::WhileSecond(_))
                        && matches!(&t.trigger, TriggerSpec::Either(pair)
                            if self.fires(&pair.0, event, w).is_some()))
                    && !self.condition_holds_for(cond, w, trigger_obj, trigger_player)
                {
                    continue;
                }
                let (mut copies, limit) = (chapter_copies(&t.trigger, event), t.limit);
                // A source of "triggers an additional time" leaving in the
                // same event still counts (CR 603.10a, `extra_triggers`).
                if (self.player_static_sources > 0 || self.lookback.is_some()) && !w.in_graveyard {
                    match self.extra_triggers(w, event, &t.trigger) {
                        Some(n) => copies += n,
                        // Torpor Orb: it doesn't trigger at all.
                        None => continue,
                    }
                }
                // CR 603.2c: "one or more ... enter", once per batch.
                let batch = matches!(
                    t.trigger,
                    TriggerSpec::EntersBatch(_)
                        | TriggerSpec::LeavesBatch { .. }
                        | TriggerSpec::LeavesGraveyard(_)
                        | TriggerSpec::DiscardsBatch { .. }
                        | TriggerSpec::SacrificesBatch(_)
                        | TriggerSpec::Batch(_)
                );
                // "one or more creatures you control deal combat damage to
                // a player": once per player dealt damage together.
                let player_batch = matches!(
                    t.trigger,
                    TriggerSpec::CreaturesDealCombatDamage(_)
                        | TriggerSpec::CreaturesDealDamage(_)
                        | TriggerSpec::NoncombatDamageToOpponent { batch: true, .. }
                );
                if player_batch
                    && self.join_player_batch(
                        &mut fired,
                        ability,
                        w.source,
                        trigger_player,
                        event_amount,
                    )
                {
                    continue;
                }
                if (batch && self.join_batch(&mut fired, ability, w.source))
                    || !self.count_trigger(limit, w.source, ability)
                {
                    continue;
                }
                let pending = PendingTrigger {
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
                };
                // "Equipped creature has 'Whenever this creature attacks,
                // you may sacrifice Sunfire Torch. ...'": each grant is its
                // own ability (CR 113.2c), carrying its granter.
                if self.pool.ability_names_granter(ability) {
                    for g in self.granters_of(w.source, ability, None) {
                        for _ in 0..copies {
                            granted.push((fired.len(), g));
                            fired.push(pending);
                        }
                    }
                    continue;
                }
                fired.extend(std::iter::repeat_n(pending, copies));
            }
        }
        if matches!(event, Event::Cycled { .. }) {
            self.cycle_triggers(event, &mut fired);
        }
        // "When Purity is put into a graveyard from anywhere": the card,
        // now in the graveyard, sees its own arrival (CR 603.6a).
        if let Event::ZoneChange {
            new,
            def,
            to: Zone::Graveyard,
            ..
        } = *event
            && self.pool.get(def).has_triggers()
        {
            self.to_graveyard_triggers(new, def, &mut fired);
        }

        if matches!(event, Event::Cast { .. }) {
            self.cast_this_triggers(event, &mut fired);
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
        if !self.delayed_triggers.is_empty() {
            self.delayed_turn_triggers(event, event_amount, &mut fired);
        }
        for (i, mut t) in fired.into_iter().enumerate() {
            t.timestamp = self.tick();
            if let Some((_, g)) = granted.iter().find(|(j, _)| *j == i) {
                self.pending_granters.push((t.timestamp, *g));
            }
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

    /// "This ability triggers only once each turn.": whether it may
    /// trigger again this turn, counting it if so.
    #[inline]
    fn count_trigger(
        &mut self,
        limit: crate::card::TurnLimit,
        source: ObjId,
        ability: AbilityId,
    ) -> bool {
        let key = (source, ability);
        if limit == crate::card::TurnLimit::Ever {
            // Kept with "activate only once" abilities: never cleared.
            if self.activated_ever.contains(&key) {
                return false;
            }
            self.activated_ever.push(key);
            return true;
        }
        let crate::card::TurnLimit::Triggers(n) = limit else {
            return true;
        };
        let times = self.limited_this_turn.iter().filter(|k| **k == key).count();
        if times >= usize::from(n) {
            return false;
        }
        self.limited_this_turn.push(key);
        true
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
                    // "the end step on your next turn".
                    TriggerSpec::OnNextTurn(inner) => match &**inner {
                        TriggerSpec::BeginningOfStep { step: s, .. } => {
                            *s == step && active == dt.controller && self.turn_number > dt.turn
                        }
                        _ => false,
                    },
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
                // "... for the rest of the game" (paradigm) stays, and so
                // does "At the beginning of each combat this turn" (Full
                // Throttle), until the cleanup step ends it (CR 603.7c).
                if dt.forever || dt.this_turn {
                    i += 1;
                } else {
                    self.delayed_triggers.remove(i);
                }
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

    /// Delayed triggered abilities that trigger on every matching event
    /// until the turn ends ("Whenever a creature blocks this turn, ...";
    /// CR 603.7c), seen as their controller's: not removed as they
    /// trigger, but at the cleanup step with the other "this turn" ones.
    /// "When you next cast ... this turn" (`TriggerSpec::Next`) triggers
    /// only once and is removed as it does.
    #[cold]
    #[inline(never)]
    fn delayed_turn_triggers(
        &mut self,
        event: &Event,
        event_amount: u32,
        fired: &mut SmallVec<[PendingTrigger; 2]>,
    ) {
        let mut i = 0;
        while i < self.delayed_triggers.len() {
            let dt = self.delayed_triggers[i];
            i += 1;
            if !dt.this_turn {
                continue;
            }
            let AbilityDef::Triggered(t) = self.pool.ability(dt.ability.ability) else {
                continue;
            };
            let (spec, once) = match &t.trigger {
                TriggerSpec::Next(inner) => (&**inner, true),
                other => (other, false),
            };
            if matches!(
                spec,
                TriggerSpec::BeginningOfStep { .. }
                    | TriggerSpec::OnNextTurn(_)
                    | TriggerSpec::Dies(TriggerObj::This)
                    | TriggerSpec::DiesUnderYourControl
                    | TriggerSpec::Leaves(TriggerObj::This)
                    | TriggerSpec::DiesOrExiled(TriggerObj::This)
            ) {
                continue;
            }
            // "Whenever that creature deals combat damage this turn",
            // "Whenever a creature dealt damage by that creature dies this
            // turn": the trigger's own object ("this") is what the delayed
            // trigger is about (the source itself for `delayed this:`).
            let source = dt.ability.trigger_obj.unwrap_or(dt.ability.source);
            // CR 603.7: once created, a delayed trigger doesn't depend on
            // its source: a copy of a spell that made it (a prepared
            // permanent's spell, CR 722.3c) has ceased to exist and left
            // no last known information, so its printed values stand in.
            let chars = match self.objects.get(source) {
                Some(o) => self.base_characteristics(o),
                None => match self.lki.get(&source) {
                    Some(l) => l.characteristics,
                    None => self.printed(dt.ability.def),
                },
            };
            let w = Watcher {
                source,
                controller: dt.controller,
                tapped: false,
                chars,
                in_graveyard: false,
            };
            let Some((trigger_obj, trigger_player)) = self.fires(spec, event, &w) else {
                continue;
            };
            // "Whenever that creature deals combat damage this turn, you
            // may exile it": an event with no object of its own (damage to
            // a player) leaves "it" the object the trigger is about.
            let trigger_obj = trigger_obj.or(dt.ability.trigger_obj);
            fired.push(PendingTrigger {
                controller: dt.controller,
                ability: StackAbility {
                    trigger_obj,
                    trigger_player,
                    trigger_amount: event_amount,
                    ..dt.ability
                },
                timestamp: 0,
            });
            if once {
                i -= 1;
                self.delayed_triggers.remove(i);
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
            let mut gone = false;
            let at = dt.ability.trigger_obj == Some(old)
                && match self.pool.ability(dt.ability.ability) {
                    // CR 702.55c: the haunting card's ability triggers from
                    // exile, only while the card is still there; the
                    // haunted creature leaving any other way ends it.
                    AbilityDef::Triggered(_)
                        if self.pool.get(dt.ability.def).haunted == Some(dt.ability.ability) =>
                    {
                        gone = true;
                        to == Zone::Graveyard
                            && self
                                .objects
                                .get(dt.ability.source)
                                .is_some_and(|o| o.zone == Zone::Exile)
                    }
                    AbilityDef::Triggered(t) => match &t.trigger {
                        TriggerSpec::Dies(TriggerObj::This) => to == Zone::Graveyard,
                        // "When it dies under your control this turn": its
                        // controller as it last existed (CR 603.10a); dying
                        // under another player's control ends it.
                        TriggerSpec::DiesUnderYourControl => {
                            gone = true;
                            to == Zone::Graveyard
                                && self
                                    .lki
                                    .get(&old)
                                    .is_some_and(|l| l.object.controller == dt.controller)
                        }
                        TriggerSpec::Leaves(TriggerObj::This) => true,
                        // CR 701.66a: to a graveyard or exile; leaving for
                        // anywhere else, it is a new object that can never
                        // trigger this (CR 400.7), so it goes too.
                        TriggerSpec::DiesOrExiled(TriggerObj::This) => {
                            gone = true;
                            matches!(to, Zone::Graveyard | Zone::Exile)
                        }
                        _ => false,
                    },
                    _ => false,
                };
            if gone && !at {
                self.delayed_triggers.remove(i);
            } else if at {
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

    /// CR 603.2d: how many additional times a triggered ability of `w`
    /// triggers on `event` ("If a triggered ability of another Elemental
    /// you control triggers, it triggers an additional time"): one for
    /// each such ability whose filter the watcher matches. `None`: an
    /// enters trigger an ability stops ("Creatures entering don't cause
    /// abilities to trigger.", CR 603.6a).
    #[cold]
    #[inline(never)]
    #[allow(clippy::too_many_lines)] // the look-back and the cause
    fn extra_triggers(&self, w: &Watcher, event: &Event, spec: &TriggerSpec) -> Option<usize> {
        // Each: the permanent, its controller, its characteristics, and
        // what it is attached to.
        type Source = (ObjId, PlayerId, Characteristics, Option<ObjId>);
        use crate::card::TriggerCause;
        // The object whose event this is, as the cause filter judges it.
        let cause = match event {
            Event::ZoneChange {
                new,
                to: Zone::Battlefield,
                ..
            } => self.objects.get(*new).and_then(|o| {
                Some((
                    TriggerCause::Enters,
                    *new,
                    o.controller,
                    self.characteristics(*new)?,
                ))
            }),
            // CR 603.10a: the creature as it last existed.
            Event::ZoneChange {
                old,
                from: Zone::Battlefield,
                to: Zone::Graveyard,
                ..
            } => self.lki.get(old).map(|l| {
                (
                    TriggerCause::Dies,
                    *old,
                    l.object.controller,
                    l.characteristics,
                )
            }),
            Event::Attack { attacker, .. } => self.objects.get(*attacker).and_then(|o| {
                Some((
                    TriggerCause::Attacks,
                    *attacker,
                    o.controller,
                    self.characteristics(*attacker)?,
                ))
            }),
            _ => None,
        };
        // CR 603.10a: for a leaves-the-battlefield event, the permanents
        // as they were before it ("Teysa Karlov dying together with the
        // creature whose death triggers": hers still applies).
        let leaves = matches!(
            event,
            Event::ZoneChange {
                from: Zone::Battlefield,
                ..
            }
        );
        let sources: SmallVec<[Source; 4]> = match &self.lookback {
            Some(lb) if leaves => lb
                .iter()
                .filter(|l| {
                    self.pool
                        .get(l.characteristics.def)
                        .flags()
                        .has_player_static()
                })
                .map(|l| {
                    let attached = self
                        .objects
                        .get(l.id)
                        .filter(|o| o.zone == Zone::Battlefield)
                        .map(|o| o.attached_to)
                        .or_else(|| self.lki.get(&l.id).map(|k| k.object.attached_to))
                        .flatten();
                    (l.id, l.controller, l.characteristics, attached)
                })
                .collect(),
            _ => self
                .battlefield
                .iter()
                .filter(|id| {
                    self.pool
                        .get(self.objects[**id].def)
                        .flags()
                        .has_player_static()
                })
                .filter_map(|&id| {
                    let o = &self.objects[id];
                    Some((id, o.controller, self.characteristics(id)?, o.attached_to))
                })
                .collect(),
        };
        let mut n = 0;
        for (id, controller, c, attached_to) in sources {
            for &a in &c.abilities {
                if let AbilityDef::TriggerAgain {
                    filter,
                    entering: e,
                    cause: k,
                    suppress,
                    condition,
                    ..
                } = self.pool.ability(a)
                    && condition.is_none_or(|i| {
                        self.condition_now(self.pool.condition(i), controller, id)
                    })
                    && e.as_ref().is_none_or(|e| {
                        cause.as_ref().is_some_and(|(kind, obj, ctl, c)| {
                            kind == k && e.matches(*obj, *ctl, c, Some(id), Some(controller))
                        })
                    })
                    // "another creature you control of the chosen type"
                    // (Roaming Throne): what this permanent chose.
                    && self.with_source_chosen(filter, id).matches(
                        w.source,
                        w.controller,
                        &w.chars,
                        Some(id),
                        Some(controller),
                    )
                    // "equipped creature" (Wizard's Staff).
                    && (!filter.enchanted || attached_to == Some(w.source))
                {
                    if *suppress {
                        if spec.is_enters() {
                            return None;
                        }
                    } else {
                        n += 1;
                    }
                }
            }
        }
        Some(n)
    }

    /// Counts one more object into a waiting "one or more" trigger of
    /// this ability and source, if there is one (CR 603.2c: it triggers
    /// once for the objects entering together); every instance of it, when
    /// it triggered more than once (CR 603.2d).
    #[cold]
    fn join_batch(
        &mut self,
        fired: &mut SmallVec<[PendingTrigger; 2]>,
        ability: AbilityId,
        source: ObjId,
    ) -> bool {
        let mut joined = false;
        let from = self.batch_from;
        for p in fired
            .iter_mut()
            .chain(
                self.pending_triggers
                    .iter_mut()
                    .filter(|p| p.timestamp >= from),
            )
            .filter(|p| p.ability.ability == ability && p.ability.source == source)
        {
            p.ability.trigger_amount += 1;
            joined = true;
        }
        joined
    }

    /// Like `join_batch`, for one player: the damage of a waiting "one or
    /// more creatures you control deal combat damage to a player" trigger
    /// about the same player grows instead.
    #[cold]
    fn join_player_batch(
        &mut self,
        fired: &mut SmallVec<[PendingTrigger; 2]>,
        ability: AbilityId,
        source: ObjId,
        player: Option<PlayerId>,
        amount: u32,
    ) -> bool {
        let from = self.batch_from;
        match fired
            .iter_mut()
            .chain(
                self.pending_triggers
                    .iter_mut()
                    .filter(|p| p.timestamp >= from),
            )
            .find(|p| {
                p.ability.ability == ability
                    && p.ability.source == source
                    && p.ability.trigger_player == player
            }) {
            Some(p) => {
                p.ability.trigger_amount += amount;
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
        let Event::Cycled {
            player,
            card,
            def,
            x,
        } = *event
        else {
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
                        // The X paid for cycling, for the trigger's X
                        // (`put_trigger_on_stack`).
                        trigger_amount: x,
                    },
                    timestamp: 0,
                });
            }
        }
    }

    /// "When this card is put into a graveyard from anywhere" (CR 603.6a):
    /// triggers from the card in the graveyard, controlled by its owner.
    #[cold]
    #[inline(never)]
    fn to_graveyard_triggers(
        &self,
        card: ObjId,
        def: crate::card::CardRef,
        fired: &mut SmallVec<[PendingTrigger; 2]>,
    ) {
        let Some(owner) = self.objects.get(card).map(|o| o.owner) else {
            return;
        };
        for &ability in &self.pool.get(def).ability_ids {
            if let AbilityDef::Triggered(t) = self.pool.ability(ability)
                && matches!(t.trigger, TriggerSpec::ThisToGraveyard)
            {
                fired.push(PendingTrigger {
                    controller: owner,
                    ability: StackAbility {
                        ability,
                        source: card,
                        def,
                        trigger_obj: Some(card),
                        trigger_player: Some(owner),
                        trigger_amount: 0,
                    },
                    timestamp: 0,
                });
            }
        }
    }

    /// "When you cast this spell" (CR 603.2): triggers from the spell
    /// itself, on the stack, controlled by its caster.
    #[cold]
    #[inline(never)]
    fn cast_this_triggers(&self, event: &Event, fired: &mut SmallVec<[PendingTrigger; 2]>) {
        let Event::Cast {
            player, spell, def, ..
        } = *event
        else {
            return;
        };
        let d = self.pool.get(def);
        if !d.may_watch(event) {
            return;
        }
        for &ability in &d.ability_ids {
            if let AbilityDef::Triggered(t) = self.pool.ability(ability)
                && t.trigger.casts_this()
            {
                // "When you cast this spell while you control a creature":
                // judged as it triggers (CR 603.2).
                if let Some(crate::card::Condition::While(i)) = t.condition
                    && !self.condition_for(self.pool.condition(i), player, spell, Some(spell))
                {
                    continue;
                }
                fired.push(PendingTrigger {
                    controller: player,
                    ability: StackAbility {
                        ability,
                        source: spell,
                        def,
                        trigger_obj: Some(spell),
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
            || self.graveyard_trigger_cards > 0
            || self.trigger_grants_exist()
            || self.emblem_triggers()
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
        // trigger (or have granted itself one: "As long as you control an
        // artifact, this creature has 'When this creature dies, draw a
        // card.'", its grant gone with it); a cycled card, a "when you
        // cycle this card" trigger.
        matches!(event, Event::ZoneChange { def, .. } | Event::Cycled { def, .. }
            if self.pool.get(*def).has_triggers()
                || self.pool.get(*def).flags().grants_triggers())
            || matches!(event, Event::Cast { def, .. } if self.pool.get(*def).may_watch(event))
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
        // Worked out once, not per object.
        let bit = event.watch_bit();
        let grants = self.trigger_grants_exist();
        // Room for a few watchers before the list regrows (a collected
        // filter starts at four and regrew on busy boards).
        let mut out: Vec<Watcher> = Vec::with_capacity(8);
        if leaves && let Some(lb) = &self.lookback {
            out.extend(
                lb.iter()
                    .filter(|l| {
                        grants
                            || self.may_watch_bit(l.characteristics.def, l.except, bit)
                            || self
                                .pool
                                .get(l.characteristics.def)
                                .flags()
                                .grants_triggers()
                    })
                    .map(|l| Watcher {
                        source: l.id,
                        controller: l.controller,
                        tapped: l.tapped,
                        chars: l.characteristics,
                        in_graveyard: false,
                    }),
            );
            if self.graveyard_trigger_cards > 0 {
                self.graveyard_watchers(event, &mut out);
            }
            self.emblem_watchers(event, &mut out);
            return out;
        }
        out.extend(self.battlefield.iter().filter_map(|id| {
            let o = &self.objects[*id];
            // Only objects whose printed abilities could see this kind
            // of event pay for a characteristics lookup (any object, once
            // abilities may have been granted).
            if !grants && !self.may_watch_bit(o.def, o.except, bit) {
                return None;
            }
            Some(Watcher {
                source: *id,
                controller: o.controller,
                tapped: o.tapped,
                chars: self.characteristics(*id)?,
                in_graveyard: false,
            })
        }));
        if self.graveyard_trigger_cards > 0 {
            self.graveyard_watchers(event, &mut out);
        }
        self.emblem_watchers(event, &mut out);
        if leaves
            && let Event::ZoneChange { old, .. } = event
            && let Some(l) = self.lki.get(old)
            && (grants
                || self.may_watch_bit(l.object.def, l.object.except, bit)
                || self.pool.get(l.object.def).flags().grants_triggers())
        {
            out.push(Watcher {
                source: *old,
                controller: l.object.controller,
                tapped: l.object.tapped,
                chars: l.characteristics,
                in_graveyard: false,
            });
        }
        out
    }

    /// Whether an emblem has a triggered ability (CR 114.4: "Whenever an
    /// artifact you control enters, this emblem deals 3 damage to any
    /// target."). Emblems are few, and most have none.
    fn emblem_triggers(&self) -> bool {
        self.emblems.iter().any(|id| {
            self.objects
                .get(*id)
                .is_some_and(|o| self.pool.get(o.def).has_triggers())
        })
    }

    /// Emblems whose triggered abilities could see `event` (CR 114.4),
    /// controlled by their owners' controllers as created.
    fn emblem_watchers(&self, event: &Event, out: &mut Vec<Watcher>) {
        for id in &self.emblems {
            let Some(o) = self.objects.get(*id) else {
                continue;
            };
            if !self.may_watch(o.def, o.except, event) {
                continue;
            }
            let Some(chars) = self.characteristics(*id) else {
                continue;
            };
            out.push(Watcher {
                source: *id,
                controller: o.controller,
                tapped: false,
                chars,
                in_graveyard: false,
            });
        }
    }

    /// Cards in graveyards with abilities that work there and could see
    /// `event` (CR 113.6k), controlled by their owners. Only called with
    /// `graveyard_trigger_cards` nonzero. A card that the event itself
    /// put into the graveyard is a new object (CR 400.7) and doesn't see
    /// it.
    #[cold]
    #[inline(never)]
    fn graveyard_watchers(&self, event: &Event, out: &mut Vec<Watcher>) {
        let arrived = match event {
            Event::ZoneChange { new, .. } => Some(*new),
            _ => None,
        };
        for p in self.players_in_game() {
            for id in &self.player(p).graveyard {
                let o = &self.objects[*id];
                let def = self.pool.get(o.def);
                if !def.flags().has_graveyard_trigger()
                    || !def.may_watch(event)
                    || Some(*id) == arrived
                {
                    continue;
                }
                out.push(Watcher {
                    source: *id,
                    controller: o.owner,
                    tapped: false,
                    chars: self.base_characteristics(o),
                    in_graveyard: true,
                });
            }
        }
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
            // "a creature of the chosen type": what the source chose.
            let f = &*self.with_source_chosen(f, w.source);
            let o = self.objects.get(obj)?;
            let c = self.characteristics(obj)?;
            // "Whenever a creature enchanted player controls enters".
            let filled;
            let f = if f.enchanted_player {
                filled = f.with_enchanted_player(self.attached_player_of(w.source));
                &filled
            } else {
                f
            };
            (f.matches(obj, o.controller, &c, Some(w.source), Some(w.controller))
                && self.status_ok(f, obj)
                && (!f.enchanted
                    || enchants(obj)
                    // "this creature or equipped creature" (reconfigure).
                    || (obj == w.source
                        && f.or
                            .as_deref()
                            .is_some_and(|g| g.relation == crate::card::Relation::This))))
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
            )
            // "When enchanted permanent dies or is put into exile".
            | (
                TriggerSpec::DiesOrExiled(what),
                Event::ZoneChange {
                    old,
                    from: Zone::Battlefield,
                    to: Zone::Graveyard | Zone::Exile,
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
                            && (!f.status.damaged_by_source
                                || self.damaged_by_this_turn.contains(&(w.source, *old)))
                            && (!f.needs_state() || self.lki_status_ok(f, *old, l))
                    }
                };
                ok.then_some((Some(*old), Some(l.object.controller)))
            }
            (TriggerSpec::BeginningOfStep { step, whose }, Event::StepBegin { step: s }) => {
                (s == step && whose_ok(*whose, self.active_player))
                    .then_some((None, Some(self.active_player)))
            }
            // "the upkeep of enchanted creature's controller": the active
            // player controls what the source is attached to.
            (TriggerSpec::AttachedControllersStep(step), Event::StepBegin { step: s }) => {
                if s != step {
                    return None;
                }
                let aura = self.objects.get(w.source)?;
                // "At the beginning of enchanted player's upkeep" (CR
                // 303.4): an Aura attached to a player.
                if let Some(p) = aura.attached_player() {
                    return (p == self.active_player).then_some((None, Some(p)));
                }
                let host = aura.attached_to?;
                (self.objects.get(host)?.controller == self.active_player)
                    .then_some((Some(host), Some(self.active_player)))
            }
            (
                TriggerSpec::LeavesBatch { filter, dies },
                Event::ZoneChange {
                    old,
                    from: Zone::Battlefield,
                    to,
                    ..
                },
            ) => {
                if *dies && *to != Zone::Graveyard {
                    return None;
                }
                let l = self.lki.get(old)?;
                (filter.matches(
                    *old,
                    l.object.controller,
                    &l.characteristics,
                    Some(w.source),
                    Some(w.controller),
                ) && (!filter.needs_state() || self.lki_status_ok(filter, *old, l)))
                    .then_some((None, Some(l.object.controller)))
            }
            (
                TriggerSpec::Leaves(what),
                Event::ZoneChange {
                    old,
                    from: Zone::Battlefield,
                    ..
                },
            )
            // CR 700.4: "dies" is put into a graveyard from the
            // battlefield.
            | (
                TriggerSpec::LeavesWithoutDying(what),
                Event::ZoneChange {
                    old,
                    from: Zone::Battlefield,
                    to: Zone::Exile | Zone::Hand | Zone::Library | Zone::Stack | Zone::Command,
                    ..
                },
            )
            | (
                TriggerSpec::ReturnedToHand(what),
                Event::ZoneChange {
                    old,
                    from: Zone::Battlefield,
                    to: Zone::Hand,
                    ..
                },
            ) => {
                let l = self.lki.get(old)?;
                let ok = match what {
                    TriggerObj::This => *old == w.source,
                    // "Whenever a modified creature you control leaves the
                    // battlefield": its state as it last existed (CR
                    // 603.10a).
                    TriggerObj::Filter(f) => {
                        f.matches(
                            *old,
                            l.object.controller,
                            &l.characteristics,
                            Some(w.source),
                            Some(w.controller),
                        ) && (!f.enchanted || enchants(*old))
                            && (!f.needs_state() || self.lki_status_ok(f, *old, l))
                    }
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
            )
            | (TriggerSpec::AttacksUnblocked(what), Event::Unblocked { attacker }) => {
                let p = on_battlefield(what, *attacker)?;
                Some((Some(*attacker), Some(p)))
            }
            // The attacker is recorded as it is declared, before the event.
            (TriggerSpec::AttacksFirstTime(what), Event::Attack { attacker, .. }) => {
                let p = on_battlefield(what, *attacker)?;
                (self
                    .attackers_this_turn
                    .iter()
                    .filter(|id| **id == *attacker)
                    .count()
                    <= 1)
                    .then_some((Some(*attacker), Some(p)))
            }
            (TriggerSpec::AttacksWith { this, min }, Event::Attack { attacker, .. }) => {
                // Once per declaration: on the source's own attack, or on
                // the first attacker declared.
                let c = self.combat.as_ref()?;
                let ok = if *this {
                    *attacker == w.source
                } else {
                    c.attackers.first().is_some_and(|a| a.id == *attacker)
                        && self.objects.get(*attacker)?.controller == w.controller
                };
                (ok && c.attackers.len() >= usize::from(*min))
                    .then_some((this.then_some(*attacker), Some(w.controller)))
            }
            (TriggerSpec::PlayerAttacksWith { whose, min }, Event::Attack { attacker, .. }) => {
                // Once per declaration, on the first attacker declared.
                let c = self.combat.as_ref()?;
                if c.attackers.first().is_none_or(|a| a.id != *attacker)
                    || c.attackers.len() < usize::from(*min)
                {
                    return None;
                }
                let p = self.objects.get(*attacker)?.controller;
                whose_ok(*whose, p).then_some((None, Some(p)))
            }
            (
                TriggerSpec::AttacksPlayerAlone(what),
                Event::Attack {
                    attacker,
                    target: Target::Player(defender),
                },
            ) => {
                let alone = self.combat.as_ref().is_some_and(|c| c.attackers.len() == 1);
                if !alone {
                    return None;
                }
                on_battlefield(what, *attacker)?;
                Some((Some(*attacker), Some(*defender)))
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
                TriggerSpec::DealsCombatDamageToPlayer(what)
                | TriggerSpec::DealsCombatDamageToPlayerOrBattle(what)
                | TriggerSpec::DealsCombatDamageToPlayerOrPlaneswalker(what),
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
            (
                TriggerSpec::DealsCombatDamageToPlayerOrBattle(what),
                Event::Damage {
                    source,
                    target: Target::Object(b),
                    combat: true,
                    ..
                },
            ) => {
                if !self.characteristics(*b).is_some_and(|c| c.is_battle()) {
                    return None;
                }
                on_battlefield(what, *source)?;
                Some((Some(*source), self.protector(*b)))
            }
            (
                TriggerSpec::DealsCombatDamageToPlayerOrPlaneswalker(what),
                Event::Damage {
                    source,
                    target: Target::Object(w),
                    combat: true,
                    ..
                },
            ) => {
                if !self.characteristics(*w).is_some_and(|c| c.is_planeswalker()) {
                    return None;
                }
                on_battlefield(what, *source)?;
                Some((Some(*source), self.objects.get(*w).map(|o| o.controller)))
            }
            (
                TriggerSpec::CreaturesDealCombatDamage(f),
                Event::Damage {
                    source,
                    target: Target::Player(p),
                    combat: true,
                    ..
                },
            )
            | (
                TriggerSpec::CreaturesDealDamage(f),
                Event::Damage {
                    source,
                    target: Target::Player(p),
                    ..
                },
            ) => {
                filter_on_battlefield(f, *source)?;
                Some((None, Some(*p)))
            }
            (
                TriggerSpec::NoncombatDamageToOpponent { yours, .. },
                Event::Damage {
                    source,
                    target: Target::Player(p),
                    combat: false,
                    ..
                },
            ) => {
                if *p == w.controller {
                    return None;
                }
                // The source as it is, or as it last existed (a spell that
                // has left the stack, a sacrificed permanent).
                let controller = self
                    .objects
                    .get(*source)
                    .map(|o| o.controller)
                    .or_else(|| self.lki.get(source).map(|l| l.object.controller));
                (!*yours || controller == Some(w.controller)).then_some((None, Some(*p)))
            }
            // CR 709.5h: this door, as it enters or later.
            (TriggerSpec::UnlocksDoor { door }, Event::Unlocked { obj, door: d, .. }) => {
                (*obj == w.source && d == door).then_some((Some(*obj), Some(w.controller)))
            }
            // CR 709.5i.
            (
                TriggerSpec::FullyUnlocks { whose },
                Event::Unlocked {
                    obj, fully: true, ..
                },
            ) => {
                let p = self.objects.get(*obj)?.controller;
                whose_ok(*whose, p).then_some((Some(*obj), Some(p)))
            }
            (
                TriggerSpec::AttacksBattle(what),
                Event::Attack {
                    attacker,
                    target: Target::Object(b),
                },
            ) => {
                if !self.characteristics(*b).is_some_and(|c| c.is_battle()) {
                    return None;
                }
                let p = on_battlefield(what, *attacker)?;
                Some((Some(*attacker), Some(p)))
            }
            (TriggerSpec::GainsLife { whose }, Event::LifeChange { player, delta, .. }) => {
                (*delta > 0 && whose_ok(*whose, *player)).then_some((None, Some(*player)))
            }
            (TriggerSpec::LosesLife { whose }, Event::LifeChange { player, delta, .. }) => {
                (*delta < 0 && whose_ok(*whose, *player)).then_some((None, Some(*player)))
            }
            // Triggers are detected as each event is emitted, so the first
            // gain is the one whose amount is all gained this turn.
            (TriggerSpec::GainsLifeFirst { whose }, Event::LifeChange { player, delta, .. }) => {
                (*delta > 0
                    && whose_ok(*whose, *player)
                    && self.tallied(crate::card::TurnEvent::LifeGained, *player) == delta.unsigned_abs())
                .then_some((None, Some(*player)))
            }
            (TriggerSpec::BecomesAttachedTo(f), Event::Attached { obj, to }) => {
                if *to != w.source {
                    return None;
                }
                let p = filter_on_battlefield(f, *obj)?;
                Some((Some(*obj), Some(p)))
            }
            // CR 731.1a: day becoming night or night becoming day, not
            // the first day.
            (TriggerSpec::DayNight, Event::DayNight { changed: true, .. }) => Some((None, None)),
            // CR 701.21a: only a permanent's controller can sacrifice it.
            // The event comes just before it leaves, so the permanent (and
            // a source sacrificing itself) is still on the battlefield.
            (TriggerSpec::Sacrificed { what, whose }, Event::Sacrificed { obj, .. }) => {
                let p = on_battlefield(what, *obj)?;
                whose_ok(*whose, p).then_some((Some(*obj), Some(p)))
            }
            (TriggerSpec::WinsFlip { whose }, Event::FlippedCoin { player, won: true })
            | (TriggerSpec::RollsDice { whose }, Event::RolledDie { player, .. })
            | (TriggerSpec::CollectsEvidence { whose }, Event::CollectedEvidence { player })
            | (TriggerSpec::Forages { whose }, Event::Foraged { player })
            | (TriggerSpec::Discovers { whose }, Event::Discovered { player, .. })
            | (TriggerSpec::GivesGift { whose }, Event::GaveGift { player }) => {
                whose_ok(*whose, *player).then_some((None, Some(*player)))
            }
            // CR 719.3a: a Case becomes solved (its second stage).
            (TriggerSpec::SolvesCase { whose }, Event::StageChanged { obj, stage: 1 }) => {
                let o = self.objects.get(*obj)?;
                let case = self
                    .characteristics(*obj)
                    .is_some_and(|c| c.has_subtype(crate::card::Subtype::CASE));
                (case && whose_ok(*whose, o.controller)).then_some((Some(*obj), Some(o.controller)))
            }
            (TriggerSpec::ManifestsDread { whose }, Event::ManifestedDread { player, milled }) => {
                whose_ok(*whose, *player).then_some((*milled, Some(*player)))
            }
            // CR 122.6: "Whenever you put one or more +1/+1 counters on a
            // creature you control": counters its controller's effect put.
            (
                TriggerSpec::YouPutCounters { what, kind },
                Event::CountersAdded {
                    obj, kind: k, by, ..
                },
            ) => {
                if *by != w.controller || kind.is_some_and(|kind| kind != *k) {
                    return None;
                }
                let p = on_battlefield(what, *obj)?;
                Some((Some(*obj), Some(p)))
            }
            // CR 700.13: "Whenever you commit a crime [during your turn]".
            // "Whenever an opponent gains control of a permanent from you".
            (TriggerSpec::OpponentGainsControl, Event::ControlChanged { obj, from, to }) => {
                (*from == w.controller && *to != w.controller)
                    .then_some((Some(*obj), Some(*to)))
            }
            (TriggerSpec::CommitsCrime { your_turn }, Event::CommittedCrime { player }) => {
                (*player == w.controller && (!*your_turn || self.active_player == *player))
                    .then_some((None, Some(*player)))
            }
            // CR 122.1: "Whenever one or more +1/+1 counters are put on
            // this creature": once per event, however many counters.
            // `CounterKind::Any`: "Whenever a counter is put on a creature
            // you control", of any kind. "Whenever a +1/+1 counter is
            // removed from this creature": once for each counter
            // (`chapter_copies`).
            (TriggerSpec::CountersPut { what, kind }, Event::CountersAdded { obj, kind: k, .. })
            | (
                TriggerSpec::CountersRemoved { what, kind, .. },
                Event::CountersRemoved { obj, kind: k, .. },
            ) => {
                if *kind != crate::card::CounterKind::Any && k != kind {
                    return None;
                }
                let p = on_battlefield(what, *obj)?;
                Some((Some(*obj), Some(p)))
            }
            // "When the fourth plan counter is put on this enchantment".
            (
                TriggerSpec::NthCounterPut { kind, n: nth },
                Event::CountersAdded {
                    obj,
                    kind: k,
                    n,
                    before,
                    ..
                },
            ) => (*obj == w.source && k == kind && *before < *nth && before + n >= *nth)
                .then_some((Some(*obj), Some(w.controller))),
            // CR 714.2c: lore counters put on this Saga take their number
            // from below a chapter number to at least it.
            (
                TriggerSpec::Chapter(mask),
                Event::CountersAdded {
                    obj,
                    kind: crate::card::CounterKind::Lore,
                    n,
                    before,
                    ..
                },
            ) => (*obj == w.source && chapters_crossed(*mask, *before, *n) != 0)
                .then_some((Some(*obj), Some(w.controller))),
            // "Whenever a face-down permanent you control is turned face up"
            // (CR 708.8): it was face down until now; the rest of the filter
            // is judged as it is face up.
            (
                TriggerSpec::TurnedFaceUp(TriggerObj::Filter(f)),
                Event::TurnedFaceUp { obj },
            ) if f.status.face_down => {
                let mut g = f.clone();
                g.status.face_down = false;
                let p = filter_on_battlefield(&g, *obj)?;
                Some((Some(*obj), Some(p)))
            }
            // CR 716.2c: "When this Class becomes level 2".
            (TriggerSpec::BecomesStage(k), Event::StageChanged { obj, stage }) => {
                (*obj == w.source && stage == k).then_some((Some(*obj), Some(w.controller)))
            }
            (TriggerSpec::BecomesTarget(what), Event::Targeted { target: obj, .. })
            | (TriggerSpec::BecomesTapped(what), Event::Tapped { obj })
            | (TriggerSpec::TappedForTeamwork(what), Event::TappedForTeamwork { obj })
            | (TriggerSpec::BecomesUntapped(what), Event::Untapped { obj })
            // CR 708.8: judged as the permanent is face up.
            | (TriggerSpec::TurnedFaceUp(what), Event::TurnedFaceUp { obj })
            | (
                TriggerSpec::DealtDamage(what),
                Event::Damage {
                    target: Target::Object(obj),
                    ..
                },
            )
            | (
                TriggerSpec::DealtCombatDamage(what),
                Event::Damage {
                    target: Target::Object(obj),
                    combat: true,
                    ..
                },
            ) => {
                let p = on_battlefield(what, *obj)?;
                Some((Some(*obj), Some(p)))
            }
            (
                TriggerSpec::DealtDamageAtLeast(what, n),
                Event::Damage {
                    target: Target::Object(obj),
                    amount,
                    ..
                },
            ) if amount >= n => {
                let p = on_battlefield(what, *obj)?;
                Some((Some(*obj), Some(p)))
            }
            // The source as it last existed, if it has left (CR 603.10a).
            (
                TriggerSpec::DealsDamageAtLeast(what, n),
                Event::Damage {
                    source,
                    target,
                    amount,
                    ..
                },
            ) if amount >= n => {
                let ok = match what {
                    TriggerObj::This => *source == w.source,
                    TriggerObj::Filter(_) => on_battlefield(what, *source).is_some(),
                };
                if !ok {
                    return None;
                }
                match target {
                    Target::Player(p) => Some((None, Some(*p))),
                    Target::Object(o) => {
                        let c = self.objects.get(*o).map(|o| o.controller);
                        Some((Some(*o), c))
                    }
                }
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
                TriggerSpec::DealsDamageTo { what, filter },
                Event::Damage {
                    source,
                    target: Target::Object(o),
                    ..
                },
            ) => {
                let ok = match what {
                    TriggerObj::This => *source == w.source,
                    TriggerObj::Filter(_) => on_battlefield(what, *source).is_some(),
                };
                if !ok {
                    return None;
                }
                let obj = self.objects.get(*o)?;
                let c = self.characteristics(*o)?;
                (filter.matches(*o, obj.controller, &c, Some(w.source), Some(w.controller))
                    && self.status_ok(filter, *o))
                .then_some((Some(*o), Some(obj.controller)))
            }
            (
                TriggerSpec::CastsFiltered {
                    whose,
                    filter,
                    from,
                },
                Event::Cast {
                    player, spell, def, ..
                },
            ) => {
                if !whose_ok(*whose, *player) {
                    return None;
                }
                if *from != crate::card::SpellFrom::Anywhere
                    && !from.allows(self.objects.get(*spell).and_then(|o| o.cast.cast_from()))
                {
                    return None;
                }
                let c = self
                    .characteristics(*spell)
                    .or_else(|| self.pool.get(*def).printed)?;
                // "Whenever you cast a spell of the chosen color" (Diamond
                // Mare): what the source chose as it entered.
                let filter = &*self.with_source_chosen(filter, w.source);
                // Heroic (CR 207.2c): "a spell that targets this creature",
                // judged with the targets chosen as it was cast (CR 601.2c).
                let targets_ok = |f: &crate::card::ObjFilter| {
                    f.targeting.as_ref().is_none_or(|t| {
                        self.objects
                            .get(*spell)
                            .is_some_and(|o| self.spell_targets(o, t, w.controller, Some(w.source)))
                    })
                };
                // "an Equipment spell or a spell that targets a creature
                // you control": each side with its own targets.
                if let Some(alt) = &filter.or
                    && (filter.targeting.is_some() || alt.targeting.is_some())
                {
                    let mut base = filter.clone();
                    base.or = None;
                    let side = |f: &crate::card::ObjFilter| {
                        targets_ok(f)
                            && f.matches(*spell, *player, &c, Some(w.source), Some(w.controller))
                            && self.status_ok(f, *spell)
                    };
                    return (side(&base) || side(alt)).then_some((Some(*spell), Some(*player)));
                }
                (targets_ok(filter)
                    && filter.matches(*spell, *player, &c, Some(w.source), Some(w.controller))
                    && self.status_ok(filter, *spell))
                .then_some((Some(*spell), Some(*player)))
            }
            (TriggerSpec::DrawsNth { whose, n }, Event::Draw { player, .. }) => {
                (whose_ok(*whose, *player) && self.tallied(crate::card::TurnEvent::Drew, *player) == u32::from(*n))
                    .then_some((None, Some(*player)))
            }
            // Valiant: by a spell or ability its controller controls, the
            // first time this turn (`targeted_by_controller`, recorded just
            // after this event).
            (TriggerSpec::TargetedByYouFirstTime, Event::Targeted { target, by }) => {
                let by_ctl = self.objects.get(*by)?.controller;
                (*target == w.source
                    && by_ctl == w.controller
                    && !self.targeted_by_controller.contains(target))
                .then_some((Some(*by), Some(by_ctl)))
            }
            (TriggerSpec::TargetedFirstTime, Event::Targeted { target, by }) => {
                let by_ctl = self.objects.get(*by)?.controller;
                (*target == w.source).then_some((Some(*by), Some(by_ctl)))
            }
            (TriggerSpec::TargetedBy { what, spells, whose }, Event::Targeted { target, by }) => {
                let b = self.objects.get(*by)?;
                if *spells && b.kind == crate::state::ObjKind::Ability {
                    return None;
                }
                let by_ctl = b.controller;
                let ok = match whose {
                    Who::You => by_ctl == w.controller,
                    Who::Opponents => by_ctl != w.controller,
                    Who::Any => true,
                };
                if !ok {
                    return None;
                }
                on_battlefield(what, *target)?;
                Some((Some(*target), Some(by_ctl)))
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
                    any,
                },
                Event::Block { blocker, attacker },
            ) => {
                // CR 509.3b, 509.3d: once per blocking pair (a blocker that
                // blocks several attackers makes one event per attacker);
                // the other creature is judged as it is now (CR 509.3f).
                let (me, that) = if *blocks && on_battlefield(what, *blocker).is_some() {
                    (*blocker, *attacker)
                } else if *blocked && on_battlefield(what, *attacker).is_some() {
                    (*attacker, *blocker)
                } else {
                    return None;
                };
                let matches = |id: ObjId| {
                    self.objects.get(id).is_some_and(|o| {
                        self.characteristics(id).is_some_and(|c| {
                            other.matches(id, o.controller, &c, Some(w.source), Some(w.controller))
                        }) && self.status_ok(other, id)
                    })
                };
                if !matches(that) {
                    return None;
                }
                // "by one or more Orcs" (CR 509.3c, 509.3e): once, on the
                // first matching pair of the declaration.
                if *any {
                    let c = self.combat.as_ref()?;
                    let first = if me == *attacker {
                        c.attackers
                            .iter()
                            .find(|a| a.id == me)
                            .and_then(|a| a.blockers.iter().copied().find(|b| matches(*b)))
                    } else {
                        c.attackers
                            .iter()
                            .filter(|a| a.blockers.contains(&me))
                            .map(|a| a.id)
                            .find(|a| matches(*a))
                    };
                    if first != Some(that) {
                        return None;
                    }
                    // No one other creature is "that creature": "it" is
                    // the blocking or blocked creature itself (Hedron
                    // Blade's equipped creature).
                    let o = self.objects.get(me)?;
                    return Some((Some(me), Some(o.controller)));
                }
                let o = self.objects.get(that)?;
                Some((Some(that), Some(o.controller)))
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
            // Magecraft (CR 707.10): "Whenever you cast or copy ..." sees
            // copies too.
            (
                TriggerSpec::CastsSpell {
                    whose,
                    noncreature,
                    instant_or_sorcery,
                    ..
                },
                Event::Cast {
                    player, spell, def, ..
                },
            )
            | (
                TriggerSpec::CastsSpell {
                    whose,
                    noncreature,
                    instant_or_sorcery,
                    or_copy: true,
                },
                Event::SpellCopied { player, spell, def },
            ) => {
                let d = self.pool.get(*def);
                let ok = whose_ok(*whose, *player)
                    && (!*noncreature || !d.is_creature())
                    && (!*instant_or_sorcery || d.is_instant() || d.is_sorcery());
                ok.then_some((Some(*spell), Some(*player)))
            }
            (
                TriggerSpec::CastsNth {
                    whose,
                    nth:
                        nth @ (crate::card::SpellOrdinal::FirstNoncreature
                        | crate::card::SpellOrdinal::FirstInstantOrSorcery),
                },
                Event::Cast {
                    player, spell, def, ..
                },
            ) => {
                // The spells cast this turn include this one.
                let kind = |d: &crate::card::CardRef| {
                    let d = self.pool.get(*d);
                    if *nth == crate::card::SpellOrdinal::FirstNoncreature {
                        !d.is_creature()
                    } else {
                        d.is_instant() || d.is_sorcery()
                    }
                };
                let n = self
                    .cast_this_turn
                    .iter()
                    .filter(|(p, d)| p == player && kind(d))
                    .count();
                (whose_ok(*whose, *player) && kind(def) && n == 1)
                    .then_some((Some(*spell), Some(*player)))
            }
            (
                TriggerSpec::CastsNth {
                    whose,
                    nth:
                        nth @ (crate::card::SpellOrdinal::FromExile
                        | crate::card::SpellOrdinal::FromGraveyard),
                },
                Event::Cast { player, spell, .. },
            ) => {
                let zone = if *nth == crate::card::SpellOrdinal::FromExile {
                    Zone::Exile
                } else {
                    Zone::Graveyard
                };
                let from = self
                    .objects
                    .get(*spell)
                    .is_some_and(|o| o.cast.cast_from() == Some(zone));
                (whose_ok(*whose, *player) && from).then_some((Some(*spell), Some(*player)))
            }
            (TriggerSpec::CastsNth { whose, nth }, Event::Cast { player, spell, .. }) => {
                let n = u8::try_from(self.tallied(crate::card::TurnEvent::Cast, *player))
                    .unwrap_or(u8::MAX);
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
                // The first of the batch to trigger it is the triggering
                // object: "if they entered or were cast from a graveyard,
                // create a token that's a copy of one of them" (Twilight
                // Diviner) judges and copies it.
                Some((Some(*new), Some(p)))
            }
            // CR 701.21a: the sacrificed permanent is still on the
            // battlefield as the event is emitted.
            (TriggerSpec::Sacrifices(f), Event::Sacrificed { obj, .. }) => {
                let o = self.objects.get(*obj)?;
                if o.controller != w.controller {
                    return None;
                }
                let c = self.characteristics(*obj)?;
                (f.matches(*obj, o.controller, &c, Some(w.source), Some(w.controller))
                    && self.status_ok(f, *obj))
                .then_some((Some(*obj), Some(o.controller)))
            }
            (TriggerSpec::Draws { whose }, Event::Draw { player, card }) => {
                whose_ok(*whose, *player).then_some((Some(*card), Some(*player)))
            }
            // CR 701.9: judged as the card is discarded, still in the hand.
            (TriggerSpec::Discards { whose, filter }, Event::Discard { player, card }) => {
                let o = self.objects.get(*card)?;
                let c = self.base_characteristics(o);
                (whose_ok(*whose, *player)
                    && filter.matches(*card, *player, &c, Some(w.source), Some(w.controller)))
                .then_some((Some(*card), Some(*player)))
            }
            // CR 603.2c: once for the cards leaving the controller's
            // graveyard together, each judged as the card it was.
            (
                TriggerSpec::LeavesGraveyard(filter),
                Event::ZoneChange {
                    new,
                    def,
                    from: Zone::Graveyard,
                    ..
                },
            ) => {
                let owner = self.objects.get(*new)?.owner;
                let c = self.pool.get(*def).printed?;
                (owner == w.controller
                    && filter.matches(*new, owner, &c, Some(w.source), Some(w.controller)))
                .then_some((None, Some(owner)))
            }
            // "is put into your graveyard from the battlefield": the
            // permanent as it last existed, owned by the source's
            // controller.
            (
                TriggerSpec::PutIntoGraveyardFromBattlefield(filter),
                Event::ZoneChange {
                    old,
                    new,
                    from: Zone::Battlefield,
                    to: Zone::Graveyard,
                    ..
                },
            ) => {
                if self.objects.get(*new)?.owner != w.controller {
                    return None;
                }
                let l = self.lki.get(old)?;
                filter
                    .matches(
                        *old,
                        l.object.controller,
                        &l.characteristics,
                        Some(w.source),
                        Some(w.controller),
                    )
                    .then_some((Some(*old), Some(w.controller)))
            }
            // "is put into your graveyard from anywhere": the card in the
            // graveyard, a new object (CR 400.7).
            (
                TriggerSpec::PutIntoGraveyard(filter),
                Event::ZoneChange {
                    new,
                    to: Zone::Graveyard,
                    ..
                },
            ) => {
                let o = self.objects.get(*new)?;
                if o.owner != w.controller || o.zone != Zone::Graveyard {
                    return None;
                }
                let c = self.base_characteristics(o);
                filter
                    .matches(*new, o.owner, &c, Some(w.source), Some(w.controller))
                    .then_some((Some(*new), Some(o.owner)))
            }
            (
                TriggerSpec::EntersFromGraveyard(filter),
                Event::ZoneChange {
                    new,
                    from: Zone::Graveyard,
                    to: Zone::Battlefield,
                    ..
                },
            ) => {
                if self.objects.get(*new)?.owner != w.controller {
                    return None;
                }
                let p = filter_on_battlefield(filter, *new)?;
                Some((Some(*new), Some(p)))
            }
            (
                TriggerSpec::EntersFrom { what, zone, yours },
                Event::ZoneChange {
                    new,
                    from,
                    to: Zone::Battlefield,
                    ..
                },
            ) if from == zone => {
                if *yours && self.objects.get(*new)?.owner != w.controller {
                    return None;
                }
                let p = on_battlefield(what, *new)?;
                Some((Some(*new), Some(p)))
            }
            // CR 700.14: the spell whose payment crossed `n`.
            (TriggerSpec::Expends(n), Event::Cast { player, spell, .. }) => {
                let i = player.index().min(3);
                let n = u16::from(*n);
                (*player == w.controller
                    && self.mana_spent_before_cast[i] < n
                    && self.mana_spent_this_turn[i] >= n)
                    .then_some((Some(*spell), Some(*player)))
            }
            (TriggerSpec::Either(pair), _) => self
                .fires(&pair.0, event, w)
                .or_else(|| self.fires(&pair.1, event, w)),
            // CR 603.2c: batched by `detect_triggers`.
            (TriggerSpec::Batch(inner) | TriggerSpec::Each(inner), _) => self.fires(inner, event, w),
            _ => self.fires_more(spec, event, w),
        }
    }

    /// `fires` for the batched and keyword-action triggers: "Whenever you
    /// discard one or more cards", "Whenever you scry or surveil", ...
    #[cold]
    #[inline(never)]
    #[allow(clippy::too_many_lines)] // one arm per trigger kind
    fn fires_more(
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
        let matches_now = |f: &crate::card::ObjFilter, obj: ObjId| -> Option<PlayerId> {
            let o = self.objects.get(obj)?;
            let c = self.characteristics(obj)?;
            (f.matches(obj, o.controller, &c, Some(w.source), Some(w.controller))
                && self.status_ok(f, obj))
            .then_some(o.controller)
        };
        match (spec, event) {
            // CR 603.2c: the cards discarded together are one batch.
            (TriggerSpec::DiscardsBatch { whose }, Event::Discard { player, .. }) => {
                whose_ok(*whose, *player).then_some((None, Some(*player)))
            }
            // CR 701.21a: only a permanent's controller sacrifices it; it
            // is still on the battlefield as the event is emitted.
            (TriggerSpec::SacrificesBatch(f), Event::Sacrificed { obj, .. }) => {
                let p = matches_now(f, *obj)?;
                (p == w.controller).then_some((None, Some(p)))
            }
            // CR 508.1m: the attack events follow the whole declaration,
            // so the first matching attacker (attacking that player)
            // stands for it.
            (
                TriggerSpec::AttacksWithFilter {
                    filter,
                    player,
                    min,
                },
                Event::Attack { attacker, target },
            ) => {
                if self.objects.get(*attacker)?.controller != w.controller {
                    return None;
                }
                let c = self.combat.as_ref()?;
                let qualifies = |a: &&crate::state::Attacker| {
                    (!*player || a.attacking == *target) && matches_now(filter, a.id).is_some()
                };
                let first = c.attackers.iter().find(qualifies)?;
                if first.id != *attacker {
                    return None;
                }
                // "two or more Spiders".
                if *min > 1 && c.attackers.iter().filter(qualifies).count() < usize::from(*min) {
                    return None;
                }
                match (player, target) {
                    (false, _) => Some((None, Some(w.controller))),
                    (true, Target::Player(p)) => Some((None, Some(*p))),
                    (true, Target::Object(_)) => None,
                }
            }
            // CR 508.1m: the first attacker at a matching player stands
            // for the declaration (or, with `each`, for that player).
            (TriggerSpec::PlayersAttacked { whose, each }, Event::Attack { attacker, target }) => {
                let Target::Player(p) = *target else {
                    return None;
                };
                if !whose_ok(*whose, p) {
                    return None;
                }
                let c = self.combat.as_ref()?;
                let first = c.attackers.iter().find(|a| match a.attacking {
                    Target::Player(q) => {
                        if *each {
                            q == p
                        } else {
                            whose_ok(*whose, q)
                        }
                    }
                    Target::Object(_) => false,
                })?;
                (first.id == *attacker).then_some((None, Some(p)))
            }
            // CR 119.9: once per event that gains or loses life, however
            // much.
            (
                TriggerSpec::GainsOrLosesLife { your_turn },
                Event::LifeChange { player, delta, .. },
            ) => (*delta != 0
                && *player == w.controller
                && (!*your_turn || self.active_player == w.controller))
                .then_some((None, Some(*player))),
            (TriggerSpec::ScriesOrSurveils, Event::Scried { player, .. }) => {
                (*player == w.controller).then_some((None, Some(*player)))
            }
            (
                TriggerSpec::Scries,
                Event::Scried {
                    player,
                    surveil: false,
                    ..
                },
            ) => (*player == w.controller).then_some((None, Some(*player))),
            (TriggerSpec::CrewsOrSaddles(what), Event::Crewed { crew, vehicle }) => {
                let p = match what {
                    TriggerObj::This => (*crew == w.source).then_some(w.controller)?,
                    TriggerObj::Filter(f) => matches_now(f, *crew)?,
                };
                Some((Some(*vehicle), Some(p)))
            }
            (TriggerSpec::BecomesSaddled(what), Event::BecameSaddled { obj }) => {
                let p = match what {
                    TriggerObj::This => (*obj == w.source).then_some(w.controller)?,
                    TriggerObj::Filter(f) => matches_now(f, *obj)?,
                };
                Some((Some(*obj), Some(p)))
            }
            // CR 701.44c: the creature explores even if it has left the
            // battlefield; then it is judged as it last existed.
            (TriggerSpec::Explores(what), Event::Explored { obj }) => match what {
                crate::card::TriggerObj::This => {
                    (*obj == w.source).then_some((Some(*obj), Some(w.controller)))
                }
                crate::card::TriggerObj::Filter(f) => {
                    if let Some(p) = matches_now(f, *obj) {
                        return Some((Some(*obj), Some(p)));
                    }
                    if self
                        .objects
                        .get(*obj)
                        .is_some_and(|o| o.zone == Zone::Battlefield)
                    {
                        return None;
                    }
                    let l = self.lki.get(obj)?;
                    f.matches(
                        *obj,
                        l.object.controller,
                        &l.characteristics,
                        Some(w.source),
                        Some(w.controller),
                    )
                    .then_some((Some(*obj), Some(l.object.controller)))
                }
            },
            // CR 106.11a: "Whenever you tap this creature for mana",
            // "Whenever a player taps a land for mana".
            (TriggerSpec::TapsForMana { what, whose }, Event::TappedForMana { obj }) => {
                let p = match what {
                    TriggerObj::This => (*obj == w.source).then_some(w.controller)?,
                    TriggerObj::Filter(f) => matches_now(f, *obj)?,
                };
                let ok = match whose {
                    Who::You => p == w.controller,
                    Who::Opponents => p != w.controller,
                    Who::Any => true,
                };
                ok.then_some((Some(*obj), Some(p)))
            }
            (TriggerSpec::YouTap(f), Event::TappedBy { obj, player }) => {
                if *player != w.controller {
                    return None;
                }
                // "an untapped creature": it was untapped until this
                // event, and is tapped now.
                let p = if f.tapped.is_some() {
                    let f = crate::card::ObjFilter {
                        tapped: None,
                        ..f.clone()
                    };
                    matches_now(&f, *obj)?
                } else {
                    matches_now(f, *obj)?
                };
                Some((Some(*obj), Some(p)))
            }
            (TriggerSpec::Activates { whose, kind }, Event::Activated { obj, source, .. }) => {
                let o = self.objects.get(*obj)?;
                let activator = o.controller;
                if !whose_ok(*whose, activator) {
                    return None;
                }
                let crate::card::AbilityDef::Activated(ab) = self.pool.ability(o.ability?.ability)
                else {
                    return None;
                };
                let ok = match kind {
                    crate::card::ActivationKind::Exhaust => {
                        ab.keyword == Some(crate::card::ActivatedKeyword::Exhaust)
                    }
                    crate::card::ActivationKind::Loyalty => ab.is_loyalty(),
                    crate::card::ActivationKind::Ninjutsu => {
                        ab.keyword == Some(crate::card::ActivatedKeyword::Ninjutsu)
                    }
                    // The source as it is, or as it last existed (a
                    // creature sacrificed to pay the cost).
                    crate::card::ActivationKind::OfCreature => self
                        .characteristics(*source)
                        .or_else(|| self.lki.get(source).map(|l| l.characteristics))
                        .is_some_and(|c| c.is_creature()),
                };
                ok.then_some((Some(*source), Some(activator)))
            }
            // CR 701.27: the ability is read from the face it transformed
            // into, so it sees only that.
            (TriggerSpec::Transforms, Event::Transformed { obj }) => {
                (*obj == w.source).then_some((Some(*obj), Some(w.controller)))
            }
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

    /// CR 603.8: state triggers whose conditions hold trigger as a player
    /// would receive priority, each only once until its ability has left
    /// the stack (or, never put there, is no longer waiting).
    pub(super) fn check_state_triggers(&mut self) {
        let mut fired: SmallVec<[PendingTrigger; 1]> = SmallVec::new();
        for &id in &self.battlefield {
            let o = &self.objects[id];
            if !self.pool.get(o.def).flags().has_state_trigger() {
                continue;
            }
            let Some(chars) = self.characteristics(id) else {
                continue;
            };
            for &ability in &chars.abilities {
                let AbilityDef::Triggered(t) = self.pool.ability(ability) else {
                    continue;
                };
                let TriggerSpec::State(cond) = t.trigger else {
                    continue;
                };
                let waiting = self
                    .pending_triggers
                    .iter()
                    .any(|p| p.ability.ability == ability && p.ability.source == id)
                    || self.stack.iter().any(|s| {
                        self.objects[*s]
                            .ability
                            .is_some_and(|a| a.ability == ability && a.source == id)
                    });
                if waiting || !self.condition_for(self.pool.condition(cond), o.controller, id, None)
                {
                    continue;
                }
                fired.push(PendingTrigger {
                    controller: o.controller,
                    ability: StackAbility {
                        ability,
                        source: id,
                        def: chars.def,
                        trigger_obj: None,
                        trigger_player: None,
                        trigger_amount: 0,
                    },
                    timestamp: 0,
                });
            }
        }
        for mut t in fired {
            t.timestamp = self.tick();
            self.pending_triggers.push(t);
        }
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
    #[allow(clippy::too_many_lines)] // one carry-over per kind of state
    pub(super) fn put_trigger(&mut self, i: usize) {
        let t = self.pending_triggers.remove(i);
        let def = t.ability.def;
        let specs = self.pool.ability(t.ability.ability).targets().to_vec();
        // CR 603.3c: a modal triggered ability with no mode whose targets
        // can be chosen is removed.
        let modal = match self.pool.ability(t.ability.ability).effects() {
            [crate::card::Effect::Modes(modes)] => Some(modes.iter().enumerate().any(|(i, m)| {
                !self.mode_used(t.ability.source, t.ability.ability, i, m)
                    && self.mode_targets_available(t.controller, m, Some(t.ability.source))
            })),
            _ => None,
        };
        if modal == Some(false)
            || !self.targets_available_for(
                t.controller,
                &specs,
                Some(t.ability.source),
                t.ability.trigger_player,
            )
        {
            // A Saga whose last chapter ability is removed for want of
            // a target is sacrificed (CR 714.4): the state-based check
            // must look again.
            self.sba_touch();
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
            marks: 0,
            chosen: Chosen::None,
            alt: crate::state::AltCast::None,
            timestamp,
            targets: SmallVec::new(),
            counters: SmallVec::new(),
            attached_to: None,
            ability: Some(t.ability),
            revealed: 0,
            sized: 0,
        });
        self.stack.push(obj);
        // CR 107.3m: "When this creature enters, it deals X damage": the
        // ability's X is that of the spell that became the permanent, for
        // its targets ("with mana value X or less") and its effects.
        if !self.entered_x.is_empty()
            && let AbilityDef::Triggered(tr) = self.pool.ability(t.ability.ability)
            && tr.trigger == TriggerSpec::Enters(crate::card::TriggerObj::This)
            && let Some(&(_, x)) = self.entered_x.iter().find(|(o, _)| *o == t.ability.source)
        {
            self.x_values.push((obj, x));
        }
        if !self.reflexive_x.is_empty()
            && let Some(i) = self
                .reflexive_x
                .iter()
                .position(|(ts, _)| *ts == t.timestamp)
        {
            let (_, x) = self.reflexive_x.swap_remove(i);
            self.x_values.push((obj, x));
        }
        if !self.pending_granters.is_empty()
            && let Some(i) = self
                .pending_granters
                .iter()
                .position(|(ts, _)| *ts == t.timestamp)
        {
            let (_, g) = self.pending_granters.swap_remove(i);
            self.granters.push((obj, g));
        }
        if !self.reflexive_moved.is_empty()
            && let Some(i) = self
                .reflexive_moved
                .iter()
                .position(|(ts, _)| *ts == t.timestamp)
        {
            let (_, moved) = self.reflexive_moved.swap_remove(i);
            self.paid_costs.push((obj, moved));
        }
        // "Cycling {X}{G}{G}" and "When you cycle this card, destroy ...
        // with mana value X": X is the value paid for cycling (CR 107.3,
        // 702.29d), carried as the trigger's amount.
        if t.ability.trigger_amount > 0
            && t.ability.trigger_obj == Some(t.ability.source)
            && let AbilityDef::Triggered(tr) = self.pool.ability(t.ability.ability)
            && tr.trigger.cycles_this()
        {
            self.x_values.push((obj, t.ability.trigger_amount));
        }
        self.consecutive_passes = 0;
        self.bump();
        self.emit(Event::TriggerOnStack {
            obj,
            source: t.ability.source,
            def,
        });
        self.flow = if modal.is_some() {
            Flow::ChoosingMode {
                spell: obj,
                sacrifice: None,
                discard: None,
            }
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
    #[allow(clippy::too_many_lines)]
    pub(super) fn resolve_ability(&mut self, id: ObjId) {
        let obj = self.objects[id].clone();
        let sa = obj.ability.expect("ability object");
        let (condition, limit, reflexive) = match self.pool.ability(sa.ability) {
            AbilityDef::Triggered(t) => (t.condition, t.limit, t.trigger == TriggerSpec::Reflexive),
            AbilityDef::Activated(_) => (None, crate::card::TurnLimit::None, false),
            _ => unreachable!("only triggered and activated abilities use the stack"),
        };
        // A modal ability's targets and effects are its chosen mode's.
        let pool = std::sync::Arc::clone(&self.pool);
        let targets = crate::state::targets_in(&pool, &obj);
        let effects = crate::state::effects_in(&pool, &obj);
        let split = self.mode_splits_of(id);
        if let Some(cond) = condition
            && !self.condition_holds(cond, sa.source, sa.trigger_obj, sa.trigger_player)
        {
            // A Saga whose last chapter ability is removed for want of
            // a target is sacrificed (CR 714.4): the state-based check
            // must look again.
            self.sba_touch();
            self.emit(Event::TriggerRemoved {
                source: sa.source,
                def: obj.def,
            });
            self.remove_from_game(id);
            return;
        }
        let mut legal = self.targets_legal(
            obj.controller,
            targets,
            &obj.targets,
            Some(sa.source),
            sa.trigger_player,
            sa.trigger_obj,
        );
        // An activated ability's X (CR 107.3): "target artifact with mana
        // value X" is judged with it.
        let x = if self.x_values.is_empty() {
            0
        } else {
            self.x_of(id)
        };
        if x > 0 || self.x_chosen(id).is_some() {
            for (i, l) in legal.iter_mut().enumerate() {
                if let (Some(spec), Some(t)) =
                    (crate::card::spec_for(targets, i), obj.targets.get(i))
                {
                    *l = *l
                        && self.is_legal_target_x(
                            obj.controller,
                            spec,
                            *t,
                            Some(sa.source),
                            Some(x),
                        );
                }
            }
        }
        if super::casting::all_illegal(&legal, &obj.targets) {
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
            // A reflexive ability's "that token" (see `Effect::Reflexive`).
            created: if reflexive { sa.trigger_obj } else { None },
            revealed: Vec::new(),
            x,
            did: true,
            chosen: Chosen::None,
            couldnt: 0,
            created_tokens: SmallVec::new(),
            counts: self.pool.counts_owner(sa.ability),
        };
        let outer = self.begin_this_way(id);
        // "Whenever one or more creatures attack you, those creatures get
        // -1/-0": the attackers at that player are the ability's group.
        if let AbilityDef::Triggered(t) = self.pool.ability(sa.ability)
            && let TriggerSpec::PlayersAttacked { whose, each } = t.trigger
        {
            self.note_attackers(obj.controller, whose, each, sa.trigger_player);
        }
        // "Do this only once each turn.": its "you may" asks only until it
        // is done (`Effect::YouMay`).
        let doing = if limit == crate::card::TurnLimit::Do {
            self.doing_once.replace((sa.source, sa.ability))
        } else {
            self.doing_once.take()
        };
        // "If this is the second time this ability has resolved this
        // turn": this resolution counts.
        let key = (sa.source, sa.ability);
        self.resolved_this_turn.push(key);
        let number = self
            .resolved_this_turn
            .iter()
            .filter(|k| **k == key)
            .count();
        let outer_number = std::mem::replace(
            &mut self.resolution_number,
            u8::try_from(number).unwrap_or(u8::MAX),
        );
        // CR 607.2a: what this ability exiles is linked to its source if
        // the source's card refers to "cards exiled with" it.
        let links = self
            .objects
            .get(sa.source)
            .map(|o| o.def)
            .or_else(|| self.lki.get(&sa.source).map(|l| l.object.def))
            .is_some_and(|d| self.pool.get(d).links_exile);
        let outer_linking = if links {
            self.linking.replace(sa.source)
        } else {
            self.linking.take()
        };
        self.apply_effects(&mut ctx, effects, &split);
        self.linking = outer_linking;
        self.resolution_number = outer_number;
        self.doing_once = doing;
        self.end_this_way(outer);
        if self.objects.contains_key(id) {
            self.remove_from_game(id);
        }
    }

    /// "Those creatures" in a `TriggerSpec::PlayersAttacked` ability (CR
    /// 508.1m): the creatures attacking the player attacked (with `each`)
    /// or any matching player, as the ability resolves, are the group its
    /// effects name (`Recipient::ThoseObjects`).
    #[cold]
    fn note_attackers(
        &mut self,
        controller: PlayerId,
        whose: Who,
        each: bool,
        attacked: Option<PlayerId>,
    ) {
        let (Some(log), Some(c)) = (&mut self.this_way, &self.combat) else {
            return;
        };
        log.group.clear();
        log.group.extend(
            c.attackers
                .iter()
                .filter(|a| match a.attacking {
                    Target::Player(q) if each => Some(q) == attacked,
                    Target::Player(q) => match whose {
                        Who::Any => true,
                        Who::You => q == controller,
                        Who::Opponents => q != controller,
                    },
                    Target::Object(_) => false,
                })
                .map(|a| a.id),
        );
    }

    /// An ability's intervening "if", evaluated against the current state.
    fn condition_holds(
        &self,
        cond: Condition,
        source: ObjId,
        trigger_obj: Option<ObjId>,
        trigger_player: Option<PlayerId>,
    ) -> bool {
        match cond {
            Condition::SourceUntapped => self
                .objects
                .get(source)
                .is_some_and(|o| o.zone == Zone::Battlefield && !o.tapped),
            Condition::SourceTapped => self
                .objects
                .get(source)
                .is_some_and(|o| o.zone == Zone::Battlefield && o.tapped),
            Condition::NoCreatures => self.no_creatures(),
            Condition::Kicked => self.was_kicked(source),
            Condition::HadNoCounters(k) => self.had_no_counters(source, k),
            Condition::HadCounters(k) => self.had_counters(source, k),
            Condition::TriggerHadCounters => trigger_obj.is_some_and(|o| self.had_any_counters(o)),
            Condition::NoSpellsLastTurn => self.spells_last_turn.iter().all(|n| *n == 0),
            Condition::TwoSpellsLastTurn => self.spells_last_turn.iter().any(|n| *n >= 2),
            Condition::EchoDue => self
                .objects
                .get(source)
                .is_some_and(crate::state::GameObject::echo_due),
            Condition::NotRenowned => self.objects.get(source).is_some_and(|o| !o.renowned()),
            Condition::WasBlocking(b) => self.was_blocking(source) == b,
            Condition::NotPrepared => self.objects.get(source).is_some_and(|o| !o.prepared()),
            Condition::Harnessed => self
                .objects
                .get(source)
                .is_some_and(|o| o.marks & crate::state::OBJ_HARNESSED != 0),
            Condition::Evolves => {
                let this = self.characteristics_or_lki(source);
                self.evolves(this.as_ref(), trigger_obj)
            }
            Condition::Increments => {
                let this = self.characteristics_or_lki(source);
                self.increments(this.as_ref(), trigger_obj)
            }
            Condition::Evoked => self
                .objects
                .get(source)
                .map(|o| o.cast.evoked())
                .or_else(|| self.lki.get(&source).map(|l| l.object.cast.evoked()))
                .unwrap_or(false),
            Condition::Blitzed => self.blitzed(source),
            Condition::MeldPair => self
                .objects
                .get(source)
                .is_some_and(|o| self.meld_partner(source, o.controller).is_some()),
            Condition::TributeNotPaid => !self.tribute_paid(source),
            // Judged only as the ability triggered.
            Condition::While(_)
            | Condition::WhileSecond(_)
            | Condition::Trains
            | Condition::Dethrones
            | Condition::AttackedOpponent => true,
            Condition::Static(i) => {
                let you = self
                    .objects
                    .get(source)
                    .map(|o| o.controller)
                    .or_else(|| self.lki.get(&source).map(|l| l.object.controller));
                you.is_some_and(|p| {
                    self.condition_about(
                        self.pool.condition(i),
                        p,
                        source,
                        trigger_obj,
                        trigger_player,
                    )
                })
            }
        }
    }

    /// The same, as the ability would trigger, for a watcher whose state
    /// was captured with the event.
    fn condition_holds_for(
        &self,
        cond: Condition,
        w: &Watcher,
        trigger_obj: Option<ObjId>,
        trigger_player: Option<PlayerId>,
    ) -> bool {
        match cond {
            Condition::SourceUntapped => !w.tapped,
            Condition::SourceTapped => w.tapped,
            Condition::NoCreatures => self.no_creatures(),
            Condition::Kicked => self.was_kicked(w.source),
            Condition::HadNoCounters(k) => self.had_no_counters(w.source, k),
            Condition::HadCounters(k) => self.had_counters(w.source, k),
            Condition::TriggerHadCounters => trigger_obj.is_some_and(|o| self.had_any_counters(o)),
            Condition::NoSpellsLastTurn => self.spells_last_turn.iter().all(|n| *n == 0),
            Condition::TwoSpellsLastTurn => self.spells_last_turn.iter().any(|n| *n >= 2),
            Condition::EchoDue => self
                .objects
                .get(w.source)
                .is_some_and(crate::state::GameObject::echo_due),
            Condition::NotRenowned => self.objects.get(w.source).is_some_and(|o| !o.renowned()),
            Condition::WasBlocking(b) => self.was_blocking(w.source) == b,
            Condition::NotPrepared => self.objects.get(w.source).is_some_and(|o| !o.prepared()),
            Condition::Harnessed => self
                .objects
                .get(w.source)
                .is_some_and(|o| o.marks & crate::state::OBJ_HARNESSED != 0),
            Condition::Evolves => self.evolves(Some(&w.chars), trigger_obj),
            Condition::Increments => self.increments(Some(&w.chars), trigger_obj),
            Condition::Evoked => self.objects.get(w.source).is_some_and(|o| o.cast.evoked()),
            Condition::Static(i) | Condition::While(i) | Condition::WhileSecond(i) => self
                .condition_about(
                    self.pool.condition(i),
                    w.controller,
                    w.source,
                    trigger_obj,
                    trigger_player,
                ),
            Condition::Trains => self.trains(w),
            Condition::Blitzed => self.blitzed(w.source),
            Condition::TributeNotPaid => !self.tribute_paid(w.source),
            Condition::Dethrones => self.dethrones(w.source),
            Condition::AttackedOpponent => self.attacked_opponent(w.controller),
            Condition::MeldPair => self.meld_partner(w.source, w.controller).is_some(),
        }
    }

    /// Tribute (CR 702.104b): an opponent put the tribute counters on the
    /// source as it entered.
    fn tribute_paid(&self, source: ObjId) -> bool {
        self.enter_notes
            .iter()
            .any(|(o, n)| *o == source && *n == crate::state::EnterNote::TributePaid)
    }

    /// Blitz (CR 702.152a): the source (as it last existed, once it has
    /// died) was cast for its blitz cost.
    fn blitzed(&self, source: ObjId) -> bool {
        self.objects
            .get(source)
            .map(|o| o.alt)
            .or_else(|| self.lki.get(&source).map(|l| l.object.alt))
            == Some(crate::state::AltCast::Blitz)
    }

    /// Training (CR 702.149a): another creature attacking under the same
    /// player's control has greater power than the source.
    fn trains(&self, w: &Watcher) -> bool {
        let Some(combat) = &self.combat else {
            return false;
        };
        let power = w.chars.power();
        combat.attackers.iter().any(|a| {
            a.id != w.source
                && self
                    .objects
                    .get(a.id)
                    .is_some_and(|o| o.controller == w.controller)
                && self
                    .characteristics(a.id)
                    .is_some_and(|c| c.power() > power)
        })
    }

    /// Dethrone (CR 702.105a): the source attacks a player with the most
    /// life or tied for the most.
    fn dethrones(&self, source: ObjId) -> bool {
        let Some(crate::state::Target::Player(p)) = self
            .combat
            .as_ref()
            .and_then(|c| c.attacker(source))
            .map(|a| a.attacking)
        else {
            return false;
        };
        let life = self.player(p).life;
        self.players_in_game().all(|q| self.player(q).life <= life)
    }

    /// Whether `you` attacked an opponent (a player, not a planeswalker or
    /// battle) with a creature this combat (melee, CR 702.121a).
    fn attacked_opponent(&self, you: PlayerId) -> bool {
        self.combat.as_ref().is_some_and(|c| {
            c.attackers.iter().any(|a| {
                matches!(a.attacking, crate::state::Target::Player(p) if p != you)
                    && self.objects.get(a.id).is_some_and(|o| o.controller == you)
            })
        })
    }

    /// Evolve (CR 702.100a): the entering creature (as it last existed, if
    /// it has left) has greater power or greater toughness than the
    /// creature with evolve. Neither can be a noncreature (CR 702.100c).
    fn evolves(&self, this: Option<&Characteristics>, other: Option<ObjId>) -> bool {
        let Some(this) = this.filter(|c| c.is_creature()) else {
            return false;
        };
        other
            .and_then(|o| self.characteristics_or_lki(o))
            .filter(Characteristics::is_creature)
            .is_some_and(|c| c.power() > this.power() || c.toughness() > this.toughness())
    }

    /// Increment (CR 702.191a): the creature with increment is a creature
    /// and the mana spent to cast the triggering spell is greater than its
    /// power or its toughness.
    fn increments(&self, this: Option<&Characteristics>, spell: Option<ObjId>) -> bool {
        let Some(this) = this.filter(|c| c.is_creature()) else {
            return false;
        };
        let spent = spell.map_or(0, |s| {
            i64::from(self.spent_on(s).count(crate::card::SpentKind::Total))
        });
        spent > i64::from(this.power()) || spent > i64::from(this.toughness())
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

    /// Whether the object had any counter, as it last existed.
    fn had_any_counters(&self, id: ObjId) -> bool {
        self.lki
            .get(&id)
            .map(|l| l.object.counters.iter().any(|(_, n)| *n > 0))
            .or_else(|| {
                self.objects
                    .get(id)
                    .map(|o| o.counters.iter().any(|(_, n)| *n > 0))
            })
            .unwrap_or(false)
    }

    /// Whether `id` was a blocking creature as it left the battlefield
    /// (or is one now, if it hasn't left).
    fn was_blocking(&self, id: ObjId) -> bool {
        match self.lki.get(&id) {
            Some(l) => l.blocking,
            None => self
                .combat
                .as_ref()
                .is_some_and(|c| c.attackers.iter().any(|a| a.blockers.contains(&id))),
        }
    }

    fn had_counters(&self, id: ObjId, k: crate::card::CounterKind) -> bool {
        self.lki
            .get(&id)
            .map(|l| l.object.counters(k) > 0)
            .or_else(|| self.objects.get(id).map(|o| o.counters(k) > 0))
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
        if !self.x_values.is_empty() {
            self.x_values.retain(|(s, _)| *s != id);
        }
        self.remove_from_zone_list(id);
        self.objects.remove(id);
        self.bump();
        // CR 714.4: a Saga waits for its chapter abilities to leave the
        // stack, which changes nothing else a state-based action sees.
        if self.saga_count > 0 {
            self.sba_touch();
        }
    }
}

/// How many times a triggered ability triggers on `event`: once, but a
/// chapter ability once for each chapter number its lore counters reach
/// (CR 714.2c).
fn chapter_copies(trigger: &TriggerSpec, event: &Event) -> usize {
    match (trigger, event) {
        (TriggerSpec::Chapter(mask), Event::CountersAdded { n, before, .. }) => {
            chapters_crossed(*mask, *before, *n).count_ones() as usize
        }
        (
            TriggerSpec::CountersRemoved {
                one_or_more: false, ..
            },
            Event::CountersRemoved { n, .. },
        ) => *n as usize,
        _ => 1,
    }
}

/// The chapter numbers in `mask` (bit `n` for chapter `n`) that `n` lore
/// counters put on a Saga with `before` of them reach (CR 714.2c): those
/// above `before` and at most `before + n`.
fn chapters_crossed(mask: u16, before: u32, n: u32) -> u16 {
    let bits = |k: u32| -> u16 {
        if k >= 15 {
            u16::MAX
        } else {
            (1u16 << (k + 1)) - 1
        }
    };
    mask & bits(before.saturating_add(n)) & !bits(before)
}
