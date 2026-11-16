//! Control-changing effects (CR 613.1b, layer 2). Control is not a
//! characteristic: it lives in `GameObject::controller`, which the rest of
//! the engine reads directly. `sync_control` derives it from the owner plus
//! every control-changing effect in timestamp order, and is run whenever
//! such an effect can have started or stopped applying: when one is
//! created, when "until end of turn" effects end, and before state-based
//! actions are checked (so an Aura entering or leaving, or Humility
//! removing Control Magic's ability, takes effect before anyone gets
//! priority).

use smallvec::SmallVec;

use crate::card::{AbilityDef, Modification};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId, Timestamp};
use crate::state::{Affected, GameState, Zone};

impl GameState {
    /// Bring every permanent's controller in line with the control-changing
    /// effects in force. Does nothing on boards without any.
    pub(super) fn sync_control(&mut self) {
        if self.control_sources == 0 && self.control_effects == 0 && !self.control_dirty {
            return;
        }
        self.sync_control_slow();
    }

    #[inline(never)]
    fn sync_control_slow(&mut self) {
        self.control_dirty = false;
        debug_assert_eq!(
            self.control_sources as usize,
            self.battlefield
                .iter()
                .filter(|id| self.pool.get(self.objects[**id].def).has_control_static())
                .count(),
            "control source count out of sync with the battlefield"
        );
        // Every control effect: when it started, what it applies to, who
        // gets control.
        let mut effects: Vec<(Timestamp, SmallVec<[ObjId; 2]>, PlayerId)> = Vec::new();
        for e in &self.effects {
            if !e.parts.contains(&Modification::Control) {
                continue;
            }
            // Spell effects lock their affected set when created (CR
            // 611.2c); a filter here would be a static ability's job.
            if let Affected::Objects(set) = &e.affected {
                effects.push((e.timestamp, set.clone(), e.controller));
            }
        }
        for id in &self.battlefield {
            let o = &self.objects[*id];
            if !self.pool.get(o.def).has_control_static() {
                continue;
            }
            // Read through current characteristics: an ability Humility
            // removed does not apply.
            let Some(c) = self.characteristics(*id) else {
                continue;
            };
            for a in &c.abilities {
                let AbilityDef::Static(sa) = self.pool.ability(*a) else {
                    continue;
                };
                if !sa.parts.contains(&Modification::Control) {
                    continue;
                }
                let targets: SmallVec<[ObjId; 2]> = if sa.affected.enchanted {
                    o.attached_to
                        .filter(|t| {
                            self.objects
                                .get(*t)
                                .is_some_and(|x| x.zone == Zone::Battlefield)
                        })
                        .into_iter()
                        .collect()
                } else {
                    self.battlefield
                        .iter()
                        .copied()
                        .filter(|t| {
                            let x = &self.objects[*t];
                            self.characteristics(*t).is_some_and(|tc| {
                                sa.affected.matches(
                                    *t,
                                    x.controller,
                                    &tc,
                                    Some(*id),
                                    Some(o.controller),
                                )
                            })
                        })
                        .collect()
                };
                effects.push((o.timestamp, targets, o.controller));
            }
        }
        effects.sort_by_key(|(t, _, _)| *t);
        let mut desired: Vec<(ObjId, PlayerId)> = self
            .battlefield
            .iter()
            .map(|id| (*id, self.objects[*id].owner))
            .collect();
        for (_, ids, controller) in &effects {
            for id in ids {
                if let Some(slot) = desired.iter_mut().find(|(d, _)| d == id) {
                    slot.1 = *controller;
                }
            }
        }
        for (id, to) in desired {
            let o = &mut self.objects[id];
            if o.controller == to {
                continue;
            }
            let from = o.controller;
            o.controller = to;
            // CR 302.6: not controlled continuously since the new
            // controller's turn began.
            o.controlled_since_turn_start = false;
            // CR 506.4: a permanent that changes control leaves combat.
            self.remove_from_combat(id);
            self.bump();
            self.emit(Event::ControlChanged { obj: id, from, to });
        }
    }

    /// CR 506.4: remove a creature from combat. An attacker it was blocking
    /// stays blocked (CR 509.1h).
    pub(super) fn remove_from_combat(&mut self, id: ObjId) {
        let Some(c) = self.combat.as_mut() else {
            return;
        };
        c.attackers.retain(|a| a.id != id);
        for a in &mut c.attackers {
            a.blockers.retain(|b| *b != id);
        }
    }
}
