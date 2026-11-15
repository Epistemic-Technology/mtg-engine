//! Object and zone primitives: creation, zone changes (CR 400.7), drawing, tapping, life, damage, destruction.

use rand::seq::SliceRandom;
use smallvec::SmallVec;

use crate::card::{CardRef, Keyword};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::state::{GameObject, GameState, LastKnown, ObjKind, Target, Zone};

impl GameState {
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

    /// Move an object to another zone. It becomes a new object with a new id
    /// (CR 400.7); the old id is remembered in `lki`. `to_top` places it on
    /// top of a library or stack; otherwise at the bottom.
    pub(super) fn move_object(&mut self, id: ObjId, to: Zone, to_top: bool) -> ObjId {
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
            },
        );
        self.effects.retain(|e| e.affected != id);
        let timestamp = self.tick();
        let new = self.objects.insert(GameObject {
            def: old.def,
            kind: old.kind,
            owner: old.owner,
            controller: old.owner,
            zone: to,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            controlled_since_turn_start: false,
            timestamp,
            targets: SmallVec::new(),
        });
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

    pub(super) fn draw(&mut self, p: PlayerId) {
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

    /// Deal damage from `source` to `target` (CR 120.3). Lifelink applies
    /// (CR 702.15); deathtouch is remembered for the next SBA check.
    pub(super) fn deal_damage(&mut self, source: ObjId, target: Target, amount: u32, combat: bool) {
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
            self.change_life(c, i32::try_from(amount).unwrap_or(i32::MAX));
        }
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
