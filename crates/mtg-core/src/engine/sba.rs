//! State-based actions (CR 704) and the game-over check.

use std::collections::HashMap;

use crate::card::{CardRef, CounterKind, Subtype, TargetSpec};
use crate::decision::{Choice, DecisionKind};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::state::{Flow, GameResult, GameState, Lookback, LossReason, Zone};

impl GameState {
    /// CR 611.2b: end the effects whose duration is linked to an object
    /// that is no longer on the battlefield, tapped, or under the same
    /// control. Whether any ended.
    #[cold]
    #[inline(never)]
    fn expire_linked(&mut self) -> bool {
        use crate::state::Duration;
        let holds = |st: &GameState, d: Duration| {
            let on = |id| st.objects.get(id).filter(|o| o.zone == Zone::Battlefield);
            match d {
                Duration::WhileOnBattlefield(id) => on(id).is_some(),
                Duration::WhileTapped(id) => on(id).is_some_and(|o| o.tapped),
                Duration::WhileControlled(id, p) => on(id).is_some_and(|o| o.controller == p),
                _ => true,
            }
        };
        let before = self.effects.len() + self.replacements.len() + self.rules.len();
        let mut live = 0u32;
        let effects = std::mem::take(&mut self.effects);
        self.effects = effects
            .into_iter()
            .filter(|e| holds(self, e.duration))
            .inspect(|e| live += u32::from(e.duration.linked()))
            .collect();
        let replacements = std::mem::take(&mut self.replacements);
        self.replacements = replacements
            .into_iter()
            .filter(|e| holds(self, e.duration))
            .inspect(|e| live += u32::from(e.duration.linked()))
            .collect();
        let rules = std::mem::take(&mut self.rules);
        self.rules = rules
            .into_iter()
            .filter(|e| holds(self, e.duration))
            .inspect(|e| live += u32::from(e.duration.linked()))
            .collect();
        self.linked_durations = live;
        if self.effects.len() + self.replacements.len() + self.rules.len() == before {
            return false;
        }
        self.bump();
        self.cbump();
        true
    }

    /// CR 702.95e: unpair creatures no longer both creatures on the
    /// battlefield under one controller. Whether any pair broke.
    #[cold]
    #[inline(never)]
    fn unpair(&mut self) -> bool {
        let before = self.pairs.len();
        // Judged with the pairs still in place (the characteristics
        // depend on them).
        let kept: Vec<(ObjId, ObjId)> = self
            .pairs
            .iter()
            .copied()
            .filter(|&(a, b)| {
                let ok = |id: ObjId| {
                    self.objects
                        .get(id)
                        .is_some_and(|o| o.zone == Zone::Battlefield)
                        && self.characteristics(id).is_some_and(|c| c.is_creature())
                };
                ok(a) && ok(b) && self.objects[a].controller == self.objects[b].controller
            })
            .collect();
        self.pairs = kept;
        if self.pairs.len() == before {
            return false;
        }
        self.cbump();
        true
    }

    // ----- state-based actions and game end ------------------------------------------

    /// CR 704.3: perform all applicable state-based actions simultaneously,
    /// repeating until none apply. Returns whether any were performed.
    ///
    /// Skipped when nothing a state-based action looks at has changed
    /// (`sba_dirty`); in debug builds the skipped check runs anyway and
    /// must find nothing, which catches a change that forgot to mark it.
    pub(super) fn sba_loop(&mut self) -> bool {
        // CR 309.6 waits on a room ability leaving the stack, which marks
        // nothing.
        if !self.sba_dirty && self.desig.dungeon != [None; 4] && self.dungeon_at_bottom() {
            self.sba_dirty = true;
        }
        if !self.sba_dirty {
            #[cfg(debug_assertions)]
            {
                let acted = self.sba_loop_inner();
                debug_assert!(
                    !acted,
                    "a state-based action applied after a change that did not mark sba_dirty"
                );
                self.sba_dirty = false;
                return acted;
            }
            #[cfg(not(debug_assertions))]
            return false;
        }
        let acted = self.sba_loop_inner();
        // The last pass found nothing, and nothing has changed since.
        self.sba_dirty = false;
        acted
    }

    #[allow(clippy::too_many_lines)] // one block per state-based action
    fn sba_loop_inner(&mut self) -> bool {
        let mut any = false;
        loop {
            // Control-changing effects are continuous (CR 611.1); their
            // consequences are settled before anything else is checked.
            self.sync_control();
            let mut acted = false;
            // CR 611.2b: "for as long as ..." effects whose object left,
            // untapped, or changed control have ended.
            if self.linked_durations > 0 && self.expire_linked() {
                self.sync_control();
            }
            // 704.5a, 704.5b, 704.5c
            for i in 0..self.num_players() {
                let p = PlayerId(u8::try_from(i).unwrap_or(0));
                let ps = self.player(p);
                if ps.lost.is_some()
                    || (ps.life <= 0 || ps.drew_from_empty_library || ps.poison >= 10)
                        && self.cant_lose(p)
                {
                    continue;
                }
                let reason = if ps.life <= 0 {
                    Some(LossReason::LifeZeroOrLess)
                } else if ps.drew_from_empty_library {
                    Some(LossReason::DrewFromEmptyLibrary)
                } else if ps.poison >= 10 {
                    Some(LossReason::Poison)
                } else {
                    None
                };
                if let Some(reason) = reason {
                    self.player_mut(p).lost = Some(reason);
                    self.emit(Event::PlayerLost { player: p, reason });
                    acted = true;
                }
            }
            // 704.5d, 704.5f, 704.5g, 704.5h, 704.5m, 704.5q: collect first,
            // act simultaneously.
            let mut to_graveyard = Vec::new();
            let mut to_destroy = Vec::new();
            let mut annihilate = Vec::new();
            let mut unattach = Vec::new();
            let mut unbestow = Vec::new();
            let mut roles: Vec<(ObjId, PlayerId, u64, ObjId)> = Vec::new();
            // 704.5d: tokens that left the battlefield cease to exist. They
            // are listed as they leave (`move_object_now`), so only the
            // battlefield itself needs a walk.
            let objects = &self.objects;
            let tokens_gone: Vec<ObjId> = self
                .stray_tokens
                .drain(..)
                .filter(|id| {
                    objects
                        .get(*id)
                        .is_some_and(|o| o.zone != Zone::Battlefield)
                })
                .collect();
            for id in self.battlefield.iter().copied() {
                let o = &self.objects[id];
                debug_assert_eq!(o.zone, Zone::Battlefield);
                // 704.5q: +1/+1 and -1/-1 counters annihilate.
                let plus = o.counters(CounterKind::PlusOnePlusOne);
                let minus = o.counters(CounterKind::MinusOneMinusOne);
                if plus > 0 && minus > 0 {
                    annihilate.push((id, plus.min(minus)));
                }
                // 704.5m: an Aura not legally attached goes to its owner's
                // graveyard.
                if let Some(spec) = self.pool.get(o.def).enchant()
                    && !o.attached_to.is_some_and(|t| {
                        self.is_legal_attachment(spec, t)
                            && (self.pool.get(o.def).aura_stays
                                || !self.protected_attachment(id, t))
                    })
                {
                    // CR 702.103f: a bestowed Aura not attached (or
                    // attached illegally) becomes unattached and is an
                    // enchantment creature again, staying put.
                    if self.pool.get(o.card).bestow_face == Some(o.def) {
                        unbestow.push(id);
                        continue;
                    }
                    to_graveyard.push(id);
                    continue;
                }
                // 704.5z: Roles, gathered here as legally attached Auras.
                if let (Some(_), Some(host)) = (self.pool.get(o.def).enchant(), o.attached_to)
                    && self
                        .characteristics(id)
                        .is_some_and(|c| c.has_subtype(Subtype::ROLE))
                {
                    roles.push((host, o.controller, o.timestamp, id));
                }
                // 704.5n: an Equipment attached to something it cannot
                // equip becomes unattached.
                if self.pool.get(o.def).is_equipment()
                    && o.attached_to.is_some_and(|t| {
                        !self.is_legal_attachment(TargetSpec::Creature, t)
                            || self.protected_attachment(id, t)
                    })
                {
                    unattach.push(id);
                }
                let Some(c) = self.characteristics(id) else {
                    continue;
                };
                // 704.5i: a planeswalker with no loyalty goes to the graveyard.
                if c.is_planeswalker() && o.counters(CounterKind::Loyalty) == 0 {
                    to_graveyard.push(id);
                    continue;
                }
                // CR 310.7, 310.8 (704.5v): a battle with no defense goes
                // to the graveyard; a Siege waits while its defeat trigger
                // (or any other ability from it) hasn't left the stack.
                if c.is_battle()
                    && o.counters(CounterKind::Defense) == 0
                    && !(c.has_subtype(Subtype::SIEGE) && self.ability_waiting_from(id))
                {
                    to_graveyard.push(id);
                    continue;
                }
                if !c.is_creature() {
                    continue;
                }
                if c.toughness <= 0 {
                    to_graveyard.push(id);
                } else if o.damage >= u32::try_from(c.toughness).unwrap_or(0) || o.deathtouch_damage
                {
                    to_destroy.push(id);
                }
            }
            // 704.5z (CR 303.7a): of the Roles one player controls on one
            // permanent, all but the newest go to the graveyard.
            if roles.len() >= 2 {
                roles.sort_unstable();
                for w in roles.windows(2) {
                    if w[0].0 == w[1].0 && w[0].1 == w[1].1 {
                        to_graveyard.push(w[0].3);
                    }
                }
            }
            // 704.5j: the legend rule.
            let legends_gone = self.legend_rule_victims();
            for id in tokens_gone {
                self.remove_from_zone_list(id);
                self.objects.remove(id);
                // A graveyard it sat in feeds characteristics (threshold).
                self.cbump();
                acted = true;
            }
            for id in unattach {
                self.objects[id].attached_to = None;
                self.cbump();
                self.emit(Event::Unattached { obj: id });
                acted = true;
            }
            for id in unbestow {
                // The board counts follow the definition it has now.
                let (old_flags, old_legend) = {
                    let o = &self.objects[id];
                    (self.flags_of(o), self.pool.get(o.def).is_legendary())
                };
                self.count_sources(old_flags, old_legend, false);
                let o = &mut self.objects[id];
                o.def = o.card;
                let (flags, legend) = {
                    let o = &self.objects[id];
                    (self.flags_of(o), self.pool.get(o.def).is_legendary())
                };
                self.count_sources(flags, legend, true);
                let o = &mut self.objects[id];
                let was_attached = o.attached_to.take().is_some();
                self.cbump();
                if was_attached {
                    self.emit(Event::Unattached { obj: id });
                }
                acted = true;
            }
            // The zone changes below are simultaneous: leaves-the-battlefield
            // triggers see the battlefield as it is now (CR 603.10a).
            if !to_graveyard.is_empty() || !to_destroy.is_empty() || !legends_gone.is_empty() {
                self.lookback = Some(self.snapshot_battlefield());
            }
            for (id, def) in legends_gone {
                self.emit(Event::LegendRule { obj: id, def });
                self.move_object(id, Zone::Graveyard, true);
                acted = true;
            }
            for (id, n) in annihilate {
                let o = &mut self.objects[id];
                o.remove_counters(CounterKind::PlusOnePlusOne, n);
                o.remove_counters(CounterKind::MinusOneMinusOne, n);
                self.cbump();
                acted = true;
            }
            for id in to_graveyard {
                self.move_object(id, Zone::Graveyard, true);
                acted = true;
            }
            for id in to_destroy {
                self.destroy(id);
                if self.objects.contains_key(id) {
                    // Indestructible: the creature stays, with its damage
                    // (CR 702.12b). Nothing changed, so this is not an
                    // action, or the loop would never end; the deathtouch
                    // flag is cleared so it is not re-examined either.
                    self.objects[id].deathtouch_damage = false;
                } else {
                    acted = true;
                }
            }
            self.lookback = None;
            // 704.5s (CR 714.4): a Saga past its final chapter.
            if self.saga_count > 0 && self.saga_sba() {
                acted = true;
            }
            // CR 702.95e: a pair breaks when either leaves the battlefield,
            // stops being a creature, or changes controller.
            if !self.pairs.is_empty() && self.unpair() {
                acted = true;
            }
            // 702.131b, 702.179a: ascend and start your engines!.
            if self.mark_sources > 0 && self.designation_sba() {
                acted = true;
            }
            // 704.5t (CR 309.6): a dungeon whose marker is on its last room.
            if self.desig.dungeon != [None; 4] && self.dungeon_sba() {
                acted = true;
            }
            if !acted {
                break;
            }
            any = true;
        }
        any
    }

    /// CR 714.4 (704.5s): a Saga with lore counters at least its final
    /// chapter number, and no chapter ability of its own waiting to leave
    /// the stack, is sacrificed by its controller.
    #[cold]
    #[inline(never)]
    fn saga_sba(&mut self) -> bool {
        let done: Vec<ObjId> = self
            .battlefield
            .iter()
            .copied()
            .filter(|id| {
                let o = &self.objects[*id];
                let last = self.pool.get(o.def).final_chapter;
                last > 0
                    && o.counters(CounterKind::Lore) >= u32::from(last)
                    && !self.chapter_waiting_from(*id)
            })
            .collect();
        if done.is_empty() {
            return false;
        }
        self.lookback = Some(self.snapshot_battlefield());
        for id in done {
            self.sacrifice(id);
        }
        self.lookback = None;
        true
    }

    /// Whether a chapter ability of `source` has triggered but not left
    /// the stack (CR 714.4).
    fn chapter_waiting_from(&self, source: ObjId) -> bool {
        let chapter = |a: crate::card::AbilityId| {
            matches!(
                self.pool.ability(a),
                crate::card::AbilityDef::Triggered(t)
                    if matches!(t.trigger, crate::card::TriggerSpec::Chapter(_))
            )
        };
        self.pending_triggers
            .iter()
            .any(|t| t.ability.source == source && chapter(t.ability.ability))
            || self.stack.iter().any(|s| {
                self.objects[*s]
                    .ability
                    .is_some_and(|a| a.source == source && chapter(a.ability))
            })
    }

    /// Whether a triggered ability from `source` has triggered but not
    /// left the stack (CR 310.7, 309.6).
    pub(super) fn ability_waiting_from(&self, source: ObjId) -> bool {
        self.pending_triggers
            .iter()
            .any(|t| t.ability.source == source)
            || self
                .stack
                .iter()
                .any(|s| self.objects[*s].ability.is_some_and(|a| a.source == source))
    }

    /// CR 704.5j: for each player controlling two or more legendary
    /// permanents with the same name, that player chooses one to keep; the
    /// rest are returned, to go to their owners' graveyards.
    fn legend_rule_victims(&mut self) -> Vec<(ObjId, CardRef)> {
        if self.legendary_count < 2 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for i in 0..self.num_players() {
            let p = PlayerId(u8::try_from(i).unwrap_or(0));
            let mut by_name: HashMap<&str, Vec<ObjId>> = HashMap::new();
            for id in self.permanents_of(p) {
                let def = self.pool.get(self.objects[id].def);
                // "Except it isn't legendary" (CR 707.9b): a copy
                // exception can take the supertype away.
                if def.is_legendary()
                    && (self.objects[id].except.is_none()
                        || self.characteristics(id).is_some_and(|c| {
                            c.supertypes.contains(crate::card::Supertype::Legendary)
                        }))
                {
                    by_name.entry(def.name.as_str()).or_default().push(id);
                }
            }
            let mut groups: Vec<Vec<ObjId>> =
                by_name.into_values().filter(|g| g.len() >= 2).collect();
            // Deterministic order for the questions.
            groups.sort_by_key(|g| self.objects[g[0]].timestamp);
            for group in groups {
                let options = group
                    .iter()
                    .enumerate()
                    .map(|(age, id)| Choice::KeepLegend {
                        obj: *id,
                        def: self.objects[*id].def,
                        age,
                    })
                    .collect();
                let keep = self.choose_mid(p, DecisionKind::LegendRule, options);
                for (k, id) in group.into_iter().enumerate() {
                    if k != keep {
                        out.push((id, self.objects[id].def));
                    }
                }
            }
        }
        out
    }

    /// The battlefield as it is now, for look-back trigger detection.
    fn snapshot_battlefield(&self) -> Vec<Lookback> {
        self.battlefield
            .iter()
            .filter_map(|id| {
                let o = &self.objects[*id];
                Some(Lookback {
                    id: *id,
                    controller: o.controller,
                    tapped: o.tapped,
                    characteristics: self.characteristics(*id)?,
                    except: o.except,
                })
            })
            .collect()
    }

    pub(super) fn check_game_over(&mut self) -> Option<GameResult> {
        if let Some(r) = self.result {
            return Some(r);
        }
        let alive: Vec<PlayerId> = self.players_in_game().collect();
        let result = match alive.len() {
            0 => GameResult::Draw,
            1 => GameResult::Win { winner: alive[0] },
            _ => return None,
        };
        self.result = Some(result);
        self.flow = Flow::GameOver;
        self.emit(Event::GameOver { result });
        Some(result)
    }
}
