//! Object and zone primitives: creation, zone changes (CR 400.7), drawing,
//! tapping, life, damage, destruction. Events that can be replaced (zone
//! changes, damage, life gain, draws, counters) go through `propose`, which
//! runs stage 1 of the pipeline (`replace`) and then performs what is left.

use rand::seq::SliceRandom;
use smallvec::SmallVec;

use super::replace::{Enter, Proposed};
use crate::card::CounterKind;
use crate::card::{CardRef, Keyword, RuleKind};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::state::{Affected, GameObject, GameState, LastKnown, ObjKind, Target, Zone};

impl GameState {
    // ----- the event pipeline ------------------------------------------------------

    /// Stage 2 of the pipeline: make an event happen (see `propose`). For
    /// zone changes, the new object's id.
    pub(super) fn perform(&mut self, ev: Proposed) -> Option<ObjId> {
        match ev {
            Proposed::Damage {
                source,
                target,
                amount,
                combat,
            } => {
                self.deal_damage_now(source, target, amount, combat);
                None
            }
            Proposed::GainLife { player, amount } => {
                self.change_life(player, i32::try_from(amount).unwrap_or(i32::MAX));
                None
            }
            Proposed::Draw { player } => {
                self.draw_now(player);
                None
            }
            Proposed::ZoneChange {
                obj,
                to,
                to_top,
                controller,
                enter,
            } => Some(self.move_object_now(obj, to, to_top, controller, enter)),
            Proposed::AddCounters { obj, kind, n } => {
                self.add_counters_now(obj, kind, n);
                None
            }
        }
    }

    // ----- objects and zones -------------------------------------------------

    pub(super) fn new_object(&mut self, def: CardRef, owner: PlayerId, zone: Zone) -> ObjId {
        let timestamp = self.tick();
        self.objects.insert(GameObject {
            def,
            kind: ObjKind::Card,
            owner,
            controller: owner,
            zone,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            controlled_since_turn_start: false,
            timestamp,
            targets: SmallVec::new(),
            counters: SmallVec::new(),
            attached_to: None,
            ability: None,
        })
    }

    pub(super) fn remove_from_zone_list(&mut self, id: ObjId) {
        let obj = &self.objects[id];
        let (zone, owner) = (obj.zone, obj.owner);
        let list: &mut Vec<ObjId> = match zone {
            Zone::Battlefield => &mut self.battlefield,
            Zone::Stack => &mut self.stack,
            Zone::Command => return,
            z => self.player_mut(owner).zone_mut(z),
        };
        if let Some(i) = list.iter().position(|x| *x == id) {
            list.remove(i);
        }
    }

    /// Move an object to another zone, subject to replacement effects
    /// ("exile it instead", "enters tapped"). It becomes a new object with a
    /// new id (CR 400.7); the old id is remembered in `lki`. `to_top` places
    /// it on top of a library or stack; otherwise at the bottom. A permanent
    /// entering this way is controlled by its owner.
    pub(super) fn move_object(&mut self, id: ObjId, to: Zone, to_top: bool) -> ObjId {
        let controller = self.objects[id].owner;
        self.propose(Proposed::ZoneChange {
            obj: id,
            to,
            to_top,
            controller,
            enter: Enter::default(),
        })
        .unwrap_or(id)
    }

    /// Put a permanent onto the battlefield under `controller`'s control
    /// (a resolving permanent spell, CR 608.3), subject to replacement
    /// effects.
    pub(super) fn put_onto_battlefield(&mut self, id: ObjId, controller: PlayerId) -> ObjId {
        self.propose(Proposed::ZoneChange {
            obj: id,
            to: Zone::Battlefield,
            to_top: true,
            controller,
            enter: Enter::default(),
        })
        .unwrap_or(id)
    }

    /// The zone change itself (stage 2).
    fn move_object_now(
        &mut self,
        id: ObjId,
        to: Zone,
        to_top: bool,
        controller: PlayerId,
        enter: Enter,
    ) -> ObjId {
        let characteristics = self
            .characteristics(id)
            .expect("moving an object that exists");
        self.remove_from_zone_list(id);
        let old = self.objects.remove(id).expect("object exists");
        self.lki.insert(
            id,
            LastKnown {
                object: old.clone(),
                characteristics,
                moved_to: None,
            },
        );
        // Locked-in effects stop applying to the object (CR 611.2c);
        // nothing else about them changes.
        for e in &mut self.effects {
            if let Affected::Objects(set) = &mut e.affected {
                set.retain(|x| *x != id);
            }
        }
        self.effects
            .retain(|e| !matches!(&e.affected, Affected::Objects(set) if set.is_empty()));
        let timestamp = self.tick();
        let mut counters = SmallVec::new();
        if let Some((kind, n)) = enter.counters
            && to == Zone::Battlefield
            && n > 0
        {
            counters.push((kind, n));
        }
        let new = self.objects.insert(GameObject {
            def: old.def,
            kind: old.kind,
            owner: old.owner,
            controller: if to == Zone::Battlefield {
                controller
            } else {
                old.owner
            },
            zone: to,
            tapped: enter.tapped && to == Zone::Battlefield,
            damage: 0,
            deathtouch_damage: false,
            controlled_since_turn_start: false,
            timestamp,
            targets: SmallVec::new(),
            counters,
            attached_to: None,
            ability: None,
        });
        if let Some(l) = self.lki.get_mut(&id) {
            l.moved_to = Some(new);
        }
        let def = self.pool.get(old.def);
        let (triggers, replacements) = (def.has_triggers(), def.has_replacements());
        if old.zone == Zone::Battlefield {
            self.trigger_sources -= u32::from(triggers);
            self.replacement_sources -= u32::from(replacements);
        }
        if to == Zone::Battlefield {
            self.trigger_sources += u32::from(triggers);
            self.replacement_sources += u32::from(replacements);
        }
        match to {
            Zone::Battlefield => self.battlefield.push(new),
            Zone::Stack => self.stack.push(new),
            Zone::Command => {}
            Zone::Library => {
                let lib = &mut self.player_mut(old.owner).library;
                if to_top {
                    lib.push(new);
                } else {
                    lib.insert(0, new);
                }
            }
            z => self.player_mut(old.owner).zone_mut(z).push(new),
        }
        self.bump();
        self.emit(Event::ZoneChange {
            old: id,
            new,
            def: old.def,
            from: old.zone,
            to,
        });
        new
    }

    pub(super) fn shuffle_library(&mut self, p: PlayerId) {
        let mut lib = std::mem::take(&mut self.player_mut(p).library);
        lib.shuffle(&mut self.rng);
        self.player_mut(p).library = lib;
        self.bump();
    }

    /// Draw a card (CR 121.1), subject to replacement effects (CR 614.11).
    pub(super) fn draw(&mut self, p: PlayerId) {
        self.propose(Proposed::Draw { player: p });
    }

    fn draw_now(&mut self, p: PlayerId) {
        if let Some(top) = self.player(p).library.last().copied() {
            let new = self.move_object(top, Zone::Hand, false);
            self.emit(Event::Draw {
                player: p,
                card: new,
            });
        } else {
            self.player_mut(p).drew_from_empty_library = true;
            self.emit(Event::DrawFromEmptyLibrary { player: p });
        }
    }

    pub(super) fn tap(&mut self, id: ObjId) {
        if let Some(o) = self.objects.get_mut(id)
            && !o.tapped
        {
            o.tapped = true;
            self.bump();
            self.emit(Event::Tapped { obj: id });
        }
    }

    pub(super) fn untap(&mut self, id: ObjId) {
        if let Some(o) = self.objects.get_mut(id)
            && o.tapped
        {
            o.tapped = false;
            self.bump();
            self.emit(Event::Untapped { obj: id });
        }
    }

    /// CR 119.3: a player gains life, subject to replacement effects and to
    /// "can't gain life" (CR 614.17c: such an event is not replaced).
    pub(super) fn gain_life(&mut self, p: PlayerId, amount: u32) {
        if amount == 0 || self.rule_in_force(RuleKind::NoLifeGain) {
            return;
        }
        self.propose(Proposed::GainLife { player: p, amount });
    }

    /// Change a life total directly (life loss, or a gain that has passed
    /// through the pipeline).
    pub(super) fn change_life(&mut self, p: PlayerId, delta: i32) {
        if delta == 0 {
            return;
        }
        let ps = self.player_mut(p);
        ps.life += delta;
        let life = ps.life;
        self.bump();
        self.emit(Event::LifeChange {
            player: p,
            delta,
            life,
        });
    }

    /// Deal damage from `source` to `target` (CR 120.3), subject to
    /// prevention and replacement effects (CR 614.2, 615).
    pub(super) fn deal_damage(&mut self, source: ObjId, target: Target, amount: u32, combat: bool) {
        if amount == 0 {
            return;
        }
        self.propose(Proposed::Damage {
            source,
            target,
            amount,
            combat,
        });
    }

    /// The damage itself (CR 120.3). Lifelink applies to the damage dealt
    /// (CR 702.15); deathtouch is remembered for the next SBA check.
    fn deal_damage_now(&mut self, source: ObjId, target: Target, amount: u32, combat: bool) {
        if amount == 0 {
            return;
        }
        let src = self.characteristics_or_lki(source);
        let lifelink = src.as_ref().is_some_and(|c| c.has(Keyword::Lifelink));
        let deathtouch = src.as_ref().is_some_and(|c| c.has(Keyword::Deathtouch));
        let controller = self
            .objects
            .get(source)
            .map(|o| o.controller)
            .or_else(|| self.lki.get(&source).map(|l| l.object.controller));
        match target {
            Target::Player(p) => {
                self.emit(Event::Damage {
                    source,
                    target,
                    amount,
                    combat,
                });
                let delta = -i32::try_from(amount).unwrap_or(i32::MAX);
                self.change_life(p, delta);
            }
            Target::Object(id) => {
                let Some(o) = self.objects.get_mut(id) else {
                    return;
                };
                if o.zone != Zone::Battlefield {
                    return;
                }
                o.damage += amount;
                if deathtouch {
                    o.deathtouch_damage = true;
                }
                self.bump();
                self.emit(Event::Damage {
                    source,
                    target,
                    amount,
                    combat,
                });
            }
        }
        if lifelink && let Some(c) = controller {
            self.gain_life(c, amount);
        }
    }

    /// CR 122.6: put counters on a permanent, subject to replacement
    /// effects.
    pub(super) fn add_counters(&mut self, id: ObjId, kind: CounterKind, n: u32) {
        let on_battlefield = self
            .objects
            .get(id)
            .is_some_and(|o| o.zone == Zone::Battlefield);
        if !on_battlefield || n == 0 {
            return;
        }
        self.propose(Proposed::AddCounters { obj: id, kind, n });
    }

    fn add_counters_now(&mut self, id: ObjId, kind: CounterKind, n: u32) {
        let Some(o) = self.objects.get_mut(id) else {
            return;
        };
        if o.zone != Zone::Battlefield || n == 0 {
            return;
        }
        o.add_counters(kind, n);
        self.bump();
        self.emit(Event::CountersAdded { obj: id, kind, n });
    }

    /// CR 122.5: remove counters from a permanent.
    pub(super) fn remove_counters(&mut self, id: ObjId, kind: CounterKind, n: u32) {
        let Some(o) = self.objects.get_mut(id) else {
            return;
        };
        let n = n.min(o.counters(kind));
        if o.zone != Zone::Battlefield || n == 0 {
            return;
        }
        o.remove_counters(kind, n);
        self.bump();
        self.emit(Event::CountersRemoved { obj: id, kind, n });
    }

    pub(super) fn destroy(&mut self, id: ObjId) {
        let Some(c) = self.characteristics(id) else {
            return;
        };
        if c.has(Keyword::Indestructible) {
            return;
        }
        let def = c.def;
        self.emit(Event::Destroyed { obj: id, def });
        self.move_object(id, Zone::Graveyard, true);
    }
}
