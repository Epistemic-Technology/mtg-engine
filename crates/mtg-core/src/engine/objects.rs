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
        if zone == Zone::Graveyard && self.pool.get(def).flags().has_graveyard_trigger() {
            self.graveyard_trigger_cards += 1;
        }
        if zone == Zone::Graveyard && self.pool.get(def).dredge > 0 {
            self.dredge_cards += 1;
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
            marks: 0,
            skip_untap: false,
            chosen: Chosen::None,
            alt: AltCast::None,
            timestamp,
            targets: SmallVec::new(),
            counters: SmallVec::new(),
            attached_to: None,
            ability: None,
            revealed: 0,
        })
    }

    pub(super) fn remove_from_zone_list(&mut self, id: ObjId) {
        let obj = &self.objects[id];
        let (zone, owner) = (obj.zone, obj.owner);
        let list: &mut Vec<ObjId> = match zone {
            Zone::Battlefield => &mut self.battlefield,
            Zone::Stack => &mut self.stack,
            Zone::Command => return,
            Zone::PhasedOut => {
                self.phased_out.retain(|p| p.id != id);
                return;
            }
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

    /// CR 702.145b: "If it is night and this permanent is represented by a
    /// double-faced card, it enters transformed." The definition a
    /// permanent enters as.
    fn daybound_entering(&self, def: CardRef) -> CardRef {
        if self.desig.day_night != crate::state::DayNight::Night || self.mark_sources == 0 {
            return def;
        }
        let d = self.pool.get(def);
        if d.marks.has(crate::card::Marks::DAYBOUND)
            && d.layout == crate::card::Layout::Transform
            && let Some(back) = d.faces.first()
        {
            return *back;
        }
        def
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
        // CR 310.4b: a battle enters with its printed defense in defense
        // counters.
        if d.types.contains(crate::card::CardType::Battle) && d.defense > 0 {
            counters.push((CounterKind::Defense, d.defense));
        }
        // CR 714.3a: a Saga enters with a lore counter.
        if d.final_chapter > 0 {
            counters.push((CounterKind::Lore, 1));
        }
        // CR 702.43a, 702.58a: modular's and graft's counters; CR 702.63a:
        // vanishing's time counters.
        for (kind, extra) in [
            (CounterKind::PlusOnePlusOne, d.modular + d.graft),
            (CounterKind::Time, d.vanishing),
        ] {
            if extra == 0 {
                continue;
            }
            match counters.iter_mut().find(|(k, _)| *k == kind) {
                Some((_, n)) => *n += extra,
                None => counters.push((kind, extra)),
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
        // "If this would be put into a graveyard from anywhere, exile it
        // instead." (a disturbed card's back face, CR 702.146).
        // CR 122.1h: a permanent with a finality counter on it that would
        // be put into a graveyard from the battlefield is exiled instead.
        let to = if to == Zone::Graveyard
            && (self.pool.get(self.objects[id].def).graveyard_exile
                || (self.objects[id].zone == Zone::Battlefield
                    && self.objects[id].counters(CounterKind::Finality) > 0))
        {
            Zone::Exile
        } else {
            to
        };
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
            self.died_this_turn
                .push((id, old.controller, characteristics));
        }
        // Revolt: under its controller's control as it left.
        if old.zone == Zone::Battlefield {
            self.left_this_turn |= 1 << old.controller.index().min(7);
            // Void (CR 702.185): "a nonland permanent left the battlefield
            // this turn".
            if !characteristics_is_land && !self.turn_log.nonland_left {
                self.turn_log.nonland_left = true;
                self.history_bump();
            }
            if to == Zone::Graveyard {
                self.turn_log.to_graveyard =
                    self.turn_log.to_graveyard.union(characteristics.types);
            }
        }
        // CR 700.11: its owner descended if a permanent card went to their
        // graveyard (a token is no card).
        if to == Zone::Graveyard && old.kind == crate::state::ObjKind::Card {
            let types = self.pool.get(old.card).types;
            if types.is_permanent_type() {
                self.turn_log.descended |= 1 << old.owner.index().min(7);
            }
            // "if a creature card was put into your graveyard from anywhere
            // this turn".
            let seen = &mut self.turn_log.cards_to_graveyard[old.owner.index().min(3)];
            if !seen.contains_all(types) {
                *seen = seen.union(types);
                self.history_bump();
            }
        }
        // "if one or more cards left your graveyard this turn".
        if old.zone == Zone::Graveyard {
            let bit = 1 << old.owner.index().min(7);
            if self.turn_log.left_graveyard & bit == 0 {
                self.turn_log.left_graveyard |= bit;
                self.history_bump();
            }
        }
        let old_flags = self.flags_of(&old);
        let (attacking, blocking) = match &self.combat {
            Some(c) if old.zone == Zone::Battlefield => (
                c.attackers.iter().any(|a| a.id == id),
                c.blocked_by(id).is_some(),
            ),
            _ => (false, false),
        };
        self.lki.insert(
            id,
            LastKnown {
                object: old.clone(),
                characteristics,
                moved_to: None,
                attacking,
                blocking,
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
            // CR 709.5d: a Room that wasn't cast as a half enters with
            // neither door unlocked.
            match self.pool.get(old.def).doors.as_deref() {
                Some(d) if d.root == old.def => (d.none, old.except),
                _ => (self.daybound_entering(old.def), old.except),
            }
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
                .map_or(Chosen::None, |k| self.choose_kind(controller, k, def))
        } else {
            Chosen::None
        };
        // CR 722.3a: "enters prepared", only a permanent with a prepare
        // spell.
        let prepared = enter.prepared && to == Zone::Battlefield && self.has_prepare_spell(def);
        if prepared {
            self.prepared_ever += 1;
        }
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
            marks: if to == Zone::Battlefield {
                let mut m = 0;
                if enter.haste {
                    m |= crate::state::OBJ_RIOT_HASTE;
                }
                if prepared {
                    m |= crate::state::OBJ_PREPARED;
                }
                m
            } else {
                0
            },
            skip_untap: false,
            chosen,
            // CR 702.62a: a creature cast by suspend has haste.
            alt: if old.zone == Zone::Stack
                && to == Zone::Battlefield
                && matches!(
                    old.alt,
                    AltCast::Suspend
                        | AltCast::Escape
                        | AltCast::Dash
                        | AltCast::Sneak
                        | AltCast::WebSlinging
                        | AltCast::Impending
                ) {
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
            // CR 303.4f: an Aura spell enters attached to its target, so its
            // "When this Aura enters, if enchanted creature ..." sees what
            // it enchants (CR 603.4).
            attached_to: if old.zone == Zone::Stack && to == Zone::Battlefield {
                match old.targets.first() {
                    Some(Target::Object(t)) if self.pool.get(def).enchant().is_some() => Some(*t),
                    _ => None,
                }
            } else {
                None
            },
            ability: None,
            revealed: 0,
        });
        if let Some(l) = self.lki.get_mut(&id) {
            l.moved_to = Some(new);
        }
        // CR 607.2a: a card the resolving ability of a permanent that
        // refers to "cards exiled with" it exiles is linked to it.
        if let Some(src) = self.linking
            && to == Zone::Exile
            && old.zone != Zone::Exile
        {
            self.link_exile(src, new, false);
        }
        // CR 607.2a: what the spell's costs exiled ("behold a Kithkin and
        // exile it") is linked to the permanent it becomes.
        if !self.linked_exiles.is_empty() && old.zone == Zone::Stack && to == Zone::Battlefield {
            for l in &mut self.linked_exiles {
                if l.source == id {
                    l.source = new;
                }
            }
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
                sacrificed: false,
                destroyed: false,
            });
        }
        // CR 702.109a: a dashed permanent returns to its owner's hand at
        // the beginning of the next end step.
        // Spider Climb cast any time a sorcery couldn't have been: its
        // permanent is sacrificed at the beginning of the next cleanup step.
        let cleanup = old.cast.instant_timing();
        if to == Zone::Battlefield
            && old.zone == Zone::Stack
            && (matches!(old.alt, AltCast::Dash | AltCast::Warp) || cleanup)
        {
            // CR 702.185a: a warped permanent is exiled at the next end
            // step.
            let ability = self.pool.builtin(if cleanup {
                crate::card::Builtin::CleanupSacrifice
            } else if old.alt == AltCast::Warp {
                crate::card::Builtin::WarpExile
            } else {
                crate::card::Builtin::DashReturn
            });
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
                forever: false,
                turn: self.turn_number,
            });
        }
        if old.zone == Zone::Battlefield {
            // CR 506.4: a permanent that leaves the battlefield leaves combat.
            self.remove_from_combat(id);
            self.count_sources(old_flags, self.pool.get(old.def).is_legendary(), false);
        }
        if to == Zone::Battlefield {
            self.entered_this_turn.push((controller, new));
            if characteristics_is_land {
                // Landfall: "you had a land enter the battlefield under
                // your control this turn".
                self.lands_entered_this_turn |= 1 << controller.index().min(7);
            }
            if characteristics_is_artifact {
                self.artifacts_entered_this_turn |= 1 << controller.index().min(7);
            }
            if characteristics.is_creature() {
                let n = &mut self.turn_log.creatures_entered[controller.index().min(3)];
                *n = n.saturating_add(1);
            }
            if !characteristics_is_land {
                let n = &mut self.turn_log.nonland_entered[controller.index().min(3)];
                *n = n.saturating_add(1);
            }
            let flags = self.flags_of(&self.objects[new]);
            self.count_sources(flags, self.pool.get(def).is_legendary(), true);
            if self.pool.is_face_down(def) {
                self.face_down_count += 1;
                // "if a face-down creature entered the battlefield under
                // your control this turn".
                self.turn_log.face_down_entered |= 1 << controller.index().min(7);
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
        if old.kind == ObjKind::Card && self.pool.get(old.card).dredge > 0 {
            if old.zone == Zone::Graveyard {
                self.dredge_cards = self.dredge_cards.saturating_sub(1);
            }
            if to == Zone::Graveyard {
                self.dredge_cards += 1;
            }
        }
        if old.kind == ObjKind::Card && self.pool.get(old.card).flags().has_graveyard_trigger() {
            if old.zone == Zone::Graveyard {
                self.graveyard_trigger_cards = self.graveyard_trigger_cards.saturating_sub(1);
            }
            if to == Zone::Graveyard {
                self.graveyard_trigger_cards += 1;
            }
        }
        if old.zone == Zone::Exile
            && (old.cast.adventure()
                || matches!(
                    old.alt,
                    AltCast::Foretold | AltCast::Plotted | AltCast::Airbent
                ))
        {
            let n = &mut self.player_mut(old.owner).adventures_in_exile;
            *n = n.saturating_sub(1);
        }
        if old.zone == Zone::Exile && old.alt == AltCast::Suspended {
            self.suspended_count = self.suspended_count.saturating_sub(1);
        }
        if old.zone == Zone::Stack && !self.x_values.is_empty() {
            // CR 107.3m: the permanent's enters triggers keep the spell's X.
            if to == Zone::Battlefield {
                let x = self.x_of(id);
                if x > 0 {
                    self.entered_x.push((new, x));
                }
            }
            self.x_values.retain(|(s, _)| *s != id);
        }
        if old.zone == Zone::Stack && !self.divisions.is_empty() {
            self.divisions.retain(|(s, _)| *s != id);
        }
        if old.zone == Zone::Stack && !self.splices.is_empty() {
            self.splices.retain(|s| s.spell != id);
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
        // "If it's neither day nor night, it becomes day as this creature
        // enters." (CR 731.1)
        if to == Zone::Battlefield
            && self.mark_sources > 0
            && self.desig.day_night == crate::state::DayNight::Neither
            && self
                .pool
                .get(self.objects[new].def)
                .marks
                .has(crate::card::Marks::BECOMES_DAY)
        {
            self.set_day_night(true);
        }
        self.emit(Event::ZoneChange {
            old: id,
            new,
            def: old.def,
            from: old.zone,
            to,
        });
        // CR 122.6: counters a permanent enters with are put on it, so a
        // Saga's first lore counter triggers its chapter I (CR 714.3a).
        if to == Zone::Battlefield && self.objects[new].counters.iter().any(|(_, n)| *n > 0) {
            let entered: SmallVec<[(CounterKind, u32); 2]> =
                self.objects[new].counters.iter().copied().collect();
            let by = self
                .effect_controller
                .unwrap_or(self.objects[new].controller);
            for (kind, n) in entered {
                if n > 0 {
                    if kind == CounterKind::PlusOnePlusOne {
                        self.turn_log.counter_put_by |= 1 << by.index().min(7);
                    }
                    // Asked once per player each turn (the cache was bumped
                    // as it entered).
                    let bit = 1 << by.index().min(7);
                    if self.turn_log.any_counter_put_by & bit == 0
                        && self.characteristics(new).is_some_and(|c| c.is_creature())
                    {
                        self.turn_log.any_counter_put_by |= bit;
                    }
                    self.emit(Event::CountersAdded {
                        obj: new,
                        kind,
                        n,
                        by,
                        before: 0,
                    });
                }
            }
        }
        // CR 709.5h: a Room entering with a door unlocked (its half was
        // cast) unlocks that door as it enters.
        if to == Zone::Battlefield
            && let Some(doors) = self.pool.get(def).doors.as_deref()
        {
            let unlocked = doors.unlocked(def);
            for door in 0..2u8 {
                if unlocked[usize::from(door)] {
                    self.emit(Event::Unlocked {
                        obj: new,
                        door,
                        fully: unlocked == [true, true],
                    });
                }
            }
        }
        if old.zone == Zone::Battlefield && !self.linked_exiles.is_empty() {
            self.return_exiled_until(id);
        }
        new
    }

    /// Link `card`, in exile, to the permanent `source` (CR 607.2a), or
    /// mark an existing link as lasting until it leaves (CR 610.3).
    pub(super) fn link_exile(&mut self, source: ObjId, card: ObjId, until: bool) {
        if let Some(l) = self
            .linked_exiles
            .iter_mut()
            .find(|l| l.source == source && l.card == card)
        {
            l.until |= until;
        } else {
            self.linked_exiles.push(crate::state::LinkedExile {
                source,
                card,
                until,
                to_hand: false,
            });
        }
        // Counts and conditions read the links.
        self.cbump();
    }

    /// How many cards `source` exiled are still in exile, all or
    /// matching `f` (CR 607.2a).
    pub(crate) fn count_exiled_with(
        &self,
        source: ObjId,
        f: Option<&crate::card::ObjFilter>,
        you: PlayerId,
    ) -> usize {
        self.exiled_with(source)
            .filter(|id| self.card_matches(*id, f, you))
            .count()
    }

    /// The card types among the cards `source` exiled (CR 607.2a).
    pub(crate) fn types_exiled_with(&self, source: ObjId) -> u32 {
        let mut types = crate::card::TypeSet::default();
        for id in self.exiled_with(source) {
            types = types.union(self.pool.get(self.objects[id].def).types);
        }
        types.len()
    }

    /// Cards `you` own in exile, and with `graveyard` in their graveyard
    /// too, all or matching `f` ("cards you own in exile and in your
    /// graveyard"). A token is not a card (CR 111.8).
    pub(crate) fn owned_in_exile(
        &self,
        you: PlayerId,
        f: Option<&crate::card::ObjFilter>,
        graveyard: bool,
    ) -> usize {
        let ps = self.player(you);
        let yard: &[ObjId] = if graveyard { &ps.graveyard } else { &[] };
        ps.exile
            .iter()
            .chain(yard.iter())
            .filter(|id| self.objects[**id].kind != crate::state::ObjKind::Token)
            .filter(|id| self.card_matches(**id, f, you))
            .count()
    }

    /// Whether a card outside the battlefield matches `f` (all do without
    /// one), judged by its card's characteristics.
    fn card_matches(&self, id: ObjId, f: Option<&crate::card::ObjFilter>, you: PlayerId) -> bool {
        f.is_none_or(|f| {
            let o = &self.objects[id];
            let c = self.base_characteristics(o);
            f.matches(id, o.owner, &c, None, Some(you))
        })
    }

    /// The cards `source` exiled that are still in exile (CR 607.2a).
    pub(crate) fn exiled_with(&self, source: ObjId) -> impl Iterator<Item = ObjId> + '_ {
        self.linked_exiles
            .iter()
            .filter(move |l| {
                l.source == source
                    && self
                        .objects
                        .get(l.card)
                        .is_some_and(|o| o.zone == Zone::Exile)
            })
            .map(|l| l.card)
    }

    /// CR 610.3: the cards `left` exiled "until this leaves the
    /// battlefield" return to the battlefield under their owners' control
    /// (610.3c) right after it leaves, without using the stack.
    #[cold]
    #[inline(never)]
    fn return_exiled_until(&mut self, left: ObjId) {
        let mut back: SmallVec<[(ObjId, bool); 2]> = SmallVec::new();
        self.linked_exiles.retain(|l| {
            let mine = l.until && l.source == left;
            if mine {
                back.push((l.card, l.to_hand));
            }
            !mine
        });
        for (card, to_hand) in back {
            if let Some(o) = self.objects.get(card)
                && o.zone == Zone::Exile
            {
                let owner = o.owner;
                if to_hand {
                    // CR 610.3: to its owner's hand, where it was.
                    self.move_object(card, Zone::Hand, false);
                } else {
                    self.put_onto_battlefield_with(card, owner, Enter::default());
                }
            }
        }
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
            self.mana_trigger_sources += d(flags.has_mana_trigger());
            self.turn_static_sources += d(flags.has_turn_static());
            self.attack_rule_sources += d(flags.has_attack_rule());
            self.grant_sources += d(flags.grants_abilities());
            self.spent_watchers += d(flags.watches_spent());
            self.valiant_watchers += d(flags.has_valiant());
            self.state_trigger_sources += d(flags.has_state_trigger());
            self.mark_sources += d(flags.has_marks());
            self.banding_sources += d(flags.has_banding());
            self.room_count += d(flags.has_room());
            self.saga_count += d(flags.has_saga());
            self.player_static_sources += d(flags.has_player_static());
            self.top_play_sources += d(flags.has_top_play());
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
            self.mana_trigger_sources -= d(flags.has_mana_trigger());
            self.turn_static_sources -= d(flags.has_turn_static());
            self.attack_rule_sources -= d(flags.has_attack_rule());
            self.grant_sources -= d(flags.grants_abilities());
            self.spent_watchers -= d(flags.watches_spent());
            self.valiant_watchers -= d(flags.has_valiant());
            self.state_trigger_sources -= d(flags.has_state_trigger());
            self.mark_sources -= d(flags.has_marks());
            self.banding_sources -= d(flags.has_banding());
            self.room_count -= d(flags.has_room());
            self.saga_count -= d(flags.has_saga());
            self.player_static_sources -= d(flags.has_player_static());
            self.top_play_sources -= d(flags.has_top_play());
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
        self.create_token_entering(
            def,
            controller,
            Enter {
                tapped,
                ..Enter::default()
            },
        )
    }

    /// `create_token`, the token entering as `enter` says ("an Incubator
    /// token with two +1/+1 counters on it", CR 122.6).
    pub(super) fn create_token_entering(
        &mut self,
        def: CardRef,
        controller: PlayerId,
        enter: Enter,
    ) -> ObjId {
        let id = self.new_object(def, controller, Zone::Command);
        self.objects[id].kind = ObjKind::Token;
        let new = self.put_onto_battlefield_with(id, controller, enter);
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
        self.create_token_copy_of(def, x, controller)
    }

    /// A token copy of the definition `def` with the copy exceptions of
    /// `x` (CR 707.2, 707.9b): what `create_token_copy` makes, also from
    /// an object's last known information.
    pub(super) fn create_token_copy_of(
        &mut self,
        def: CardRef,
        x: Option<CardRef>,
        controller: PlayerId,
    ) -> ObjId {
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

    /// CR 708.8: a face-down permanent turns face up as the card it is;
    /// megamorph adds a +1/+1 counter if its cost was `paid` (CR
    /// 702.37b).
    pub(super) fn turn_face_up(&mut self, id: ObjId, paid: bool) {
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
        // CR 708.11: "As this creature is turned face up, put ... counters
        // on it", applied while it turns face up.
        let pool = std::sync::Arc::clone(&self.pool);
        for a in &pool.get(card).abilities {
            if let crate::card::AbilityDef::AsTurnedFaceUp { kind, n } = a {
                let n = self.eval_num_of(*n, id, self.objects[id].controller);
                self.add_counters(id, *kind, n);
            }
        }
        if mega && paid {
            self.add_counters(id, CounterKind::PlusOnePlusOne, 1);
        }
        self.emit(Event::TurnedFaceUp { obj: id });
    }

    /// CR 708.2a: a face-up permanent turned face down by an effect that
    /// lists no characteristics is a 2/2 face-down creature with no text
    /// (no ward, even if its card has disguise). It stays the same object;
    /// what anyone has looked at is forgotten.
    pub(super) fn turn_face_down(&mut self, id: ObjId) {
        let o = &self.objects[id];
        let (old_flags, legend) = (self.flags_of(o), self.pool.get(o.def).is_legendary());
        self.count_sources(old_flags, legend, false);
        let face_down = self.pool.face_down(false);
        let o = &mut self.objects[id];
        o.def = face_down;
        o.except = None;
        o.revealed = 0;
        self.face_down_count += 1;
        let flags = self.flags_of(&self.objects[id]);
        self.count_sources(flags, false, true);
        self.cbump();
    }

    /// CR 707: `id` becomes a copy of `of`, keeping `except`'s exceptions
    /// (CR 707.9b). Counters, damage, and attachments stay; the board
    /// counts follow the new definition.
    pub(super) fn become_copy(&mut self, id: ObjId, of: ObjId, except: Option<CardRef>) {
        let (def, x) = {
            let o = &self.objects[of];
            (o.def, except.or(o.except))
        };
        self.set_copy_def(id, def, x);
        self.emit(Event::BecameCopy { obj: id, of: def });
    }

    /// CR 611.2b: the copy effects whose duration ends now (`None`: "until
    /// end of turn", at cleanup; `Some(p)`: "until your next turn", as
    /// `p`'s turn begins): each permanent still the same object, and
    /// still that copy, gets back the values it had.
    #[cold]
    #[inline(never)]
    pub(super) fn end_temp_copies(&mut self, next_turn_of: Option<PlayerId>) {
        let mut ending: SmallVec<[crate::state::TempCopy; 2]> = SmallVec::new();
        self.temp_copies.retain(|t| {
            let ends = t.next_turn_of == next_turn_of;
            if ends {
                ending.push(*t);
            }
            !ends
        });
        // The latest first, so an earlier copy's values come back last.
        for t in ending.into_iter().rev() {
            if self
                .objects
                .get(t.obj)
                .is_some_and(|o| o.zone == Zone::Battlefield && o.def == t.copied)
            {
                self.set_copy_def(t.obj, t.def, t.except);
            }
        }
    }

    /// Give `id` the copiable values of `def` with `x`'s exceptions.
    fn set_copy_def(&mut self, id: ObjId, def: CardRef, x: Option<CardRef>) {
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
    }

    /// CR 701.21: sacrifice a permanent. Indestructible does not matter.
    pub(super) fn sacrifice(&mut self, id: ObjId) {
        let Some(o) = self.objects.get(id) else {
            return;
        };
        if o.zone != Zone::Battlefield {
            return;
        }
        let (def, who) = (o.def, o.controller);
        // "If you've sacrificed an artifact this turn".
        // "the number of permanents you've sacrificed this turn".
        let n = &mut self.turn_log.sacrificed_n[who.index().min(3)];
        *n = n.saturating_add(1);
        if let Some(c) = self.characteristics(id) {
            let seen = &mut self.turn_log.sacrificed[who.index().min(3)];
            *seen = seen.union(c.types);
        }
        self.history_bump();
        self.emit(Event::Sacrificed { obj: id, def });
        self.move_object(id, Zone::Graveyard, true);
        self.note_moved_how(id, true);
    }

    /// Mark the zone change of `id` just logged "this way" as a sacrifice
    /// or a destruction (`Moved::sacrificed`).
    fn note_moved_how(&mut self, id: ObjId, sacrificed: bool) {
        if let Some(m) = self
            .this_way
            .as_mut()
            .and_then(|l| l.moved.iter_mut().rev().find(|m| m.old == id))
        {
            if sacrificed {
                m.sacrificed = true;
            } else {
                m.destroyed = true;
            }
        }
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
        // Nobody knows where a card is in a shuffled library (CR 701.24).
        for id in &lib {
            self.objects[*id].revealed = 0;
        }
        self.player_mut(p).library = lib;
        self.bump();
    }

    /// `p` rolls a `sides`-sided die (CR 706.1a): a result from 1 to
    /// `sides`, equally likely, from the game's random number generator
    /// (as a shuffle draws on it, so a game replays from its seed and a
    /// search resamples it with the hidden cards, `GameState::determinize`).
    pub(super) fn roll_die(&mut self, p: PlayerId, sides: u8) -> u8 {
        let result = rand::Rng::random_range(&mut self.rng, 1..=sides.max(1));
        self.emit(Event::RolledDie {
            player: p,
            sides,
            result,
        });
        result
    }

    /// Reveal a card in a hand (CR 701.20a): every player has seen it,
    /// and knows it until it moves (`GameObject::revealed`). Anything not
    /// in a hand is only announced.
    pub(super) fn reveal_in_hand(&mut self, id: ObjId) {
        let Some(o) = self.objects.get_mut(id) else {
            return;
        };
        if o.zone == Zone::Hand {
            o.revealed = u8::MAX;
        }
        let def = o.def;
        self.emit(Event::Revealed { obj: id, def });
    }

    /// "Target player reveals their hand" (CR 701.20a): every card in it
    /// is known to every player until it moves.
    pub(super) fn reveal_hand(&mut self, p: PlayerId) {
        for i in 0..self.player(p).hand.len() {
            let id = self.player(p).hand[i];
            self.objects[id].revealed = u8::MAX;
        }
        self.emit(Event::RevealedHand { player: p });
    }

    /// Draw a card (CR 121.1), subject to replacement effects (CR 614.11).
    pub(super) fn draw(&mut self, p: PlayerId) {
        if self.dredge_cards > 0 && self.dredge(p) {
            return;
        }
        self.propose(Proposed::Draw { player: p });
    }

    /// Dredge (CR 702.52a): instead of drawing, `p` may mill N cards and
    /// return a card with dredge N from their graveyard to their hand, if
    /// their library has at least N cards (CR 702.52b). Asked before other
    /// replacement effects of the draw. Whether they did.
    #[cold]
    #[inline(never)]
    fn dredge(&mut self, p: PlayerId) -> bool {
        use crate::decision::{Choice, DecisionKind};
        let library = self.player(p).library.len();
        let mut options = vec![Choice::Done];
        for id in &self.player(p).graveyard {
            let o = &self.objects[*id];
            let n = self.pool.get(o.def).dredge;
            if n > 0 && usize::from(n) <= library {
                options.push(Choice::Discard {
                    card: *id,
                    def: o.def,
                });
            }
        }
        if options.len() == 1 {
            return false;
        }
        let k = self.choose_mid(p, DecisionKind::ChooseCard, options.clone());
        let Choice::Discard { card, def } = options[k] else {
            return false;
        };
        self.mill(p, u32::from(self.pool.get(def).dredge));
        if self
            .objects
            .get(card)
            .is_some_and(|o| o.zone == Zone::Graveyard)
        {
            self.move_object(card, Zone::Hand, false);
        }
        true
    }

    fn draw_now(&mut self, p: PlayerId) {
        if let Some(top) = self.player(p).library.last().copied() {
            let new = self.move_object(top, Zone::Hand, false);
            let n = &mut self.drawn_this_turn[p.index().min(3)];
            *n = n.saturating_add(1);
            // The move bumped before the count changed, and what it ran
            // may have cached characteristics since ("as long as you've
            // drawn two or more cards this turn").
            if self.turn_static_sources > 0 {
                self.cbump();
            }
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

    /// Tap `id` by a spell or ability `player` controls: "Whenever you tap
    /// an untapped creature an opponent controls" sees it.
    pub(super) fn tap_by(&mut self, id: ObjId, player: PlayerId) {
        if self.objects.get(id).is_none_or(|o| o.tapped) {
            return;
        }
        self.tap(id);
        if self.objects.get(id).is_some_and(|o| o.tapped) {
            self.emit(Event::TappedBy { obj: id, player });
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
            // CR 611.2b: "for as long as it remains tapped" may end.
            if self.linked_durations > 0 {
                self.sba_touch();
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

    /// A record of the turn that static conditions read without a zone
    /// change ("as long as you gained life this turn") got its first entry
    /// for some player: refresh characteristics if such a static is on the
    /// battlefield. At most a few times a turn.
    pub(super) fn history_bump(&mut self) {
        if self.turn_static_sources > 0
            && self
                .battlefield
                .iter()
                .any(|b| self.pool.get(self.objects[*b].def).history_static)
        {
            self.cbump();
        }
    }

    /// Change a life total directly (life loss, or a gain that has passed
    /// through the pipeline).
    pub(super) fn change_life(&mut self, p: PlayerId, delta: i32) {
        if delta == 0 {
            return;
        }
        let bit = 1 << p.index().min(7);
        if delta < 0 {
            let lost = &mut self.life_lost_this_turn[p.index().min(3)];
            *lost = lost.saturating_add(delta.unsigned_abs());
            if self.lost_life_this_turn & bit == 0 {
                self.lost_life_this_turn |= bit;
                self.history_bump();
            }
            if let Some(log) = &mut self.this_way {
                log.life_lost = log.life_lost.saturating_add(delta.unsigned_abs());
            }
        } else {
            let gained = &mut self.turn_log.life_gained[p.index().min(3)];
            *gained = gained.saturating_add(delta.unsigned_abs());
            if self.turn_log.gained_life & bit == 0 {
                self.turn_log.gained_life |= bit;
            }
            // "If you've gained 3 or more life this turn" may now hold.
            self.history_bump();
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

    /// The total toxic N among a source's abilities (CR 702.164c).
    fn toxic_total(&self, c: &crate::state::Characteristics) -> u32 {
        c.abilities
            .iter()
            .map(|a| match self.pool.ability(*a) {
                crate::card::AbilityDef::Toxic(n) => *n,
                _ => 0,
            })
            .sum()
    }

    /// The damage itself (CR 120.3). Lifelink applies to the damage dealt
    /// (CR 702.15); deathtouch is remembered for the next SBA check.
    #[allow(clippy::too_many_lines)] // one branch per kind of recipient
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
        if let Some(log) = &mut self.this_way {
            log.damage = log.damage.saturating_add(amount);
        }
        match target {
            Target::Player(p) => {
                // CR 702.16d: protection from a color (a player's, "you
                // gain protection from the color of your choice")
                // prevents damage from sources of that color.
                if !self.player_grants.is_empty() && self.player_protected_from(p, source) {
                    return;
                }
                if self.damaged_this_turn & (1 << p.index().min(7)) == 0 {
                    self.damaged_this_turn |= 1 << p.index().min(7);
                    self.history_bump();
                }
                if !combat && self.turn_log.noncombat_damaged & (1 << p.index().min(7)) == 0 {
                    self.turn_log.noncombat_damaged |= 1 << p.index().min(7);
                    self.history_bump();
                }
                self.note_damage_source(source);
                // CR 702.173a: an Assassin's combat damage enables
                // freerunning for its controller this turn.
                if combat
                    && let Some(a) = self.pool.assassin()
                    && let (Some(c), Some(ctl)) = (src.as_ref(), controller)
                    && c.is_creature()
                    && c.has_subtype(a)
                {
                    self.assassin_damage |= 1 << ctl.index().min(7);
                }
                self.emit(Event::Damage {
                    source,
                    target,
                    amount,
                    combat,
                });
                // CR 702.99a: an encoded creature's trigger.
                if combat && !self.encoded.is_empty() {
                    self.cipher_triggers(source);
                }
                if infect {
                    // CR 702.90b: damage to a player as poison counters.
                    self.add_poison(p, amount);
                } else {
                    let delta = -i32::try_from(amount).unwrap_or(i32::MAX);
                    self.change_life(p, delta);
                }
                // CR 702.164c: toxic adds poison counters with combat damage.
                if combat && let Some(c) = src.as_ref() {
                    let toxic = self.toxic_total(c);
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
                    && c.protected_from(
                        sc,
                        self.objects
                            .get(source)
                            .is_none_or(|s| s.controller != o.controller),
                        &self.pool,
                    )
                {
                    return;
                }
                // CR 120.4a: damage beyond lethal damage (beyond its
                // loyalty for a planeswalker) is excess, for "the excess
                // damage dealt this way".
                if let Some(lethal) = self
                    .this_way
                    .is_some()
                    .then(|| self.lethal_left(id, &c, deathtouch))
                    && let Some(log) = &mut self.this_way
                {
                    log.excess = log.excess.saturating_add(amount.saturating_sub(lethal));
                    if !log.damaged.contains(&id) {
                        log.damaged.push(id);
                    }
                }
                // CR 120.3c: damage to a planeswalker removes loyalty;
                // CR 120.3e: damage to a creature is marked on it.
                if c.is_planeswalker() {
                    self.remove_counters(id, CounterKind::Loyalty, amount);
                }
                // CR 310.6: damage to a battle removes defense counters.
                if c.is_battle() {
                    self.remove_counters(id, CounterKind::Defense, amount);
                }
                if c.is_creature() {
                    let o = &mut self.objects[id];
                    if !wither {
                        o.damage += amount;
                    }
                    if deathtouch {
                        o.deathtouch_damage = true;
                    }
                    if !self.damaged_by_this_turn.contains(&(source, id)) {
                        self.damaged_by_this_turn.push((source, id));
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

    /// The damage a permanent can be dealt before any more is excess (CR
    /// 120.4a): lethal damage to a creature, given the damage marked on it
    /// (1 from a source with deathtouch, CR 702.2c), its loyalty to a
    /// planeswalker; with both types the lesser, so the excess is the
    /// greatest.
    pub(super) fn lethal_left(
        &self,
        id: ObjId,
        c: &crate::state::Characteristics,
        deathtouch: bool,
    ) -> u32 {
        let o = &self.objects[id];
        let mut left = u32::MAX;
        if c.is_creature() {
            let t = u32::try_from(c.toughness().max(0)).unwrap_or(0);
            let lethal = t.saturating_sub(o.damage);
            left = left.min(if deathtouch { lethal.min(1) } else { lethal });
        }
        if c.is_planeswalker() {
            left = left.min(o.counters(CounterKind::Loyalty));
        }
        if c.is_battle() {
            left = left.min(o.counters(CounterKind::Defense));
        }
        left
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
        let before = o.counters(kind);
        o.add_counters(kind, n);
        let by = self.effect_controller.unwrap_or(o.controller);
        if kind == CounterKind::PlusOnePlusOne {
            self.turn_log.counter_put |= 1 << o.controller.index().min(7);
            self.turn_log.counter_put_by |= 1 << by.index().min(7);
        }
        self.cbump();
        // "if you put a counter on a creature this turn": asked once per
        // player each turn, after the bump (the counters may change what
        // the permanent is).
        let bit = 1 << by.index().min(7);
        if self.turn_log.any_counter_put_by & bit == 0
            && self.characteristics(id).is_some_and(|c| c.is_creature())
        {
            self.turn_log.any_counter_put_by |= bit;
        }
        self.emit(Event::CountersAdded {
            obj: id,
            kind,
            n,
            by,
            before,
        });
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
        let mayhem = self.pool.get(self.objects[card].def).mayhem.is_some();
        let new = self.move_object(card, Zone::Graveyard, true);
        if mayhem
            && self
                .objects
                .get(new)
                .is_some_and(|o| o.zone == Zone::Graveyard)
        {
            self.mayhem_discarded.push(new);
        }
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
        // "Remove a counter from this creature": one at a time, of the kind
        // the engine picks for the player (-1/-1 first, then kinds that
        // don't change power or toughness, then the rest).
        if kind == CounterKind::Any {
            for _ in 0..n {
                let Some(o) = self.objects.get(id) else {
                    return;
                };
                let has = |k: CounterKind| o.counters(k) > 0;
                let pick = if has(CounterKind::MinusOneMinusOne) {
                    Some(CounterKind::MinusOneMinusOne)
                } else {
                    o.counters
                        .iter()
                        .filter(|(_, c)| *c > 0)
                        .map(|(k, _)| *k)
                        .min_by_key(|k| (k.pt().is_some(), *k == CounterKind::PlusOnePlusOne))
                };
                let Some(k) = pick else {
                    return;
                };
                self.remove_counters(id, k, 1);
            }
            return;
        }
        let Some(o) = self.objects.get_mut(id) else {
            return;
        };
        let n = n.min(o.counters(kind));
        if o.zone != Zone::Battlefield || n == 0 {
            return;
        }
        o.remove_counters(kind, n);
        let defeated = kind == CounterKind::Defense && o.counters(kind) == 0;
        self.cbump();
        self.emit(Event::CountersRemoved { obj: id, kind, n });
        // CR 310.12b: a Siege's intrinsic "When the last defense counter is
        // removed from this permanent, exile it, then you may cast it
        // transformed without paying its mana cost."
        if defeated
            && let Some(c) = self.characteristics(id)
            && c.is_battle()
            && c.has_subtype(crate::card::Subtype::SIEGE)
        {
            let ability = self.pool.builtin(crate::card::Builtin::SiegeDefeated);
            let controller = self.objects[id].controller;
            self.queue_builtin(controller, ability, id, c.def);
        }
    }

    /// Unlock a Room's door (CR 709.5e-f): the permanent takes the
    /// definition of its new set of unlocked doors.
    /// A Class gains a level or a Case becomes solved (CR 716.2a,
    /// 719.3a): the permanent becomes that stage of its card, if it is at
    /// an earlier one. It stays the same object.
    pub(super) fn become_stage(&mut self, id: ObjId, stage: u8) {
        let Some(o) = self.objects.get(id) else {
            return;
        };
        if o.zone != Zone::Battlefield {
            return;
        }
        let stages = &self.pool.get(o.def).stages;
        let (Some(now), Some(&to)) = (
            stages.iter().position(|s| *s == o.def),
            stages.get(usize::from(stage)),
        ) else {
            return;
        };
        if now >= usize::from(stage) {
            return;
        }
        let flags = self.flags_of(&self.objects[id]);
        let legendary = self.pool.get(self.objects[id].def).is_legendary();
        self.count_sources(flags, legendary, false);
        self.objects[id].def = to;
        let flags = self.flags_of(&self.objects[id]);
        self.count_sources(flags, legendary, true);
        self.cbump();
        self.emit(Event::StageChanged { obj: id, stage });
    }

    pub(super) fn unlock(&mut self, id: ObjId, door: u8) {
        let Some(o) = self.objects.get(id) else {
            return;
        };
        let Some(doors) = self.pool.get(o.def).doors.as_deref().copied() else {
            return;
        };
        let mut unlocked = doors.unlocked(o.def);
        if unlocked[usize::from(door)] {
            return;
        }
        unlocked[usize::from(door)] = true;
        let flags = self.flags_of(&self.objects[id]);
        self.count_sources(flags, false, false);
        self.objects[id].def = doors.def_for(unlocked);
        let flags = self.flags_of(&self.objects[id]);
        self.count_sources(flags, false, true);
        self.cbump();
        self.emit(Event::Unlocked {
            obj: id,
            door,
            fully: unlocked == [true, true],
        });
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
        // "That creature can't be regenerated this turn".
        let regenerate = regenerate && (self.no_regen.is_empty() || !self.no_regen.contains(&id));
        self.propose(Proposed::Destroy {
            obj: id,
            regenerate,
        });
    }

    /// CR 702.26b: `id` phases out, and what's attached to it with it
    /// (`indirect`, CR 702.26g). It stays the same object, out of the
    /// battlefield list, so nothing sees it; it leaves combat. No zone
    /// change happens and nothing triggers (CR 702.26d).
    pub(super) fn phase_out(&mut self, id: ObjId, indirect: bool) {
        let Some(o) = self.objects.get(id) else {
            return;
        };
        if o.zone != Zone::Battlefield {
            return;
        }
        let controller = o.controller;
        let attached: SmallVec<[ObjId; 2]> = self
            .battlefield
            .iter()
            .copied()
            .filter(|a| self.objects[*a].attached_to == Some(id))
            .collect();
        self.remove_from_combat(id);
        // The board counts are of the battlefield list, which it leaves.
        let (flags, legend) = {
            let o = &self.objects[id];
            (self.flags_of(o), self.pool.get(o.def).is_legendary())
        };
        self.count_sources(flags, legend, false);
        self.battlefield.retain(|x| *x != id);
        self.objects[id].zone = Zone::PhasedOut;
        self.phased_out.push(crate::state::PhasedOut {
            id,
            controller,
            indirect,
        });
        for a in attached {
            self.phase_out(a, true);
        }
        self.cbump();
    }

    /// CR 702.26a: the permanents that phased out under `p`'s control
    /// phase in, with those that phased out with them (CR 702.26g);
    /// returns them.
    pub(super) fn phase_in_all(&mut self, p: PlayerId) -> SmallVec<[ObjId; 2]> {
        let mut back: SmallVec<[ObjId; 2]> = self
            .phased_out
            .iter()
            .filter(|x| x.controller == p && !x.indirect)
            .map(|x| x.id)
            .collect();
        let mut i = 0;
        while i < back.len() {
            let id = back[i];
            for x in &self.phased_out {
                if x.indirect && self.objects.get(x.id).and_then(|o| o.attached_to) == Some(id) {
                    back.push(x.id);
                }
            }
            i += 1;
        }
        self.phased_out.retain(|x| !back.contains(&x.id));
        for id in &back {
            if let Some(o) = self.objects.get_mut(*id) {
                o.zone = Zone::Battlefield;
                self.battlefield.push(*id);
                let (flags, legend) = {
                    let o = &self.objects[*id];
                    (self.flags_of(o), self.pool.get(o.def).is_legendary())
                };
                self.count_sources(flags, legend, true);
            }
        }
        if !back.is_empty() {
            self.cbump();
        }
        back
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
        self.note_moved_how(id, false);
    }
}
