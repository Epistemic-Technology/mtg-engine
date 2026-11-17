//! Object and zone primitives: creation, zone changes (CR 400.7), drawing,
//! tapping, life, damage, destruction. Events that can be replaced (zone
//! changes, damage, life gain, draws, counters) go through `propose`, which
//! runs stage 1 of the pipeline (`replace`) and then performs what is left.

use rand::seq::SliceRandom;
use smallvec::SmallVec;

use super::replace::{Enter, Proposed};
use crate::card::CounterKind;
use crate::card::{CardRef, DefFlags, Keyword, RuleKind};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::state::{Affected, CastInfo, GameObject, GameState, LastKnown, ObjKind, Target, Zone};

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
            Proposed::Destroy { obj, .. } => {
                self.destroy_now(obj);
                None
            }
        }
    }

    // ----- objects and zones -------------------------------------------------

    pub(super) fn new_object(&mut self, def: CardRef, owner: PlayerId, zone: Zone) -> ObjId {
        let timestamp = self.tick();
        if zone == Zone::Graveyard && self.pool.get(def).flashback.is_some() {
            self.player_mut(owner).flashback_in_graveyard += 1;
        }
        self.objects.insert(GameObject {
            def,
            card: def,
            except: None,
            cast: CastInfo::default(),
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
        self.put_onto_battlefield_with(id, controller, Enter::default())
    }

    /// `put_onto_battlefield` with how the effect says it enters ("with a
    /// +1/+1 counter on it"), before replacement effects.
    pub(super) fn put_onto_battlefield_with(
        &mut self,
        id: ObjId,
        controller: PlayerId,
        enter: Enter,
    ) -> ObjId {
        self.propose(Proposed::ZoneChange {
            obj: id,
            to: Zone::Battlefield,
            to_top: true,
            controller,
            enter,
        })
        .unwrap_or(id)
    }

    /// Counters a permanent enters the battlefield with: those the event
    /// says ("with a +1/+1 counter"), and a planeswalker's printed loyalty
    /// (CR 306.5b). Nothing off the battlefield.
    fn entering_counters(
        &self,
        def: CardRef,
        to: Zone,
        enter: Enter,
    ) -> SmallVec<[(CounterKind, u32); 2]> {
        let mut counters = SmallVec::new();
        if to != Zone::Battlefield {
            return counters;
        }
        if let Some((kind, n)) = enter.counters
            && n > 0
        {
            counters.push((kind, n));
        }
        let d = self.pool.get(def);
        if d.types.contains(crate::card::CardType::Planeswalker) && d.loyalty > 0 {
            counters.push((CounterKind::Loyalty, d.loyalty));
        }
        counters
    }

    /// The zone change itself (stage 2). A copy effect set on the object
    /// before it enters the battlefield (Clone, a token copy) carries over;
    /// leaving the battlefield, the object is its printed card again (CR
    /// 400.7, 707.?).
    #[allow(clippy::too_many_lines)]
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
        let old_flags = self.flags_of(&old);
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
        let (def, except) = if to == Zone::Battlefield {
            (old.def, old.except)
        } else {
            (old.card, None)
        };
        let counters = self.entering_counters(def, to, enter);
        let new = self.objects.insert(GameObject {
            def,
            card: old.card,
            except,
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
            // CR 702.33c: a permanent spell's kicker was paid, so the
            // permanent was kicked.
            cast: if old.zone == Zone::Stack && to == Zone::Battlefield {
                old.cast.kept_by_permanent()
            } else {
                CastInfo::default()
            },
            timestamp,
            targets: SmallVec::new(),
            counters,
            attached_to: None,
            ability: None,
        });
        if let Some(l) = self.lki.get_mut(&id) {
            l.moved_to = Some(new);
        }
        if old.zone == Zone::Battlefield {
            // CR 506.4: a permanent that leaves the battlefield leaves combat.
            self.remove_from_combat(id);
            self.count_sources(old_flags, self.pool.get(old.def).is_legendary(), false);
        }
        if to == Zone::Battlefield {
            let flags = self.flags_of(&self.objects[new]);
            self.count_sources(flags, self.pool.get(def).is_legendary(), true);
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
        if old.kind == ObjKind::Token && to != Zone::Battlefield {
            self.stray_tokens.push(new);
        }
        if old.kind == ObjKind::Card && self.pool.get(old.card).flashback.is_some() {
            if old.zone == Zone::Graveyard {
                let n = &mut self.player_mut(old.owner).flashback_in_graveyard;
                *n = n.saturating_sub(1);
            }
            if to == Zone::Graveyard {
                self.player_mut(old.owner).flashback_in_graveyard += 1;
            }
        }
        if old.zone == Zone::Stack && !self.x_values.is_empty() {
            self.x_values.retain(|(s, _)| *s != id);
        }
        // Graveyards feed characteristic-defining abilities (Tarmogoyf).
        let affects_chars = |z: Zone| matches!(z, Zone::Battlefield | Zone::Graveyard);
        if affects_chars(old.zone) || affects_chars(to) {
            self.cbump();
        } else {
            self.bump();
        }
        self.emit(Event::ZoneChange {
            old: id,
            new,
            def: old.def,
            from: old.zone,
            to,
        });
        new
    }

    /// Maintain the per-board counts that gate the engine's slow paths as
    /// a permanent with these flags arrives on (`add`) or leaves the
    /// battlefield.
    pub(super) fn count_sources(&mut self, flags: DefFlags, legendary: bool, add: bool) {
        let d = |on: bool| u32::from(on);
        if add {
            self.trigger_sources += d(flags.has_triggers());
            self.replacement_sources += d(flags.has_replacements());
            self.control_sources += d(flags.has_control_static());
            self.legendary_count += d(legendary);
            self.activated_sources += d(flags.has_activated());
            self.life_cda_sources += d(flags.has_life_cda());
        } else {
            self.trigger_sources -= d(flags.has_triggers());
            self.replacement_sources -= d(flags.has_replacements());
            self.control_sources -= d(flags.has_control_static());
            self.legendary_count -= d(legendary);
            self.activated_sources -= d(flags.has_activated());
            self.life_cda_sources -= d(flags.has_life_cda());
            // Whatever it controlled goes back (CR 611.2c does not apply to
            // static abilities).
            self.control_dirty |= flags.has_control_static();
        }
    }

    /// CR 111.2: create a token under `controller`'s control. The token is
    /// made in the command zone and put onto the battlefield through the
    /// pipeline, so "enters" effects and triggers see it.
    pub(super) fn create_token(&mut self, def: CardRef, controller: PlayerId) -> ObjId {
        let id = self.new_object(def, controller, Zone::Command);
        self.objects[id].kind = ObjKind::Token;
        let new = self.put_onto_battlefield(id, controller);
        self.emit(Event::TokenCreated {
            obj: new,
            def,
            controller,
        });
        new
    }

    /// CR 707.2, 111.4: create a token that is a copy of `of` (its copiable
    /// values, exceptions included), with `except`'s own exceptions if
    /// given.
    pub(super) fn create_token_copy(
        &mut self,
        of: ObjId,
        controller: PlayerId,
        except: Option<CardRef>,
    ) -> ObjId {
        let (def, x) = {
            let o = &self.objects[of];
            (o.def, except.or(o.except))
        };
        let id = self.new_object(def, controller, Zone::Command);
        let t = &mut self.objects[id];
        t.kind = ObjKind::Token;
        t.except = x;
        let new = self.put_onto_battlefield(id, controller);
        self.emit(Event::TokenCreated {
            obj: new,
            def,
            controller,
        });
        new
    }

    /// CR 707: `id` becomes a copy of `of`, keeping `except`'s exceptions
    /// (CR 707.9b). Counters, damage, and attachments stay; the board
    /// counts follow the new definition.
    pub(super) fn become_copy(&mut self, id: ObjId, of: ObjId, except: Option<CardRef>) {
        let (def, x) = {
            let o = &self.objects[of];
            (o.def, except.or(o.except))
        };
        let (old_flags, old_legend) = {
            let o = &self.objects[id];
            (self.flags_of(o), self.pool.get(o.def).is_legendary())
        };
        self.count_sources(old_flags, old_legend, false);
        let o = &mut self.objects[id];
        o.def = def;
        o.except = x;
        let (flags, legend) = {
            let o = &self.objects[id];
            (self.flags_of(o), self.pool.get(o.def).is_legendary())
        };
        self.count_sources(flags, legend, true);
        self.cbump();
        self.emit(Event::BecameCopy { obj: id, of: def });
    }

    /// CR 701.21: sacrifice a permanent. Indestructible does not matter.
    pub(super) fn sacrifice(&mut self, id: ObjId) {
        let Some(o) = self.objects.get(id) else {
            return;
        };
        if o.zone != Zone::Battlefield {
            return;
        }
        let def = o.def;
        self.emit(Event::Sacrificed { obj: id, def });
        self.move_object(id, Zone::Graveyard, true);
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
            self.sba_touch();
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
        // A life total is a characteristic only through a CDA that reads it.
        if self.life_cda_sources > 0 {
            self.cbump();
        } else {
            self.bump();
        }
        self.sba_touch();
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
                let Some(o) = self.objects.get(id) else {
                    return;
                };
                if o.zone != Zone::Battlefield {
                    return;
                }
                let Some(c) = self.characteristics(id) else {
                    return;
                };
                // CR 120.3c: damage to a planeswalker removes loyalty;
                // CR 120.3e: damage to a creature is marked on it.
                if c.is_planeswalker() {
                    self.remove_counters(id, CounterKind::Loyalty, amount);
                }
                if c.is_creature() {
                    let o = &mut self.objects[id];
                    o.damage += amount;
                    if deathtouch {
                        o.deathtouch_damage = true;
                    }
                }
                self.bump();
                self.sba_touch();
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
        self.cbump();
        self.emit(Event::CountersAdded { obj: id, kind, n });
    }

    /// CR 122.5: remove counters from a permanent.
    /// Remove every counter from a permanent (Vampire Hexmage).
    pub(super) fn remove_all_counters(&mut self, id: ObjId) {
        let counters = self.objects[id].counters.clone();
        for (kind, n) in counters {
            self.remove_counters(id, kind, n);
        }
    }

    /// Put the top `n` cards of `p`'s library into their graveyard (CR 701.13).
    pub(super) fn mill(&mut self, p: PlayerId, n: u32) {
        for _ in 0..n {
            let Some(top) = self.player(p).library.last().copied() else {
                return;
            };
            self.move_object(top, Zone::Graveyard, true);
        }
    }

    /// Create an emblem for `controller` (CR 114.1): an object in the
    /// command zone whose static abilities apply like a permanent's.
    pub(super) fn create_emblem(&mut self, def: CardRef, controller: PlayerId) -> ObjId {
        let id = self.new_object(def, controller, Zone::Command);
        self.objects[id].kind = ObjKind::Emblem;
        self.emblems.push(id);
        self.cbump();
        self.emit(Event::EmblemCreated {
            obj: id,
            def,
            controller,
        });
        id
    }

    /// Discard a card from hand (CR 701.8).
    pub(super) fn discard_card(&mut self, p: PlayerId, card: ObjId) {
        self.emit(Event::Discard { player: p, card });
        self.move_object(card, Zone::Graveyard, true);
    }

    pub(super) fn remove_counters(&mut self, id: ObjId, kind: CounterKind, n: u32) {
        let Some(o) = self.objects.get_mut(id) else {
            return;
        };
        let n = n.min(o.counters(kind));
        if o.zone != Zone::Battlefield || n == 0 {
            return;
        }
        o.remove_counters(kind, n);
        self.cbump();
        self.emit(Event::CountersRemoved { obj: id, kind, n });
    }

    /// Destroy a permanent (CR 701.7), unless it is indestructible (CR
    /// 702.12b). Regeneration shields may replace it (CR 701.15).
    pub(super) fn destroy(&mut self, id: ObjId) {
        self.destroy_with(id, true);
    }

    pub(super) fn destroy_with(&mut self, id: ObjId, regenerate: bool) {
        if !self.objects.contains_key(id) {
            return;
        }
        self.propose(Proposed::Destroy {
            obj: id,
            regenerate,
        });
    }

    fn destroy_now(&mut self, id: ObjId) {
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
