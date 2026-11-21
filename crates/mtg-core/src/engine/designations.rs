//! Player designations and progress tracks: the monarch (CR 725), the
//! initiative (CR 726), day and night (CR 731), the city's blessing (CR
//! 702.131), speed (CR 702.179), the Ring tempting you (CR 701.54), and
//! venturing into dungeons (CR 309, 701.49).
//!
//! The state lives in `GameState::desig`, inline and small. The inherent
//! triggered abilities of the monarch, the initiative, speed, and The Ring
//! are the pool's built-ins (`CardPool::desig`), queued here as events are
//! emitted, only once `Designations::live` is set; their sources are
//! command-zone objects made the first time one is needed.

use smallvec::SmallVec;

use crate::card::{AbilityId, CardRef, Marks};
use crate::decision::{Choice, DecisionKind};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::state::{
    DayNight, DungeonMarker, GameState, ObjKind, PendingTrigger, StackAbility, Target, Zone,
};

/// Which sourceless designation an inherent trigger belongs to.
#[derive(Copy, Clone)]
enum Inherent {
    Monarch,
    Initiative,
    Speed,
}

impl GameState {
    fn bit(p: PlayerId) -> u8 {
        1 << p.index().min(7)
    }

    /// `p`'s Ring-bearer (CR 701.54a): the creature chosen, while it stays
    /// on the battlefield under `p`'s control.
    pub fn ring_bearer(&self, p: PlayerId) -> Option<ObjId> {
        let id = self.desig.ring_bearer[p.index().min(3)]?;
        self.objects
            .get(id)
            .is_some_and(|o| o.zone == Zone::Battlefield && o.controller == p)
            .then_some(id)
    }

    /// Whether `id` is its controller's Ring-bearer.
    pub fn is_ring_bearer(&self, id: ObjId) -> bool {
        self.desig.tempted != [0; 4]
            && self
                .objects
                .get(id)
                .is_some_and(|o| self.ring_bearer(o.controller) == Some(id))
    }

    /// The command-zone object standing for a sourceless designation's
    /// triggered abilities, made the first time one triggers.
    fn inherent_source(&mut self, which: Inherent, controller: PlayerId) -> (ObjId, CardRef) {
        let d = *self.pool.desig();
        let (slot, def) = match which {
            Inherent::Monarch => (0, d.monarch),
            Inherent::Initiative => (1, d.initiative),
            Inherent::Speed => (2, d.speed),
        };
        if let Some(id) = self.desig.inherent[slot]
            && self.objects.contains_key(id)
        {
            return (id, def);
        }
        let id = self.new_object(def, controller, Zone::Command);
        self.objects[id].kind = ObjKind::Emblem;
        self.desig.inherent[slot] = Some(id);
        (id, def)
    }

    fn queue(
        &mut self,
        controller: PlayerId,
        ability: AbilityId,
        (source, def): (ObjId, CardRef),
        trigger_obj: Option<ObjId>,
        trigger_player: Option<PlayerId>,
    ) {
        let t = PendingTrigger {
            controller,
            ability: StackAbility {
                ability,
                source,
                def,
                trigger_obj,
                trigger_player,
                trigger_amount: 0,
            },
            timestamp: self.tick(),
        };
        if self.logging {
            self.log.push(Event::Triggered {
                source,
                ability,
                controller,
            });
        }
        self.pending_triggers.push(t);
    }

    /// The Ring emblem's triggered abilities that trigger on `bearer`, as
    /// many as the Ring has tempted its controller (CR 701.54c).
    fn ring_trigger(
        &mut self,
        bearer: ObjId,
        level: u8,
        ability: AbilityId,
        trigger_obj: Option<ObjId>,
    ) {
        let Some(p) = self.objects.get(bearer).map(|o| o.controller) else {
            return;
        };
        if self.desig.tempted(p) < level || self.ring_bearer(p) != Some(bearer) {
            return;
        }
        let Some(ring) = self.desig.ring[p.index().min(3)] else {
            return;
        };
        let def = self.pool.desig().ring;
        self.queue(p, ability, (ring, def), trigger_obj, None);
    }

    /// The designations' inherent triggered abilities that `event`
    /// triggers (CR 725.2, 726.2, 702.179d, 701.54c).
    pub(crate) fn designation_triggers(&mut self, event: &Event) {
        let d = *self.pool.desig();
        match *event {
            Event::StepBegin {
                step: crate::turn::Step::Upkeep,
            } if self.desig.initiative == Some(self.active_player) => {
                let p = self.active_player;
                let src = self.inherent_source(Inherent::Initiative, p);
                self.queue(p, d.initiative_upkeep, src, None, Some(p));
            }
            Event::StepBegin {
                step: crate::turn::Step::End,
            } if self.desig.monarch == Some(self.active_player) => {
                let p = self.active_player;
                let src = self.inherent_source(Inherent::Monarch, p);
                self.queue(p, d.monarch_draw, src, None, Some(p));
            }
            Event::Damage {
                source,
                target: Target::Player(hit),
                amount,
                combat: true,
            } if amount > 0 => {
                let attacker = self
                    .objects
                    .get(source)
                    .or_else(|| self.lki.get(&source).map(|l| &l.object))
                    .map(|o| o.controller);
                if self.desig.monarch == Some(hit) {
                    let src = self.inherent_source(Inherent::Monarch, hit);
                    self.queue(hit, d.monarch_steal, src, Some(source), attacker);
                }
                // "One or more creatures a player controls": once per
                // player for damage dealt at the same time.
                if self.desig.initiative == Some(hit)
                    && let Some(c) = attacker
                    && !self.pending_triggers.iter().any(|t| {
                        t.ability.ability == d.initiative_steal
                            && t.ability.trigger_player == Some(c)
                    })
                {
                    let src = self.inherent_source(Inherent::Initiative, hit);
                    self.queue(hit, d.initiative_steal, src, Some(source), Some(c));
                }
                self.ring_trigger(source, 4, d.ring_damage, None);
            }
            Event::Attack { attacker, .. } => self.ring_trigger(attacker, 2, d.ring_loot, None),
            Event::Block { blocker, attacker } => {
                self.ring_trigger(attacker, 3, d.ring_block, Some(blocker));
            }
            Event::LifeChange { player, delta, .. } if delta < 0 => {
                let a = self.active_player;
                let speed = self.desig.speed(a);
                if player != a && speed > 0 && speed < 4 && self.desig.sped & Self::bit(a) == 0 {
                    self.desig.sped |= Self::bit(a);
                    let src = self.inherent_source(Inherent::Speed, a);
                    self.queue(a, d.speed_up, src, None, Some(a));
                }
            }
            _ => {}
        }
    }

    // ----- the monarch and the initiative --------------------------------------

    /// CR 725.3: `p` becomes the monarch; the old monarch stops being it.
    pub(super) fn become_monarch(&mut self, p: PlayerId) {
        self.desig.live = true;
        if self.desig.monarch != Some(p) {
            self.desig.monarch = Some(p);
            self.cbump();
        }
        self.emit(Event::BecameMonarch { player: p });
    }

    /// CR 726.3: `p` takes the initiative, which triggers its venture into
    /// Undercity even if `p` already had it (CR 726.5).
    pub(super) fn take_initiative(&mut self, p: PlayerId) {
        self.desig.live = true;
        if self.desig.initiative != Some(p) {
            self.desig.initiative = Some(p);
            self.cbump();
        }
        self.emit(Event::TookInitiative { player: p });
        let d = *self.pool.desig();
        let src = self.inherent_source(Inherent::Initiative, p);
        self.queue(p, d.initiative_taken, src, None, Some(p));
    }

    // ----- speed and the city's blessing ----------------------------------------

    /// Set `p`'s speed (CR 702.179), capped at 4.
    pub(super) fn set_speed(&mut self, p: PlayerId, speed: u8) {
        let speed = speed.min(4);
        if self.desig.speed(p) == speed {
            return;
        }
        self.desig.live = true;
        self.desig.speed[p.index().min(3)] = speed;
        self.cbump();
        self.emit(Event::Speed { player: p, speed });
    }

    /// CR 702.179d: the speed trigger's effect, if speed is still below 4.
    pub(super) fn increase_speed(&mut self, p: PlayerId) {
        let s = self.desig.speed(p);
        if s < 4 {
            self.set_speed(p, s + 1);
        }
    }

    /// CR 702.131a-b: the city's blessing, for the rest of the game, if
    /// `p` controls ten or more permanents.
    pub(super) fn ascend(&mut self, p: PlayerId) -> bool {
        if self.desig.blessed(p) || self.permanents_of(p).count() < 10 {
            return false;
        }
        self.desig.blessed |= Self::bit(p);
        self.cbump();
        self.emit(Event::CityBlessing { player: p });
        true
    }

    /// CR 702.195a: an enduring story, for the rest of the game, if `p`
    /// controls three or more permanents that are artifacts, Sagas, and/or
    /// legendary.
    pub(super) fn storied(&mut self, p: PlayerId) -> bool {
        if self.desig.storied(p) {
            return false;
        }
        let n = self
            .permanents_of(p)
            .filter(|id| {
                self.characteristics(*id).is_some_and(|c| {
                    c.types.contains(crate::card::CardType::Artifact)
                        || c.supertypes.contains(crate::card::Supertype::Legendary)
                        || c.subtypes.contains(&crate::card::Subtype::SAGA)
                })
            })
            .count();
        if n < 3 {
            return false;
        }
        self.desig.storied |= Self::bit(p);
        self.cbump();
        self.emit(Event::EnduringStory { player: p });
        true
    }

    /// The state-based parts of ascend (CR 702.131b) and start your
    /// engines! (CR 702.179a): permanents with them give their controllers
    /// the city's blessing and a speed of 1.
    pub(super) fn designation_sba(&mut self) -> bool {
        let mut acted = false;
        let marked: SmallVec<[(ObjId, PlayerId, Marks); 4]> = self
            .battlefield
            .iter()
            .filter_map(|id| {
                let o = &self.objects[*id];
                let m = self.pool.get(o.def).marks;
                (!m.is_empty()).then_some((*id, o.controller, m))
            })
            .collect();
        for (id, p, m) in marked {
            // CR 702.145c-d, f-g: with a daybound permanent, it becomes
            // day if it's neither; at night it transforms. With a
            // nightbound one, it becomes night if it's neither (there is
            // no daybound permanent, or it would be day); by day it
            // transforms.
            if m.has(Marks::DAYBOUND) || m.has(Marks::NIGHTBOUND) {
                let day = m.has(Marks::DAYBOUND);
                if self.desig.day_night == DayNight::Neither {
                    self.set_day_night(day);
                    acted = true;
                }
                let wrong = if day { DayNight::Night } else { DayNight::Day };
                if self.desig.day_night == wrong && self.transform_now(id) {
                    acted = true;
                }
            }
            if m.has(Marks::ENGINES) && self.desig.speed(p) == 0 {
                self.set_speed(p, 1);
                acted = true;
            }
            if m.has(Marks::ASCEND) && self.ascend(p) {
                acted = true;
            }
            if m.has(Marks::STORIED) && self.storied(p) {
                acted = true;
            }
        }
        acted
    }

    // ----- day and night ----------------------------------------------------

    /// It becomes day (`day`) or night (CR 731.1).
    pub(super) fn set_day_night(&mut self, day: bool) {
        let now = if day { DayNight::Day } else { DayNight::Night };
        let was = self.desig.day_night;
        if was == now {
            return;
        }
        self.desig.day_night = now;
        self.cbump();
        self.emit(Event::DayNight {
            day,
            changed: was != DayNight::Neither,
        });
    }

    /// CR 502.2, 731.2: the untap step's check of the previous turn.
    pub(super) fn day_night_check(&mut self) {
        let Some(prev) = self.desig.last_active else {
            return;
        };
        let cast = self.spells_last_turn[prev.index().min(3)];
        match self.desig.day_night {
            DayNight::Day if cast == 0 => self.set_day_night(false),
            DayNight::Night if cast >= 2 => self.set_day_night(true),
            _ => {}
        }
    }

    // ----- the Ring ---------------------------------------------------------

    /// CR 701.54: the Ring tempts `p`. They get The Ring emblem if they
    /// have none, then choose a creature they control as their
    /// Ring-bearer (the same one again, if they like).
    pub(super) fn ring_tempts(&mut self, p: PlayerId) {
        self.desig.live = true;
        let i = p.index().min(3);
        if self.desig.ring[i].is_none_or(|r| !self.objects.contains_key(r)) {
            let def = self.pool.desig().ring;
            let id = self.new_object(def, p, Zone::Command);
            self.objects[id].kind = ObjKind::Emblem;
            self.desig.ring[i] = Some(id);
            self.emit(Event::EmblemCreated {
                obj: id,
                def,
                controller: p,
            });
        }
        self.desig.tempted[i] = self.desig.tempted[i].saturating_add(1);
        let creatures: Vec<ObjId> = self.creatures_of(p).collect();
        let bearer = match creatures.len() {
            0 => None,
            1 => Some(creatures[0]),
            _ => {
                let options: Vec<Choice> =
                    creatures.iter().map(|id| Choice::Sacrifice(*id)).collect();
                let k = self.choose_mid(p, DecisionKind::ChooseCard, options);
                Some(creatures[k])
            }
        };
        if bearer.is_some() {
            self.desig.ring_bearer[i] = bearer;
        }
        self.cbump();
        self.emit(Event::RingTempted { player: p, bearer });
    }

    // ----- dungeons ---------------------------------------------------------

    /// CR 701.49: `p` ventures into the dungeon (`undercity`: into
    /// Undercity, CR 701.49d).
    pub(super) fn venture(&mut self, p: PlayerId, undercity: bool) {
        let Some(m) = self.desig.dungeon(p) else {
            self.enter_dungeon(p, undercity);
            return;
        };
        let def = self.objects[m.obj].def;
        let next = self.pool.get(def).rooms[usize::from(m.room)].next.clone();
        match next.len() {
            // CR 701.49c: from the bottommost room, the dungeon is
            // completed and a new one begins.
            0 => {
                self.complete_dungeon(p);
                self.enter_dungeon(p, undercity);
            }
            1 => self.enter_room(p, m.obj, next[0]),
            // CR 701.49b: the player chooses which arrow to follow.
            _ => {
                let options = (0..next.len()).map(Choice::Mode).collect();
                let k = self.choose_mid(p, DecisionKind::ChooseOption { def }, options);
                self.enter_room(p, m.obj, next[k]);
            }
        }
    }

    /// CR 309.2a, 309.4a: `p` chooses a dungeon card and puts it into the
    /// command zone, the venture marker on its topmost room. Undercity
    /// only by venturing into Undercity, and then only it (CR 701.49d).
    fn enter_dungeon(&mut self, p: PlayerId, undercity: bool) {
        let choices: Vec<CardRef> = self
            .pool
            .dungeons()
            .iter()
            .copied()
            .filter(|d| (self.pool.get(*d).name == "Undercity") == undercity)
            .collect();
        let def = match choices.len() {
            0 => return,
            1 => choices[0],
            _ => {
                let options = (0..choices.len()).map(Choice::Mode).collect();
                let k = self.choose_mid(p, DecisionKind::ChooseOption { def: choices[0] }, options);
                choices[k]
            }
        };
        let obj = self.new_object(def, p, Zone::Command);
        self.enter_room(p, obj, 0);
    }

    /// The venture marker moves into room `room`: its room ability
    /// triggers (CR 309.4c).
    fn enter_room(&mut self, p: PlayerId, obj: ObjId, room: u8) {
        self.desig.dungeon[p.index().min(3)] = Some(DungeonMarker { obj, room });
        let def = self.objects[obj].def;
        let ability = self.pool.get(def).rooms[usize::from(room)].id;
        self.bump();
        self.sba_touch();
        self.emit(Event::Ventured {
            player: p,
            dungeon: def,
            room,
        });
        self.queue(p, ability, (obj, def), None, Some(p));
    }

    /// CR 309.7: `p`'s dungeon is removed from the game, completed.
    fn complete_dungeon(&mut self, p: PlayerId) {
        let i = p.index().min(3);
        let Some(m) = self.desig.dungeon[i].take() else {
            return;
        };
        // The card leaves the game (CR 309.2c); its object stays, inert,
        // for room abilities still on the stack to name their source.
        let def = self.objects[m.obj].def;
        self.desig.completed[i] = self.desig.completed[i].saturating_add(1);
        self.cbump();
        self.emit(Event::CompletedDungeon {
            player: p,
            dungeon: def,
        });
    }

    /// Whether a venture marker is on a bottommost room (CR 309.6 may
    /// apply).
    pub(super) fn dungeon_at_bottom(&self) -> bool {
        self.desig.dungeon.iter().flatten().any(|m| {
            self.objects.get(m.obj).is_some_and(|o| {
                self.pool.get(o.def).rooms[usize::from(m.room)]
                    .next
                    .is_empty()
            })
        })
    }

    /// CR 309.6 (704.5t): a dungeon whose venture marker is on its
    /// bottommost room, and that isn't the source of a room ability that
    /// has triggered but not left the stack, is removed: completed.
    pub(super) fn dungeon_sba(&mut self) -> bool {
        let mut acted = false;
        for i in 0..self.num_players() {
            let p = PlayerId(u8::try_from(i).unwrap_or(0));
            let Some(m) = self.desig.dungeon(p) else {
                continue;
            };
            let bottom = self.objects.get(m.obj).is_some_and(|o| {
                self.pool.get(o.def).rooms[usize::from(m.room)]
                    .next
                    .is_empty()
            });
            let waiting = self
                .pending_triggers
                .iter()
                .any(|t| t.ability.source == m.obj)
                || self
                    .stack
                    .iter()
                    .any(|s| self.objects[*s].ability.is_some_and(|a| a.source == m.obj));
            if bottom && !waiting {
                self.complete_dungeon(p);
                acted = true;
            }
        }
        acted
    }
}
