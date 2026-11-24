//! How a cast begins (CR 601.2a): every way to cast a card is a
//! `CastWay`, and `begin_way` moves the card to the stack recording the
//! same facts for each (where it was cast from, the alternative used, the
//! face it is cast as).

use crate::card::{CardRef, Layout};
use crate::decision::Choice;
use crate::ids::{ObjId, PlayerId};
use crate::state::{AltCast, Flow, GameState, Zone};

/// One way to cast a card: what the cast records on the spell.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::struct_excessive_bools)] // one per fact the spell records
pub(super) struct CastWay {
    /// The card (or the copy) cast.
    pub card: ObjId,
    /// The alternative used (`AltCast`).
    pub alt: AltCast,
    /// The characteristics the spell has, if not the card's: a face (an
    /// adventure, a split half, a bestowed Aura, CR 715.3, 709.3,
    /// 702.103b) or the face-down 2/2 (CR 708.2a).
    pub face: Option<CardRef>,
    /// Cast for its evoke cost (CR 702.74a).
    pub evoked: bool,
    /// Cast with flashback, or otherwise exiled as it leaves the stack
    /// (aftermath, harmonize; CR 702.34a, 702.127a, 702.180a).
    pub flashback: bool,
    /// An adventurer's or omen's spell (CR 715.3).
    pub adventure: bool,
}

impl CastWay {
    pub(super) fn new(card: ObjId, alt: AltCast) -> Self {
        CastWay {
            card,
            alt,
            face: None,
            evoked: false,
            flashback: false,
            adventure: false,
        }
    }

    fn face(self, face: Option<CardRef>) -> Self {
        CastWay { face, ..self }
    }
}

impl GameState {
    /// The way a priority action casts its card, or `None` if it isn't
    /// one cast from where the card is (a land played as a face, CR
    /// 712.12; a prepared permanent's copy, cast by `cast_prepared`).
    pub(super) fn way_of(&self, player: PlayerId, choice: &Choice) -> Option<CastWay> {
        let def_of = |card: ObjId| self.pool.get(self.objects[card].def);
        Some(match *choice {
            Choice::CastEvoke(card) => CastWay {
                evoked: true,
                ..CastWay::new(card, AltCast::None)
            },
            Choice::CastDash(card) => CastWay::new(card, AltCast::Dash),
            Choice::CastSneak(card) => CastWay::new(card, AltCast::Sneak),
            Choice::CastImpending(card) => CastWay::new(card, AltCast::Impending),
            Choice::CastWebSlinging(card) => CastWay::new(card, AltCast::WebSlinging),
            Choice::CastWarp(card) => CastWay::new(card, AltCast::Warp),
            Choice::CastFree(card) => CastWay::new(card, AltCast::Free),
            // CR 702.180a: from the graveyard, exiled as it leaves the
            // stack (as with flashback).
            Choice::CastHarmonize { card, .. } => CastWay {
                flashback: true,
                ..CastWay::new(card, AltCast::Harmonize)
            },
            // CR 702.103b: on the stack it is the Aura, enchant creature,
            // from the start.
            Choice::CastBestow(card) => {
                CastWay::new(card, AltCast::Bestow).face(def_of(card).bestow_face)
            }
            // CR 702.96b: on the stack it is the spell without targets.
            Choice::CastOverload(card) => {
                CastWay::new(card, AltCast::Overload).face(def_of(card).overload_face)
            }
            // CR 702.113a: the spell with the land target and the awaken
            // effects after its own.
            Choice::CastAwaken(card) => {
                CastWay::new(card, AltCast::Awaken).face(def_of(card).awaken_face)
            }
            // CR 702.146a, 712.8c: cast transformed, the back face's
            // characteristics on the stack.
            Choice::CastDisturb(card) => {
                CastWay::new(card, AltCast::Disturb).face(def_of(card).faces.first().copied())
            }
            // CR 718.3: a prototype card's alternative cost casts it
            // prototyped, the spell its prototype face; CR 702.148a: a
            // cleaved spell is its cleave face.
            Choice::CastAlt(card) => {
                let def = def_of(card);
                CastWay::new(card, AltCast::AltCost).face(def.prototype_face.or(def.cleave_face))
            }
            // CR 708.2a: the spell has only face-down characteristics.
            Choice::CastFaceDown(card) => {
                let ward = matches!(
                    def_of(card).morph.as_deref(),
                    Some((_, crate::card::MorphKind::Disguise))
                );
                CastWay::new(card, AltCast::None).face(Some(self.pool.face_down(ward)))
            }
            Choice::CastFace { card, face } => {
                if self.objects[card].zone == Zone::Battlefield
                    || self.land_face(card, face).is_some()
                {
                    return None;
                }
                let main = def_of(card);
                CastWay {
                    adventure: matches!(main.layout, Layout::Adventure | Layout::Omen),
                    ..CastWay::new(card, AltCast::None)
                }
                .face(main.faces.get(usize::from(face)).copied())
            }
            Choice::Cast(id) => self.cast_way_of(player, id),
            _ => return None,
        })
    }

    /// The way `Choice::Cast` casts `id`, from where it is.
    fn cast_way_of(&self, player: PlayerId, id: ObjId) -> CastWay {
        let o = &self.objects[id];
        // CR 702.127a: a split card in the graveyard is cast as its
        // aftermath half, and exiled as it leaves the stack (as with
        // flashback).
        if o.zone == Zone::Graveyard
            && let Some(i) = self.aftermath_index(id)
        {
            return CastWay {
                flashback: true,
                ..CastWay::new(id, AltCast::None)
            }
            .face(self.pool.get(o.def).faces.get(usize::from(i)).copied());
        }
        // "If you cast a spell this way, pay life equal to its mana value
        // rather than pay its mana cost." (CR 118.9): only when no
        // permission to play the top card lets it be cast for its mana cost.
        if o.zone == Zone::Library {
            let perms = self.top_permissions(player, id);
            let alt = if !perms.is_empty() && perms.iter().all(|t| t.life) {
                AltCast::PayLife
            } else {
                AltCast::None
            };
            return CastWay::new(id, alt);
        }
        // CR 702.34a: a card cast from the graveyard is cast with
        // flashback.
        let gdef = self.pool.get(o.def);
        // "You may cast this card from your graveyard" given until end of
        // turn: for its mana cost, and not exiled (a grant of flashback
        // too would exile it, so the plain cast is taken).
        if o.zone == Zone::Graveyard
            && gdef.flashback.is_none()
            && self.granted_graveyard_cast.contains(&id)
        {
            return CastWay::new(id, AltCast::None);
        }
        // Flashback granted until end of turn: for its mana cost.
        // Flashback a permanent gives it (CR 113.6b): for its granted cost.
        let live = o.zone == Zone::Graveyard && self.card_grants_live();
        let escape = gdef.flashback.is_none()
            && !self.granted_flashback.contains(&id)
            && !(live && self.granted_flashback(id).is_some());
        // Jump-start or retrace a permanent gives it (CR 702.133a,
        // 702.81a): for its mana cost and a card discarded.
        if escape
            && live
            && let Some(alt) = self.granted_discard_way(id)
        {
            return CastWay {
                flashback: alt == AltCast::JumpStart,
                ..CastWay::new(id, alt)
            };
        }
        // Mayhem: a card discarded this turn, cast for its mayhem cost (a
        // card with escape too uses escape only when it wasn't discarded
        // this turn).
        let mayhem = escape
            && (gdef.mayhem.is_some() || live && self.granted_mayhem(id).is_some())
            && self.mayhem_discarded.contains(&id);
        let flashback = o.zone == Zone::Graveyard && !escape;
        let alt = match (o.zone, o.alt) {
            (Zone::Hand, _) => AltCast::Hand,
            (Zone::Graveyard, _) if mayhem => AltCast::Mayhem,
            (Zone::Graveyard, _) if escape => AltCast::Escape,
            (Zone::Exile, AltCast::Foretold) => AltCast::Foretell,
            // CR 702.170d: without paying its mana cost.
            (Zone::Exile, AltCast::Plotted) => AltCast::Free,
            // "You may cast that card without paying its mana cost."
            (Zone::Exile, _)
                if self.exile_casts.as_ref().is_some_and(|e| {
                    e.playable
                        .iter()
                        .any(|pe| pe.card == id && pe.player == player && pe.free)
                }) =>
            {
                AltCast::Free
            }
            // CR 701.65a: for {2}.
            (Zone::Exile, AltCast::Airbent) => AltCast::Airbend,
            _ => AltCast::None,
        };
        CastWay {
            flashback,
            ..CastWay::new(id, alt)
        }
    }

    /// Cast `choice`'s card the way it names: what the choice uses up (a
    /// once-each-turn permission, the creature tapped for harmonize), then
    /// `begin_way`.
    pub(super) fn cast_choice(&mut self, player: PlayerId, choice: &Choice) {
        let Some(way) = self.way_of(player, choice) else {
            return;
        };
        match *choice {
            // "You may cast spells from your hand without paying their
            // mana costs." (CR 118.9); a once-each-turn permission is used.
            Choice::CastFree(card) => {
                if let Some(super::casting::FreeUse::Once(src, a)) =
                    self.free_cast_permission(player, card)
                {
                    self.activated_once.push((src, a));
                }
            }
            // "Once each turn, you may cast ... from the top of your
            // library": that permission is used unless another lets the
            // card be cast.
            Choice::Cast(id) if self.objects[id].zone == Zone::Library => {
                let perms = self.top_permissions(player, id);
                if !perms.iter().any(|t| !t.once)
                    && let Some((src, a)) = perms.first().and_then(|t| t.src)
                {
                    self.activated_once.push((src, a));
                }
            }
            _ => {}
        }
        // CR 702.180b: the creature to tap is chosen now, and its power
        // comes off the cost.
        let tapped = match *choice {
            Choice::CastHarmonize { tap: Some(c), .. } => Some((
                c,
                self.characteristics(c)
                    .map_or(0, |ch| u32::try_from(ch.power.max(0)).unwrap_or(0)),
            )),
            _ => None,
        };
        let spell = self.begin_way(player, way);
        if let Some((c, power)) = tapped {
            self.harmonized.push((spell, c, power));
        }
    }

    /// CR 601.2a: the card moves to the stack, cast `way`: where it was
    /// cast from ("if you cast it from your hand"), the alternative, the
    /// face, evoke, flashback, and adventure are recorded alike for every
    /// way; modes come before X, X before targets (CR 601.2b). The spell.
    pub(super) fn begin_way(&mut self, player: PlayerId, way: CastWay) -> ObjId {
        self.consecutive_passes = 0;
        let card = way.card;
        let from = self.objects[card].zone;
        // CR 307.1: whether a sorcery could have been cast now.
        let sorcery_timing =
            player == self.active_player && self.step.is_main() && self.stack.is_empty();
        // A copy of a card cast from exile (cipher, CR 702.99a) is a copy
        // on the stack too (CR 707.12).
        let copy = self.objects[card].cast.copy();
        // CR 601.3: what the permission it is cast by adds to its cost,
        // kept for the payment (`spell_basis`).
        let rider = match &self.exile_casts {
            Some(e) if e.riders => self.permission_rider(player, card),
            _ => crate::card::CastRider::default(),
        };
        let spell = self.move_object(card, Zone::Stack, true);
        if !rider.is_none()
            && let Some(e) = &mut self.exile_casts
        {
            let objects = &self.objects;
            e.rider_spells
                .retain(|(s, _)| objects.get(*s).is_some_and(|o| o.zone == Zone::Stack));
            e.rider_spells.push((spell, rider));
        }
        if copy {
            self.objects[spell].cast = self.objects[spell].cast.copied();
        }
        self.objects[spell].controller = player;
        self.objects[spell].alt = way.alt;
        if let Some(d) = way.face {
            self.objects[spell].def = d;
            self.cbump();
        }
        let cast = &mut self.objects[spell].cast;
        cast.set_cast_from(from);
        if way.evoked {
            cast.set_evoked(true);
        }
        if way.flashback {
            cast.set_flashback(true);
        }
        if way.adventure {
            cast.set_adventure(true);
        }
        let def = self.pool.get(self.objects[spell].def);
        if def.flash_cleanup && !sorcery_timing {
            self.objects[spell].cast.set_instant_timing();
        }
        // "If you cast this spell during your main phase": judged now.
        if let Some(c) = def.cast_snapshot
            && self.condition_now(self.pool.condition(c), player, spell)
        {
            self.objects[spell].cast.set_as_cast();
        }
        self.flow = if def.modes().is_some() {
            Flow::ChoosingMode {
                spell,
                sacrifice: None,
                discard: None,
            }
        } else {
            self.after_mode(spell)
        };
        spell
    }
}
