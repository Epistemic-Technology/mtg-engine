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
use crate::state::{
    Affected, AltCast, CastInfo, Chosen, GameObject, GameState, LastKnown, ObjKind, Target, Zone,
};

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
        if zone == Zone::Graveyard && self.pool.get(def).graveyard_active() {
            self.player_mut(owner).flashback_in_graveyard += 1;
        }
        if self.pool.get(def).flags().has_enters_choice() {
            self.enters_choice_cards += 1;
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
            echo_due: false,
            skip_untap: false,
            chosen: Chosen::None,
            alt: AltCast::None,
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
        // CR 702.43a: modular's counters.
        if d.modular > 0 {
            match counters
                .iter_mut()
                .find(|(k, _)| *k == CounterKind::PlusOnePlusOne)
            {
                Some((_, n)) => *n += d.modular,
                None => counters.push((CounterKind::PlusOnePlusOne, d.modular)),
            }
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
        let characteristics_is_land = characteristics.is_land();
        let characteristics_is_artifact = characteristics
            .types
            .contains(crate::card::CardType::Artifact);
        self.remove_from_zone_list(id);
        let old = self.objects.remove(id).expect("object exists");
        if old.zone == Zone::Battlefield && self.pool.is_face_down(old.def) {
            self.face_down_count = self.face_down_count.saturating_sub(1);
        }
        if old.zone == Zone::Battlefield && to == Zone::Graveyard && characteristics.is_creature() {
            self.creatures_died_this_turn = self.creatures_died_this_turn.saturating_add(1);
        }
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
        // CR 614.12: "As this enters, choose ..." is chosen before it
        // enters, by the player it enters under.
        let chosen = if to == Zone::Battlefield {
            self.pool
                .get(def)
                .enters_choice()
                .map_or(Chosen::None, |k| self.choose_kind(controller, k))
        } else {
            Chosen::None
        };
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
            echo_due: to == Zone::Battlefield,
            skip_untap: false,
            chosen,
            // CR 702.62a: a creature cast by suspend has haste.
            alt: if old.zone == Zone::Stack
                && to == Zone::Battlefield
                && matches!(old.alt, AltCast::Suspend | AltCast::Escape | AltCast::Dash)
            {
                old.alt
            } else {
                AltCast::None
            },
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
        // CR 601.2h: the permanent a spell becomes remembers the mana
        // spent to cast it ("if {B} was spent to cast it").
        if !self.spent_mana.is_empty()
            && old.zone == Zone::Stack
            && to == Zone::Battlefield
            && let Some(i) = self.spent_mana.iter().position(|(o, _)| *o == id)
        {
            self.spent_mana[i].0 = new;
        }
        // "If a creature card was exiled this way" (CR 608.2c).
        if let Some(log) = &mut self.this_way {
            log.moved.push(crate::state::Moved {
                old: id,
                new,
                from: old.zone,
                to,
                owner: old.owner,
            });
        }
        // CR 702.109a: a dashed permanent returns to its owner's hand at
        // the beginning of the next end step.
        if to == Zone::Battlefield && old.zone == Zone::Stack && old.alt == AltCast::Dash {
            let ability = self.pool.builtin(crate::card::Builtin::DashReturn);
            self.delayed_triggers.push(crate::state::DelayedTrigger {
                controller,
                ability: crate::state::StackAbility {
                    ability,
                    source: new,
                    def,
                    trigger_obj: Some(new),
                    trigger_player: None,
                    trigger_amount: 0,
                },
                this_turn: false,
            });
        }
        if old.zone == Zone::Battlefield {
            // CR 506.4: a permanent that leaves the battlefield leaves combat.
            self.remove_from_combat(id);
            self.count_sources(old_flags, self.pool.get(old.def).is_legendary(), false);
        }
        if to == Zone::Battlefield {
            if characteristics_is_land {
                // Landfall: "you had a land enter the battlefield under
                // your control this turn".
                self.lands_entered_this_turn |= 1 << controller.index().min(7);
            }
            if characteristics_is_artifact {
                self.artifacts_entered_this_turn |= 1 << controller.index().min(7);
            }
            let flags = self.flags_of(&self.objects[new]);
            self.count_sources(flags, self.pool.get(def).is_legendary(), true);
            if self.pool.is_face_down(def) {
                self.face_down_count += 1;
            }
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
        if old.kind == ObjKind::Card && self.pool.get(old.card).graveyard_active() {
            if old.zone == Zone::Graveyard {
                let n = &mut self.player_mut(old.owner).flashback_in_graveyard;
                *n = n.saturating_sub(1);
            }
            if to == Zone::Graveyard {
                self.player_mut(old.owner).flashback_in_graveyard += 1;
            }
        }
        if old.zone == Zone::Exile && (old.cast.adventure() || matches!(old.alt, AltCast::Foretold))
        {
            let n = &mut self.player_mut(old.owner).adventures_in_exile;
            *n = n.saturating_sub(1);
        }
        if old.zone == Zone::Exile && old.alt == AltCast::Suspended {
            self.suspended_count = self.suspended_count.saturating_sub(1);
        }
        if old.zone == Zone::Stack && !self.x_values.is_empty() {
            self.x_values.retain(|(s, _)| *s != id);
        }
        if old.zone == Zone::Stack && !self.divisions.is_empty() {
            self.divisions.retain(|(s, _)| *s != id);
        }
        // Graveyards feed characteristic-defining abilities (Tarmogoyf).
        let affects_chars = |z: Zone| matches!(z, Zone::Battlefield | Zone::Graveyard);
        // A static counting cards in hand (Empyrial Armor) sees hand moves.
        let hand_static = (old.zone == Zone::Hand || to == Zone::Hand)
            && self.turn_static_sources > 0
            && self
                .battlefield
                .iter()
                .any(|b| self.pool.get(self.objects[*b].def).hand_static);
        if affects_chars(old.zone) || affects_chars(to) || hand_static {
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
            self.tap_static_sources += d(flags.has_tap_static());
            self.cost_change_sources += d(flags.has_cost_change());
            self.restricted_mana_sources += d(flags.has_restricted_mana());
            self.turn_static_sources += d(flags.has_turn_static());
            self.attack_rule_sources += d(flags.has_attack_rule());
            self.grant_sources += d(flags.grants_abilities());
            self.spent_watchers += d(flags.watches_spent());
        } else {
            self.trigger_sources -= d(flags.has_triggers());
            self.replacement_sources -= d(flags.has_replacements());
            self.control_sources -= d(flags.has_control_static());
            self.legendary_count -= d(legendary);
            self.activated_sources -= d(flags.has_activated());
            self.life_cda_sources -= d(flags.has_life_cda());
            self.tap_static_sources -= d(flags.has_tap_static());
            self.cost_change_sources -= d(flags.has_cost_change());
            self.restricted_mana_sources -= d(flags.has_restricted_mana());
            self.turn_static_sources -= d(flags.has_turn_static());
            self.attack_rule_sources -= d(flags.has_attack_rule());
            self.grant_sources -= d(flags.grants_abilities());
            self.spent_watchers -= d(flags.watches_spent());
            // Whatever it controlled goes back (CR 611.2c does not apply to
            // static abilities).
            self.control_dirty |= flags.has_control_static();
        }
    }

    /// CR 111.2: create a token under `controller`'s control. The token is
    /// made in the command zone and put onto the battlefield through the
    /// pipeline, so "enters" effects and triggers see it.
    pub(super) fn create_token(&mut self, def: CardRef, controller: PlayerId) -> ObjId {
        self.create_token_with(def, controller, false)
    }

    /// `create_token`, the token entering tapped if `tapped` ("create a
    /// tapped Powerstone token").
    pub(super) fn create_token_with(
        &mut self,
        def: CardRef,
        controller: PlayerId,
        tapped: bool,
    ) -> ObjId {
        let id = self.new_object(def, controller, Zone::Command);
        self.objects[id].kind = ObjKind::Token;
        let new = self.put_onto_battlefield_with(
            id,
            controller,
            Enter {
                tapped,
                counters: None,
            },
        );
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
    /// CR 708.8: a face-down permanent turns face up as the card it is;
    /// megamorph adds a +1/+1 counter (CR 702.37b).
    pub(super) fn turn_face_up(&mut self, id: ObjId) {
        let o = &self.objects[id];
        let (old_flags, card) = (self.flags_of(o), o.card);
        self.count_sources(old_flags, false, false);
        self.face_down_count = self.face_down_count.saturating_sub(1);
        self.objects[id].def = card;
        let def = self.pool.get(card);
        let (flags, legend, mega) = (
            self.flags_of(&self.objects[id]),
            def.is_legendary(),
            matches!(
                def.morph.as_deref(),
                Some((_, crate::card::MorphKind::Megamorph))
            ),
        );
        self.count_sources(flags, legend, true);
        self.cbump();
        if mega {
            self.add_counters(id, CounterKind::PlusOnePlusOne, 1);
        }
        self.emit(Event::TurnedFaceUp { obj: id });
    }

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

    /// Put `id` into its owner's library at `place`. Where the owner has a
    /// choice ("on their choice of the top or bottom", "second from the
    /// top or on the bottom"), they make it as the object moves.
    pub(super) fn put_in_library(&mut self, id: ObjId, place: crate::card::LibraryPlace) {
        use crate::card::LibraryPlace;
        let Some(o) = self.objects.get(id) else {
            return;
        };
        let (owner, def) = (o.owner, o.def);
        let choose = |s: &mut Self| {
            s.choose_mid(
                owner,
                crate::decision::DecisionKind::TopOrBottom { def },
                vec![
                    crate::decision::Choice::Accept,
                    crate::decision::Choice::Decline,
                ],
            ) == 0
        };
        // `n`th from the top, or `None` for the bottom.
        let depth = match place {
            LibraryPlace::Top | LibraryPlace::Shuffle => Some(1),
            LibraryPlace::Bottom => None,
            LibraryPlace::FromTop(n) => Some(n),
            LibraryPlace::TopOrBottom => choose(self).then_some(1),
            LibraryPlace::FromTopOrBottom(n) => choose(self).then_some(n),
        };
        let new = self.move_object(id, Zone::Library, depth.is_some());
        if !self
            .objects
            .get(new)
            .is_some_and(|o| o.zone == Zone::Library)
        {
            return;
        }
        if place == LibraryPlace::Shuffle {
            self.shuffle_library(owner);
            return;
        }
        // CR 401.7: "Nth from the top" of a library with fewer than N-1
        // other cards is the bottom.
        if let Some(n) = depth
            && n > 1
        {
            let lib = &mut self.player_mut(owner).library;
            if let Some(pos) = lib.iter().rposition(|x| *x == new) {
                let card = lib.remove(pos);
                let at = lib.len().saturating_sub(usize::from(n) - 1);
                lib.insert(at, card);
                self.bump();
            }
        }
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
            let n = &mut self.drawn_this_turn[p.index().min(3)];
            *n = n.saturating_add(1);
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
            self.tap_epoch = self.tap_epoch.wrapping_add(1);
            if self.tap_static_sources > 0 {
                self.cbump();
            }
            self.emit(Event::Tapped { obj: id });
        }
    }

    pub(super) fn untap(&mut self, id: ObjId) {
        // "Can't become untapped": nothing happens, and a stun counter
        // stays (it is removed only as the permanent would untap). Only a
        // tapped permanent pays for the lookup.
        if self.objects.get(id).is_some_and(|o| o.tapped)
            && self
                .characteristics(id)
                .is_some_and(|c| c.has(Keyword::CantBecomeUntapped))
        {
            return;
        }
        // CR 122.1d: a stun counter is removed instead.
        if self
            .objects
            .get(id)
            .is_some_and(|o| o.tapped && o.counters(CounterKind::Stun) > 0)
        {
            self.remove_counters(id, CounterKind::Stun, 1);
            return;
        }
        if let Some(o) = self.objects.get_mut(id)
            && o.tapped
        {
            o.tapped = false;
            self.bump();
            self.tap_epoch = self.tap_epoch.wrapping_add(1);
            if self.tap_static_sources > 0 {
                self.cbump();
            }
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

    /// Give a player poison counters (CR 122.1f). Characteristics may
    /// read them ("as long as an opponent has three or more poison
    /// counters"), and poison changes are rare, so every cached
    /// characteristic is invalidated.
    pub(super) fn add_poison(&mut self, p: PlayerId, n: u32) {
        if n == 0 {
            return;
        }
        self.player_mut(p).poison += n;
        self.cbump();
        self.sba_touch();
    }

    /// Change a life total directly (life loss, or a gain that has passed
    /// through the pipeline).
    pub(super) fn change_life(&mut self, p: PlayerId, delta: i32) {
        if delta == 0 {
            return;
        }
        if delta < 0
            && let Some(log) = &mut self.this_way
        {
            log.life_lost = log.life_lost.saturating_add(delta.unsigned_abs());
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
        let infect = src.as_ref().is_some_and(|c| c.has(Keyword::Infect));
        let wither = infect || src.as_ref().is_some_and(|c| c.has(Keyword::Wither));
        let controller = self
            .objects
            .get(source)
            .map(|o| o.controller)
            .or_else(|| self.lki.get(&source).map(|l| l.object.controller));
        match target {
            Target::Player(p) => {
                self.damaged_this_turn |= 1 << p.index().min(7);
                self.note_damage_source(source);
                self.emit(Event::Damage {
                    source,
                    target,
                    amount,
                    combat,
                });
                if infect {
                    // CR 702.90b: damage to a player as poison counters.
                    self.add_poison(p, amount);
                } else {
                    let delta = -i32::try_from(amount).unwrap_or(i32::MAX);
                    self.change_life(p, delta);
                }
                // CR 702.164c: toxic adds poison counters with combat damage.
                if combat && let Some(c) = src.as_ref() {
                    let toxic: u32 = c
                        .abilities
                        .iter()
                        .map(|a| match self.pool.ability(*a) {
                            crate::card::AbilityDef::Toxic(n) => *n,
                            _ => 0,
                        })
                        .sum();
                    if toxic > 0 {
                        self.add_poison(p, toxic);
                    }
                }
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
                // CR 702.16d: damage from a source with the quality is
                // prevented.
                if let Some(sc) = src.as_ref()
                    && c.protected_from(sc, &self.pool)
                {
                    return;
                }
                // CR 120.3c: damage to a planeswalker removes loyalty;
                // CR 120.3e: damage to a creature is marked on it.
                if c.is_planeswalker() {
                    self.remove_counters(id, CounterKind::Loyalty, amount);
                }
                if c.is_creature() {
                    let o = &mut self.objects[id];
                    if !wither {
                        o.damage += amount;
                    }
                    if deathtouch {
                        o.deathtouch_damage = true;
                    }
                }
                self.bump();
                self.sba_touch();
                self.note_damage_source(source);
                self.emit(Event::Damage {
                    source,
                    target,
                    amount,
                    combat,
                });
                // CR 702.80c, 702.90c: damage to a creature from a source
                // with wither or infect is -1/-1 counters.
                if wither && c.is_creature() {
                    self.add_counters(id, CounterKind::MinusOneMinusOne, amount);
                }
            }
        }
        if lifelink && let Some(c) = controller {
            self.gain_life(c, amount);
        }
    }

    /// Remember that `source` dealt damage this turn ("creature that dealt
    /// damage this turn").
    fn note_damage_source(&mut self, source: ObjId) {
        if !self.dealt_damage_this_turn.contains(&source) {
            self.dealt_damage_this_turn.push(source);
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
        self.discarded_this_turn |= 1 << p.index().min(7);
        self.emit(Event::Discard { player: p, card });
        // CR 702.35a: a card with madness is discarded into exile, and its
        // trigger lets its owner cast it for the madness cost.
        if self.pool.get(self.objects[card].def).madness.is_some() {
            let new = self.move_object(card, Zone::Exile, true);
            if let Some(o) = self.objects.get_mut(new)
                && o.zone == Zone::Exile
            {
                o.alt = AltCast::MadnessExiled;
                let def = o.def;
                let ability = self.pool.builtin(crate::card::Builtin::Madness);
                self.queue_builtin(p, ability, new, def);
            }
            return;
        }
        self.move_object(card, Zone::Graveyard, true);
    }

    /// Puts one of the pool's built-in triggered abilities (madness,
    /// suspend, cascade) among the pending triggers, from `source`.
    pub(super) fn queue_builtin(
        &mut self,
        controller: PlayerId,
        ability: crate::card::AbilityId,
        source: ObjId,
        def: CardRef,
    ) {
        let timestamp = self.tick();
        self.pending_triggers.push(crate::state::PendingTrigger {
            controller,
            ability: crate::state::StackAbility {
                ability,
                source,
                def,
                trigger_obj: None,
                trigger_player: None,
                trigger_amount: 0,
            },
            timestamp,
        });
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
