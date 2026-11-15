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
    Characteristics, Flow, GameObject, GameState, ObjKind, PendingTrigger, StackAbility, Target,
    Zone,
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
                    && !condition_holds_for(cond, w)
                {
                    continue;
                }
                fired.push(PendingTrigger {
                    controller: w.controller,
                    ability: StackAbility {
                        ability,
                        source: w.source,
                        trigger_obj,
                        trigger_player,
                    },
                    timestamp: 0,
                });
            }
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

    /// Cheap pre-check: could anything have a triggered ability that sees
    /// this event?
    fn any_trigger_sources(&self, event: &Event) -> bool {
        debug_assert_eq!(
            self.trigger_sources as usize,
            self.battlefield
                .iter()
                .filter(|id| self.pool.get(self.objects[**id].def).has_triggers())
                .count(),
            "trigger source count out of sync with the battlefield"
        );
        if self.trigger_sources > 0 {
            return true;
        }
        if let Some(lb) = &self.lookback
            && lb
                .iter()
                .any(|l| self.pool.get(l.characteristics.def).has_triggers())
        {
            return true;
        }
        // The object that left may itself have a leaves-the-battlefield
        // trigger.
        matches!(event, Event::ZoneChange { def, .. } if self.pool.get(*def).has_triggers())
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
                    self.pool
                        .get(l.characteristics.def)
                        .may_watch(&self.pool, event)
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
                // of event pay for a characteristics lookup.
                if !self.pool.get(o.def).may_watch(&self.pool, event) {
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
            && self.pool.get(l.object.def).may_watch(&self.pool, event)
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
        // Does `obj` (currently on the battlefield) match?
        let on_battlefield = |what: &TriggerObj, obj: ObjId| -> Option<PlayerId> {
            let o = self.objects.get(obj)?;
            let ok = match what {
                TriggerObj::This => obj == w.source,
                TriggerObj::Filter(f) => {
                    let c = self.characteristics(obj)?;
                    f.matches(obj, o.controller, &c, Some(w.source), Some(w.controller))
                }
            };
            ok.then_some(o.controller)
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
            (TriggerSpec::BeginningOfStep { step, whose }, Event::StepBegin { step: s }) => {
                (s == step && whose_ok(*whose, self.active_player))
                    .then_some((None, Some(self.active_player)))
            }
            (TriggerSpec::Attacks(what), Event::Attack { attacker, .. }) => {
                let p = on_battlefield(what, *attacker)?;
                Some((Some(*attacker), Some(p)))
            }
            (TriggerSpec::Blocks(what), Event::Block { blocker, .. }) => {
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
            (
                TriggerSpec::CastsSpell { whose, noncreature },
                Event::Cast {
                    player, spell, def, ..
                },
            ) => {
                let ok = whose_ok(*whose, *player)
                    && (!*noncreature || !self.pool.get(*def).is_creature());
                ok.then_some((Some(*spell), Some(*player)))
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
        let def = self
            .objects
            .get(t.ability.source)
            .map(|o| o.def)
            .or_else(|| self.lki.get(&t.ability.source).map(|l| l.object.def))
            .expect("a trigger's source existed");
        let specs = self.pool.ability(t.ability.ability).targets().to_vec();
        if specs
            .iter()
            .any(|s| self.legal_targets(t.controller, *s).is_empty())
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
            kind: ObjKind::Ability,
            owner: t.controller,
            controller: t.controller,
            zone: Zone::Stack,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            controlled_since_turn_start: false,
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
        self.flow = if specs.is_empty() {
            Flow::Priority
        } else {
            Flow::Casting {
                spell: obj,
                next_target: 0,
            }
        };
    }

    // ----- resolution --------------------------------------------------------------

    /// CR 608.2 for an ability object on top of the stack: check its
    /// intervening "if" (CR 603.4) and targets, apply its effects, and
    /// remove it (CR 608.2n).
    pub(super) fn resolve_ability(&mut self, id: ObjId) {
        let obj = self.objects[id].clone();
        let sa = obj.ability.expect("ability object");
        let AbilityDef::Triggered(t) = self.pool.ability(sa.ability).clone() else {
            unreachable!("only triggered abilities go on the stack so far");
        };
        if let Some(cond) = t.condition
            && !self.condition_holds(cond, sa.source)
        {
            self.emit(Event::TriggerRemoved {
                source: sa.source,
                def: obj.def,
            });
            self.remove_from_game(id);
            return;
        }
        let legal: Vec<bool> = t
            .targets
            .iter()
            .zip(obj.targets.iter())
            .map(|(spec, target)| self.is_legal_target(obj.controller, *spec, *target))
            .collect();
        if !t.targets.is_empty() && legal.iter().all(|l| !l) {
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
        let ctx = EffectCtx {
            source: sa.source,
            controller: obj.controller,
            targets: obj.targets.to_vec(),
            legal,
            trigger_obj: sa.trigger_obj,
            trigger_player: sa.trigger_player,
        };
        for effect in &t.effects {
            self.apply_effect(&ctx, effect);
        }
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
        }
    }

    /// Remove an object that ceases to exist (an ability leaving the stack).
    pub(super) fn remove_from_game(&mut self, id: ObjId) {
        self.remove_from_zone_list(id);
        self.objects.remove(id);
        self.bump();
    }
}

fn condition_holds_for(cond: Condition, w: &Watcher) -> bool {
    match cond {
        Condition::SourceUntapped => !w.tapped,
    }
}
