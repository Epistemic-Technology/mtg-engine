//! Priority actions, casting (CR 601), mana payment, and resolution (CR 608).

use smallvec::SmallVec;

use crate::card::{
    AbilityDef, AbilityId, ActivatedAbility, ActivatedKeyword, Affects, CardDest, CardSet,
    ChoiceKind, Cost, CounterKind, Effect, EffectCond, Keyword, KeywordSet, Layout, Modification,
    PtValue, Recipient, Replacement, Replaces, SacCost, SearchTo, Subtype, TargetSpec,
};
use crate::decision::{Choice, DecisionKind};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::mana::{Color, ColorSet, Mana, ManaRequirement};
use crate::state::{
    Affected, AltCast, CastInfo, Chosen, ContinuousEffect, DelayedTrigger, Duration, GameObject,
    GameState, ObjKind, ReplacementEffect, RuleEffect, StackAbility, Target, Zone,
};

impl GameState {
    // ----- priority actions --------------------------------------------------------

    #[allow(clippy::too_many_lines)] // one block per zone cast from
    pub(super) fn priority_options(&self, p: PlayerId) -> Vec<Choice> {
        // Room for a typical listing, so it isn't regrown push by push.
        let mut options = Vec::with_capacity(8);
        let sorcery_timing =
            p == self.active_player && self.step.is_main() && self.stack.is_empty();
        let ps = self.player(p);
        let sources = self.mana_sources(p);
        let pool = ps.mana_pool;
        let changers = self.cost_changers();
        for id in &ps.hand {
            let def = self.pool.get(self.objects[*id].def);
            // Abilities activated from the hand (cycling, CR 702.29) cost
            // mana, or life ("Cycling—Pay 2 life.", CR 119.4); instant
            // speed unless "Activate only as a sorcery" (transmute, CR
            // 702.53a).
            if self.flags_of(&self.objects[*id]).has_activated() {
                for a in &def.ability_ids {
                    if let AbilityDef::Activated(ab) = self.pool.ability(*a)
                        && ab.from_hand
                        && (!ab.sorcery_speed || sorcery_timing)
                        && self.activation_allowed(ab, *a, p, *id)
                        // "only once each turn" (forecast, CR 702.57b).
                        && (ab.per_turn == 0
                            || self
                                .activated_once
                                .iter()
                                .filter(|x| **x == (*id, *a))
                                .count()
                                < usize::from(ab.per_turn))
                        && (ab.cost.life == 0
                            || ps.life >= i32::try_from(ab.cost.life).unwrap_or(i32::MAX))
                        && self.payable_for(
                            p,
                            &sources,
                            &ab.cost.mana.requirements(0),
                            Spend::Ability(self.characteristics(*id).as_ref()),
                        )
                        && self.targets_available(p, &ab.targets, Some(*id))
                    {
                        // CR 702.49a: one option per unblocked attacker
                        // that could be returned.
                        if ab.keyword == Some(ActivatedKeyword::Ninjutsu) {
                            for c in self.unblocked_attackers(p) {
                                options.push(Choice::Activate {
                                    source: *id,
                                    ability: *a,
                                    sacrifice: Some(c),
                                    discard: None,
                                });
                            }
                            continue;
                        }
                        options.push(Choice::Activate {
                            source: *id,
                            ability: *a,
                            sacrifice: None,
                            discard: None,
                        });
                    }
                }
            }
            // CR 712.11b, 712.12: a modal double-faced card's back face is
            // played or cast instead.
            if def.layout == Layout::ModalDfc
                && let Some(&back) = def.faces.first()
            {
                self.back_face_option(
                    p,
                    *id,
                    back,
                    sorcery_timing,
                    &sources,
                    &changers,
                    &mut options,
                );
            }
            if def.is_land() {
                if sorcery_timing && self.may_play_another_land(p) {
                    options.push(Choice::PlayLand(*id));
                }
                // CR 702.168a, 708.4: a land card with disguise (Branch of
                // Vitu-Ghazi) may be cast face down as a 2/2 creature.
                if def.morph.is_some() && sorcery_timing {
                    self.face_down_option(p, *id, &sources, &mut options);
                }
                // CR 715.3: a land with an Adventure (Midgar, City of Mako
                // // Reactor Raid) is played as a land or cast as the
                // Adventure.
                if matches!(def.layout, Layout::Adventure | Layout::Omen) {
                    for (i, f) in def.faces.iter().enumerate() {
                        let face = self.pool.get(*f);
                        if self.castable(p, face, *id, sorcery_timing, &sources, &pool, &changers) {
                            options.push(Choice::CastFace {
                                card: *id,
                                face: u8::try_from(i).unwrap_or(0),
                            });
                        }
                    }
                }
                continue;
            }
            // "Cast this spell only during combat" (CR 601.3).
            if let Some(c) = def.cast_only
                && !self.condition_now(self.pool.condition(c), p, *id)
            {
                continue;
            }
            // CR 715.3, 709.3: an adventurer's spell or a split card's
            // half is cast as that face; a split card with fuse, both
            // halves as its third face (CR 702.102a).
            if !def.faces.is_empty()
                && matches!(def.layout, Layout::Adventure | Layout::Omen | Layout::Split)
            {
                for (i, f) in def.faces.iter().enumerate() {
                    let face = self.pool.get(*f);
                    // CR 702.127a: an aftermath half only from a graveyard.
                    if face.keywords.contains(Keyword::Aftermath) {
                        continue;
                    }
                    if self.castable(p, face, *id, sorcery_timing, &sources, &pool, &changers) {
                        options.push(Choice::CastFace {
                            card: *id,
                            face: u8::try_from(i).unwrap_or(0),
                        });
                    }
                }
                if def.layout == Layout::Split {
                    continue;
                }
            }
            // Timing first, as `castable` does: off-timing cards, most of
            // the hand at most priorities, skip the call.
            if (sorcery_timing || def.is_instant() || self.has_flash(p, def, *id))
                && self.castable(p, def, *id, sorcery_timing, &sources, &pool, &changers)
                && self.undone_cast != Some((*id, self.version))
            {
                options.push(Choice::Cast(*id));
            }
            // Most cards have no other way to be cast from the hand.
            if !def.other_casts {
                continue;
            }
            // CR 702.74a: cast for the evoke cost instead.
            if let Some(ev) = &def.evoke
                && sorcery_timing
                && self.payable_for(
                    p,
                    &sources,
                    &ev.requirements(0),
                    Spend::Spell(&self.printed(self.objects[*id].def)),
                )
                && self.targets_available(p, def.unkicked_cast_targets(), Some(*id))
            {
                options.push(Choice::CastEvoke(*id));
            }
            // CR 118.9: an alternative cost of the card's own.
            if def.alt_cost.is_some()
                && self.alt_castable(p, def, *id, sorcery_timing, &sources, &changers)
            {
                options.push(Choice::CastAlt(*id));
            }
            // CR 702.109a: cast for the dash cost instead.
            if let Some(dash) = &def.dash
                && (sorcery_timing || self.has_flash(p, def, *id))
                && self.payable_for(
                    p,
                    &sources,
                    &dash.requirements(0),
                    Spend::Spell(&self.printed(self.objects[*id].def)),
                )
                && self.targets_available(p, def.unkicked_cast_targets(), Some(*id))
            {
                options.push(Choice::CastDash(*id));
            }
            // CR 702.185a: cast for the warp cost instead.
            if let Some(warp) = &def.warp
                && (sorcery_timing || self.has_flash(p, def, *id))
                && self.payable_for(
                    p,
                    &sources,
                    &warp.requirements(0),
                    Spend::Spell(&self.printed(self.objects[*id].def)),
                )
                && self.targets_available(p, def.unkicked_cast_targets(), Some(*id))
            {
                options.push(Choice::CastWarp(*id));
            }
            // CR 702.190a: cast for the sneak cost during its controller's
            // declare blockers step, an unblocked attacker to return.
            if def.sneak.is_some() && self.sneak_castable(p, def, *id, &sources) {
                options.push(Choice::CastSneak(*id));
            }
            // CR 702.188a: cast for the web-slinging cost, a tapped
            // creature to return.
            if def.web_slinging.is_some()
                && self.web_slinging_castable(p, def, *id, sorcery_timing, &sources)
            {
                options.push(Choice::CastWebSlinging(*id));
            }
            // CR 702.176a: cast for the impending cost.
            if let Some(imp) = def.impending.as_deref()
                && (sorcery_timing || self.has_flash(p, def, *id))
                && self.payable_for(
                    p,
                    &sources,
                    &imp.1.requirements(0),
                    Spend::Spell(&self.printed(self.objects[*id].def)),
                )
                && self.additional_payable(p, &def.additional, *id)
                && self.targets_available(p, def.unkicked_cast_targets(), Some(*id))
            {
                options.push(Choice::CastImpending(*id));
            }
            // CR 702.103a: cast for the bestow cost, as an Aura spell.
            if let Some(face) = def.bestow_face
                && self.bestow_castable(p, def, face, *id, sorcery_timing, &sources, &changers)
            {
                options.push(Choice::CastBestow(*id));
            }
            // CR 702.96a: cast for the overload cost, without targets.
            if let Some(face) = def.overload_face
                && self.overload_castable(p, def, face, *id, sorcery_timing, &sources, &changers)
            {
                options.push(Choice::CastOverload(*id));
            }
            // CR 702.113a: cast for the awaken cost.
            if let Some(face) = def.awaken_face
                && self.awaken_castable(p, def, face, *id, sorcery_timing, &sources, &changers)
            {
                options.push(Choice::CastAwaken(*id));
            }
            // CR 702.37c: face down as a 2/2 creature spell for {3}.
            if def.morph.is_some() && sorcery_timing {
                self.face_down_option(p, *id, &sources, &mut options);
            }
            if def.suspend.is_some() || def.foretell.is_some() || def.plot.is_some() {
                self.exile_actions(p, *id, sorcery_timing, &sources, &mut options);
            }
        }
        // CR 116.2b: turning a face-down permanent face up.
        if self.face_down_count > 0 {
            self.face_up_options(p, &sources, &mut options);
        }
        // CR 722.3c: a copy of a prepared permanent's prepare spell.
        if self.prepared_ever > 0 {
            self.prepared_options(p, sorcery_timing, &sources, &changers, &mut options);
        }
        // CR 709.5e: unlocking a Room's door, as a sorcery.
        if self.room_count > 0 && sorcery_timing {
            self.unlock_options(p, &sources, &mut options);
        }
        // CR 715.3d: a card on an adventure may be cast from exile (its
        // main face).
        if ps.adventures_in_exile > 0 {
            for id in &ps.exile {
                let o = &self.objects[*id];
                // CR 702.143a: a foretold card, on a later turn, for its
                // foretell cost.
                if o.alt == AltCast::Foretold {
                    let def = self.pool.get(o.def);
                    let timing_ok =
                        sorcery_timing || def.is_instant() || self.has_flash(p, def, *id);
                    if timing_ok
                        && !self
                            .exile_casts
                            .as_ref()
                            .is_some_and(|e| e.foretold_this_turn.contains(id))
                        && self.exile_castable(p, *id, AltCast::Foretell)
                    {
                        options.push(Choice::Cast(*id));
                    }
                    continue;
                }
                // CR 701.65a: an airbent card, for {2}, with its usual
                // timing; a land can't be played this way.
                if o.alt == AltCast::Airbent {
                    let def = self.pool.get(o.def);
                    let timing_ok =
                        sorcery_timing || def.is_instant() || self.has_flash(p, def, *id);
                    if timing_ok && !def.is_land() && self.exile_castable(p, *id, AltCast::Airbend)
                    {
                        options.push(Choice::Cast(*id));
                    }
                    continue;
                }
                // CR 702.170d: a plotted card, free, during its owner's
                // main phase with the stack empty, on a later turn.
                if o.alt == AltCast::Plotted {
                    if sorcery_timing
                        && !self
                            .exile_casts
                            .as_ref()
                            .is_some_and(|e| e.foretold_this_turn.contains(id))
                        && self.exile_castable(p, *id, AltCast::Free)
                    {
                        options.push(Choice::Cast(*id));
                    }
                    continue;
                }
                if !o.cast.adventure() {
                    continue;
                }
                let def = self.pool.get(o.def);
                // CR 715.3d: a land on an adventure is played, not cast.
                if def.is_land() {
                    if sorcery_timing && self.may_play_another_land(p) {
                        options.push(Choice::PlayLand(*id));
                    }
                    continue;
                }
                if self.castable(p, def, *id, sorcery_timing, &sources, &pool, &changers) {
                    options.push(Choice::Cast(*id));
                }
            }
        }
        // Impulse draw: exiled cards the player may play, paying their
        // costs, with the usual timing and land-play allowance (CR 305.2,
        // 601.3).
        if let Some(e) = &self.exile_casts
            && !e.playable.is_empty()
        {
            for pe in &e.playable {
                let Some(o) = self.objects.get(pe.card) else {
                    continue;
                };
                // CR 702.185a: a warped card, not until a later turn.
                if pe.player != p
                    || o.zone != Zone::Exile
                    || pe.own_ends == crate::state::WARP_LATER
                {
                    continue;
                }
                // "for as long as you control Lightning".
                if pe.until == crate::card::PlayUntil::WhileYouControlSource
                    && !self
                        .objects
                        .get(pe.source)
                        .is_some_and(|s| s.zone == Zone::Battlefield && s.controller == p)
                {
                    continue;
                }
                let def = self.pool.get(o.def);
                if def.layout == Layout::ModalDfc
                    && let Some(&back) = def.faces.first()
                    && !options.contains(&Choice::CastFace {
                        card: pe.card,
                        face: 0,
                    })
                {
                    self.back_face_option(
                        p,
                        pe.card,
                        back,
                        sorcery_timing,
                        &sources,
                        &changers,
                        &mut options,
                    );
                }
                if def.is_land() {
                    if sorcery_timing
                        && self.may_play_another_land(p)
                        && !pe.cast_only
                        && !options.contains(&Choice::PlayLand(pe.card))
                    {
                        options.push(Choice::PlayLand(pe.card));
                    }
                    continue;
                }
                // "... without paying its mana cost", with its usual
                // timing.
                if pe.free {
                    if (sorcery_timing || def.is_instant() || self.has_flash(p, def, pe.card))
                        && self.exile_castable(p, pe.card, AltCast::Free)
                        && !options.contains(&Choice::Cast(pe.card))
                    {
                        options.push(Choice::Cast(pe.card));
                    }
                    continue;
                }
                if !def.faces.is_empty()
                    && matches!(def.layout, Layout::Adventure | Layout::Omen | Layout::Split)
                {
                    for (i, f) in def.faces.iter().enumerate() {
                        let face = self.pool.get(*f);
                        // CR 702.127a, 702.102a: an aftermath half only
                        // from a graveyard, fused halves only from a hand.
                        if face.keywords.contains(Keyword::Aftermath)
                            || (def.layout == Layout::Split && i >= 2)
                        {
                            continue;
                        }
                        if self.castable(
                            p,
                            face,
                            pe.card,
                            sorcery_timing,
                            &sources,
                            &pool,
                            &changers,
                        ) {
                            options.push(Choice::CastFace {
                                card: pe.card,
                                face: u8::try_from(i).unwrap_or(0),
                            });
                        }
                    }
                    if def.layout == Layout::Split {
                        continue;
                    }
                }
                if self.castable(p, def, pe.card, sorcery_timing, &sources, &pool, &changers)
                    && !options.contains(&Choice::Cast(pe.card))
                {
                    options.push(Choice::Cast(pe.card));
                }
            }
        }
        // CR 607.2a, 601.3: cards exiled with a permanent whose ability
        // lets its controller play them ("You may cast spells from among
        // cards exiled with this creature").
        if !self.linked_exiles.is_empty() {
            for (card, land_ok, spell_ok) in self.exiled_permissions(p) {
                let def = self.pool.get(self.objects[card].def);
                if def.is_land() {
                    if land_ok
                        && sorcery_timing
                        && self.may_play_another_land(p)
                        && !options.contains(&Choice::PlayLand(card))
                    {
                        options.push(Choice::PlayLand(card));
                    }
                } else if spell_ok
                    && !options.contains(&Choice::Cast(card))
                    && self.castable(p, def, card, sorcery_timing, &sources, &pool, &changers)
                {
                    options.push(Choice::Cast(card));
                }
            }
        }
        // CR 305.1: "You may play lands from your graveyard."
        if self.player_static_sources > 0
            && sorcery_timing
            && self.may_play_another_land(p)
            && self.lands_from_graveyard(p)
        {
            for id in &ps.graveyard {
                if self.pool.get(self.objects[*id].def).is_land()
                    && !options.contains(&Choice::PlayLand(*id))
                {
                    options.push(Choice::PlayLand(*id));
                }
            }
        }
        // CR 601.3, 305.1: the top card of the library, by a permanent's
        // permission ("You may cast creature spells from the top of your
        // library").
        if self.top_play_sources > 0
            && let Some(&top) = ps.library.last()
            && !self.top_permissions(p, top).is_empty()
        {
            let def = self.pool.get(self.objects[top].def);
            if def.is_land() {
                if sorcery_timing && ps.lands_played_this_turn < 1 {
                    options.push(Choice::PlayLand(top));
                }
            } else if self.castable(p, def, top, sorcery_timing, &sources, &pool, &changers) {
                options.push(Choice::Cast(top));
            }
        }
        // CR 702.34a: a card given flashback ("The flashback cost is
        // equal to its mana cost"), with its usual timing.
        for id in &self.granted_flashback {
            let Some(o) = self.objects.get(*id) else {
                continue;
            };
            if o.zone != Zone::Graveyard || o.owner != p || options.contains(&Choice::Cast(*id)) {
                continue;
            }
            let def = self.pool.get(o.def);
            if def.flashback.is_none()
                && self.castable(p, def, *id, sorcery_timing, &sources, &pool, &changers)
            {
                options.push(Choice::Cast(*id));
            }
        }
        // CR 702.180a: a card given harmonize, its harmonize cost its mana
        // cost.
        for id in &self.granted_harmonize {
            let Some(o) = self.objects.get(*id) else {
                continue;
            };
            let def = self.pool.get(o.def);
            if o.zone == Zone::Graveyard && o.owner == p && def.harmonize.is_none() {
                self.harmonize_options(
                    p,
                    def,
                    *id,
                    sorcery_timing,
                    &sources,
                    &changers,
                    &mut options,
                );
            }
        }
        // CR 702.34a: instants and sorceries with flashback may be cast
        // from the graveyard for their flashback cost.
        if ps.flashback_in_graveyard > 0 {
            for id in &ps.graveyard {
                let def = self.pool.get(self.objects[*id].def);
                // CR 702.84a and friends: abilities activated from the
                // graveyard (unearth).
                if def.graveyard_active() {
                    for a in &def.ability_ids {
                        if let AbilityDef::Activated(ab) = self.pool.ability(*a)
                            && ab.from_graveyard
                            && (!ab.sorcery_speed || sorcery_timing)
                            && self.activation_allowed(ab, *a, p, *id)
                            && self.payable_for(
                                p,
                                &sources,
                                &ab.cost.mana.requirements(0),
                                Spend::Ability(self.characteristics(*id).as_ref()),
                            )
                            && self.targets_available(p, &ab.targets, Some(*id))
                        {
                            // "Exile an instant or sorcery card from your
                            // graveyard" beside it: other cards, enough.
                            if let Some(g) = &ab.cost.exile_cards
                                && !g.x
                                && self.exile_candidates(p, g.filter.as_ref(), *id).len()
                                    < g.n as usize
                            {
                                continue;
                            }
                            // "{2}{B}, Tap an untapped creature you control:
                            // Return this card from your graveyard ...".
                            if ab.cost.taps_chosen() {
                                for c in self.tap_cost_options(p, ab, *id, &sources) {
                                    options.push(Choice::Activate {
                                        source: *id,
                                        ability: *a,
                                        sacrifice: c,
                                        discard: None,
                                    });
                                }
                                continue;
                            }
                            // "{2}, Sacrifice an artifact or creature:
                            // Return this card from your graveyard ...":
                            // one option per permanent that could be
                            // sacrificed, none without enough of them.
                            if let Some(SacCost::Filter(f)) = &ab.cost.sacrifice {
                                let c = self.sacrifice_candidates(p, f, *id);
                                if c.len() < ab.cost.chosen_count() {
                                    continue;
                                }
                                for s in c {
                                    options.push(Choice::Activate {
                                        source: *id,
                                        ability: *a,
                                        sacrifice: Some(s),
                                        discard: None,
                                    });
                                }
                                continue;
                            }
                            // "{2}{U}, Discard two cards: Return this
                            // card from your graveyard ...": one option
                            // per card that could be discarded, none
                            // without enough of them.
                            if ab.cost.discard > 0 {
                                let mut c = self.discard_candidates(
                                    p,
                                    ab.cost.discard_filter.as_ref(),
                                    *id,
                                );
                                if c.len() < ab.cost.discard as usize {
                                    continue;
                                }
                                if ab.cost.discard_random {
                                    c.truncate(1);
                                }
                                for d in c {
                                    options.push(Choice::Activate {
                                        source: *id,
                                        ability: *a,
                                        sacrifice: None,
                                        discard: Some(d),
                                    });
                                }
                                continue;
                            }
                            options.push(Choice::Activate {
                                source: *id,
                                ability: *a,
                                sacrifice: None,
                                discard: None,
                            });
                        }
                    }
                }
                // CR 702.146a: cast transformed for the disturb cost.
                if def.disturb.is_some()
                    && self.disturb_castable(p, def, *id, sorcery_timing, &sources, &changers)
                {
                    options.push(Choice::CastDisturb(*id));
                }
                // CR 702.180a: cast for the harmonize cost, tapping up to
                // one untapped creature.
                if def.harmonize.is_some() {
                    self.harmonize_options(
                        p,
                        def,
                        *id,
                        sorcery_timing,
                        &sources,
                        &changers,
                        &mut options,
                    );
                }
                // CR 702.127a: a split card's aftermath half, for its mana
                // cost (`Choice::Cast` of the card in the graveyard).
                if def.keywords.contains(Keyword::Aftermath) {
                    if let Some(face) = self.aftermath_face(*id)
                        && self.castable(
                            p,
                            self.pool.get(face),
                            *id,
                            sorcery_timing,
                            &sources,
                            &pool,
                            &changers,
                        )
                    {
                        options.push(Choice::Cast(*id));
                    }
                    continue;
                }
                if def.flashback.is_none() {
                    if (def.escape.is_some() && self.escape_castable(p, *id, sorcery_timing))
                        || (def.mayhem.is_some() && self.mayhem_castable(p, *id, sorcery_timing))
                    {
                        options.push(Choice::Cast(*id));
                    }
                    continue;
                }
                let Some(fb) = &def.flashback else {
                    continue;
                };
                let instant_speed = def.is_instant() || self.has_flash(p, def, *id);
                if !(instant_speed || sorcery_timing) {
                    continue;
                }
                let mut req = fb.mana.requirements(0);
                adjust_generic(&mut req, self.cost_delta(p, def, *id, &changers));
                if !self.payable_for(
                    p,
                    &sources,
                    &req,
                    Spend::Spell(&self.printed(self.objects[*id].def)),
                ) {
                    continue;
                }
                if fb.life > 0 && ps.life < i32::try_from(fb.life).unwrap_or(i32::MAX) {
                    continue;
                }
                // Jump-start, retrace: a card to discard (CR 702.133a,
                // 702.81a).
                if fb.discard > 0
                    && self
                        .discard_candidates(p, fb.discard_filter.as_ref(), *id)
                        .len()
                        < fb.discard as usize
                {
                    continue;
                }
                // "Flashback—Sacrifice a Mountain.", "Flashback—Tap three
                // untapped white creatures you control."
                // "Flashback—{1}{R}, Behold three Elementals.": its other
                // parts too (CR 601.2h, 702.34a).
                if !(if fb.sacrifice.is_some() && fb.taps_chosen() {
                    self.cost_option_payable(p, fb, *id, &sources, req, false)
                } else {
                    self.additional_payable(p, fb, *id)
                }) {
                    continue;
                }
                if def.additional_or.is_some() || def.additional.taps_chosen() {
                    if !self.additional_options_payable(p, def, *id, &sources, &req) {
                        continue;
                    }
                } else if !self.additional_payable(p, &def.additional, *id) {
                    continue;
                }
                let targets_ok = match def.modes() {
                    Some(modes) => modes
                        .iter()
                        .any(|m| self.targets_available(p, &m.targets, Some(*id))),
                    None => self.spell_targets_available(p, def, *id),
                };
                if targets_ok {
                    options.push(Choice::Cast(*id));
                }
            }
        }
        self.activation_options(p, sorcery_timing, &sources, &mut options);
        if !self.player_rules.is_empty() {
            self.forbid_by_player_rules(p, &mut options);
        }
        // "Spells with the chosen name can't be cast." (CR 201.3).
        if self.player_static_sources > 0 {
            let banned = |def: crate::card::CardRef| self.name_cant_be_cast(def);
            options.retain(|c| match c {
                Choice::CastFace { card, face } => {
                    self.land_face(*card, *face).is_some()
                        || !self.objects.get(*card).is_some_and(|o| {
                            self.pool
                                .get(o.def)
                                .faces
                                .get(usize::from(*face))
                                .is_some_and(|f| banned(*f))
                        })
                }
                Choice::Cast(id)
                | Choice::CastEvoke(id)
                | Choice::CastDash(id)
                | Choice::CastWarp(id)
                | Choice::CastSneak(id)
                | Choice::CastAlt(id)
                | Choice::CastBestow(id)
                | Choice::CastOverload(id)
                | Choice::CastAwaken(id)
                | Choice::CastWebSlinging(id)
                | Choice::CastImpending(id)
                | Choice::CastHarmonize { card: id, .. } => {
                    !self.objects.get(*id).is_some_and(|o| banned(o.def))
                }
                Choice::CastDisturb(id) => !self.objects.get(*id).is_some_and(|o| {
                    self.pool
                        .get(o.def)
                        .faces
                        .get(1)
                        .is_some_and(|f| banned(*f))
                }),
                _ => true,
            });
        }
        // "Your opponents can't cast spells during your turn."
        if self.player_static_sources > 0 && self.cant_cast_spells(p) {
            options.retain(|c| match c {
                Choice::CastFace { card, face } => self.land_face(*card, *face).is_some(),
                Choice::Cast(_)
                | Choice::CastEvoke(_)
                | Choice::CastDash(_)
                | Choice::CastWarp(_)
                | Choice::CastSneak(_)
                | Choice::CastAlt(_)
                | Choice::CastBestow(_)
                | Choice::CastOverload(_)
                | Choice::CastAwaken(_)
                | Choice::CastDisturb(_)
                | Choice::CastFaceDown(_) => false,
                _ => true,
            });
        }
        // CR 702.61a: with a split second spell on the stack, no spells
        // are cast and no abilities other than mana abilities activated;
        // special actions remain (CR 702.61b).
        if !self.stack.is_empty()
            && self.stack.iter().any(|s| {
                self.objects.get(*s).is_some_and(|o| {
                    self.pool
                        .get(o.def)
                        .keywords
                        .contains(crate::card::Keyword::SplitSecond)
                })
            })
        {
            options.retain(|c| {
                matches!(
                    c,
                    Choice::TurnFaceUp(_)
                        | Choice::Unlock { .. }
                        | Choice::Suspend(_)
                        | Choice::Foretell(_)
                        | Choice::Plot(_)
                )
            });
        }
        options.push(Choice::Pass);
        options
    }

    /// Leave out what a "can't" effect on `p` forbids this turn: land
    /// plays ("can't play lands", CR 305.1) and casting creature spells
    /// ("can't cast creature spells"; a face-down spell is a creature
    /// spell, CR 708.4, and a bestowed one an Aura, CR 702.103b).
    fn forbid_by_player_rules(&self, p: PlayerId, options: &mut Vec<Choice>) {
        let lands = self.player_rule(p, crate::card::PlayerRuleKind::CantPlayLands);
        let creatures = self.player_rule(p, crate::card::PlayerRuleKind::CantCastCreatureSpells);
        if !lands && !creatures {
            return;
        }
        let creature = |id: &ObjId| {
            self.objects
                .get(*id)
                .is_some_and(|o| self.pool.get(o.def).is_creature())
        };
        options.retain(|c| match c {
            Choice::PlayLand(_) => !lands,
            Choice::CastFace { card, face } if self.land_face(*card, *face).is_some() => !lands,
            Choice::Cast(id)
            | Choice::CastEvoke(id)
            | Choice::CastDash(id)
            | Choice::CastWarp(id)
            | Choice::CastSneak(id)
            | Choice::CastWebSlinging(id)
            | Choice::CastImpending(id)
            | Choice::CastHarmonize { card: id, .. }
            | Choice::CastAlt(id) => !(creatures && creature(id)),
            Choice::CastFaceDown(_) => !creatures,
            Choice::CastFace { card, face } => {
                !(creatures
                    && self.objects.get(*card).is_some_and(|o| {
                        self.pool
                            .get(o.def)
                            .faces
                            .get(usize::from(*face))
                            .is_some_and(|f| self.pool.get(*f).is_creature())
                    }))
            }
            _ => true,
        });
    }

    /// `eval_num` for a static ability of `source`'s, outside any
    /// resolution (an "as this is turned face up" amount).
    pub(crate) fn eval_num_of(
        &self,
        n: crate::card::Num,
        source: ObjId,
        controller: PlayerId,
    ) -> u32 {
        let ctx = EffectCtx {
            source,
            controller,
            targets: Vec::new(),
            division: Vec::new(),
            legal: Vec::new(),
            trigger_obj: None,
            trigger_player: None,
            trigger_amount: 0,
            created: None,
            revealed: Vec::new(),
            x: 0,
            did: true,
            chosen: Chosen::None,
            couldnt: 0,
            created_tokens: SmallVec::new(),
            counts: None,
        };
        self.eval_num(n, &ctx)
    }

    /// A variable amount's value as an effect happens: X from the spell, or
    /// one of the source card's counts (CR 107.3 for X; "the number of
    /// ..." judged from the effect's controller).
    pub(super) fn eval_num(&self, n: crate::card::Num, ctx: &EffectCtx) -> u32 {
        use crate::card::Num;
        let i = match n {
            Num::Count(i) => i as usize,
            Num::ThatMuch => return ctx.trigger_amount,
            other => return other.eval(ctx.x),
        };
        let def = ctx.counts.or_else(|| {
            self.objects
                .get(ctx.source)
                .map(|o| o.def)
                .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
        });
        let pool = std::sync::Arc::clone(&self.pool);
        let Some(expr) = def.and_then(|d| pool.get(d).counts.get(i)) else {
            return 0;
        };
        self.eval_count(expr, ctx)
    }

    /// A count's value as an effect happens, judged from its controller.
    #[allow(clippy::too_many_lines)] // one arm per count
    fn eval_count(&self, expr: &crate::card::CountExpr, ctx: &EffectCtx) -> u32 {
        use crate::card::CountExpr;
        let you = ctx.controller;
        let n = match expr {
            CountExpr::Times(k, inner) => return k.saturating_mul(self.eval_count(inner, ctx)),
            CountExpr::ExiledWith(f) => self.count_exiled_with(ctx.source, f.as_ref(), you),
            CountExpr::CardsInExile { filter, graveyard } => {
                self.owned_in_exile(you, filter.as_ref(), *graveyard)
            }
            CountExpr::Divided { by, up, inner } => {
                return CountExpr::divide(self.eval_count(inner, ctx), *by, *up);
            }
            CountExpr::Permanents(f) => {
                let f = &*self.chosen_filter(f, ctx);
                let host = f.attached_to.map(|a| self.attached_host(ctx, a));
                // "for each creature target player controls".
                let Some(that) = self.that_player_in(f, ctx) else {
                    return 0;
                };
                self.battlefield
                    .iter()
                    .filter(|id| {
                        let o = &self.objects[**id];
                        that.is_none_or(|p| o.controller == p)
                            && host.is_none_or(|h| h.is_some() && o.attached_to == h)
                            && self.characteristics(**id).is_some_and(|c| {
                                f.matches(**id, o.controller, &c, Some(ctx.source), Some(you))
                            })
                            && self.status_ok(f, **id)
                    })
                    .count()
            }
            CountExpr::SpentColors => self.spent_on(ctx.source).colors() as usize,
            CountExpr::TimesKicked => self
                .objects
                .get(ctx.source)
                .map(|o| o.cast)
                .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.cast))
                .map_or(0, |c| c.kicks() as usize),
            CountExpr::SpentTreasure => {
                self.spent_on(ctx.source)
                    .count(crate::card::SpentKind::Treasure) as usize
            }
            CountExpr::Domain => self.basic_land_types(you),
            CountExpr::CardsInHand => self.player(you).hand.len(),
            CountExpr::Plus(k, inner) => return k.saturating_add(self.eval_count(inner, ctx)),
            CountExpr::Difference(k, inner) => {
                return k.saturating_sub(self.eval_count(inner, ctx));
            }
            CountExpr::CardsInGraveyard(f) => {
                let f = f.as_ref().map(|f| self.chosen_filter(f, ctx));
                self.graveyard_count(std::slice::from_ref(&you), f.as_deref(), you)
            }
            CountExpr::CardsInAllGraveyards(f) => {
                let f = f.as_ref().map(|f| self.chosen_filter(f, ctx));
                let players: SmallVec<[PlayerId; 4]> = self.players_in_game().collect();
                self.graveyard_count(&players, f.as_deref(), you)
            }
            CountExpr::SourcePower => {
                return self
                    .characteristics_or_lki(ctx.source)
                    .map_or(0, |c| u32::try_from(c.power().max(0)).unwrap_or(0));
            }
            CountExpr::TargetStat { n, toughness } => {
                return match ctx.targets.get(*n) {
                    Some(Target::Object(id)) => self.characteristics_or_lki(*id).map_or(0, |c| {
                        let v = if *toughness { c.toughness() } else { c.power() };
                        u32::try_from(v.max(0)).unwrap_or(0)
                    }),
                    _ => 0,
                };
            }
            CountExpr::Life => return u32::try_from(self.player(you).life.max(0)).unwrap_or(0),
            CountExpr::Votes(i) => {
                return match ctx.chosen {
                    Chosen::Votes(v) => u32::from(v.get(usize::from(*i)).copied().unwrap_or(0)),
                    _ => 0,
                };
            }
            // Parley: "For each nonland card revealed this way" (every
            // player's top card, `Effect::RevealTop`).
            CountExpr::ThisWay(crate::card::ThisWay::Revealed, f) => ctx
                .revealed
                .iter()
                .filter(|id| {
                    let Some(o) = self.objects.get(**id) else {
                        return false;
                    };
                    f.as_ref().is_none_or(|f| {
                        let c = self.base_characteristics(o);
                        f.matches(**id, o.owner, &c, Some(ctx.source), Some(ctx.controller))
                    })
                })
                .count(),
            CountExpr::ThisWay(verb, f) => self.this_way.as_ref().map_or(0, |log| {
                log.moved
                    .iter()
                    .filter(|m| {
                        m.is(*verb) && f.as_ref().is_none_or(|f| self.moved_matches(ctx, m, f))
                    })
                    .count()
            }),
            CountExpr::LifeLostThisWay => {
                return self.this_way.as_ref().map_or(0, |log| log.life_lost);
            }
            CountExpr::BlockingIt { beyond } => {
                let n = u32::try_from(self.blocking_it(ctx).len()).unwrap_or(u32::MAX);
                return n.saturating_sub(*beyond);
            }
            // "that card's mana value": the card revealed this way, as it
            // last existed once it has moved (CR 608.2h).
            CountExpr::CardManaValue => {
                let def = ctx.created.and_then(|id| {
                    self.objects
                        .get(id)
                        .map(|o| o.def)
                        .or_else(|| self.lki.get(&id).map(|l| l.object.def))
                });
                return def.map_or(0, |d| self.pool.get(d).mana_cost.mana_value());
            }
            CountExpr::TargetManaValue(n) => {
                let Some(Target::Object(id)) = ctx.targets.get(*n) else {
                    return 0;
                };
                let def = self
                    .objects
                    .get(*id)
                    .map(|o| o.def)
                    .or_else(|| self.lki.get(id).map(|l| l.object.def));
                // CR 202.3e: on the stack, X is its chosen value.
                return def.map_or(0, |d| {
                    let cost = &self.pool.get(d).mana_cost;
                    let xs = cost
                        .symbols()
                        .iter()
                        .filter(|s| matches!(s, crate::mana::ManaSymbol::X))
                        .count();
                    cost.mana_value() + self.x_of(*id) * u32::try_from(xs).unwrap_or(0)
                });
            }
            CountExpr::GreatestPower(f) => {
                let f = &*self.chosen_filter(f, ctx);
                let best = self
                    .battlefield
                    .iter()
                    .filter_map(|id| {
                        let o = &self.objects[*id];
                        let c = self.characteristics(*id)?;
                        (f.matches(*id, o.controller, &c, Some(ctx.source), Some(you))
                            && self.status_ok(f, *id))
                        .then(|| c.power())
                    })
                    .max()
                    .unwrap_or(0);
                return u32::try_from(best.max(0)).unwrap_or(0);
            }
            // CR 702.82c: "for each creature it devoured", the sacrificed
            // permanents as they last existed.
            CountExpr::Devoured(f) => self
                .enter_notes
                .iter()
                .filter(|(o, note)| {
                    *o == ctx.source
                        && matches!(note, crate::state::EnterNote::Devoured(d)
                        if self.lki.get(d).is_some_and(|l| f.matches(
                            *d,
                            l.object.controller,
                            &l.characteristics,
                            Some(ctx.source),
                            Some(you),
                        )))
                })
                .count(),
            CountExpr::DifferentlyNamed(f) => {
                let f = &*self.chosen_filter(f, ctx);
                let mut names: SmallVec<[&str; 8]> = SmallVec::new();
                for id in &self.battlefield {
                    let o = &self.objects[*id];
                    if let Some(c) = self.characteristics(*id)
                        && f.matches(*id, o.controller, &c, Some(ctx.source), Some(you))
                        && self.status_ok(f, *id)
                    {
                        let name = self.name_of(*id);
                        if !name.is_empty() && !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
                names.len()
            }
            CountExpr::Party(roles) => {
                // CR 700.8: the largest set of creatures you control
                // filling distinct roles. With four roles, the reachable
                // sets of filled roles fit in sixteen bits.
                let mut reachable: u16 = 1; // bit `s`: the role set `s`
                for id in self.permanents_of(you) {
                    let Some(c) = self.characteristics(id) else {
                        continue;
                    };
                    let mut mask = 0u8;
                    for (r, f) in roles.iter().enumerate() {
                        if f.matches(id, you, &c, Some(ctx.source), Some(you)) {
                            mask |= 1 << r;
                        }
                    }
                    if mask == 0 {
                        continue;
                    }
                    let mut next = reachable;
                    for s in 0..16u16 {
                        if reachable & (1 << s) != 0 {
                            for r in 0..4u16 {
                                if mask & (1 << r) != 0 && s & (1 << r) == 0 {
                                    next |= 1 << (s | (1 << r));
                                }
                            }
                        }
                    }
                    reachable = next;
                }
                return (0..16u16)
                    .filter(|s| reachable & (1 << s) != 0)
                    .map(u16::count_ones)
                    .max()
                    .unwrap_or(0);
            }
            // CR 700.5: each permanent's mana cost (a face-down one has
            // none, CR 708.2).
            CountExpr::Devotion(colors) => {
                return self
                    .permanents_of(you)
                    .filter_map(|id| self.characteristics(id))
                    .map(|c| self.pool.get(c.def).mana_cost.devotion(*colors))
                    .sum();
            }
            CountExpr::GreatestManaValue => {
                return self
                    .permanents_of(you)
                    .filter_map(|id| self.characteristics(id))
                    .map(|c| self.pool.get(c.def).mana_cost.mana_value())
                    .max()
                    .unwrap_or(0);
            }
            CountExpr::CardsDrawnThisTurn => {
                return u32::from(self.drawn_this_turn[you.index().min(3)]);
            }
            CountExpr::SpellsCastThisTurn { other } => {
                let n = self
                    .cast_this_turn
                    .iter()
                    .filter(|(p, _)| *p == you)
                    .count();
                let own = *other
                    && self
                        .objects
                        .get(ctx.source)
                        .is_some_and(|o| o.zone == Zone::Stack && o.cast.cast_from().is_some());
                return u32::try_from(n.saturating_sub(usize::from(own))).unwrap_or(u32::MAX);
            }
            CountExpr::OtherInstantSorceryCast { before } => {
                let pool = &self.pool;
                let instant_or_sorcery = |d: crate::card::CardRef| {
                    let d = pool.get(d);
                    d.is_instant() || d.is_sorcery()
                };
                // The spell itself: the triggering spell, or the source.
                let spell = ctx.trigger_obj.unwrap_or(ctx.source);
                let def = self
                    .objects
                    .get(spell)
                    .map(|o| o.def)
                    .or_else(|| self.lki.get(&spell).map(|l| l.object.def));
                let casts = &self.cast_this_turn;
                // Its own cast, the latest of that card by you.
                let own = def.and_then(|d| casts.iter().rposition(|(p, c)| *p == you && *c == d));
                let upto = if *before {
                    own.unwrap_or(casts.len())
                } else {
                    casts.len()
                };
                let n = casts[..upto]
                    .iter()
                    .filter(|(p, d)| *p == you && instant_or_sorcery(*d))
                    .count();
                let n = if *before {
                    n
                } else {
                    n.saturating_sub(usize::from(own.is_some()))
                };
                return u32::try_from(n).unwrap_or(u32::MAX);
            }
            CountExpr::SourceToughness => {
                return self
                    .characteristics_or_lki(ctx.source)
                    .map_or(0, |c| u32::try_from(c.toughness().max(0)).unwrap_or(0));
            }
            CountExpr::ColorsAmongPermanents => {
                let mut seen = crate::mana::ColorSet::default();
                for id in self.permanents_of(you) {
                    if let Some(c) = self.characteristics(id) {
                        for col in c.colors.iter() {
                            seen = seen.with(col);
                        }
                    }
                }
                seen.iter().count()
            }
            CountExpr::OpponentsAttacked => {
                let Some(combat) = &self.combat else {
                    return 0;
                };
                let mut seen: SmallVec<[PlayerId; 4]> = SmallVec::new();
                for a in &combat.attackers {
                    if let Target::Player(p) = a.attacking
                        && p != you
                        && self.objects.get(a.id).is_some_and(|o| o.controller == you)
                        && !seen.contains(&p)
                    {
                        seen.push(p);
                    }
                }
                seen.len()
            }
            // Only the active player declares attackers (CR 508.1).
            CountExpr::AttackedWithThisTurn if self.active_player == you => {
                self.attackers_this_turn.len()
            }
            CountExpr::AttackedWithThisTurn => 0,
            CountExpr::CardsInOpponentsGraveyards(f) => {
                let f = f.as_ref().map(|f| self.chosen_filter(f, ctx));
                let players: SmallVec<[PlayerId; 4]> =
                    self.players_in_game().filter(|p| *p != you).collect();
                self.graveyard_count(&players, f.as_deref(), you)
            }
            // Judged on the creatures as they last existed (CR 700.4).
            CountExpr::DiedThisTurn(f) => self
                .died_this_turn
                .iter()
                .filter(|(id, controller, c)| f.matches(*id, *controller, c, None, Some(you)))
                .count(),
            CountExpr::GreatestPowerInGraveyard(f) => {
                return u32::try_from(self.greatest_power_in_graveyard(you, f).max(0)).unwrap_or(0);
            }
            CountExpr::ThoseObjects => {
                return self
                    .this_way
                    .as_ref()
                    .map_or(0, |log| u32::try_from(log.group.len()).unwrap_or(u32::MAX));
            }
            // CR 608.2h: the triggering object as it last existed.
            CountExpr::TriggerCounters(kind) => {
                return ctx.trigger_obj.map_or(0, |id| match self.objects.get(id) {
                    Some(o) if o.zone == Zone::Battlefield => o.counters(*kind),
                    o => self
                        .lki
                        .get(&id)
                        .map(|l| l.object.counters(*kind))
                        .or_else(|| o.map(|o| o.counters(*kind)))
                        .unwrap_or(0),
                });
            }
            CountExpr::TriggerStat(stat) => {
                return ctx.trigger_obj.map_or(0, |id| self.stat_value(id, *stat));
            }
            CountExpr::AttachedStat(stat) => {
                return self
                    .objects
                    .get(ctx.source)
                    .and_then(|o| o.attached_to)
                    .map_or(0, |id| self.stat_value(id, *stat));
            }
            // CR 608.2h: the moved object as it last existed.
            CountExpr::MovedStat { verb, stat } => {
                return self.this_way.as_ref().map_or(0, |log| {
                    log.moved
                        .iter()
                        .find(|m| m.is(*verb))
                        .map_or(0, |m| self.stat_value(m.old, *stat))
                });
            }
            CountExpr::TargetHand(n) => match ctx.targets.get(*n) {
                Some(Target::Player(p)) => self.player(*p).hand.len(),
                _ => 0,
            },
            CountExpr::GreatestToughness(f) => {
                let f = &*self.chosen_filter(f, ctx);
                let best = self
                    .battlefield
                    .iter()
                    .filter_map(|id| {
                        let o = &self.objects[*id];
                        let c = self.characteristics(*id)?;
                        (f.matches(*id, o.controller, &c, Some(ctx.source), Some(you))
                            && self.status_ok(f, *id))
                        .then(|| c.toughness())
                    })
                    .max()
                    .unwrap_or(0);
                return u32::try_from(best.max(0)).unwrap_or(0);
            }
            CountExpr::Sum(a, b) => {
                return self
                    .eval_count(a, ctx)
                    .saturating_add(self.eval_count(b, ctx));
            }
            CountExpr::LifeLostThisTurn => {
                return self.life_lost_this_turn[you.index().min(3)];
            }
            // CR 608.2h: once the source has left the battlefield, the
            // counters it had there.
            CountExpr::SourceCounters(kind) => {
                return match self.objects.get(ctx.source) {
                    Some(o) if o.zone == Zone::Battlefield => o.counters(*kind),
                    o => self
                        .lki
                        .get(&ctx.source)
                        .map(|l| l.object.counters(*kind))
                        .or_else(|| o.map(|o| o.counters(*kind)))
                        .unwrap_or(0),
                };
            }
            CountExpr::HostGraveyard(f) => {
                let host = self
                    .objects
                    .get(ctx.source)
                    .and_then(|o| o.attached_to)
                    .and_then(|h| self.objects.get(h));
                let Some(owner) = host.map(|h| h.controller) else {
                    return 0;
                };
                let f = f.as_ref().map(|f| self.chosen_filter(f, ctx));
                self.graveyard_count(std::slice::from_ref(&owner), f.as_deref(), you)
            }
            CountExpr::CardTypesInAllGraveyards => {
                return self
                    .players_in_game()
                    .flat_map(|p| self.player(p).graveyard.iter())
                    .map(|id| &self.objects[*id])
                    .filter(|o| o.kind != crate::state::ObjKind::Token)
                    .fold(crate::card::TypeSet::default(), |acc, o| {
                        acc.union(self.pool.get(o.def).types)
                    })
                    .len();
            }
            CountExpr::DamageThisWay { excess } => {
                return self
                    .this_way
                    .as_ref()
                    .map_or(0, |log| if *excess { log.excess } else { log.damage });
            }
            CountExpr::LifeGainedThisTurn => {
                return self.turn_log.life_gained[you.index().min(3)];
            }
            CountExpr::SacrificedThisTurn => {
                return u32::from(self.turn_log.sacrificed_n[you.index().min(3)]);
            }
            // Every attacker declared this turn, by any player (CR 508.1).
            CountExpr::AttackedThisTurn => self.attackers_this_turn.len(),
            CountExpr::OpponentsLostLife => self
                .players_in_game()
                .filter(|p| *p != you && self.lost_life_this_turn & (1 << p.index().min(7)) != 0)
                .count(),
            CountExpr::CreaturesEnteredThisTurn => {
                usize::from(self.turn_log.creatures_entered[you.index().min(3)])
            }
            CountExpr::DiscardedThisTurn => usize::from(self.turn_log.discards[you.index().min(3)]),

            CountExpr::OpponentsMoreCards => {
                let mine = self.player(you).hand.len();
                self.players_in_game()
                    .filter(|p| *p != you && self.player(*p).hand.len() > mine)
                    .count()
            }
            // X of the resolving spell (the source), or the source's X.
            CountExpr::TwoToX => return 1u32 << self.x_of(ctx.source).min(31),
            // CR 608.2h: once the source has left the battlefield, the
            // counters it had there.
            CountExpr::SourceAllCounters => {
                let total = |o: &crate::state::GameObject| o.counters.iter().map(|(_, n)| *n).sum();
                return match self.objects.get(ctx.source) {
                    Some(o) if o.zone == Zone::Battlefield => total(o),
                    o => self
                        .lki
                        .get(&ctx.source)
                        .map(|l| total(&l.object))
                        .or_else(|| o.map(total))
                        .unwrap_or(0),
                };
            }
            CountExpr::CardTypesInGraveyard => {
                return self
                    .player(you)
                    .graveyard
                    .iter()
                    .map(|id| &self.objects[*id])
                    .filter(|o| o.kind != crate::state::ObjKind::Token)
                    .fold(crate::card::TypeSet::default(), |acc, o| {
                        acc.union(self.pool.get(o.def).types)
                    })
                    .len();
            }
            // CR 107.1b: a negative total is 0.
            CountExpr::Total(stat, f) => {
                let f = &*self.chosen_filter(f, ctx);
                let sum: i64 = self
                    .battlefield
                    .iter()
                    .filter_map(|id| {
                        let o = &self.objects[*id];
                        let c = self.characteristics(*id)?;
                        (f.matches(*id, o.controller, &c, Some(ctx.source), Some(you))
                            && self.status_ok(f, *id))
                        .then(|| match stat {
                            crate::card::Stat::Power => i64::from(c.power()),
                            crate::card::Stat::Toughness => i64::from(c.toughness()),
                            crate::card::Stat::ManaValue => {
                                i64::from(self.pool.get(c.def).mana_cost.mana_value())
                            }
                        })
                    })
                    .sum();
                return u32::try_from(sum.max(0)).unwrap_or(u32::MAX);
            }
        };
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// An object's power, toughness, or mana value, as it last existed if
    /// it has left its zone (CR 608.2h); a negative value counts as 0.
    fn stat_value(&self, id: ObjId, stat: crate::card::Stat) -> u32 {
        use crate::card::Stat;
        match stat {
            Stat::Power | Stat::Toughness => self.characteristics_or_lki(id).map_or(0, |c| {
                let v = if stat == Stat::Power {
                    c.power()
                } else {
                    c.toughness()
                };
                u32::try_from(v.max(0)).unwrap_or(0)
            }),
            Stat::ManaValue => {
                let def = self
                    .objects
                    .get(id)
                    .map(|o| o.def)
                    .or_else(|| self.lki.get(&id).map(|l| l.object.def));
                // CR 202.3e: on the stack, X is its chosen value.
                def.map_or(0, |d| {
                    let cost = &self.pool.get(d).mana_cost;
                    let xs = cost
                        .symbols()
                        .iter()
                        .filter(|s| matches!(s, crate::mana::ManaSymbol::X))
                        .count();
                    cost.mana_value() + self.x_of(id) * u32::try_from(xs).unwrap_or(0)
                })
            }
        }
    }

    /// What `Effect::ChooseAct` may act on, for its controller.
    fn act_candidates(
        &self,
        ctx: &EffectCtx,
        act: crate::card::ChooseAct,
        filter: &crate::card::ObjFilter,
    ) -> Vec<ObjId> {
        use crate::card::ChooseAct;
        let p = ctx.controller;
        let card_ok = |id: ObjId| {
            let o = &self.objects[id];
            let c = self.base_characteristics(o);
            filter.matches(id, p, &c, Some(ctx.source), Some(p)) && self.status_ok(filter, id)
        };
        // "return an enchantment to its owner's hand": with no controller
        // named, any permanent (Drake Familiar); a sacrifice or tap is of
        // the player's own (CR 701.21a).
        let any_controller =
            matches!(act, ChooseAct::Bounce | ChooseAct::Untap) && filter.who.is_none();
        match act {
            // CR 702.154a: not attacking, and able to pay {T} (CR 302.6).
            ChooseAct::Enlist => self
                .battlefield
                .iter()
                .copied()
                .filter(|id| {
                    let o = &self.objects[*id];
                    o.controller == p
                        && !o.tapped
                        && *id != ctx.source
                        && !self
                            .combat
                            .as_ref()
                            .is_some_and(|cb| cb.attackers.iter().any(|a| a.id == *id))
                        && self.characteristics(*id).is_some_and(|c| {
                            filter.matches(*id, p, &c, Some(ctx.source), Some(p))
                                && (o.controlled_since_turn_start || c.has(Keyword::Haste))
                        })
                })
                .collect(),
            ChooseAct::Tap
            | ChooseAct::Untap
            | ChooseAct::Sacrifice
            | ChooseAct::Bounce
            | ChooseAct::AttachToTarget(_) => self
                .battlefield
                .iter()
                .copied()
                .filter(|id| any_controller || self.objects[*id].controller == p)
                .filter(|id| {
                    (act != ChooseAct::Tap || !self.objects[*id].tapped)
                        && (act != ChooseAct::Untap || self.objects[*id].tapped)
                        && self
                            .characteristics(*id)
                            .is_some_and(|c| filter.matches(*id, p, &c, Some(ctx.source), Some(p)))
                        && self.status_ok(filter, *id)
                })
                .collect(),
            ChooseAct::Discard | ChooseAct::HandToBottom => self
                .player(p)
                .hand
                .iter()
                .copied()
                .filter(|id| card_ok(*id))
                .collect(),
            ChooseAct::ExileFromGraveyard
            | ChooseAct::ReturnFromGraveyard
            | ChooseAct::GraveyardToTop
            | ChooseAct::ReturnRandomFromGraveyard => self
                .player(p)
                .graveyard
                .iter()
                .copied()
                .filter(|id| card_ok(*id))
                .collect(),
            // "a card an opponent owns from exile": any exile, judged by
            // the card's owner.
            ChooseAct::ExileToGraveyard => self
                .players
                .iter()
                .flat_map(|ps| ps.exile.iter().copied())
                .filter(|id| {
                    let o = &self.objects[*id];
                    let c = self.base_characteristics(o);
                    filter.matches(*id, o.owner, &c, Some(ctx.source), Some(p))
                })
                .collect(),
            // "from among the milled cards": those still in the graveyard.
            ChooseAct::MilledToHand => self.this_way.as_ref().map_or_else(Vec::new, |log| {
                log.moved
                    .iter()
                    .filter(|m| m.is(crate::card::ThisWay::Milled))
                    .map(|m| m.new)
                    .filter(|id| {
                        self.objects
                            .get(*id)
                            .is_some_and(|o| o.zone == Zone::Graveyard)
                            && card_ok(*id)
                    })
                    .collect()
            }),
        }
    }

    /// The card a resolving spell or ability comes from, for decisions
    /// that name it.
    fn resolving_def(&self, ctx: &EffectCtx) -> crate::card::CardRef {
        self.objects
            .get(ctx.source)
            .map(|o| o.def)
            .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
            .expect("a resolving spell or its last known information exists")
    }

    /// A vote (CR 701.38a): starting with the controller and in turn
    /// order, each player votes for one of `choices`, with the extra votes
    /// their permanents give them at the same time (CR 701.38d). Returns
    /// the votes for each choice.
    fn vote(&mut self, ctx: &EffectCtx, kind: DecisionKind, choices: &[Choice]) -> Vec<u32> {
        let mut tally = vec![0u32; choices.len()];
        let players: SmallVec<[PlayerId; 4]> = self.players_in_game().collect();
        let start = players
            .iter()
            .position(|p| *p == ctx.controller)
            .unwrap_or(0);
        for k in 0..players.len() {
            let p = players[(start + k) % players.len()];
            let (extra, optional) = self.extra_votes(p);
            for _ in 0..=extra {
                let i = self.choose_mid(p, kind, choices.to_vec());
                tally[i] += 1;
            }
            // "you may vote an additional time": or not.
            for _ in 0..optional {
                let mut options = choices.to_vec();
                options.push(Choice::Decline);
                let i = self.choose_mid(p, kind, options);
                if let Some(n) = tally.get_mut(i) {
                    *n += 1;
                }
            }
        }
        tally
    }

    /// The additional votes `p`'s permanents give them (CR 701.38d): those
    /// they get, and those they may cast.
    fn extra_votes(&self, p: PlayerId) -> (u32, u32) {
        let (mut get, mut may) = (0, 0);
        for id in self.permanents_of(p) {
            let Some(c) = self.characteristics(id) else {
                continue;
            };
            for a in &c.abilities {
                if let AbilityDef::ExtraVote { optional } = self.pool.ability(*a) {
                    if *optional {
                        may += 1;
                    } else {
                        get += 1;
                    }
                }
            }
        }
        (get, may)
    }

    /// `Effect::ChooseAct`: the controller chooses `n` candidates (up to
    /// `n` with `up_to`) and each is acted on; returns how many were.
    /// Without `up_to`, nothing happens unless all `n` can be chosen.
    fn choose_act(
        &mut self,
        ctx: &mut EffectCtx,
        act: crate::card::ChooseAct,
        filter: &crate::card::ObjFilter,
        n: usize,
        up_to: bool,
    ) -> usize {
        use crate::card::ChooseAct;
        let mut cands = self.act_candidates(ctx, act, filter);
        if n == 0 || (!up_to && cands.len() < n) {
            return 0;
        }
        let p = ctx.controller;
        let mut done = 0;
        while done < n && !cands.is_empty() {
            let id = if act == ChooseAct::ReturnRandomFromGraveyard {
                // "at random": from the game's random number generator.
                cands[rand::Rng::random_range(&mut self.rng, 0..cands.len())]
            } else if !up_to && cands.len() == n - done {
                cands[0]
            } else {
                let mut options: Vec<Choice> = cands
                    .iter()
                    .map(|id| match self.objects[*id].zone {
                        Zone::Battlefield => Choice::Sacrifice(*id),
                        _ => Choice::Discard {
                            card: *id,
                            def: self.objects[*id].def,
                        },
                    })
                    .collect();
                if up_to {
                    options.push(Choice::Done);
                }
                let k = self.choose_mid(p, DecisionKind::ChooseCard, options.clone());
                match options[k] {
                    Choice::Sacrifice(id) | Choice::Discard { card: id, .. } => id,
                    _ => break,
                }
            };
            cands.retain(|c| *c != id);
            match act {
                ChooseAct::Tap => self.tap_by(id, ctx.controller),
                ChooseAct::Enlist => {
                    self.tap(id);
                    let power = self.characteristics(id).map_or(0, |c| i32::from(c.power));
                    if self
                        .objects
                        .get(ctx.source)
                        .is_some_and(|o| o.zone == Zone::Battlefield)
                    {
                        let pump = Modification::ModifyPt {
                            power,
                            toughness: 0,
                        };
                        self.effect_on(
                            ctx,
                            ctx.source,
                            smallvec::smallvec![pump],
                            Duration::UntilEndOfTurn,
                        );
                    }
                }
                ChooseAct::Untap => self.untap(id),
                ChooseAct::Sacrifice => self.sacrifice(id),
                ChooseAct::Discard => self.discard_card(p, id),
                ChooseAct::HandToBottom => {
                    self.move_object(id, Zone::Library, false);
                }
                ChooseAct::GraveyardToTop => {
                    self.move_object(id, Zone::Library, true);
                }
                ChooseAct::ExileToGraveyard => {
                    ctx.created = Some(self.move_object(id, Zone::Graveyard, false));
                }
                ChooseAct::ExileFromGraveyard => {
                    ctx.created = Some(self.move_object(id, Zone::Exile, false));
                }
                ChooseAct::Bounce
                | ChooseAct::ReturnFromGraveyard
                | ChooseAct::ReturnRandomFromGraveyard
                | ChooseAct::MilledToHand => {
                    ctx.created = Some(self.move_object(id, Zone::Hand, false));
                }
                // An illegal target by now: nothing to attach to (CR
                // 608.2b).
                ChooseAct::AttachToTarget(n) => {
                    let n = usize::from(n);
                    if let Some(Target::Object(host)) = ctx.targets.get(n).copied()
                        && ctx.legal.get(n).copied().unwrap_or(true)
                        && self
                            .objects
                            .get(host)
                            .is_some_and(|o| o.zone == Zone::Battlefield)
                    {
                        self.attach(id, host);
                    }
                }
            }
            done += 1;
        }
        done
    }

    /// Bolster N (CR 701.39a): N +1/+1 counters on a creature the
    /// controller controls with the least toughness among them, chosen if
    /// several are tied.
    fn bolster(&mut self, ctx: &EffectCtx, n: u32) {
        let p = ctx.controller;
        let creatures: SmallVec<[(ObjId, i32); 8]> = self
            .permanents_of(p)
            .filter_map(|id| {
                self.characteristics(id)
                    .filter(crate::state::Characteristics::is_creature)
                    .map(|c| (id, c.toughness()))
            })
            .collect();
        let Some(least) = creatures.iter().map(|(_, t)| *t).min() else {
            return;
        };
        let tied: SmallVec<[ObjId; 4]> = creatures
            .iter()
            .filter(|(_, t)| *t == least)
            .map(|(id, _)| *id)
            .collect();
        let id = if tied.len() == 1 {
            tied[0]
        } else {
            let options = tied
                .iter()
                .map(|id| Choice::Target(Target::Object(*id)))
                .collect();
            tied[self.choose_mid(p, DecisionKind::ChooseCard, options)]
        };
        self.add_counters(id, CounterKind::PlusOnePlusOne, n);
    }

    /// Populate (CR 701.36a): a copy of a creature token the controller
    /// controls, chosen; with none, nothing (CR 701.36b).
    fn populate(&mut self, ctx: &mut EffectCtx) {
        let p = ctx.controller;
        let tokens: SmallVec<[ObjId; 4]> = self
            .permanents_of(p)
            .filter(|id| {
                self.objects[*id].kind == ObjKind::Token
                    && self.characteristics(*id).is_some_and(|c| c.is_creature())
            })
            .collect();
        let of = match tokens.len() {
            0 => return,
            1 => tokens[0],
            _ => {
                let options = tokens
                    .iter()
                    .map(|id| Choice::Target(Target::Object(*id)))
                    .collect();
                tokens[self.choose_mid(p, DecisionKind::ChooseCard, options)]
            }
        };
        // CR 614.1a: token-creation replacement effects apply.
        let def = self.objects[of].def;
        for (d, k) in self.tokens_to_create(p, def, 1) {
            for _ in 0..k {
                let new = if d == def {
                    self.create_token_copy(of, p, None)
                } else {
                    self.create_token(d, p)
                };
                if self.objects.contains_key(new) {
                    ctx.created = Some(new);
                }
            }
        }
    }

    /// Fabricate N (CR 702.123a): N +1/+1 counters on the source if its
    /// controller wants them and it is still on the battlefield, otherwise
    /// N Servos.
    fn fabricate(&mut self, ctx: &mut EffectCtx, n: u32, token: crate::card::CardRef) {
        let p = ctx.controller;
        let here = self
            .objects
            .get(ctx.source)
            .filter(|o| o.zone == Zone::Battlefield)
            .map(|o| o.def);
        if let Some(def) = here
            && self.choose_mid(
                p,
                DecisionKind::YouMay { def },
                vec![Choice::Accept, Choice::Decline],
            ) == 0
        {
            self.add_counters(ctx.source, CounterKind::PlusOnePlusOne, n);
            return;
        }
        for (d, k) in self.tokens_to_create(p, token, n) {
            for _ in 0..k {
                let new = self.create_token(d, p);
                if self.objects.contains_key(new) {
                    ctx.created = Some(new);
                }
            }
        }
    }

    /// Whether the controller can do `e` now, for `Effect::OneOf`: a
    /// choice needs something to choose; anything else can be done.
    fn can_do(&self, ctx: &EffectCtx, e: &Effect) -> bool {
        match e {
            Effect::Waterbend(mana) => {
                let p = ctx.controller;
                let req = mana.requirements(0);
                let mut sources = self.mana_sources(p);
                sources.extend(self.waterbend_sources(p, req.generic, None));
                let ps = self.player(p);
                self.payable(p, &sources, &ps.mana_pool, &req, ps.life, None)
            }
            Effect::ChooseAct {
                act,
                filter,
                count,
                up_to,
            } => {
                *up_to
                    || self.act_candidates(ctx, *act, filter).len()
                        >= self.eval_num(*count, ctx) as usize
            }
            Effect::Discard {
                who: crate::card::Recipient::You,
                ..
            }
            | Effect::HandToLibrary {
                who: crate::card::Recipient::You,
                ..
            }
            | Effect::DiscardRandom {
                who: crate::card::Recipient::You,
                ..
            } => !self.player(ctx.controller).hand.is_empty(),
            _ => true,
        }
    }

    /// An ability of a card in a graveyard ("return this card from your
    /// graveyard to your hand") whose card has since left it: the card is
    /// a new object elsewhere (CR 400.7) and the ability can't find it.
    fn source_left_graveyard(&self, ctx: &EffectCtx) -> bool {
        !self.objects.contains_key(ctx.source)
            && self
                .lki
                .get(&ctx.source)
                .is_some_and(|l| l.object.zone == Zone::Graveyard)
    }

    /// `p` discards `n` cards of their choice (CR 701.9), one at a time.
    fn discard_n(&mut self, p: PlayerId, n: u32) {
        for _ in 0..n {
            let options: Vec<Choice> = self
                .player(p)
                .hand
                .iter()
                .map(|id| Choice::Discard {
                    card: *id,
                    def: self.objects[*id].def,
                })
                .collect();
            if options.is_empty() {
                break;
            }
            let k = self.choose_mid(p, DecisionKind::Discard, options.clone());
            if let Choice::Discard { card, .. } = options[k] {
                self.discard_card(p, card);
            }
        }
    }

    /// Whether an object the resolution moved matches `f`, as it last
    /// existed before moving (CR 608.2h).
    fn moved_matches(
        &self,
        ctx: &EffectCtx,
        m: &crate::state::Moved,
        f: &crate::card::ObjFilter,
    ) -> bool {
        // Each alternative on its own, with "token" (a status qualifier
        // `matches` leaves to the caller) judged as the object last
        // existed: "a permanent you controlled or a token".
        self.lki.get(&m.old).is_some_and(|l| {
            let mut alt = Some(f);
            while let Some(a) = alt {
                let one = crate::card::ObjFilter {
                    or: None,
                    ..a.clone()
                };
                if one.matches(
                    m.old,
                    l.object.controller,
                    &l.characteristics,
                    Some(ctx.source),
                    Some(ctx.controller),
                ) && one
                    .status
                    .token
                    .is_none_or(|t| (l.object.kind == crate::state::ObjKind::Token) == t)
                {
                    return true;
                }
                alt = a.or.as_deref();
            }
            false
        })
    }

    /// "If a creature card was exiled this way" (CR 608.2c): what the
    /// resolution, or the costs paid for it, did.
    pub(super) fn this_way_holds(&self, ctx: &EffectCtx, cond: &crate::card::ThisWayCond) -> bool {
        // "If you didn't put a card into your hand this way" (no filter,
        // negated): nothing moved so.
        let nothing = cond.negated && cond.filter.is_none();
        let Some(log) = &self.this_way else {
            return nothing;
        };
        if nothing {
            return !log
                .moved
                .iter()
                .any(|m| m.is(cond.verb) && (!cond.by_you || m.owner == ctx.controller));
        }
        log.moved.iter().any(|m| {
            m.is(cond.verb)
                && (!cond.by_you || m.owner == ctx.controller)
                && cond
                    .filter
                    .as_ref()
                    .is_none_or(|f| self.moved_matches(ctx, m, f) != cond.negated)
        })
    }

    /// Cards in `players`' graveyards matching `f` (all with none), judged
    /// for `you`.
    /// The greatest power among `you`'s graveyard cards matching `f`
    /// (printed power, CR 208.3), 0 with none.
    pub(crate) fn greatest_power_in_graveyard(
        &self,
        you: PlayerId,
        f: &crate::card::ObjFilter,
    ) -> i32 {
        self.player(you)
            .graveyard
            .iter()
            .filter_map(|id| {
                let o = &self.objects[*id];
                let c = self.base_characteristics(o);
                (f.matches(*id, o.owner, &c, None, Some(you)) && self.status_ok(f, *id))
                    .then(|| c.power())
            })
            .max()
            .unwrap_or(0)
    }

    fn graveyard_count(
        &self,
        players: &[PlayerId],
        f: Option<&crate::card::ObjFilter>,
        you: PlayerId,
    ) -> usize {
        players
            .iter()
            .flat_map(|p| self.player(*p).graveyard.iter())
            .filter(|id| {
                f.is_none_or(|f| {
                    let o = &self.objects[**id];
                    let c = self.base_characteristics(o);
                    f.matches(**id, o.owner, &c, None, Some(you)) && self.status_ok(f, **id)
                })
            })
            .count()
    }

    /// CR 702.103a: whether `p` could cast the card `id` for its bestow
    /// cost, as the Aura `face` targeting a creature.
    #[cold]
    #[inline(never)]
    #[allow(clippy::too_many_arguments)] // the parts precomputed once per listing
    fn bestow_castable(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        face_ref: crate::card::CardRef,
        id: ObjId,
        sorcery_timing: bool,
        sources: &Sources,
        changers: &[ObjId],
    ) -> bool {
        let Some(cost) = &def.bestow else {
            return false;
        };
        if !(sorcery_timing || self.has_flash(p, def, id)) {
            return false;
        }
        let face = self.pool.get(face_ref);
        let mut req = cost.requirements(0);
        if !changers.is_empty() || self.next_spell_less[p.index().min(3)] > 0 {
            adjust_generic(&mut req, self.cost_delta(p, face, id, changers));
        }
        self.payable_for(p, sources, &req, Spend::Spell(&self.printed(face_ref)))
            && self.targets_available(p, face.unkicked_cast_targets(), Some(id))
    }

    /// Whether `p` could cast the card `id` for its overload cost now
    /// (CR 702.96a): as the spell `face`, which has no targets.
    #[allow(clippy::too_many_arguments)] // the parts precomputed once per listing
    #[cold]
    #[inline(never)]
    fn overload_castable(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        face_ref: crate::card::CardRef,
        id: ObjId,
        sorcery_timing: bool,
        sources: &Sources,
        changers: &[ObjId],
    ) -> bool {
        let Some(cost) = &def.overload else {
            return false;
        };
        if !(sorcery_timing || def.is_instant() || self.has_flash(p, def, id)) {
            return false;
        }
        let face = self.pool.get(face_ref);
        let mut req = cost.requirements(0);
        if !changers.is_empty() || self.next_spell_less[p.index().min(3)] > 0 {
            adjust_generic(&mut req, self.cost_delta(p, face, id, changers));
        }
        self.payable_for(p, sources, &req, Spend::Spell(&self.printed(face_ref)))
    }

    /// CR 702.113a: whether `p` could cast `def` for its awaken cost now,
    /// with a land they control to target and the spell's own targets.
    #[cold]
    #[inline(never)]
    #[allow(clippy::too_many_arguments)] // the parts precomputed once per listing
    fn awaken_castable(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        face_ref: crate::card::CardRef,
        id: ObjId,
        sorcery_timing: bool,
        sources: &Sources,
        changers: &[ObjId],
    ) -> bool {
        let Some((_, cost)) = &def.awaken else {
            return false;
        };
        if !(sorcery_timing || def.is_instant() || self.has_flash(p, def, id)) {
            return false;
        }
        let face = self.pool.get(face_ref);
        if !self.targets_available(p, face.cast_targets(), Some(id)) {
            return false;
        }
        let mut req = cost.requirements(0);
        if !changers.is_empty() || self.next_spell_less[p.index().min(3)] > 0 {
            adjust_generic(&mut req, self.cost_delta(p, face, id, changers));
        }
        self.payable_for(p, sources, &req, Spend::Spell(&self.printed(face_ref)))
    }

    /// CR 702.146a: whether `p` could cast `def` (the card `id`, in their
    /// graveyard) transformed for its disturb cost now: the back face's
    /// timing and targets.
    #[cold]
    #[inline(never)]
    fn disturb_castable(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        id: ObjId,
        sorcery_timing: bool,
        sources: &Sources,
        changers: &[ObjId],
    ) -> bool {
        let (Some(cost), Some(face_ref)) = (&def.disturb, def.faces.first().copied()) else {
            return false;
        };
        let face = self.pool.get(face_ref);
        if !(sorcery_timing || face.is_instant() || self.has_flash(p, face, id)) {
            return false;
        }
        if !self.targets_available(p, face.cast_targets(), Some(id)) {
            return false;
        }
        let mut req = cost.requirements(0);
        if !changers.is_empty() || self.next_spell_less[p.index().min(3)] > 0 {
            adjust_generic(&mut req, self.cost_delta(p, face, id, changers));
        }
        self.payable_for(p, sources, &req, Spend::Spell(&self.printed(face_ref)))
    }

    /// CR 702.180a: the ways `p` may cast `id` from their graveyard for
    /// its harmonize cost now: tapping no creature, or one untapped
    /// creature they control with power above 0 (its power off the
    /// generic mana, and it no longer a mana source), each if the rest
    /// can be paid.
    #[cold]
    #[inline(never)]
    #[allow(clippy::too_many_arguments)] // the parts precomputed once per listing
    fn harmonize_options(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        id: ObjId,
        sorcery_timing: bool,
        sources: &Sources,
        changers: &[ObjId],
        options: &mut Vec<Choice>,
    ) {
        // Its own harmonize cost, or one given to it: its mana cost.
        let cost = def.harmonize.as_deref().unwrap_or(&def.mana_cost);
        if !(sorcery_timing || def.is_instant() || self.has_flash(p, def, id))
            || !self.additional_payable(p, &def.additional, id)
        {
            return;
        }
        let targets_ok = match def.modes() {
            Some(modes) => modes
                .iter()
                .any(|m| self.targets_available(p, &m.targets, Some(id))),
            None => self.targets_available(p, def.unkicked_cast_targets(), Some(id)),
        };
        if !targets_ok {
            return;
        }
        let mut req = cost.requirements(0);
        if def.self_cost.is_some()
            || def.cond_cost.is_some()
            || !changers.is_empty()
            || self.next_spell_less[p.index().min(3)] > 0
        {
            adjust_generic(&mut req, self.cost_delta(p, def, id, changers));
        }
        let printed = self.printed(self.objects[id].def);
        if self.payable_for(p, sources, &req, Spend::Spell(&printed)) {
            options.push(Choice::CastHarmonize {
                card: id,
                tap: None,
            });
        }
        let tappers: Vec<(ObjId, i16)> = self
            .creatures_of(p)
            .filter(|c| !self.objects[*c].tapped)
            .filter_map(|c| Some((c, self.characteristics(c)?.power)))
            .filter(|(_, power)| *power > 0)
            .collect();
        for (c, power) in tappers {
            let mut r = req;
            adjust_generic(&mut r, -i32::from(power));
            if self.payable_for(p, &without(sources, &[c]), &r, Spend::Spell(&printed)) {
                options.push(Choice::CastHarmonize {
                    card: id,
                    tap: Some(c),
                });
            }
        }
    }

    /// Whether `p` could cast `def` (the card `id`, or one of its faces)
    /// now: timing, mana, additional costs, and targets (CR 601.2).
    #[inline]
    #[allow(clippy::too_many_arguments)] // the parts precomputed once per listing
    fn castable(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        id: ObjId,
        sorcery_timing: bool,
        sources: &Sources,
        pool: &crate::mana::ManaPool,
        changers: &[ObjId],
    ) -> bool {
        // CR 118.6: no mana cost is a cost that can't be paid.
        if def.no_mana_cost {
            return false;
        }
        let instant_speed = def.is_instant() || self.has_flash(p, def, id);
        if !(instant_speed || sorcery_timing) {
            return false;
        }
        let mut req = def.mana_cost.requirements(0);
        if def.self_cost.is_some()
            || def.cond_cost.is_some()
            || !changers.is_empty()
            || self.next_spell_less[p.index().min(3)] > 0
        {
            adjust_generic(&mut req, self.cost_delta(p, def, id, changers));
        }
        // Cheaper with a fitting target: castable if one exists (targets
        // are then held to what's payable).
        if let Some(tc) = &def.target_cost
            && tc.0 < 0
            && def.cast_targets().first().is_some_and(|spec| {
                self.legal_targets(p, *spec, Some(id))
                    .iter()
                    .any(|t| self.target_fits(&tc.1, *t, p, id))
            })
        {
            adjust_generic(&mut req, tc.0);
        }
        // Dearer with a fitting target: castable for the base cost only if
        // some legal target doesn't fit (Vanish into Eternity with only
        // creatures to target costs {3} more).
        if let Some(tc) = &def.target_cost
            && tc.0 > 0
            && def.cast_targets().first().is_some_and(|spec| {
                self.legal_targets(p, *spec, Some(id))
                    .iter()
                    .all(|t| self.target_fits(&tc.1, *t, p, id))
            })
        {
            adjust_generic(&mut req, tc.0);
        }
        let keywords = self.spell_keywords(p, def, id).0;
        let helped = Keyword::helps_pay(keywords) || def.additional.waterbend;
        let restricted = match (self.restricted_mana_possible(), &def.printed) {
            (false, _) | (_, None) => None,
            (true, Some(c)) => self.with_restricted(p, sources, Spend::Spell(c)),
        };
        let all = if helped || restricted.is_some() {
            let mut all = restricted.unwrap_or_else(|| sources.clone());
            if helped {
                all.extend(self.helper_sources(p, def, keywords));
            }
            Some(all)
        } else {
            None
        };
        let srcs = all.as_ref().unwrap_or(sources);
        // CR 601.2b: a choice of additional costs, or one that taps
        // permanents the mana might need.
        if def.additional_or.is_some() || def.additional.taps_chosen() {
            if !self.additional_options_payable(p, def, id, srcs, &req) {
                return false;
            }
        } else if !self.payable(p, srcs, pool, &req, self.player(p).life, None)
            || !self.additional_payable(p, &def.additional, id)
        {
            return false;
        }
        // A modal spell needs some mode to have its targets (CR 601.2b);
        // a spree spell, one it can also pay that mode's cost for (CR
        // 702.172a).
        if !def.spree.is_empty()
            && let Some(modes) = def.modes()
        {
            return def.spree.iter().enumerate().any(|(i, c)| {
                let mut more = req;
                more.add(&c.requirements(0));
                modes
                    .get((1 << i) - 1)
                    .is_some_and(|m| self.targets_available(p, &m.targets, Some(id)))
                    && self.payable(p, srcs, pool, &more, self.player(p).life, None)
            });
        }
        match def.modes() {
            Some(modes) => modes
                .iter()
                .any(|m| self.targets_available(p, &m.targets, Some(id))),
            None => self.spell_targets_available(p, def, id),
        }
    }

    #[cold]
    #[inline(never)]
    fn face_down_option(
        &self,
        p: PlayerId,
        id: ObjId,
        sources: &Sources,
        options: &mut Vec<Choice>,
    ) {
        let ps = self.player(p);
        if self.payable(
            p,
            sources,
            &ps.mana_pool,
            &self.face_down_cost(p),
            ps.life,
            None,
        ) {
            options.push(Choice::CastFaceDown(id));
        }
    }

    /// CR 702.37c: a face-down spell costs {3}, less what "face-down
    /// spells you cast this turn cost {1} less" takes off for `p`.
    fn face_down_cost(&self, p: PlayerId) -> ManaRequirement {
        let mut req = three();
        let less = self.turn_log.face_down_less[p.index().min(3)];
        req.generic = req.generic.saturating_sub(less.into());
        req
    }

    /// CR 712.11b, 712.12: a modal double-faced card's back face, played
    /// as a land with the land play's timing and allowance, or cast as a
    /// spell judged by that face alone (CR 712.11c).
    #[allow(clippy::too_many_arguments)]
    fn back_face_option(
        &self,
        p: PlayerId,
        id: ObjId,
        back: crate::card::CardRef,
        sorcery_timing: bool,
        sources: &Sources,
        changers: &[ObjId],
        options: &mut Vec<Choice>,
    ) {
        let face = self.pool.get(back);
        let ps = self.player(p);
        let ok = if face.is_land() {
            sorcery_timing && self.may_play_another_land(p)
        } else {
            self.castable(
                p,
                face,
                id,
                sorcery_timing,
                sources,
                &ps.mana_pool,
                changers,
            )
        };
        if ok {
            options.push(Choice::CastFace { card: id, face: 0 });
        }
    }

    /// Cast `card` as its face `face` (CR 715.3, 709.3): the spell has
    /// only the face's characteristics; the card is still the card.
    /// `exiled`: cast from a graveyard with aftermath, so it is exiled as
    /// it leaves the stack (CR 702.127a).
    pub(super) fn cast_face(&mut self, player: PlayerId, card: ObjId, face: u8, exiled: bool) {
        self.consecutive_passes = 0;
        let main = self.pool.get(self.objects[card].def);
        let face_def = main.faces[face as usize];
        let adventure = matches!(main.layout, Layout::Adventure | Layout::Omen);
        let spell = self.move_object(card, Zone::Stack, true);
        self.objects[spell].controller = player;
        self.objects[spell].def = face_def;
        self.objects[spell].cast.set_adventure(adventure);
        if exiled {
            self.objects[spell].cast.set_flashback(true);
        }
        self.cbump();
        let def = self.pool.get(face_def);
        self.flow = if def.modes().is_some() {
            crate::state::Flow::ChoosingMode {
                spell,
                sacrifice: None,
                discard: None,
            }
        } else {
            self.after_mode(spell)
        };
    }

    /// A split card's half with aftermath (CR 702.127a), by its index
    /// among the card's faces.
    pub(super) fn aftermath_index(&self, card: ObjId) -> Option<u8> {
        let d = self.pool.get(self.objects.get(card)?.def);
        if d.layout != Layout::Split || !d.keywords.contains(Keyword::Aftermath) {
            return None;
        }
        d.faces
            .iter()
            .position(|f| self.pool.get(*f).keywords.contains(Keyword::Aftermath))
            .and_then(|i| u8::try_from(i).ok())
    }

    /// The definition of that half.
    fn aftermath_face(&self, card: ObjId) -> Option<crate::card::CardRef> {
        let i = self.aftermath_index(card)?;
        self.pool
            .get(self.objects[card].def)
            .faces
            .get(usize::from(i))
            .copied()
    }

    /// The land face a `CastFace` choice plays (a modal double-faced
    /// card's, CR 712.12), if it is one.
    pub(crate) fn land_face(&self, card: ObjId, face: u8) -> Option<crate::card::CardRef> {
        let o = self.objects.get(card)?;
        let f = *self.pool.get(o.def).faces.get(usize::from(face))?;
        (o.zone != Zone::Battlefield && self.pool.get(f).is_land()).then_some(f)
    }

    /// Whether `def` has a prepare spell (CR 722.2a).
    pub(super) fn has_prepare_spell(&self, def: crate::card::CardRef) -> bool {
        let d = self.pool.get(def);
        d.layout == Layout::Prepare && !d.faces.is_empty()
    }

    /// The prepare spell a prepared permanent's copy has (CR 722.3c): its
    /// copiable values' own, else (it has since become a copy of something
    /// without one) its card's.
    pub(super) fn prepare_spell(&self, id: ObjId) -> Option<crate::card::CardRef> {
        let o = &self.objects[id];
        [o.def, o.card]
            .into_iter()
            .find(|d| self.has_prepare_spell(*d))
            .map(|d| self.pool.get(d).faces[0])
    }

    /// CR 722.3a: a permanent with a prepare spell that isn't prepared
    /// becomes prepared; anything else is unchanged.
    pub(super) fn become_prepared(&mut self, id: ObjId) {
        let Some(id) = self.current_object(id) else {
            return;
        };
        let o = &self.objects[id];
        if o.zone != Zone::Battlefield || o.prepared() || !self.has_prepare_spell(o.def) {
            return;
        }
        self.objects[id].marks |= crate::state::OBJ_PREPARED;
        self.prepared_ever += 1;
    }

    /// CR 722.3c: the prepared permanents' controller may cast a copy of
    /// each one's prepare spell, offered as casting that face of it.
    #[cold]
    #[inline(never)]
    fn prepared_options(
        &self,
        p: PlayerId,
        sorcery_timing: bool,
        sources: &Sources,
        changers: &[ObjId],
        options: &mut Vec<Choice>,
    ) {
        let pool = self.player(p).mana_pool;
        for id in self.permanents_of(p) {
            if !self.objects[id].prepared() {
                continue;
            }
            let Some(face) = self.prepare_spell(id) else {
                continue;
            };
            if self.castable(
                p,
                self.pool.get(face),
                id,
                sorcery_timing,
                sources,
                &pool,
                changers,
            ) {
                options.push(Choice::CastFace { card: id, face: 0 });
            }
        }
    }

    #[cold]
    #[inline(never)]
    fn unlock_options(&self, p: PlayerId, sources: &Sources, options: &mut Vec<Choice>) {
        let ps = self.player(p);
        for id in self.permanents_of(p) {
            let def = self.objects[id].def;
            let Some(doors) = self.pool.get(def).doors.as_deref() else {
                continue;
            };
            let unlocked = doors.unlocked(def);
            for door in 0..2u8 {
                if unlocked[usize::from(door)] {
                    continue;
                }
                let cost = &self.pool.get(doors.half(door)).mana_cost;
                if self.payable(
                    p,
                    sources,
                    &ps.mana_pool,
                    &cost.requirements(0),
                    ps.life,
                    None,
                ) {
                    options.push(Choice::Unlock { obj: id, door });
                }
            }
        }
    }

    #[cold]
    #[inline(never)]
    fn face_up_options(&self, p: PlayerId, sources: &Sources, options: &mut Vec<Choice>) {
        let ps = self.player(p);
        for id in self.permanents_of(p) {
            if let Some(cost) = self.face_up_cost(id)
                && self.payable(
                    p,
                    sources,
                    &ps.mana_pool,
                    &cost.mana.requirements(0),
                    ps.life,
                    None,
                )
                && self.additional_payable(p, &cost, id)
            {
                options.push(Choice::TurnFaceUp(id));
            }
        }
    }

    /// Suspend (CR 702.62a: whenever the card could be cast), foretell
    /// (CR 702.143a: any time during its owner's turn), and plot (CR
    /// 702.170a: at sorcery speed) from the hand.
    #[cold]
    #[inline(never)]
    fn exile_actions(
        &self,
        p: PlayerId,
        id: ObjId,
        sorcery_timing: bool,
        sources: &Sources,
        options: &mut Vec<Choice>,
    ) {
        let ps = self.player(p);
        let def = self.pool.get(self.objects[id].def);
        if let Some((_, cost)) = def.suspend.as_deref()
            && (sorcery_timing || def.is_instant() || self.has_flash(p, def, id))
            && self.payable(
                p,
                sources,
                &ps.mana_pool,
                &cost.requirements(0),
                ps.life,
                None,
            )
        {
            options.push(Choice::Suspend(id));
        }
        if def.foretell.is_some()
            && p == self.active_player
            && self.payable_for(
                p,
                sources,
                &crate::mana::ManaCost::parse("{2}")
                    .expect("a mana cost")
                    .requirements(0),
                Spend::Foretell,
            )
        {
            options.push(Choice::Foretell(id));
        }
        // CR 702.170a: plot, any time its owner could cast a sorcery.
        if let Some(cost) = def.plot.as_deref()
            && sorcery_timing
            && self.payable(
                p,
                sources,
                &ps.mana_pool,
                &cost.requirements(0),
                ps.life,
                None,
            )
        {
            options.push(Choice::Plot(id));
        }
    }

    /// Collect evidence `n` (CR 701.59a): exile cards from `p`'s graveyard
    /// (other than `except`) with total mana value `n` or greater, the
    /// engine picking them: the greatest mana values first, then dropping
    /// the smallest picks the total doesn't need. Sparing what the spell or
    /// ability being paid for targets. Whether it was done.
    /// CR 701.61a: `p` forages: exiles three cards from their graveyard
    /// (the oldest, other than `except`) or sacrifices a Food. With both
    /// possible, `ask` (the source's definition, as an effect resolves)
    /// lets them choose (`choose "1"` the cards, `choose "2"` a Food);
    /// paying a cost, the engine exiles the cards. Whether they foraged.
    #[cold]
    #[inline(never)]
    pub(super) fn forage(
        &mut self,
        p: PlayerId,
        except: ObjId,
        ask: Option<crate::card::CardRef>,
    ) -> bool {
        let cards = self.forage_cards(p, except);
        let foods = self.forage_foods(p);
        let can_exile = cards.len() >= 3;
        if !can_exile && foods.is_empty() {
            return false;
        }
        let exile = match ask {
            Some(def) if can_exile && !foods.is_empty() => {
                self.choose_mid(
                    p,
                    DecisionKind::ChooseOption { def },
                    vec![Choice::Mode(0), Choice::Mode(1)],
                ) == 0
            }
            _ => can_exile,
        };
        if exile {
            for id in cards.into_iter().take(3) {
                self.move_object(id, Zone::Exile, true);
            }
        } else {
            self.sacrifice(foods[0]);
        }
        self.emit(Event::Foraged { player: p });
        true
    }

    pub(super) fn collect_evidence(&mut self, p: PlayerId, n: u32, except: ObjId) -> bool {
        if !self.evidence_payable(p, n, except) {
            return false;
        }
        let mut cands = self.evidence_cards(p, except);
        let targeted = |s: &GameState, id: ObjId| {
            s.stack
                .last()
                .and_then(|t| s.objects.get(*t))
                .is_some_and(|top| top.targets.contains(&Target::Object(id)))
        };
        cands.sort_by_key(|(id, mv)| (targeted(self, *id), std::cmp::Reverse(*mv)));
        let mut picks: Vec<(ObjId, u32)> = Vec::new();
        let mut total = 0;
        for c in cands {
            if total >= n {
                break;
            }
            total += c.1;
            picks.push(c);
        }
        // Drop picks the total doesn't need, smallest first.
        picks.sort_by_key(|(_, mv)| *mv);
        let mut i = 0;
        while i < picks.len() {
            if total - picks[i].1 >= n {
                total -= picks[i].1;
                picks.remove(i);
            } else {
                i += 1;
            }
        }
        for (id, _) in picks {
            self.move_object(id, Zone::Exile, true);
        }
        self.emit(Event::CollectedEvidence { player: p });
        true
    }

    /// The creature a blight cost puts its counters on (CR 701.68a): the
    /// engine picks the toughest creature `p` controls.
    fn blight_pick(&self, p: PlayerId) -> Option<ObjId> {
        self.creatures_of(p)
            .max_by_key(|id| self.characteristics(*id).map_or(0, |c| c.toughness))
    }

    /// What turning the face-down permanent `id` face up costs: its morph
    /// or disguise cost, or a manifested creature card's mana cost (CR
    /// 701.34a); `None` if it isn't face down or can't be turned up.
    pub(super) fn face_up_cost(&self, id: ObjId) -> Option<Cost> {
        let o = self.objects.get(id)?;
        if !self.pool.is_face_down(o.def) || o.zone != Zone::Battlefield {
            return None;
        }
        let card = self.pool.get(o.card);
        match card.morph.as_deref() {
            Some((cost, _)) => Some(cost.clone()),
            None if card.is_creature() => Some(Cost {
                mana: card.mana_cost.clone(),
                ..Cost::default()
            }),
            None => None,
        }
    }

    /// Turn the face-down permanent `id` face up (CR 702.37e), paying its
    /// morph cost: mana, life, and the card discarded or permanent
    /// returned for it (`pick`, chosen by the player).
    pub(super) fn pay_face_up(&mut self, p: PlayerId, id: ObjId, cost: &Cost, pick: Option<ObjId>) {
        self.pay(p, &cost.mana.requirements(0));
        let (sac, discard) = if cost.discard > 0 {
            (None, pick)
        } else {
            (pick, None)
        };
        self.pay_other_costs(p, cost, id, sac, discard);
        self.turn_face_up(id, true);
    }

    /// Other untapped creatures `p` controls that could crew `vehicle`
    /// (or saddle it, with `saddle`), with the power each counts for (CR
    /// 702.122a, 702.171a; summoning sickness doesn't matter). One that
    /// can't crew Vehicles can't pay a crew cost (CR 702.122d); one that
    /// crews using its toughness, or as though its power were greater,
    /// counts that instead.
    fn crew_candidates(
        &self,
        p: PlayerId,
        vehicle: ObjId,
        saddle: bool,
    ) -> SmallVec<[(ObjId, u32); 4]> {
        self.creatures_of(p)
            .filter(|id| *id != vehicle && !self.objects[*id].tapped)
            .filter_map(|id| {
                let c = self.characteristics(id)?;
                if !saddle && c.has(Keyword::CantCrew) {
                    return None;
                }
                let mut n = c.power();
                for a in &c.abilities {
                    if let crate::card::AbilityDef::CrewsAs {
                        saddles,
                        toughness,
                        plus,
                    } = self.pool.ability(*a)
                        && (*saddles || !saddle)
                    {
                        if *toughness {
                            n = c.toughness();
                        } else {
                            n = n.saturating_add(i32::try_from(*plus).unwrap_or(0));
                        }
                    }
                }
                Some((id, u32::try_from(n.max(0)).unwrap_or(0)))
            })
            .collect()
    }

    fn crew_power(&self, p: PlayerId, vehicle: ObjId, saddle: bool) -> u32 {
        self.crew_candidates(p, vehicle, saddle)
            .iter()
            .map(|(_, n)| n)
            .sum()
    }

    /// The creatures that crew: the weakest one strong enough alone, or
    /// else the strongest until the power suffices.
    fn crew_choice(
        &self,
        p: PlayerId,
        vehicle: ObjId,
        need: u32,
        saddle: bool,
    ) -> SmallVec<[ObjId; 4]> {
        let mut c = self.crew_candidates(p, vehicle, saddle);
        if let Some((id, _)) = c.iter().filter(|(_, n)| *n >= need).min_by_key(|(_, n)| *n) {
            return smallvec::smallvec![*id];
        }
        c.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let mut total = 0;
        let mut out = SmallVec::new();
        for (id, n) in c {
            if total >= need {
                break;
            }
            total += n;
            out.push(id);
        }
        out
    }

    /// Whether some permanent may have abilities its definition lacks.
    pub(super) fn grants_exist(&self) -> bool {
        self.grant_sources > 0 || self.eot_grants > 0
    }

    /// Convoke and improvise (CR 702.51, 702.126): untapped creatures (each
    /// paying one mana of its color, or generic) and untapped artifacts
    /// (generic) that may be tapped to help cast `def`; after mana
    /// sources, so they are used last.
    pub(super) fn helper_sources(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        keywords: KeywordSet,
    ) -> Sources {
        let convoke = keywords.contains(Keyword::Convoke);
        let improvise = keywords.contains(Keyword::Improvise);
        let mut out = Sources::new();
        // CR 702.66a: each card exiled from the graveyard pays for {1}.
        if keywords.contains(Keyword::Delve) {
            for id in &self.player(p).graveyard {
                out.push((*id, Mana::Colorless));
            }
        }
        // CR 701.67b: an additional waterbend cost's artifacts and
        // creatures pay only its own generic mana.
        if def.additional.waterbend {
            let n = def.additional.mana.requirements(0).generic;
            out.extend(self.waterbend_sources(p, n, None));
        }
        if !convoke && !improvise {
            return out;
        }
        for id in self.permanents_of(p) {
            if self.objects[id].tapped {
                continue;
            }
            let Some(c) = self.characteristics(id) else {
                continue;
            };
            if convoke && c.is_creature() {
                let mut any = false;
                for col in c.colors.iter() {
                    out.push((id, Mana::Colored(col)));
                    any = true;
                }
                if !any {
                    out.push((id, Mana::Colorless));
                }
            } else if improvise && c.types.contains(crate::card::CardType::Artifact) {
                out.push((id, Mana::Colorless));
            }
        }
        out
    }

    /// CR 701.67a: the untapped artifacts and creatures `p` controls that
    /// may each be tapped to pay {1} of a waterbend cost, at most `cap` of
    /// them, `exclude` (a source tapping for its own {T}) left out. After
    /// mana sources, so they are used last.
    pub(super) fn waterbend_sources(
        &self,
        p: PlayerId,
        cap: u32,
        exclude: Option<ObjId>,
    ) -> Sources {
        let mut out = Sources::new();
        for id in self.permanents_of(p) {
            if out.len() >= cap as usize {
                break;
            }
            if self.objects[id].tapped || Some(id) == exclude {
                continue;
            }
            if self.characteristics(id).is_some_and(|c| {
                c.is_creature() || c.types.contains(crate::card::CardType::Artifact)
            }) {
                out.push((id, Mana::Colorless));
            }
        }
        out
    }

    /// The sources that may pay for the spell `spell` being cast.
    fn spell_sources(&self, spell: ObjId) -> Sources {
        let o = &self.objects[spell];
        let mut s = self.mana_sources(o.controller);
        if self.restricted_mana_possible()
            && let Some(c) = self.characteristics(spell)
            && let Some(all) = self.with_restricted(o.controller, &s, Spend::Spell(&c))
        {
            s = all;
        }
        let def = self.pool.get(o.def);
        let keywords = self.spell_keywords(o.controller, def, spell).0;
        if Keyword::helps_pay(keywords) || def.additional.waterbend {
            s.extend(self.helper_sources(o.controller, def, keywords));
        }
        s
    }

    /// Permanents with a static ability that changes what spells cost.
    /// CR 601.3, 305.1: the permissions `p` has to play `card`, the top
    /// card of their library, from permanents they control
    /// (`AbilityDef::PlayFromTop`): each (source, ability, once each
    /// turn). A once-each-turn permission already used this turn is left
    /// out.
    pub(super) fn top_permissions(
        &self,
        p: PlayerId,
        card: ObjId,
    ) -> SmallVec<[(ObjId, crate::card::AbilityId, bool); 2]> {
        let mut out = SmallVec::new();
        if self.top_play_sources == 0 || self.player(p).library.last() != Some(&card) {
            return out;
        }
        let o = &self.objects[card];
        let def = self.pool.get(o.def);
        let land = def.is_land();
        let chars = self.base_characteristics(o);
        for src in &self.battlefield {
            let s = &self.objects[*src];
            if s.controller != p || !self.flags_of(s).has_top_play() {
                continue;
            }
            for a in &self.pool.get(s.def).ability_ids {
                let AbilityDef::PlayFromTop {
                    lands,
                    spells,
                    once,
                } = self.pool.ability(*a)
                else {
                    continue;
                };
                let ok = if land {
                    *lands
                } else {
                    // The mana value isn't a characteristic `matches` sees.
                    spells.as_ref().is_some_and(|f| {
                        f.mana_value.is_none_or(|(cmp, n)| {
                            cmp.holds(def.mana_cost.mana_value(), u32::from(n))
                        }) && f.matches(card, p, &chars, Some(*src), Some(p))
                    })
                };
                if ok && !(*once && self.activated_once.contains(&(*src, *a))) {
                    out.push((*src, *a, *once));
                }
            }
        }
        out
    }

    /// CR 607.2a, 601.3: the cards exiled with permanents `p` controls
    /// that those permanents let `p` play (`AbilityDef::PlayExiled`): each
    /// (card, as a land, as a spell).
    #[cold]
    #[inline(never)]
    pub(super) fn exiled_permissions(&self, p: PlayerId) -> SmallVec<[(ObjId, bool, bool); 2]> {
        let mut out: SmallVec<[(ObjId, bool, bool); 2]> = SmallVec::new();
        for l in &self.linked_exiles {
            let Some(s) = self.objects.get(l.source) else {
                continue;
            };
            let Some(o) = self.objects.get(l.card) else {
                continue;
            };
            if s.zone != Zone::Battlefield || s.controller != p || o.zone != Zone::Exile {
                continue;
            }
            let def = self.pool.get(o.def);
            let chars = self.base_characteristics(o);
            let (mut land_ok, mut spell_ok) = (false, false);
            for a in &self.pool.get(s.def).ability_ids {
                let AbilityDef::PlayExiled {
                    lands,
                    spells,
                    your_turn,
                    owned,
                } = self.pool.ability(*a)
                else {
                    continue;
                };
                if (*your_turn && self.active_player != p) || (*owned && o.owner != p) {
                    continue;
                }
                land_ok |= *lands;
                // The mana value isn't a characteristic `matches` sees.
                spell_ok |= spells.as_ref().is_some_and(|f| {
                    f.mana_value
                        .is_none_or(|(cmp, n)| cmp.holds(def.mana_cost.mana_value(), u32::from(n)))
                        && f.matches(l.card, p, &chars, Some(l.source), Some(p))
                });
            }
            if (land_ok || spell_ok) && !out.iter().any(|(c, ..)| *c == l.card) {
                out.push((l.card, land_ok, spell_ok));
            }
        }
        out
    }

    pub(super) fn cost_changers(&self) -> SmallVec<[ObjId; 2]> {
        if self.cost_change_sources == 0 {
            return SmallVec::new();
        }
        self.battlefield
            .iter()
            .copied()
            .filter(|id| self.flags_of(&self.objects[*id]).has_cost_change())
            .collect()
    }

    /// CR 601.2f: how much more (or, below zero, less) generic mana `def`
    /// costs `p` to cast as the card `id`, from its own text and from
    /// `changers`' static abilities.
    pub(super) fn cost_delta(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        id: ObjId,
        changers: &[ObjId],
    ) -> i32 {
        let mut d = -i32::from(self.next_spell_less[p.index().min(3)]);
        if let Some((delta, cond)) = def.cond_cost {
            let cond = self.pool.condition(cond);
            // "If you've cast another instant or sorcery spell this turn":
            // the spell being cast isn't cast until its costs are paid (CR
            // 601.2i), so it is never among the spells counted, even once
            // it is on the stack paying them.
            let met = if let crate::card::StaticCondition::CastThisTurn {
                filter,
                at_least,
                other: true,
            } = cond
            {
                self.cast_this_turn
                    .iter()
                    .filter(|(q, def)| {
                        *q == p && filter.matches_characteristics(&self.printed(*def))
                    })
                    .count()
                    >= *at_least as usize
            } else {
                self.condition_now(cond, p, id)
            };
            if met {
                d += delta;
            }
        }
        if let Some(sc) = &def.self_cost {
            let ctx = EffectCtx {
                source: id,
                controller: p,
                targets: Vec::new(),
                division: Vec::new(),
                legal: Vec::new(),
                trigger_obj: None,
                trigger_player: None,
                trigger_amount: 0,
                created: None,
                revealed: Vec::new(),
                x: 0,
                did: true,
                chosen: Chosen::None,
                couldnt: 0,
                created_tokens: SmallVec::new(),
                counts: None,
            };
            let n = sc.per.saturating_mul(self.eval_num(sc.times, &ctx));
            d -= i32::try_from(n).unwrap_or(i32::MAX);
        }
        for src in changers {
            let o = &self.objects[*src];
            for a in &self.pool.get(o.def).ability_ids {
                if let AbilityDef::CostChange(cc) = self.pool.ability(*a)
                    && self.cost_change_applies(cc, p, def, id, *src)
                {
                    d += cc.generic;
                    // CR 702.41a: affinity, given to the spell.
                    if let Some(f) = cc.grant.as_ref().and_then(|g| g.affinity.as_ref()) {
                        let n = self
                            .permanents_of(p)
                            .filter(|q| {
                                self.characteristics(*q)
                                    .is_some_and(|c| f.matches(*q, p, &c, Some(*src), Some(p)))
                            })
                            .count();
                        d -= i32::try_from(n).unwrap_or(i32::MAX);
                    }
                }
            }
        }
        d
    }

    /// Whether the cost change (or grant) `cc` of `src` applies to `def`,
    /// the card or spell `id` that `p` casts.
    fn cost_change_applies(
        &self,
        cc: &crate::card::CostChange,
        p: PlayerId,
        def: &crate::card::CardDef,
        id: ObjId,
        src: ObjId,
    ) -> bool {
        let Some(chars) = def.printed.as_ref() else {
            return false;
        };
        let o = &self.objects[src];
        let whose = match cc.whose {
            crate::card::Who::Any => true,
            crate::card::Who::You => p == o.controller,
            crate::card::Who::Opponents => p != o.controller,
        };
        // "Instant and sorcery spells you cast from your hand have
        // cascade": from the hand, or cast from it.
        let from_ok = cc.grant.as_ref().is_none_or(|g| {
            !g.from_hand
                || self.objects.get(id).is_some_and(|c| match c.zone {
                    Zone::Stack => c.cast.cast_from() == Some(Zone::Hand),
                    z => z == Zone::Hand,
                })
        });
        // "Creature spells you cast with mana value 6 or greater": the
        // mana value isn't a characteristic `matches` sees.
        let mv_ok = cc
            .spells
            .mana_value
            .is_none_or(|(cmp, n)| cmp.holds(def.mana_cost.mana_value(), u32::from(n)));
        whose
            && from_ok
            && mv_ok
            && cc
                .spells
                .matches(id, p, chars, Some(src), Some(o.controller))
            && cc
                .cond
                .is_none_or(|i| self.condition_now(self.pool.condition(i), o.controller, src))
    }

    /// The keywords `def` has as the spell (or card) `id` that `p` casts:
    /// its own, and those static abilities give it ("Spells you cast have
    /// delve"); and whether it has storm.
    pub(super) fn spell_keywords(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        id: ObjId,
    ) -> (KeywordSet, bool) {
        let mut kw = def.keywords;
        let mut storm = def.storm;
        if self.cost_change_sources == 0 {
            return (kw, storm);
        }
        for src in self.cost_changers() {
            let o = &self.objects[src];
            for a in &self.pool.get(o.def).ability_ids {
                if let AbilityDef::CostChange(cc) = self.pool.ability(*a)
                    && let Some(g) = &cc.grant
                    && self.cost_change_applies(cc, p, def, id, src)
                {
                    kw = kw.union(g.keywords);
                    storm |= g.storm;
                }
            }
        }
        (kw, storm)
    }

    /// Cards in `p`'s hand that could be discarded for a cost of `source`
    /// with this filter ("discard a creature card").
    /// Whether `p` could cast `def` (the card `id`) now for the alternative
    /// cost its text offers (CR 118.9): its condition, timing, the mana,
    /// enough permanents to sacrifice or return, and targets. Additional
    /// costs that also choose a permanent aren't combined with it.
    #[cold]
    /// Whether `p` may cast `def`'s card `id` as though it had flash: the
    /// keyword (CR 702.8), or "This spell has flash as long as you control
    /// a Merfolk" while that holds.
    #[inline]
    fn has_flash(&self, p: PlayerId, def: &crate::card::CardDef, id: ObjId) -> bool {
        def.keywords.contains(Keyword::Flash)
            || def.flash_cleanup
            || def
                .flash_if
                .is_some_and(|c| self.condition_now(self.pool.condition(c), p, id))
            || (self.cost_change_sources > 0 && self.flash_granted(p, def, id))
    }

    /// Whether a static ability makes the spell `id` uncounterable ("Spells
    /// you control can't be countered"; a `SpellGrant`).
    #[cold]
    #[inline(never)]
    fn uncounterable_granted(&self, id: ObjId) -> bool {
        let Some(o) = self.objects.get(id) else {
            return false;
        };
        let (p, def) = (o.controller, self.pool.get(o.def));
        self.cost_changers().into_iter().any(|src| {
            let s = &self.objects[src];
            self.pool.get(s.def).ability_ids.iter().any(|a| {
                matches!(self.pool.ability(*a), AbilityDef::CostChange(cc)
                    if cc.grant.as_ref().is_some_and(|g| g.uncounterable)
                        && self.cost_change_applies(cc, p, def, id, src))
            })
        })
    }

    /// Whether a static ability lets `p` cast `def`'s card `id` as though
    /// it had flash ("You may cast noncreature spells as though they had
    /// flash", CR 702.8; a `SpellGrant` of the keyword).
    #[cold]
    #[inline(never)]
    fn flash_granted(&self, p: PlayerId, def: &crate::card::CardDef, id: ObjId) -> bool {
        self.cost_changers().into_iter().any(|src| {
            let o = &self.objects[src];
            self.pool.get(o.def).ability_ids.iter().any(|a| {
                matches!(self.pool.ability(*a), AbilityDef::CostChange(cc)
                    if cc.grant.as_ref().is_some_and(|g| g.keywords.contains(Keyword::Flash))
                        && self.cost_change_applies(cc, p, def, id, src))
            })
        })
    }

    #[inline(never)]
    fn alt_castable(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        id: ObjId,
        sorcery_timing: bool,
        sources: &Sources,
        changers: &[ObjId],
    ) -> bool {
        let Some(alt) = &def.alt_cost else {
            return false;
        };
        let instant_speed = def.is_instant() || self.has_flash(p, def, id);
        if !(instant_speed || sorcery_timing)
            || def.additional.sacrifice.is_some()
            || def.additional_or.is_some()
        {
            return false;
        }
        if let Some(c) = alt.condition
            && !self.condition_now(self.pool.condition(c), p, id)
        {
            return false;
        }
        if let Some(SacCost::Filter(f)) = &alt.cost.sacrifice
            && self.sacrifice_candidates(p, f, id).len() < alt.count.max(1) as usize
        {
            return false;
        }
        // "exile a creature card from your graveyard".
        if let Some(g) = &alt.cost.exile_cards
            && self.exile_candidates(p, g.filter.as_ref(), id).len() < g.n as usize
        {
            return false;
        }
        // CR 119.4: life can be paid only if the life total is at least
        // the amount.
        if alt.cost.life > 0
            && self.player(p).life < i32::try_from(alt.cost.life).unwrap_or(i32::MAX)
        {
            return false;
        }
        // CR 601.2f: cost reductions (affinity) apply to an alternative
        // cost too.
        let mut req = alt.cost.mana.requirements(0);
        if def.self_cost.is_some()
            || def.cond_cost.is_some()
            || !changers.is_empty()
            || self.next_spell_less[p.index().min(3)] > 0
        {
            adjust_generic(&mut req, self.cost_delta(p, def, id, changers));
        }
        // CR 702.119a: emerge, reduced by the mana value of the creature
        // sacrificed: some creature must leave the rest payable without
        // its own mana.
        let spend = Spend::Spell(&self.printed(self.objects[id].def));
        let payable = if def.emerge
            && let Some(SacCost::Filter(f)) = &alt.cost.sacrifice
        {
            !self
                .emerge_options(p, f, id, &req, sources, spend)
                .is_empty()
        } else {
            self.payable_for(p, sources, &req, spend)
        };
        if !payable || !self.additional_payable(p, &def.additional, id) {
            return false;
        }
        // CR 702.148a: cleaved, the spell's targets are its cleave face's.
        let def = def.cleave_face.map_or(def, |f| self.pool.get(f));
        match def.modes() {
            Some(modes) => modes
                .iter()
                .any(|m| self.targets_available(p, &m.targets, Some(id))),
            None => self.spell_targets_available(p, def, id),
        }
    }

    /// CR 702.188a: whether `p` may cast `id` for its web-slinging cost
    /// now: whenever they could cast it, with a tapped creature to return.
    #[cold]
    #[inline(never)]
    fn web_slinging_castable(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        id: ObjId,
        sorcery_timing: bool,
        sources: &Sources,
    ) -> bool {
        let Some(cost) = &def.web_slinging else {
            return false;
        };
        if !(sorcery_timing || def.is_instant() || self.has_flash(p, def, id))
            || def.additional.sacrifice.is_some()
            || def.additional_or.is_some()
            || self.web_slinging_returns(p).is_empty()
        {
            return false;
        }
        if !self.payable_for(
            p,
            sources,
            &cost.requirements(0),
            Spend::Spell(&self.printed(self.objects[id].def)),
        ) || !self.additional_payable(p, &def.additional, id)
        {
            return false;
        }
        match def.modes() {
            Some(modes) => modes
                .iter()
                .any(|m| self.targets_available(p, &m.targets, Some(id))),
            None => self.targets_available(p, def.unkicked_cast_targets(), Some(id)),
        }
    }

    /// CR 702.188a: the tapped creatures `p` controls, any of which a
    /// web-slinging cost may return to its owner's hand.
    pub(super) fn web_slinging_returns(&self, p: PlayerId) -> Vec<ObjId> {
        self.creatures_of(p)
            .filter(|c| self.objects[*c].tapped)
            .collect()
    }

    /// CR 702.190a: whether `p` may cast `id` for its sneak cost now: any
    /// time they could cast an instant during their declare blockers step,
    /// with an unblocked attacker to return.
    #[cold]
    #[inline(never)]
    fn sneak_castable(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        id: ObjId,
        sources: &Sources,
    ) -> bool {
        let Some(cost) = &def.sneak else {
            return false;
        };
        if self.step != crate::turn::Step::DeclareBlockers
            || p != self.active_player
            || def.additional.sacrifice.is_some()
            || def.additional_or.is_some()
            || self.unblocked_attackers(p).is_empty()
        {
            return false;
        }
        if !self.payable_for(
            p,
            sources,
            &cost.requirements(0),
            Spend::Spell(&self.printed(self.objects[id].def)),
        ) || !self.additional_payable(p, &def.additional, id)
        {
            return false;
        }
        match def.modes() {
            Some(modes) => modes
                .iter()
                .any(|m| self.targets_available(p, &m.targets, Some(id))),
            None => self.spell_targets_available(p, def, id),
        }
    }

    /// Whether `p` can pay a spell's additional costs (CR 601.2h): enough
    /// life (CR 119.4), other cards to discard, something to sacrifice,
    /// cards to exile from the graveyard. The spell's own card is never one
    /// of them, wherever it is cast from.
    pub(super) fn additional_payable(&self, p: PlayerId, add: &Cost, spell: ObjId) -> bool {
        let ps = self.player(p);
        if add.life > 0 && ps.life < i32::try_from(add.life).unwrap_or(i32::MAX) {
            return false;
        }
        if add.discard > 0
            && self
                .discard_candidates(p, add.discard_filter.as_ref(), spell)
                .len()
                < add.discard as usize
        {
            return false;
        }
        if let Some(SacCost::Filter(f)) = &add.sacrifice
            && self.sacrifice_candidates(p, f, spell).len() < add.chosen_count()
        {
            return false;
        }
        if let Some(g) = &add.exile_cards
            && !g.x
            && self.exile_candidates(p, g.filter.as_ref(), spell).len() < g.n as usize
        {
            return false;
        }
        if let Some(b) = &add.behold
            && self.behold_candidates(p, b, spell).len() < b.n as usize
        {
            return false;
        }
        // CR 701.68b, 701.59b.
        self.other_costs_payable(p, add, spell)
    }

    /// What `p` could behold for `b` (CR 701.69a): matching permanents
    /// they control, then matching cards in their hand, never `this` (the
    /// spell being cast). For "and exile it" the hand comes first: the
    /// engine picks, and keeping its permanents is usually better.
    pub(super) fn behold_candidates(
        &self,
        p: PlayerId,
        b: &crate::card::Behold,
        this: ObjId,
    ) -> SmallVec<[ObjId; 4]> {
        let mut board: SmallVec<[ObjId; 4]> = self
            .permanents_of(p)
            .filter(|s| {
                *s != this
                    && self.characteristics(*s).is_some_and(|c| {
                        b.filter
                            .matches(*s, self.objects[*s].controller, &c, Some(this), Some(p))
                    })
            })
            .collect();
        let hand = self.player(p).hand.iter().copied().filter(|h| {
            *h != this
                && self
                    .characteristics(*h)
                    .is_some_and(|c| b.filter.matches_characteristics(&c))
        });
        if b.exile {
            let mut out: SmallVec<[ObjId; 4]> = hand.collect();
            out.extend(board);
            out
        } else {
            board.extend(hand);
            board
        }
    }

    /// Pay a behold cost (CR 701.69a): choose the permanents or reveal
    /// the cards; "and exile it" exiles them, linked to `this` (CR
    /// 607.2a).
    #[cold]
    fn pay_behold(&mut self, p: PlayerId, b: &crate::card::Behold, this: ObjId) {
        let picks: SmallVec<[ObjId; 4]> = self
            .behold_candidates(p, b, this)
            .into_iter()
            .take(b.n as usize)
            .collect();
        if picks.len() < b.n as usize {
            return;
        }
        for id in picks {
            if self.objects[id].zone == Zone::Hand {
                self.reveal_in_hand(id);
            }
            if b.exile {
                let new = self.move_object(id, Zone::Exile, true);
                if self.objects.get(new).is_some_and(|o| o.zone == Zone::Exile) {
                    self.linked_exiles.push(crate::state::LinkedExile {
                        source: this,
                        card: new,
                        until: false,
                        to_hand: false,
                    });
                }
            }
        }
        if let Some(o) = self.objects.get_mut(this) {
            o.cast.set_beheld();
        }
    }

    /// Whether either of a spell's additional costs ("sacrifice a creature
    /// or pay {3}") could be paid, apart from mana.
    fn some_additional_payable(&self, p: PlayerId, def: &crate::card::CardDef, id: ObjId) -> bool {
        self.additional_payable(p, &def.additional, id)
            || def
                .additional_or
                .as_deref()
                .is_some_and(|c| self.additional_payable(p, c, id))
    }

    /// Whether a spell whose mana cost is `req` could be cast paying one
    /// of its additional costs (CR 601.2b, 601.2h): with two ("sacrifice
    /// a creature or pay {3}"), either; one that taps permanents ("tap
    /// three untapped white creatures you control") leaves enough mana.
    #[cold]
    #[inline(never)]
    fn additional_options_payable(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        id: ObjId,
        sources: &Sources,
        req: &ManaRequirement,
    ) -> bool {
        std::iter::once(&def.additional)
            .chain(def.additional_or.as_deref())
            .any(|c| self.cost_option_payable(p, c, id, sources, *req, true))
    }

    /// Whether `p` could pay `req` plus the additional cost `c` of the
    /// spell `id`.
    pub(super) fn cost_option_payable(
        &self,
        p: PlayerId,
        c: &Cost,
        id: ObjId,
        sources: &Sources,
        mut req: ManaRequirement,
        add_mana: bool,
    ) -> bool {
        if !self.additional_payable(p, c, id) {
            return false;
        }
        if add_mana {
            req.add(&c.mana.requirements(0));
        }
        let ps = self.player(p);
        if let (true, Some(SacCost::Filter(f))) = (c.taps_chosen(), &c.sacrifice) {
            let Some(picks) = self.tap_picks(p, f, id, false, c.chosen_count(), None, sources)
            else {
                return false;
            };
            let rest = without(sources, &picks);
            return self.payable(p, &rest, &ps.mana_pool, &req, ps.life, None);
        }
        self.payable(p, sources, &ps.mana_pool, &req, ps.life, None)
    }

    /// The permanents a "tap two untapped creatures you control" cost of
    /// `source` taps (CR 118.3): `first` if chosen, then the engine's
    /// pick of the rest, preferring ones that make no mana (in `sources`)
    /// so the mana can still be paid. `None` if there aren't `n`. The
    /// source isn't one of them when its own {T} is also paid
    /// (`skip_source`).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn tap_picks(
        &self,
        p: PlayerId,
        f: &crate::card::ObjFilter,
        source: ObjId,
        skip_source: bool,
        n: usize,
        first: Option<ObjId>,
        sources: &Sources,
    ) -> Option<SmallVec<[ObjId; 4]>> {
        let mut cands = self.sacrifice_candidates(p, f, source);
        if skip_source {
            cands.retain(|c| *c != source);
        }
        let mut picks: SmallVec<[ObjId; 4]> = SmallVec::new();
        if let Some(c) = first {
            if !cands.contains(&c) {
                return None;
            }
            picks.push(c);
        }
        let makes_mana = |c: &ObjId| sources.iter().any(|(s, _)| s == c);
        // Stable: the permanents' order otherwise.
        cands.sort_by_key(makes_mana);
        for c in cands {
            if picks.len() >= n {
                break;
            }
            if !picks.contains(&c) {
                picks.push(c);
            }
        }
        (picks.len() >= n).then_some(picks)
    }

    /// Cards in `p`'s graveyard a "exile a creature card from your
    /// graveyard" cost of `source` could exile, oldest first.
    pub(super) fn exile_candidates(
        &self,
        p: PlayerId,
        filter: Option<&crate::card::ObjFilter>,
        source: ObjId,
    ) -> SmallVec<[ObjId; 8]> {
        self.player(p)
            .graveyard
            .iter()
            .copied()
            .filter(|id| {
                *id != source
                    && filter.is_none_or(|f| {
                        self.characteristics(*id)
                            .is_some_and(|c| f.matches(*id, p, &c, Some(source), Some(p)))
                    })
            })
            .collect()
    }

    /// Which of `spell`'s two additional costs its caster could pay
    /// (CR 601.2b): `Mode(0)` the first, `Mode(1)` the other.
    pub(super) fn additional_cost_options(&self, spell: ObjId) -> Vec<Choice> {
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let sources = self.spell_sources(spell);
        std::iter::once(&def.additional)
            .chain(def.additional_or.as_deref())
            .enumerate()
            .filter(|(i, c)| {
                let req = self.spell_requirements_as(spell, 0, *i == 1);
                self.cost_option_payable(o.controller, c, spell, &sources, req, false)
            })
            .map(|(i, _)| Choice::Mode(i))
            .collect()
    }

    /// The permanents `spell`'s caster could tap first for its "tap two
    /// untapped creatures you control" additional cost `add`, leaving the
    /// mana payable.
    pub(super) fn tap_first_options(&self, spell: ObjId, add: &Cost) -> Vec<ObjId> {
        let Some(SacCost::Filter(f)) = &add.sacrifice else {
            return Vec::new();
        };
        let p = self.objects[spell].controller;
        let sources = self.spell_sources(spell);
        let req = self.spell_requirements(spell, self.x_of(spell));
        let ps = self.player(p);
        self.sacrifice_candidates(p, f, spell)
            .into_iter()
            .filter(|c| {
                self.tap_picks(p, f, spell, false, add.chosen_count(), Some(*c), &sources)
                    .is_some_and(|picks| {
                        self.payable(
                            p,
                            &without(&sources, &picks),
                            &ps.mana_pool,
                            &req,
                            ps.life,
                            None,
                        )
                    })
            })
            .collect()
    }

    /// The additional cost a spell being cast pays: the second of two if
    /// that was chosen (CR 601.2b).
    pub(super) fn additional_of(&self, spell: ObjId) -> &Cost {
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        // "Flashback—Sacrifice three creatures.": the flashback cost's
        // chosen permanents are settled like an additional cost's (CR
        // 702.34a, 601.2h); such a card has no additional cost of its own.
        if let Some(fb) = def.flashback.as_deref()
            && (fb.sacrifice.is_some() || fb.discard > 0)
            && o.cast.flashback()
        {
            return fb;
        }
        // "Kicker—Sacrifice an artifact or creature.": likewise (CR
        // 702.33a); such a card has no additional cost either.
        if let Some(k) = def.kicker_cost.as_deref()
            && o.cast.kicked()
        {
            return k;
        }
        match &def.additional_or {
            Some(c) if o.cast.other_cost() => c,
            _ => &def.additional,
        }
    }

    pub(super) fn discard_candidates(
        &self,
        p: PlayerId,
        filter: Option<&crate::card::ObjFilter>,
        source: ObjId,
    ) -> Vec<ObjId> {
        // Grandeur: "Discard another card named ~" (the source's name).
        let filled = filter
            .filter(|f| f.same.is_some())
            .map(|f| self.with_same_of(f, Some(source)));
        let filter = filled.as_ref().or(filter);
        self.player(p)
            .hand
            .iter()
            .copied()
            .filter(|id| {
                *id != source
                    && filter.is_none_or(|f| {
                        self.characteristics(*id)
                            .is_some_and(|c| f.matches(*id, p, &c, Some(source), Some(p)))
                            && self.status_ok(f, *id)
                    })
            })
            .collect()
    }

    /// Whether `spell` (being cast) has a kicker its caster could pay on
    /// top of its cost (CR 702.33b).
    /// For a spell with `CardDef::modes_if` whose condition is a cost
    /// announced with the modes ("If this spell was kicked / cast using
    /// teamwork / its additional cost was paid, choose both instead"):
    /// whether that cost can be paid now. `None` for other conditions.
    pub(super) fn mode_linked_cost(&self, spell: ObjId) -> Option<bool> {
        use crate::card::StaticCondition as C;
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let mi = def.modes_if?;
        Some(match self.pool.condition(mi.cond) {
            C::Kicked => self.kicker_payable(spell),
            C::Teamwork => self.teamwork_tappers(o.controller, def.teamwork).is_some(),
            C::OptionalCostPaid => def
                .optional_cost
                .as_deref()
                .is_some_and(|c| self.other_costs_payable(o.controller, c, spell)),
            _ => return None,
        })
    }

    /// The answer a kicker, teamwork, or optional-cost decision must take
    /// for a spell whose modes depend on that cost (`mode_linked_cost`):
    /// paid with the larger choice of modes; declined with one mode when
    /// "choose both instead" requires both. `None`: either answer.
    pub(super) fn linked_cost_answer(&self, spell: ObjId) -> Option<bool> {
        self.mode_linked_cost(spell)?;
        let def = self.pool.get(self.objects[spell].def);
        let must = def.modes_if.is_some_and(|m| m.must);
        if self.mode_split_of(spell).is_some() {
            Some(true)
        } else if must {
            Some(false)
        } else {
            None
        }
    }

    pub(super) fn kicker_payable(&self, spell: ObjId) -> bool {
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let Some(k) = &def.kicker else {
            return false;
        };
        if o.cast.flashback() {
            return false;
        }
        // "Kicker—Sacrifice two lands.": enough to sacrifice (or return),
        // and the life to pay (CR 119.4).
        if let Some(c) = def.kicker_cost.as_deref() {
            if let Some(SacCost::Filter(f)) = &c.sacrifice
                && self.sacrifice_candidates(o.controller, f, spell).len() < c.chosen_count()
            {
                return false;
            }
            if i64::from(c.life) > i64::from(self.player(o.controller).life)
                || !self.other_costs_payable(o.controller, c, spell)
            {
                return false;
            }
        }
        let mut req = def.mana_cost.requirements(0);
        req.add(&k.requirements(0));
        adjust_generic(
            &mut req,
            self.cost_delta(o.controller, def, spell, &self.cost_changers()),
        );
        self.spell_payable(o.controller, spell, &req)
    }

    /// Whether `spell` (being cast) has a buyback cost its caster could pay
    /// on top of its cost and any kicker (CR 702.27).
    pub(super) fn buyback_payable(&self, spell: ObjId) -> bool {
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let Some(b) = &def.buyback else {
            return false;
        };
        if o.cast.flashback() {
            return false;
        }
        if let Some(f) = def.buyback_sacrifice.as_deref()
            && self.sacrifice_candidates(o.controller, f, spell).is_empty()
        {
            return false;
        }
        let mut req = self.spell_requirements(spell, 0);
        req.add(&b.requirements(0));
        self.spell_payable(o.controller, spell, &req)
    }

    /// Whether `spell` (being cast) has an offspring cost its caster could
    /// pay on top of its cost (CR 702.175a).
    pub(super) fn offspring_payable(&self, spell: ObjId) -> bool {
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let Some(c) = &def.offspring else {
            return false;
        };
        let mut req = self.spell_requirements(spell, 0);
        req.add(&c.requirements(0));
        self.spell_payable(o.controller, spell, &req)
    }

    /// Soulbond's pairing (CR 702.95a, c): the source with a creature
    /// chosen from the unpaired creatures its controller controls, or
    /// with the creature that entered; either may be declined.
    #[cold]
    #[inline(never)]
    fn soulbond(&mut self, ctx: &EffectCtx, triggering: bool) {
        let p = ctx.controller;
        let eligible = |s: &Self, id: ObjId| {
            s.objects
                .get(id)
                .is_some_and(|o| o.zone == Zone::Battlefield && o.controller == p)
                && s.characteristics(id).is_some_and(|c| c.is_creature())
                && s.partner(id).is_none()
        };
        let Some(src) = self
            .current_object(ctx.source)
            .filter(|s| eligible(self, *s))
        else {
            return;
        };
        let candidates: Vec<ObjId> = if triggering {
            ctx.trigger_obj
                .and_then(|t| self.current_object(t))
                .filter(|t| *t != src && eligible(self, *t))
                .into_iter()
                .collect()
        } else {
            self.creatures_of(p)
                .filter(|c| *c != src && eligible(self, *c))
                .collect()
        };
        if candidates.is_empty() {
            return;
        }
        let mut options: Vec<Choice> = candidates.iter().map(|c| Choice::Sacrifice(*c)).collect();
        options.push(Choice::Decline);
        let k = self.choose_mid(p, DecisionKind::ChooseCard, options);
        if let Some(other) = candidates.get(k) {
            self.pairs.push((src, *other));
            self.cbump();
        }
    }

    /// Transform a double-faced permanent to its other face (CR 701.27);
    /// it stays the same object. Whether it did.
    /// The creature named as `source`'s meld pair (its `Effect::Meld`)
    /// that `you` own and control, if `you` also own and control `source`
    /// on the battlefield (CR 701.42a's "if you both own and control ...").
    pub(super) fn meld_partner(&self, source: ObjId, you: PlayerId) -> Option<ObjId> {
        let o = self.objects.get(source)?;
        if o.zone != Zone::Battlefield || o.owner != you || o.controller != you {
            return None;
        }
        let partner = self.pool.get(o.def).abilities.iter().find_map(|a| {
            a.effects().iter().find_map(|e| match e {
                Effect::Meld { partner, .. } => Some(partner.clone()),
                _ => None,
            })
        })?;
        self.permanents_of(you).find(|id| {
            *id != source
                && self.objects[*id].owner == you
                && self.name_of(*id) == &*partner
                && self.characteristics(*id).is_some_and(|c| c.is_creature())
        })
    }

    /// CR 701.42a, 712.4a: exile `source` and its pair, then put them onto
    /// the battlefield as one permanent with `into`'s characteristics, if
    /// both are still the pair's cards in exile (CR 701.42b-c: a token, or
    /// a card a replacement moved elsewhere, isn't melded; the cards stay
    /// where they are).
    fn meld(&mut self, source: ObjId, you: PlayerId, partner: &str, into: &str) {
        let Some(other) = self.meld_partner(source, you) else {
            return;
        };
        let Some(into) = self.pool.lookup(into) else {
            return;
        };
        let a = self.move_object(source, Zone::Exile, true);
        let b = self.move_object(other, Zone::Exile, true);
        let pair = |st: &Self, id: ObjId, name: Option<&str>| {
            st.objects.get(id).is_some_and(|o| {
                o.zone == Zone::Exile
                    && o.kind == crate::state::ObjKind::Card
                    && name.is_none_or(|n| st.pool.get(o.card).name == n)
            })
        };
        if !pair(self, a, None) || !pair(self, b, Some(partner)) {
            return;
        }
        self.objects[a].def = into;
        let new = self.put_onto_battlefield(a, you);
        if self
            .objects
            .get(new)
            .is_some_and(|o| o.zone == Zone::Battlefield)
        {
            self.melded.push((new, b));
        }
    }

    /// CR 710.2, 710.4: an unflipped flip card on the battlefield turns to
    /// its flipped half; it stays the same object. Nothing for anything
    /// else (a flipped permanent's definition has no faces).
    pub(super) fn flip_now(&mut self, id: ObjId) {
        let Some(o) = self.objects.get(id) else {
            return;
        };
        let d = self.pool.get(o.def);
        if o.zone != Zone::Battlefield || d.layout != Layout::Flip {
            return;
        }
        let Some(flipped) = d.faces.first().copied() else {
            return;
        };
        let legendary = d.is_legendary();
        let flags = self.flags_of(&self.objects[id]);
        self.count_sources(flags, legendary, false);
        self.objects[id].def = flipped;
        let flags = self.flags_of(&self.objects[id]);
        self.count_sources(flags, self.pool.get(flipped).is_legendary(), true);
        self.cbump();
    }

    pub(super) fn transform_now(&mut self, id: ObjId) -> bool {
        let Some(o) = self.objects.get(id) else {
            return false;
        };
        if o.zone != Zone::Battlefield {
            return false;
        }
        let d = self.pool.get(o.def);
        // CR 712.3: a modal double-faced card may transform too.
        if !matches!(d.layout, Layout::Transform | Layout::ModalDfc) {
            return false;
        }
        let Some(other) = d.faces.first().copied() else {
            return false;
        };
        let legendary = d.is_legendary();
        let flags = self.flags_of(&self.objects[id]);
        self.count_sources(flags, legendary, false);
        self.objects[id].def = other;
        let flags = self.flags_of(&self.objects[id]);
        self.count_sources(flags, self.pool.get(other).is_legendary(), true);
        self.emit(Event::Transformed { obj: id });
        self.cbump();
        true
    }

    /// How many times `spell`'s caster could pay its multikicker cost on
    /// top of its cost (CR 702.33c), at most 15.
    pub(super) fn multikicker_max(&self, spell: ObjId) -> u32 {
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let Some(c) = &def.multikicker else {
            return 0;
        };
        if o.cast.flashback() {
            return 0;
        }
        let one = c.requirements(0);
        let mut req = self.spell_requirements(spell, 0);
        let mut n = 0;
        while n < 15 {
            req.add(&one);
            if !self.spell_payable(o.controller, spell, &req) {
                break;
            }
            n += 1;
        }
        n
    }

    /// How many times `spell`'s caster could pay its replicate cost on top
    /// of its cost (CR 702.56a), at most 15.
    pub(super) fn replicate_max(&self, spell: ObjId) -> u32 {
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let Some(c) = &def.replicate else {
            return 0;
        };
        if o.cast.flashback() {
            return 0;
        }
        let one = c.requirements(0);
        let mut req = self.spell_requirements(spell, 0);
        let mut n = 0;
        while n < 15 {
            req.add(&one);
            if !self.spell_payable(o.controller, spell, &req) {
                break;
            }
            n += 1;
        }
        n
    }

    /// The mana a spell being cast costs with X fixed: its mana cost (or
    /// its flashback cost, CR 702.34a) plus its kicker if kicked.
    pub(super) fn spell_requirements(&self, spell: ObjId, x: u32) -> ManaRequirement {
        self.spell_requirements_as(spell, x, self.objects[spell].cast.other_cost())
    }

    /// The splice costs of the cards spliced onto `spell` (CR 702.47a,
    /// 601.2f), strive's cost once for each target beyond the first, and
    /// the replicate cost once for each time it was paid (CR 702.56a).
    fn add_splice_and_strive(
        &self,
        spell: ObjId,
        def: &crate::card::CardDef,
        targets: usize,
        req: &mut ManaRequirement,
    ) {
        if !self.splices.is_empty()
            && let Some(sp) = self.splices.iter().find(|s| s.spell == spell)
        {
            for (_, d) in &sp.cards {
                if let Some(c) = &self.pool.get(*d).splice {
                    req.add(&c.mana.requirements(0));
                }
            }
        }
        if let Some(s) = &def.strive {
            for _ in 1..targets {
                req.add(&s.requirements(0));
            }
        }
        // CR 702.56a: the replicate cost, once for each time it was paid.
        if let Some(r) = &def.replicate {
            for _ in 0..self.objects[spell].cast.replicated() {
                req.add(&r.requirements(0));
            }
        }
    }

    /// `spell_requirements`, paying the second of two additional costs if
    /// `other` (CR 601.2b, 601.2f: the total cost includes it).
    pub(super) fn spell_requirements_as(
        &self,
        spell: ObjId,
        x: u32,
        other: bool,
    ) -> ManaRequirement {
        self.spell_requirements_mode(spell, x, other, self.objects[spell].cast.mode())
    }

    /// `spell_requirements_as` with `mode` chosen, for a spree spell's
    /// mode costs (CR 702.172a).
    #[allow(clippy::too_many_lines)] // one arm per alternative cost
    fn spell_requirements_mode(
        &self,
        spell: ObjId,
        x: u32,
        other: bool,
        mode: u8,
    ) -> ManaRequirement {
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let mut req = match (&def.flashback, o.cast.flashback()) {
            (Some(fb), true) => fb.mana.requirements(x),
            // Cast without paying its mana cost: X is 0 (CR 107.3b).
            _ if matches!(o.alt, AltCast::Free | AltCast::Suspend | AltCast::FreeExile) => {
                ManaRequirement::default()
            }
            (_, false) if o.alt == AltCast::Madness && def.madness.is_some() => {
                def.madness.as_ref().expect("checked").requirements(x)
            }
            (_, false) if o.alt == AltCast::Miracle && def.miracle.is_some() => {
                def.miracle.as_ref().expect("checked").requirements(x)
            }
            (_, false) if o.alt == AltCast::Foretell && def.foretell.is_some() => {
                def.foretell.as_ref().expect("checked").requirements(x)
            }
            (_, false) if o.alt == AltCast::Escape && def.escape.is_some() => {
                def.escape.as_ref().expect("checked").0.requirements(x)
            }
            (_, false) if o.alt == AltCast::Mayhem && def.mayhem.is_some() => {
                def.mayhem.as_ref().expect("checked").requirements(x)
            }
            (_, false) if o.alt == AltCast::Bestow && def.bestow.is_some() => {
                def.bestow.as_ref().expect("checked").requirements(x)
            }
            (_, false) if o.alt == AltCast::Dash && def.dash.is_some() => {
                def.dash.as_ref().expect("checked").requirements(x)
            }
            (_, false) if o.alt == AltCast::Warp && def.warp.is_some() => {
                def.warp.as_ref().expect("checked").requirements(x)
            }
            // CR 701.65a: {2} rather than its mana cost (X is 0).
            (_, false) if o.alt == AltCast::Airbend => ManaRequirement {
                generic: 2,
                ..ManaRequirement::default()
            },
            (_, false) if o.alt == AltCast::Sneak && def.sneak.is_some() => {
                def.sneak.as_ref().expect("checked").requirements(x)
            }
            (_, false) if o.alt == AltCast::Impending && def.impending.is_some() => {
                def.impending.as_ref().expect("checked").1.requirements(x)
            }
            (_, false) if o.alt == AltCast::WebSlinging && def.web_slinging.is_some() => {
                def.web_slinging.as_ref().expect("checked").requirements(x)
            }
            // CR 702.180a: cast from the graveyard (as with flashback, the
            // spell is exiled as it leaves the stack).
            _ if o.alt == AltCast::Harmonize => def
                .harmonize
                .as_deref()
                .unwrap_or(&def.mana_cost)
                .requirements(x),
            (_, false) if o.alt == AltCast::Overload && def.overload.is_some() => {
                def.overload.as_ref().expect("checked").requirements(x)
            }
            // The face has no awaken of its own: the card's cost.
            (_, false) if o.alt == AltCast::Awaken && self.pool.get(o.card).awaken.is_some() => {
                self.pool
                    .get(o.card)
                    .awaken
                    .as_ref()
                    .expect("checked")
                    .1
                    .requirements(x)
            }
            // The back face has no disturb of its own: the card's cost.
            (_, false) if o.alt == AltCast::Disturb && self.pool.get(o.card).disturb.is_some() => {
                self.pool
                    .get(o.card)
                    .disturb
                    .as_ref()
                    .expect("checked")
                    .requirements(x)
            }
            (_, false) if o.alt == AltCast::AltCost && def.alt_cost.is_some() => def
                .alt_cost
                .as_ref()
                .expect("checked")
                .cost
                .mana
                .requirements(x),
            (_, false) if o.cast.evoked() && def.evoke.is_some() => {
                def.evoke.as_ref().expect("checked").requirements(x)
            }
            // CR 702.37c: a face-down spell costs {3}.
            _ if self.pool.is_face_down(o.def) => self.face_down_cost(o.controller),
            _ => def.mana_cost.requirements(x),
        };
        // CR 107.3: "Spend only black mana on X." X is then paid as that
        // many black symbols.
        if x > 0
            && req.generic >= x
            && let Some(c) = def
                .ability_ids
                .iter()
                .find_map(|a| match self.pool.ability(*a) {
                    AbilityDef::XManaColor(c) => Some(*c),
                    _ => None,
                })
        {
            req.generic -= x;
            req.colored[c as usize] += x;
        }
        if o.cast.kicked()
            && let Some(k) = &def.kicker
        {
            req.add(&k.requirements(0));
        }
        // CR 702.33c: the multikicker cost, once for each time it was paid.
        if let Some(k) = &def.multikicker {
            for _ in 0..o.cast.kicks() {
                req.add(&k.requirements(0));
            }
        }
        if o.cast.bought_back()
            && let Some(b) = &def.buyback
        {
            req.add(&b.requirements(0));
        }
        if o.cast.offspring()
            && let Some(b) = &def.offspring
        {
            req.add(&b.requirements(0));
        }
        self.add_splice_and_strive(spell, def, o.targets.len(), &mut req);
        // CR 702.42a: every mode, for the entwine cost on top.
        if let Some(e) = &def.entwine
            && self.mode_split_of(spell).is_some()
        {
            req.add(&e.requirements(0));
        }
        // CR 702.120a: escalate, once for each mode beyond the first.
        if let Some(e) = &def.escalate {
            for _ in 1..(u32::from(mode) + 1).count_ones() {
                req.add(&e.requirements(0));
            }
        }
        // CR 702.172a: the chosen spree modes' costs.
        if !def.spree.is_empty() {
            let bits = u32::from(mode) + 1;
            for (i, c) in def.spree.iter().enumerate() {
                if bits & (1 << i) != 0 {
                    req.add(&c.requirements(0));
                }
            }
        }
        // "As an additional cost to cast this spell, sacrifice a creature
        // or pay {3}".
        if let Some(c) = def.additional_or.as_deref().filter(|_| other)
            && !c.mana.is_empty()
        {
            req.add(&c.mana.requirements(0));
        } else if !other && !def.additional.mana.is_empty() {
            req.add(&def.additional.mana.requirements(0));
        }
        if def.self_cost.is_some()
            || def.cond_cost.is_some()
            || !self.cost_changers().is_empty()
            || self.next_spell_less[o.controller.index().min(3)] > 0
        {
            adjust_generic(
                &mut req,
                self.cost_delta(o.controller, def, spell, &self.cost_changers()),
            );
        }
        if let Some(tc) = &def.target_cost
            && let Some(t) = o.targets.first()
            && self.target_fits(&tc.1, *t, o.controller, spell)
        {
            adjust_generic(&mut req, tc.0);
        }
        // CR 702.180a: reduced by the tapped creature's power.
        if o.alt == AltCast::Harmonize
            && let Some((_, _, power)) = self.harmonized.iter().find(|(s, ..)| *s == spell)
        {
            adjust_generic(&mut req, -i32::try_from(*power).unwrap_or(i32::MAX));
        }
        req
    }

    /// Mayhem: a card its owner discarded this turn, in their graveyard,
    /// castable for its mayhem cost; timing rules still apply. A land
    /// isn't cast.
    #[cold]
    #[inline(never)]
    fn mayhem_castable(&self, p: PlayerId, card: ObjId, sorcery_timing: bool) -> bool {
        let def = self.pool.get(self.objects[card].def);
        let Some(cost) = def.mayhem.as_deref() else {
            return false;
        };
        if def.is_land() || !self.mayhem_discarded.contains(&card) {
            return false;
        }
        let instant = def.is_instant() || self.has_flash(p, def, card);
        if !(instant || sorcery_timing) {
            return false;
        }
        let mut req = cost.requirements(0);
        adjust_generic(
            &mut req,
            self.cost_delta(p, def, card, &self.cost_changers()),
        );
        if !self.spell_payable(p, card, &req) {
            return false;
        }
        match def.modes() {
            Some(modes) => modes
                .iter()
                .any(|m| self.targets_available(p, &m.targets, Some(card))),
            None => self.spell_targets_available(p, def, card),
        }
    }

    /// CR 702.138a: a card with escape in the graveyard, castable for its
    /// escape cost with enough other cards to exile.
    #[cold]
    #[inline(never)]
    fn escape_castable(&self, p: PlayerId, card: ObjId, sorcery_timing: bool) -> bool {
        let def = self.pool.get(self.objects[card].def);
        let Some((cost, n)) = def.escape.as_deref() else {
            return false;
        };
        let instant = def.is_instant() || self.has_flash(p, def, card);
        if !(instant || sorcery_timing) || self.player(p).graveyard.len() <= *n as usize {
            return false;
        }
        let mut req = cost.requirements(0);
        adjust_generic(
            &mut req,
            self.cost_delta(p, def, card, &self.cost_changers()),
        );
        if !self.spell_payable(p, card, &req) {
            return false;
        }
        match def.modes() {
            Some(modes) => modes
                .iter()
                .any(|m| self.targets_available(p, &m.targets, Some(card))),
            None => self.spell_targets_available(p, def, card),
        }
    }

    /// Whether `card` (exiled) could be cast `alt`'s way now: a nonland
    /// card with targets for some mode, whose cost (if any) is payable.
    fn exile_castable(&self, p: PlayerId, card: ObjId, alt: AltCast) -> bool {
        self.exile_castable_as(p, card, alt, self.objects[card].def)
    }

    /// `exile_castable`, the spell having the characteristics of `face`
    /// (CR 712.11c: only the face to be cast is evaluated).
    fn exile_castable_as(
        &self,
        p: PlayerId,
        card: ObjId,
        alt: AltCast,
        face: crate::card::CardRef,
    ) -> bool {
        let def = self.pool.get(face);
        if def.is_land() {
            return false;
        }
        let targets_ok = match def.modes() {
            Some(modes) => modes
                .iter()
                .any(|m| self.targets_available(p, &m.targets, Some(card))),
            None => self.spell_targets_available(p, def, card),
        };
        // CR 601.2h: casting it without paying its mana cost still pays its
        // additional costs (Improvised Club's sacrifice).
        if !targets_ok || !self.some_additional_payable(p, def, card) {
            return false;
        }
        let mut req = match alt {
            AltCast::Madness => def.madness.as_ref().map(|c| c.requirements(0)),
            AltCast::Miracle => def.miracle.as_ref().map(|c| c.requirements(0)),
            AltCast::Foretell => def.foretell.as_ref().map(|c| c.requirements(0)),
            // CR 701.65a.
            AltCast::Airbend => Some(ManaRequirement {
                generic: 2,
                ..ManaRequirement::default()
            }),
            // "You may cast the copy" (CR 707.12): for its mana cost.
            AltCast::None => Some(def.mana_cost.requirements(0)),
            _ => return true,
        };
        let Some(mut req) = req.take() else {
            return false;
        };
        adjust_generic(
            &mut req,
            self.cost_delta(p, def, card, &self.cost_changers()),
        );
        self.spell_payable(p, card, &req)
    }

    /// "You may cast it": if `card` can be cast `alt`'s way, its owner
    /// chooses; a yes casts it as the resolution finishes. Whether it will
    /// be cast.
    pub(super) fn offer_exile_cast(&mut self, card: ObjId, alt: AltCast) -> bool {
        let p = self.objects[card].owner;
        self.offer_exile_cast_by(card, alt, p)
    }

    pub(super) fn offer_exile_cast_by(&mut self, card: ObjId, alt: AltCast, p: PlayerId) -> bool {
        let face = self.objects[card].def;
        self.offer_exile_cast_as(card, alt, p, face, card)
    }

    /// `offer_exile_cast_by`, the decision naming `shown` (the encoded card
    /// a copy is made of, which exists before the resolution began).
    pub(super) fn offer_exile_cast_shown(
        &mut self,
        card: ObjId,
        alt: AltCast,
        p: PlayerId,
        shown: ObjId,
    ) -> bool {
        let face = self.objects[card].def;
        self.offer_exile_cast_as(card, alt, p, face, shown)
    }

    /// `offer_exile_cast_by`, cast as `face` (transformed, CR 712.11a),
    /// the decision naming `shown`; the caller remembers the face.
    pub(super) fn offer_exile_cast_as(
        &mut self,
        card: ObjId,
        alt: AltCast,
        p: PlayerId,
        face: crate::card::CardRef,
        shown: ObjId,
    ) -> bool {
        if !self.exile_castable_as(p, card, alt, face) {
            return false;
        }
        let k = self.choose_mid(
            p,
            DecisionKind::CastFromExile { card: shown },
            vec![Choice::Accept, Choice::Decline],
        );
        if k == 0 {
            self.exile_casts
                .get_or_insert_with(Box::default)
                .pending
                .push((p, card, alt));
            true
        } else {
            false
        }
    }

    /// CR 702.85a: exile cards from the top of the library until a nonland
    /// card with lesser mana value; it may be cast free; the rest go to the
    /// bottom in a random order.
    /// Discover N (CR 701.57a) is the same with mana value N or less, and
    /// the card goes to the hand if not cast.
    fn cascade(&mut self, ctx: &EffectCtx, discover: Option<u32>) {
        let p = ctx.controller;
        let mv = if let Some(n) = discover {
            n + 1
        } else {
            let def = self
                .objects
                .get(ctx.source)
                .map(|o| o.def)
                .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def));
            let Some(def) = def else {
                return;
            };
            self.pool.get(def).mana_cost.mana_value()
        };
        let mut exiled: Vec<ObjId> = Vec::new();
        let mut hit = None;
        while let Some(top) = self.player(p).library.last().copied() {
            let new = self.move_object(top, Zone::Exile, true);
            let d = self.pool.get(self.objects[new].def);
            if !d.is_land() && d.mana_cost.mana_value() < mv {
                hit = Some(new);
                break;
            }
            exiled.push(new);
        }
        if let Some(h) = hit
            && !self.offer_exile_cast_by(h, AltCast::Free, p)
        {
            if discover.is_some() {
                self.move_object(h, Zone::Hand, false);
            } else {
                exiled.push(h);
            }
        }
        rand::seq::SliceRandom::shuffle(exiled.as_mut_slice(), &mut self.rng);
        for id in exiled {
            if self.objects.get(id).is_some_and(|o| o.zone == Zone::Exile) {
                self.move_object(id, Zone::Library, false);
            }
        }
    }

    /// CR 702.60a: reveal the top N cards (set aside in exile while the
    /// player chooses); each with the source's name may be cast without
    /// paying its mana cost; the rest go on the bottom in their order.
    fn ripple(&mut self, ctx: &EffectCtx) {
        let p = ctx.controller;
        let def = self
            .objects
            .get(ctx.source)
            .map(|o| o.def)
            .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def));
        let Some(def) = def else {
            return;
        };
        let d = self.pool.get(def);
        let (n, name) = (d.ripple, d.name.clone());
        let mut revealed = Vec::new();
        for _ in 0..n {
            let Some(top) = self.player(p).library.last().copied() else {
                break;
            };
            revealed.push(self.move_object(top, Zone::Exile, true));
        }
        let mut rest = Vec::new();
        for id in revealed {
            let same = self
                .objects
                .get(id)
                .is_some_and(|o| o.zone == Zone::Exile && self.pool.get(o.def).name == name);
            if !(same && self.offer_exile_cast_by(id, AltCast::Free, p)) {
                rest.push(id);
            }
        }
        for id in rest {
            if self.objects.get(id).is_some_and(|o| o.zone == Zone::Exile) {
                self.move_object(id, Zone::Library, false);
            }
        }
    }

    /// CR 702.110a: the controller may sacrifice a creature; if they do,
    /// the source's "when this exploits a creature" abilities trigger.
    fn exploit(&mut self, ctx: &EffectCtx) {
        let p = ctx.controller;
        let mut options: Vec<Choice> = self.creatures_of(p).map(Choice::Sacrifice).collect();
        if options.is_empty() {
            return;
        }
        options.push(Choice::Decline);
        let k = self.choose_mid(p, DecisionKind::Edict, options.clone());
        let Choice::Sacrifice(victim) = options[k] else {
            return;
        };
        let def = self
            .objects
            .get(ctx.source)
            .map(|o| o.def)
            .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def));
        self.sacrifice(victim);
        let Some(def) = def else {
            return;
        };
        let pool = std::sync::Arc::clone(&self.pool);
        for a in &pool.get(def).ability_ids {
            if let AbilityDef::Triggered(t) = pool.ability(*a)
                && t.trigger == crate::card::TriggerSpec::Exploits
            {
                self.queue_builtin(p, *a, ctx.source, def);
            }
        }
    }

    /// CR 702.99a: as a spell with cipher resolves, its controller may
    /// exile it encoded on a creature they control; whether they did.
    #[cold]
    #[inline(never)]
    fn encode(&mut self, spell: ObjId, p: PlayerId) -> bool {
        let options: Vec<Choice> = self
            .creatures_of(p)
            .map(|id| Choice::Target(Target::Object(id)))
            .collect();
        if options.is_empty() {
            return false;
        }
        let def = self.objects[spell].def;
        if self.choose_mid(
            p,
            DecisionKind::YouMay { def },
            vec![Choice::Accept, Choice::Decline],
        ) != 0
        {
            return false;
        }
        let k = self.choose_mid(p, DecisionKind::ChooseCard, options.clone());
        let Choice::Target(Target::Object(creature)) = options[k] else {
            return false;
        };
        let card = self.move_object(spell, Zone::Exile, true);
        self.encoded.push((card, creature));
        true
    }

    /// CR 702.99a: a creature with encoded cards dealt combat damage to a
    /// player: each encoded card's trigger, controlled by the creature's
    /// controller. Lapsed encodings are dropped (702.99c).
    #[cold]
    #[inline(never)]
    pub(super) fn cipher_triggers(&mut self, creature: ObjId) {
        self.encoded.retain(|(card, c)| {
            self.objects
                .get(*card)
                .is_some_and(|o| o.zone == Zone::Exile)
                && self
                    .objects
                    .get(*c)
                    .is_some_and(|o| o.zone == Zone::Battlefield)
        });
        let cards: Vec<ObjId> = self
            .encoded
            .iter()
            .filter(|(_, c)| *c == creature)
            .map(|(card, _)| *card)
            .collect();
        let controller = self.objects[creature].controller;
        let ability = self.pool.builtin(crate::card::Builtin::Cipher);
        for card in cards {
            let def = self.objects[card].def;
            self.queue_builtin(controller, ability, card, def);
        }
    }

    /// CR 702.34a: a spell cast with flashback is exiled instead of going
    /// anywhere else as it leaves the stack.
    pub(super) fn leave_stack(&mut self, spell: ObjId) {
        // CR 704.5e: a copy of a spell ceases to exist off the stack.
        if self.objects[spell].cast.copy() {
            self.remove_from_game(spell);
            return;
        }
        // CR 702.81a: retrace casts from the graveyard without exiling.
        let o = &self.objects[spell];
        let zone = if (o.cast.flashback() && !self.pool.get(o.def).retrace)
            || o.alt == AltCast::FreeExile
        {
            Zone::Exile
        } else {
            Zone::Graveyard
        };
        self.move_object(spell, zone, true);
    }

    /// CR 602: activated abilities of permanents `p` controls whose costs
    /// can be paid and whose targets exist. An ability with a "sacrifice a
    /// Goblin" cost is offered once per permanent that could be sacrificed.
    #[allow(clippy::too_many_lines)] // one check per cost kind
    fn activation_options(
        &self,
        p: PlayerId,
        sorcery_timing: bool,
        sources: &Sources,
        options: &mut Vec<Choice>,
    ) {
        let grants = self.grants_exist();
        if self.activated_sources == 0 && !grants {
            return;
        }
        let pool = self.player(p).mana_pool;
        let life = self.player(p).life;
        let all_sources = sources;
        for id in self.permanents_of(p) {
            let o = &self.objects[id];
            if !grants && !self.flags_of(o).has_activated() {
                continue;
            }
            let Some(c) = self.characteristics(id) else {
                continue;
            };
            // Restricted mana that may be spent on this source's abilities
            // first (CR 106.6).
            let restricted = self.with_restricted(p, all_sources, Spend::Ability(Some(&c)));
            let sources = restricted.as_ref().unwrap_or(all_sources);
            let sick = c.is_creature() && !o.controlled_since_turn_start && !c.has(Keyword::Haste);
            // CR 602.5: "its activated abilities can't be activated"
            // (Arrest), or all but its mana abilities (CR 605.1a).
            let barred = c.has(Keyword::CantActivate);
            let barred_nonmana = c.has(Keyword::CantActivateNonMana);
            for a in &c.abilities {
                let AbilityDef::Activated(ab) = self.pool.ability(*a) else {
                    continue;
                };
                if barred || (barred_nonmana && !ab.is_mana_ability()) {
                    continue;
                }
                if ab.sorcery_speed && !sorcery_timing {
                    continue;
                }
                if !self.activation_allowed(ab, *a, p, id) {
                    continue;
                }
                if ab.from_hand
                    || ab.from_graveyard
                    || (ab.per_turn > 0
                        && self
                            .activated_once
                            .iter()
                            .filter(|x| **x == (id, *a))
                            .count()
                            >= usize::from(ab.per_turn))
                {
                    continue;
                }
                if let Some(k) = ab.keyword
                    && !self.keyword_allows(k, id, *a)
                {
                    continue;
                }
                // CR 302.6: a creature's {T} needs it to have been controlled
                // since the turn began.
                if ab.cost.tap && (o.tapped || sick) {
                    continue;
                }
                let req = self.activation_mana(id, ab);
                // CR 701.67a: artifacts and creatures help pay a waterbend
                // cost.
                let bent: Sources;
                let sources = if ab.cost.waterbend {
                    bent = sources
                        .iter()
                        .copied()
                        .chain(self.waterbend_sources(p, req.generic, ab.cost.tap.then_some(id)))
                        .collect();
                    &bent
                } else {
                    sources
                };
                // A source tapped as a cost cannot also tap for mana.
                let payable = if ab.cost.tap {
                    let rest: Sources = sources.iter().filter(|(s, _)| *s != id).copied().collect();
                    self.payable(p, &rest, &pool, &req, life, Some(id))
                } else {
                    self.payable(p, sources, &pool, &req, life, Some(id))
                };
                if !payable {
                    continue;
                }
                if !self.targets_available(p, &ab.targets, Some(id)) {
                    continue;
                }
                // A modal ability needs a mode it can choose (CR 700.2).
                if let [crate::card::Effect::Modes(modes)] = ab.effects.as_slice()
                    && !modes.iter().enumerate().any(|(i, m)| {
                        !self.mode_used(id, *a, i, m)
                            && self.targets_available(p, &m.targets, Some(id))
                    })
                {
                    continue;
                }
                if ab.cost.crew > 0 && self.crew_power(p, id, ab.saddles()) < ab.cost.crew {
                    continue;
                }
                if let Some((k, n)) = ab.cost.remove_counters
                    && if ab.cost.remove_from_creature {
                        self.counter_cost_creature(p, k, n).is_none()
                    } else {
                        o.counters(k) < n
                    }
                {
                    continue;
                }
                if ab.cost.energy > self.player(p).energy {
                    continue;
                }
                // CR 701.68b: blighting needs a creature to put counters
                // on; CR 701.59b: collecting evidence, enough mana value.
                if !self.other_costs_payable(p, &ab.cost, id) {
                    continue;
                }
                if ab.cost.life > 0
                    && self.player(p).life < i32::try_from(ab.cost.life).unwrap_or(i32::MAX)
                {
                    continue;
                }
                // CR 606.3: one loyalty ability per planeswalker per turn, at
                // sorcery speed; CR 606.5: a −N cost needs N loyalty.
                if ab.cost.loyalty != 0 {
                    if !sorcery_timing || self.loyalty_activated.contains(&id) {
                        continue;
                    }
                    let need = u32::try_from(-ab.cost.loyalty).unwrap_or(0);
                    if o.counters(CounterKind::Loyalty) < need {
                        continue;
                    }
                }
                if let Some(g) = &ab.cost.exile_cards
                    && self.exile_candidates(p, g.filter.as_ref(), id).len() < g.n as usize
                {
                    continue;
                }
                // One option per way to pay the costs that need a choice.
                let sacrifices: Vec<Option<ObjId>> = match &ab.cost.sacrifice {
                    None | Some(SacCost::This) => vec![None],
                    Some(SacCost::Filter(_)) if ab.cost.taps_chosen() => {
                        self.tap_cost_options(p, ab, id, sources)
                    }
                    Some(SacCost::Filter(f)) => {
                        let c = self.sacrifice_candidates(p, f, id);
                        if c.len() < ab.cost.chosen_count() {
                            continue;
                        }
                        c.into_iter().map(Some).collect()
                    }
                };
                let discards: Vec<Option<ObjId>> = if ab.cost.discard > 0 {
                    let mut c: Vec<Option<ObjId>> = self
                        .discard_candidates(p, ab.cost.discard_filter.as_ref(), id)
                        .into_iter()
                        .map(Some)
                        .collect();
                    // "Discard a card at random": no choice to offer; the
                    // card is picked as the cost is paid.
                    if ab.cost.discard_random {
                        c.truncate(1);
                    }
                    c
                } else {
                    vec![None]
                };
                if discards.is_empty() {
                    continue;
                }
                for sacrifice in &sacrifices {
                    for discard in &discards {
                        options.push(Choice::Activate {
                            source: id,
                            ability: *a,
                            sacrifice: *sacrifice,
                            discard: *discard,
                        });
                    }
                }
            }
        }
    }

    /// The choices of the first permanent an activated ability's "tap two
    /// untapped Beasts you control" cost taps (CR 602.2b, 118.3): each
    /// with enough others (not the source when its own {T} is paid too)
    /// that the mana can still be paid without them.
    #[cold]
    #[inline(never)]
    fn tap_cost_options(
        &self,
        p: PlayerId,
        ab: &ActivatedAbility,
        source: ObjId,
        sources: &Sources,
    ) -> Vec<Option<ObjId>> {
        let Some(SacCost::Filter(f)) = &ab.cost.sacrifice else {
            return Vec::new();
        };
        let n = ab.cost.chosen_count();
        let req = self.activation_mana(source, ab);
        let ps = self.player(p);
        let mut cands = self.sacrifice_candidates(p, f, source);
        if ab.cost.tap {
            cands.retain(|c| *c != source);
        }
        cands
            .iter()
            .copied()
            .filter(|c| {
                self.tap_picks(p, f, source, ab.cost.tap, n, Some(*c), sources)
                    .is_some_and(|picks| {
                        req == ManaRequirement::default()
                            || !picks.iter().any(|x| sources.iter().any(|(s, _)| s == x))
                            || self.payable(
                                p,
                                &without(sources, &picks),
                                &ps.mana_pool,
                                &req,
                                ps.life,
                                Some(source),
                            )
                    })
            })
            .map(Some)
            .collect()
    }

    /// CR 509.1h: `p`'s unblocked attacking creatures; none until
    /// blockers are declared (before then, attackers are neither blocked
    /// nor unblocked).
    pub(super) fn unblocked_attackers(&self, p: PlayerId) -> SmallVec<[ObjId; 4]> {
        let declared = matches!(
            self.step,
            crate::turn::Step::DeclareBlockers
                | crate::turn::Step::FirstStrikeDamage
                | crate::turn::Step::CombatDamage
                | crate::turn::Step::EndOfCombat
        );
        match &self.combat {
            Some(cb) if declared => cb
                .attackers
                .iter()
                .filter(|a| !a.blocked && self.objects.get(a.id).is_some_and(|o| o.controller == p))
                .map(|a| a.id)
                .collect(),
            _ => SmallVec::new(),
        }
    }

    /// CR 602.5b: an ability's own restrictions on when it may be
    /// activated ("Activate only during your upkeep", "Activate only if you
    /// control an enchantment"), judged for the activating player.
    pub(super) fn activation_allowed(
        &self,
        ab: &crate::card::ActivatedAbility,
        ability: AbilityId,
        p: PlayerId,
        source: ObjId,
    ) -> bool {
        ab.conditions
            .iter()
            .all(|c| self.condition_now(c, p, source))
            && !(ab.once_ever && self.activated_ever.contains(&(source, ability)))
            // Pithing Needle (CR 201.3, 605.1a).
            && !(self.player_static_sources > 0
                && self.name_cant_activate(source, ab.is_mana_ability()))
    }

    /// Whether an ability with this keyword may be activated now (CR
    /// 702.142a boast: only if the creature attacked this turn, once each
    /// turn; CR 702.177a exhaust, 702.193a power-up: only once).
    fn keyword_allows(&self, k: ActivatedKeyword, id: ObjId, a: AbilityId) -> bool {
        if k.once_ever() {
            !self.activated_ever.contains(&(id, a))
        } else {
            self.attackers_this_turn.contains(&id) && !self.activated_once.contains(&(id, a))
        }
    }

    /// The mana an activated ability of `source` costs: CR 702.193a, a
    /// power-up ability of a permanent that entered this turn costs its
    /// mana cost less (702.193b, 118.7).
    pub(super) fn activation_mana(&self, source: ObjId, ab: &ActivatedAbility) -> ManaRequirement {
        self.activation_mana_x(source, ab, 0)
    }

    /// `activation_mana` with X as `x` (CR 107.3); "Spend only black mana
    /// on X." pays X as that many black symbols.
    pub(super) fn activation_mana_x(
        &self,
        source: ObjId,
        ab: &ActivatedAbility,
        x: u32,
    ) -> ManaRequirement {
        let mut req = ab.cost.mana.requirements(x);
        if x > 0
            && req.generic >= x
            && let Some(o) = self.objects.get(source)
            && let Some(c) =
                self.pool
                    .get(o.def)
                    .ability_ids
                    .iter()
                    .find_map(|a| match self.pool.ability(*a) {
                        AbilityDef::XManaColor(c) => Some(*c),
                        _ => None,
                    })
        {
            req.generic -= x;
            req.colored[c as usize] += x;
        }
        if ab.keyword == Some(ActivatedKeyword::PowerUp)
            && let Some(o) = self.objects.get(source)
            && o.zone == Zone::Battlefield
            && o.timestamp >= self.turn_started_at
        {
            req.reduce_by(&self.pool.get(o.def).mana_cost);
        }
        if let Some(r) = &ab.reduction {
            self.reduce_activation(source, r, &mut req);
        }
        if self.player_static_sources > 0 {
            self.static_activation_cost(source, ab, &mut req);
        }
        req
    }

    /// "Activated abilities of artifacts you control cost {1} less to
    /// activate" (`PlayerStaticKind::ActivationCost`; CR 601.2f by way of
    /// 602.2b): the static abilities of its controller's permanents that
    /// reduce `ab`, an ability of the permanent `source`.
    #[cold]
    #[inline(never)]
    fn static_activation_cost(
        &self,
        source: ObjId,
        ab: &ActivatedAbility,
        req: &mut ManaRequirement,
    ) {
        let Some(obj) = self.objects.get(source) else {
            return;
        };
        if obj.zone != Zone::Battlefield {
            return;
        }
        let p = obj.controller;
        let Some(chars) = self.characteristics(source) else {
            return;
        };
        for id in &self.battlefield {
            let holder = &self.objects[*id];
            if holder.controller != p || !self.pool.get(holder.def).flags().has_player_static() {
                continue;
            }
            for a in &self.pool.get(holder.def).ability_ids {
                let AbilityDef::PlayerStatic {
                    kind:
                        crate::card::PlayerStaticKind::ActivationCost {
                            sources,
                            by,
                            min_one,
                            keyword,
                        },
                    condition,
                } = self.pool.ability(*a)
                else {
                    continue;
                };
                if keyword.is_some_and(|k| ab.keyword != Some(k))
                    || !self
                        .pool
                        .filter(*sources)
                        .matches(source, p, &chars, Some(*id), Some(p))
                    || condition
                        .is_some_and(|i| !self.condition_now(self.pool.condition(i), p, *id))
                {
                    continue;
                }
                let mut off = u32::from(*by);
                // "This effect can't reduce the mana in that cost to less
                // than one mana."
                if *min_one {
                    off = off.min(req.total().saturating_sub(1));
                }
                req.generic -= off.min(req.generic);
            }
        }
    }

    /// CR 601.2f (by way of 602.2b): "This ability costs {1} less to
    /// activate for each Town you control", "... if you've drawn three or
    /// more cards this turn".
    #[cold]
    fn reduce_activation(
        &self,
        source: ObjId,
        r: &crate::card::ActivationReduction,
        req: &mut ManaRequirement,
    ) {
        let Some(p) = self.objects.get(source).map(|o| o.controller) else {
            return;
        };
        let times = match (&r.each, &r.condition) {
            (Some(count), _) => {
                let ctx = EffectCtx {
                    source,
                    controller: p,
                    targets: Vec::new(),
                    division: Vec::new(),
                    legal: Vec::new(),
                    trigger_obj: None,
                    trigger_player: None,
                    trigger_amount: 0,
                    created: None,
                    revealed: Vec::new(),
                    x: 0,
                    did: true,
                    chosen: Chosen::None,
                    couldnt: 0,
                    created_tokens: SmallVec::new(),
                    counts: None,
                };
                self.eval_count(count, &ctx)
            }
            (None, Some(c)) => u32::from(self.condition_now(c, p, source)),
            (None, None) => 0,
        };
        // Past the ability's whole cost, more reductions change nothing.
        for _ in 0..times.min(32) {
            req.reduce_by(&r.by);
        }
    }

    /// CR 602.2a: announce an activated ability; it goes on the stack and
    /// its targets are chosen next (`Flow::Casting`), then costs are paid
    /// (`finish_activation`).
    pub(super) fn activate(
        &mut self,
        player: PlayerId,
        source: ObjId,
        ability: AbilityId,
        sacrifice: Option<ObjId>,
        discard: Option<ObjId>,
    ) {
        self.consecutive_passes = 0;
        let def = self.objects[source].def;
        if let AbilityDef::Activated(a) = self.pool.ability(ability) {
            if a.cost.loyalty != 0 {
                self.loyalty_activated.push(source);
            }
            if a.per_turn > 0 || a.keyword == Some(ActivatedKeyword::Boast) {
                self.activated_once.push((source, ability));
            }
            if a.once_ever || a.keyword.is_some_and(ActivatedKeyword::once_ever) {
                self.activated_ever.push((source, ability));
            }
        }
        let timestamp = self.tick();
        let obj = self.objects.insert(GameObject {
            def,
            card: def,
            except: None,
            cast: CastInfo::default(),
            kind: ObjKind::Ability,
            owner: player,
            controller: player,
            zone: Zone::Stack,
            tapped: false,
            damage: 0,
            deathtouch_damage: false,
            controlled_since_turn_start: false,
            marks: 0,
            skip_untap: false,
            chosen: Chosen::None,
            alt: crate::state::AltCast::None,
            timestamp,
            targets: SmallVec::new(),
            counters: SmallVec::new(),
            attached_to: None,
            ability: Some(StackAbility {
                ability,
                source,
                def,
                trigger_obj: None,
                trigger_player: None,
                trigger_amount: 0,
            }),
            revealed: 0,
            sized: 0,
        });
        self.stack.push(obj);
        self.bump();
        // CR 602.2b, 601.2b: X in the activation cost is announced before
        // targets are chosen. (A mana ability's X is paid as it is
        // activated, Wizard's Rockets, and isn't asked.)
        let has_x = matches!(
            self.pool.ability(ability),
            AbilityDef::Activated(a) if (a.cost.mana.has_x()
                || a.cost.exile_cards.as_ref().is_some_and(|g| g.x))
                && !a.is_mana_ability()
        );
        // CR 602.2b, 601.2b: a modal activated ability's mode is chosen
        // first.
        let modal = matches!(
            self.pool.ability(ability).effects(),
            [crate::card::Effect::Modes(_)]
        );
        self.flow = if modal {
            crate::state::Flow::ChoosingMode {
                spell: obj,
                sacrifice,
                discard,
            }
        } else if has_x {
            crate::state::Flow::ChoosingX {
                spell: obj,
                sacrifice,
                discard,
            }
        } else {
            crate::state::Flow::Casting {
                spell: obj,
                next_target: 0,
                sacrifice,
                discard,
            }
        };
    }

    /// The creatures `spell`, being cast for its emerge cost, could
    /// sacrifice and still be paid for.
    pub(super) fn emerge_sacrifices(&self, spell: ObjId, f: &crate::card::ObjFilter) -> Vec<ObjId> {
        let p = self.objects[spell].controller;
        let sources = self.spell_sources(spell);
        let req = self.spell_requirements(spell, self.x_of(spell));
        let printed = self.printed(self.objects[spell].def);
        self.emerge_options(p, f, spell, &req, &sources, Spend::Spell(&printed))
    }

    /// The creatures an emerge cost `req` (before its reduction) could be
    /// paid with by sacrificing (CR 702.119a): each lowers the generic
    /// part by its mana value and no longer taps for mana.
    fn emerge_options(
        &self,
        p: PlayerId,
        f: &crate::card::ObjFilter,
        spell: ObjId,
        req: &ManaRequirement,
        sources: &[(ObjId, Mana)],
        spend: Spend,
    ) -> Vec<ObjId> {
        self.sacrifice_candidates(p, f, spell)
            .into_iter()
            .filter(|c| {
                let Some(ch) = self.characteristics(*c) else {
                    return false;
                };
                let mv = self.pool.get(ch.def).mana_cost.mana_value();
                let mut r = *req;
                adjust_generic(&mut r, -i32::try_from(mv).unwrap_or(i32::MAX));
                let left: Vec<(ObjId, Mana)> =
                    sources.iter().filter(|(s, _)| s != c).copied().collect();
                self.payable_for(p, &left, &r, spend)
            })
            .collect()
    }

    /// Permanents `p` could sacrifice for a "sacrifice a ..." cost of
    /// `source`.
    pub(super) fn sacrifice_candidates(
        &self,
        p: PlayerId,
        f: &crate::card::ObjFilter,
        source: ObjId,
    ) -> Vec<ObjId> {
        // "Sacrifice six creatures named Shadowborn Apostle": named like
        // the source.
        let filled;
        let f = if f.same.is_some() {
            filled = self.with_same_of(f, Some(source));
            &filled
        } else {
            f
        };
        self.permanents_of(p)
            .filter(|s| {
                let sc = &self.objects[*s];
                self.characteristics(*s)
                    .is_some_and(|c| f.matches(*s, sc.controller, &c, Some(source), Some(p)))
                    && self.status_ok(f, *s)
            })
            .collect()
    }

    /// The values of X the caster can pay for (CR 601.2b, 601.2h), in
    /// increasing order.
    pub(super) fn x_options(&self, spell: ObjId) -> Vec<Choice> {
        const MAX_X: u32 = 30;
        if matches!(
            self.objects[spell].alt,
            AltCast::Free | AltCast::Suspend | AltCast::FreeExile
        ) {
            return vec![Choice::X(0)];
        }
        let caster = self.objects[spell].controller;
        // An activated ability's X (CR 602.2b): its cost, from the sources
        // that may pay for its source's abilities.
        let ability =
            self.objects[spell]
                .ability
                .and_then(|sa| match self.pool.ability(sa.ability) {
                    AbilityDef::Activated(a) => Some((sa.source, a)),
                    _ => None,
                });
        let sources = match ability {
            Some((source, a)) => {
                let mut s = self.ability_sources(caster, source);
                if a.cost.waterbend {
                    let own = a.cost.tap.then_some(source);
                    s.extend(self.waterbend_sources(caster, u32::MAX, own));
                }
                s
            }
            None => self.spell_sources(spell),
        };
        let exclude = ability.map(|(source, _)| source);
        let pool = self.player(caster).mana_pool;
        // "X target creatures": X can't exceed the creatures there are to
        // target (CR 601.2c).
        let max_x = match self.targets_of(spell).last() {
            Some(s @ TargetSpec::Many(i)) if self.pool.many(*i).min == crate::card::Num::X => {
                u32::try_from(self.legal_targets(caster, *s, Some(spell)).len()).unwrap_or(MAX_X)
            }
            _ => MAX_X,
        };
        // "As an additional cost to cast this spell, discard X cards": X
        // can't exceed the other cards in hand.
        let add = self.additional_of(spell);
        let max_x = if add.discard_x {
            let n = self
                .discard_candidates(caster, add.discard_filter.as_ref(), spell)
                .len();
            max_x.min(u32::try_from(n).unwrap_or(MAX_X))
        } else {
            max_x
        };
        // "As an additional cost to cast this spell, pay X life": no more
        // than the caster's life (CR 119.4).
        let max_x = if add.life_x {
            max_x.min(u32::try_from(self.player(caster).life.max(0)).unwrap_or(0))
        } else {
            max_x
        };
        // "exile X cards from your graveyard": no more than there are (an
        // activated ability's cost, or a spell's additional cost).
        let exile = match ability {
            Some((_, a)) => &a.cost.exile_cards,
            None => &add.exile_cards,
        };
        let max_x = match exile {
            Some(g) if g.x => {
                let n = self
                    .exile_candidates(caster, g.filter.as_ref(), spell)
                    .len();
                max_x.min(u32::try_from(n).unwrap_or(MAX_X))
            }
            _ => max_x,
        };
        // "with mana value X": only an X some object fits (the targets
        // that follow must be chosen, CR 601.2c).
        let x_specs: SmallVec<[TargetSpec; 2]> = self
            .targets_of(spell)
            .iter()
            .copied()
            .filter(|s| {
                !matches!(s, TargetSpec::UpTo(_))
                    && match self.pool.base_spec(*s) {
                        TargetSpec::Filtered(f)
                        | TargetSpec::SpellFiltered(f)
                        | TargetSpec::GraveyardCard { filter: f, .. } => {
                            self.pool.filter(f).status.mana_value_x
                        }
                        _ => false,
                    }
            })
            .collect();
        (0..=max_x.min(MAX_X))
            .take_while(|x| {
                let req = match ability {
                    Some((source, a)) => self.activation_mana_x(source, a, *x),
                    None => self.spell_requirements(spell, *x),
                };
                self.payable(
                    caster,
                    &sources,
                    &pool,
                    &req,
                    self.player(caster).life,
                    exclude,
                )
            })
            .filter(|x| x_specs.iter().all(|s| self.x_fits(caster, *s, spell, *x)))
            .map(Choice::X)
            .collect()
    }

    /// Whether some object is a legal target for `spec` of `spell` with X
    /// as `x`.
    fn x_fits(&self, caster: PlayerId, spec: TargetSpec, spell: ObjId, x: u32) -> bool {
        // An ability's source is the permanent (protection judges it).
        let source = self.objects[spell].ability.map_or(spell, |a| a.source);
        let fits = |id: &ObjId| {
            *id != spell
                && self.is_legal_target_x(caster, spec, Target::Object(*id), Some(source), Some(x))
        };
        if self.pool.base_spec(spec).is_graveyard() {
            self.players_in_game()
                .any(|p| self.player(p).graveyard.iter().any(fits))
        } else if self.pool.base_spec(spec).is_spell() {
            // "counter target spell with mana value X".
            self.stack.iter().any(fits)
        } else {
            self.battlefield.iter().any(fits)
        }
    }

    /// The damage or counters `spell` divides among its variable target
    /// group (CR 601.2d), as it is being cast; `None` if it divides
    /// nothing.
    pub(super) fn divided_total(&self, spell: ObjId) -> Option<u32> {
        // "you may sacrifice an artifact. If you do, this creature deals 4
        // damage divided as you choose among any number of targets"
        // (Kuldotha Flamefiend): the division is announced with the
        // targets all the same.
        fn find(effects: &[Effect]) -> Option<crate::card::Num> {
            effects.iter().find_map(|e| match e {
                Effect::YouMay(inner) | Effect::IfYouDo(inner) | Effect::IfKicked(inner) => {
                    find(inner)
                }
                Effect::DealDamageDivided { amount, .. } => Some(*amount),
                Effect::DistributeCounters { count, .. } => Some(*count),
                // "Prevent the next 4 damage that would be dealt this turn
                // to any number of targets, divided as you choose" (Remedy).
                Effect::CreateReplacement(crate::card::ReplacementAbility {
                    replaces:
                        Replaces::Damage {
                            to: Affects::Targets(_, u8::MAX),
                            ..
                        },
                    replacement: Replacement::PreventNext(n),
                    ..
                }) => Some(*n),
                _ => None,
            })
        }
        let amount = find(self.effects_of(spell))?;
        let o = &self.objects[spell];
        let ctx = EffectCtx {
            source: o.ability.map_or(spell, |a| a.source),
            controller: o.controller,
            targets: Vec::new(),
            division: Vec::new(),
            legal: Vec::new(),
            trigger_obj: o.ability.and_then(|a| a.trigger_obj),
            trigger_player: o.ability.and_then(|a| a.trigger_player),
            trigger_amount: o.ability.map_or(0, |a| a.trigger_amount),
            created: None,
            revealed: Vec::new(),
            x: self.x_of(spell),
            did: true,
            chosen: Chosen::None,
            couldnt: 0,
            created_tokens: SmallVec::new(),
            counts: None,
        };
        Some(self.eval_num(amount, &ctx))
    }

    /// How many targets `spell` may choose for its variable group `i`
    /// (CR 601.2c): X is the spell's; a divided amount needs a target for
    /// each part and at least one target to divide among (CR 601.2d).
    pub(super) fn many_bounds(&self, spell: ObjId, i: u16) -> (usize, usize) {
        let m = self.pool.many(i);
        let x = self.x_of(spell);
        let mut min = m.min.eval(x) as usize;
        let mut max = m.max.map_or(usize::MAX, |n| n.eval(x) as usize);
        if let Some(total) = self.divided_total(spell) {
            max = max.min(total as usize);
            if total > 0 {
                min = min.max(1);
            }
        }
        (min.min(max), max)
    }

    /// The division decision for a spell or ability whose targets are
    /// chosen (CR 601.2d), or `None` once it is settled. One target takes
    /// everything without a question.
    pub(super) fn division_decision(&mut self, spell: ObjId) -> Option<crate::decision::Decision> {
        let specs = self.targets_of(spell);
        if !matches!(specs.last(), Some(TargetSpec::Many(_)))
            || self.divisions.iter().any(|(s, _)| *s == spell)
        {
            return None;
        }
        let first = specs.len() - 1;
        let total = self.divided_total(spell)?;
        let k = self.objects[spell].targets.len().saturating_sub(first);
        if k <= 1 {
            self.divisions
                .push((spell, if k == 1 { vec![total] } else { Vec::new() }));
            return None;
        }
        let options: Vec<Choice> = divisions(total, k)
            .into_iter()
            .map(Choice::Divide)
            .collect();
        Some(crate::decision::Decision {
            player: self.objects[spell].controller,
            kind: DecisionKind::Divide { spell },
            options,
        })
    }

    /// An ability object on the stack has its targets. An activated ability
    /// now pays its costs; a triggered one is done.
    pub(super) fn finish_ability_targets(
        &mut self,
        obj: ObjId,
        sacrifice: Option<ObjId>,
        discard: Option<ObjId>,
    ) {
        self.emit_targeted(obj);
        let activated = self.objects[obj]
            .ability
            .is_some_and(|sa| matches!(self.pool.ability(sa.ability), AbilityDef::Activated(_)));
        if activated {
            self.finish_activation(obj, sacrifice, discard);
        } else {
            self.flow = crate::state::Flow::Priority;
        }
    }

    /// Whether a nonmodal spell can be cast for its targets: those it has
    /// unkicked, or, for a gift whose promise replaces its targets ("If
    /// the gift was promised, instead ... target ...", CR 702.174m), those
    /// it has with the gift promised (an opponent can always be promised
    /// it).
    pub(super) fn spell_targets_available(
        &self,
        p: PlayerId,
        def: &crate::card::CardDef,
        id: ObjId,
    ) -> bool {
        // A fused split spell: each half's "target" words are its own
        // (CR 115.3).
        if let Some((_, t)) = def.fused {
            let specs = def.cast_targets();
            return self.plain_targets_in_parts(p, specs, &[usize::from(t)], Some(id))
                && self.related_targets_available(p, specs, Some(id), None);
        }
        self.targets_available(p, def.unkicked_cast_targets(), Some(id))
            || (def.gift.is_some()
                && def.kicked_drops.is_some()
                && self.players_in_game().any(|q| q != p)
                && self.targets_available(p, def.kicked_cast_targets(), Some(id)))
            || (def.teamwork > 0
                && def.kicked_drops.is_some()
                && self.teamwork_tappers(p, def.teamwork).is_some()
                && self.targets_available(p, def.kicked_cast_targets(), Some(id)))
    }

    /// Whether every one of `specs` has a legal target for `p` (for an
    /// ability of `source`).
    pub(super) fn targets_available(
        &self,
        p: PlayerId,
        specs: &[TargetSpec],
        source: Option<ObjId>,
    ) -> bool {
        self.targets_available_for(p, specs, source, None)
    }

    /// `targets_available` for a mode of a modal spell or ability. A
    /// combined mode ("choose two") is several modes' "target" words, and
    /// one object may be chosen for each of them (CR 115.3): only targets
    /// within one part must differ.
    pub(super) fn mode_targets_available(
        &self,
        p: PlayerId,
        m: &crate::card::Mode,
        source: Option<ObjId>,
    ) -> bool {
        if m.split.is_none() {
            return self.targets_available(p, &m.targets, source);
        }
        let starts = mode_target_starts(m);
        self.plain_targets_in_parts(p, &m.targets, &starts, source)
            && self.related_targets_available(p, &m.targets, source, None)
    }

    /// `targets_available` for a triggered ability whose triggering player
    /// is `trigger_player` ("target creature that player controls").
    pub(super) fn targets_available_for(
        &self,
        p: PlayerId,
        specs: &[TargetSpec],
        source: Option<ObjId>,
        trigger_player: Option<PlayerId>,
    ) -> bool {
        self.plain_targets_in_parts(p, specs, &[], source)
            && self.related_targets_available(p, specs, source, trigger_player)
    }

    /// The "that player" part of `targets_available_for`.
    fn related_targets_available(
        &self,
        p: PlayerId,
        specs: &[TargetSpec],
        source: Option<ObjId>,
        trigger_player: Option<PlayerId>,
    ) -> bool {
        // "that player": some choice of the targets it relates to leaves
        // one for it.
        specs
            .iter()
            .enumerate()
            .all(|(i, s)| match self.that_player_of(*s) {
                // A target player relation restricts groups and counts, not
                // targets.
                None
                | Some(
                    crate::card::ThatPlayer::Target(_)
                    | crate::card::ThatPlayer::TargetOrController { .. },
                ) => true,
                Some(crate::card::ThatPlayer::Triggering) => {
                    matches!(s, TargetSpec::UpTo(_))
                        || self
                            .legal_targets(p, *s, source)
                            .into_iter()
                            .any(|t| self.that_player_ok(*s, t, trigger_player, &[]))
                }
                Some(crate::card::ThatPlayer::ControllerOf(n)) => {
                    let Some(first) = specs.get(usize::from(n)) else {
                        return true;
                    };
                    self.legal_targets(p, *first, source)
                        .into_iter()
                        .any(|t0| self.related_target_exists(p, specs, i, t0, n, source))
                }
            })
    }

    /// Whether spec `i`, relating to target `n` chosen as `t0`, has a legal
    /// target controlled by `t0`'s controller other than `t0` (CR 115.3).
    pub(super) fn related_target_exists(
        &self,
        p: PlayerId,
        specs: &[TargetSpec],
        i: usize,
        t0: Target,
        n: u8,
        source: Option<ObjId>,
    ) -> bool {
        let mut chosen = vec![Target::Player(p); usize::from(n) + 1];
        chosen[usize::from(n)] = t0;
        self.legal_targets(p, specs[i], source)
            .into_iter()
            .any(|t| t != t0 && self.that_player_ok(specs[i], t, None, &chosen))
    }

    /// Target options for `spec` of stack object `spell` held to "that
    /// player" (`ObjFilter::that_player`): a target controlled by the
    /// triggering player or an earlier target's controller, and a target a
    /// later one relates to only if it leaves that one a choice ("Target
    /// creature an opponent controls deals damage ... to another target
    /// creature that player controls", CR 601.2c).
    pub(super) fn retain_that_player(
        &self,
        spell: ObjId,
        specs: &[TargetSpec],
        spec: TargetSpec,
        options: &mut Vec<Choice>,
    ) {
        let o = &self.objects[spell];
        let index = o.targets.len();
        // "that share a card type" (`SameWhat::SharesType`) relates to an
        // earlier target too.
        let shares = matches!(
            self.pool.base_spec(spec),
            TargetSpec::Filtered(f) if matches!(
                self.pool.filter(f).same,
                Some(crate::card::Same {
                    what: crate::card::SameWhat::SharesType,
                    ..
                })
            )
        );
        if shares || self.that_player_of(spec).is_some() {
            let trigger_player = o.ability.and_then(|a| a.trigger_player);
            options.retain(|c| match c {
                Choice::Target(t) => self.that_player_ok(spec, *t, trigger_player, &o.targets),
                _ => true,
            });
        }
        let related: SmallVec<[usize; 2]> = (index + 1..specs.len())
            .filter(|i| {
                matches!(
                    self.that_player_of(specs[*i]),
                    Some(crate::card::ThatPlayer::ControllerOf(n)) if usize::from(n) == index
                )
            })
            .collect();
        if related.is_empty() {
            return;
        }
        let source = Some(o.ability.map_or(spell, |a| a.source));
        let n = u8::try_from(index).unwrap_or(u8::MAX);
        options.retain(|c| match c {
            Choice::Target(t) => related.iter().all(|i| {
                matches!(specs[*i], TargetSpec::UpTo(_))
                    || self.related_target_exists(o.controller, specs, *i, *t, n, source)
            }),
            _ => true,
        });
    }

    /// The "that player" relation of a target spec's filter, if any.
    pub(super) fn that_player_of(&self, spec: TargetSpec) -> Option<crate::card::ThatPlayer> {
        match self.pool.base_spec(spec) {
            TargetSpec::Filtered(f) | TargetSpec::GraveyardCard { filter: f, .. } => {
                self.pool.filter(f).that_player
            }
            _ => None,
        }
    }

    /// `starts`: where each part of a combined mode after the first begins
    /// (targets of different parts may be the same object).
    fn plain_targets_in_parts(
        &self,
        p: PlayerId,
        all: &[TargetSpec],
        starts: &[usize],
        source: Option<ObjId>,
    ) -> bool {
        all.iter().enumerate().all(|(i, s)| {
            let lo = starts
                .iter()
                .copied()
                .filter(|&b| b <= i)
                .max()
                .unwrap_or(0);
            let hi = starts
                .iter()
                .copied()
                .filter(|&b| b > i)
                .min()
                .unwrap_or(all.len());
            self.plain_target_available(p, &all[lo..hi.max(lo)], *s, source)
        })
    }

    /// Whether `s`, one of `specs`, has enough legal targets: CR 115.3,
    /// "two target lands" needs two different lands.
    fn plain_target_available(
        &self,
        p: PlayerId,
        specs: &[TargetSpec],
        s: TargetSpec,
        source: Option<ObjId>,
    ) -> bool {
        match s {
            TargetSpec::UpTo(_) => true,
            // "One or two target creatures" needs one; X can be 0.
            TargetSpec::Many(i) => match self.pool.many(i).min {
                crate::card::Num::N(n) => {
                    n == 0 || self.legal_targets(p, s, source).len() >= n as usize
                }
                _ => true,
            },
            // "Move all counters from target creature onto another target
            // creature": the other target is a different creature too.
            TargetSpec::OtherCreature | TargetSpec::OtherPermanent | TargetSpec::AnyOther => {
                let base = match s {
                    TargetSpec::OtherCreature => TargetSpec::Creature,
                    TargetSpec::AnyOther => TargetSpec::Any,
                    _ => TargetSpec::Permanent,
                };
                self.legal_targets(p, s, source).len()
                    >= specs.iter().filter(|x| **x == s || **x == base).count()
            }
            _ => {
                self.legal_targets(p, s, source).len() >= specs.iter().filter(|x| **x == s).count()
            }
        }
    }

    /// The permanents a stack object targets become its targets (CR 115.?):
    /// "becomes the target of a spell or ability" triggers see it.
    fn emit_targeted(&mut self, obj: ObjId) {
        self.check_crime(obj);
        self.emit_targeted_objects(obj);
    }

    /// CR 700.13: targeting an opponent, anything an opponent controls,
    /// or a card in an opponent's graveyard is a crime, once per spell or
    /// ability however many such targets it has.
    fn check_crime(&mut self, obj: ObjId) {
        let who = self.objects[obj].controller;
        let bit = 1 << who.index().min(7);
        if self.objects[obj].targets.iter().any(|t| match t {
            Target::Player(p) => *p != who,
            Target::Object(id) => self.objects.get(*id).is_some_and(|o| {
                if o.zone == Zone::Graveyard {
                    o.owner != who
                } else {
                    o.controller != who && matches!(o.zone, Zone::Battlefield | Zone::Stack)
                }
            }),
        }) {
            if self.turn_log.crime & bit == 0 {
                self.turn_log.crime |= bit;
                self.history_bump();
            }
            self.emit(Event::CommittedCrime { player: who });
        }
    }

    /// The permanents a stack object targets become its targets.
    fn emit_targeted_objects(&mut self, obj: ObjId) {
        let targets: SmallVec<[ObjId; 2]> = self.objects[obj]
            .targets
            .iter()
            .filter(|t| !t.is_none())
            .filter_map(|t| match t {
                Target::Object(id) => Some(*id),
                Target::Player(_) => None,
            })
            .collect();
        for target in targets {
            self.emit(Event::Targeted { target, by: obj });
            // Valiant: the first time each turn its controller targets it
            // (the trigger above saw it unrecorded).
            if self.valiant_watchers > 0
                && !self.targeted_by_controller.contains(&target)
                && self.objects.get(target).is_some_and(|t| {
                    t.zone == Zone::Battlefield && t.controller == self.objects[obj].controller
                })
            {
                self.targeted_by_controller.push(target);
            }
        }
    }

    /// The modes of a modal spell being cast that have their targets
    /// (CR 601.2b).
    pub(super) fn mode_options(&self, spell: ObjId) -> Vec<Choice> {
        let o = &self.objects[spell];
        let Some(modes) = self.modes_of(spell) else {
            return Vec::new();
        };
        let source = Some(o.ability.map_or(spell, |a| a.source));
        let def = self.pool.get(o.def);
        // CR 702.172a: a spree spell's modes cost more; those the caster
        // can pay for (all of them if none, so the cast can go on to fail
        // at payment rather than offer nothing).
        if !def.spree.is_empty() && o.kind == ObjKind::Card {
            let with_targets: Vec<usize> = modes
                .iter()
                .enumerate()
                .filter(|(_, m)| self.mode_targets_available(o.controller, m, source))
                .map(|(i, _)| i)
                .collect();
            let other = o.cast.other_cost();
            let payable: Vec<Choice> = with_targets
                .iter()
                .filter(|i| {
                    let req = self.spell_requirements_mode(
                        spell,
                        0,
                        other,
                        u8::try_from(**i).unwrap_or(0),
                    );
                    self.spell_payable(o.controller, spell, &req)
                })
                .map(|i| Choice::Mode(*i))
                .collect();
            return if payable.is_empty() {
                with_targets.into_iter().map(Choice::Mode).collect()
            } else {
                payable
            };
        }
        modes
            .iter()
            .enumerate()
            .filter(|(i, m)| {
                // "Choose one that hasn't been chosen" (CR 700.2).
                o.ability
                    .is_none_or(|a| !self.mode_used(a.source, a.ability, *i, m))
            })
            .filter(|(_, m)| self.mode_targets_available(o.controller, m, source))
            .filter(|(_, m)| {
                // "Choose one. If ..., choose both instead." (CR 700.2).
                let Some(mi) = def.modes_if.filter(|_| o.kind == ObjKind::Card) else {
                    return true;
                };
                // A condition on a cost announced with the modes (CR
                // 601.2b: kicker, teamwork, an optional additional cost):
                // the larger choice commits to paying it, so it needs the
                // cost to be payable now (`linked_cost_answer`).
                if let Some(payable) = self.mode_linked_cost(spell) {
                    return m.split.is_none() || payable;
                }
                let holds = self.condition_now(self.pool.condition(mi.cond), o.controller, spell);
                if m.split.is_some() {
                    holds
                } else {
                    !(holds && mi.must)
                }
            })
            .filter(|(i, m)| {
                // Entwined or escalated, the spell costs more (CR 702.42a,
                // 702.120a).
                if m.split.is_none() || o.kind == ObjKind::Ability {
                    return true;
                }
                let (e, times) = if let Some(e) = &def.escalate {
                    (e, u32::try_from(*i + 1).unwrap_or(0).count_ones() - 1)
                } else if let Some(e) = &def.entwine {
                    (e, 1)
                } else {
                    return true;
                };
                let mut req = def.mana_cost.requirements(0);
                for _ in 0..times {
                    req.add(&e.requirements(0));
                }
                adjust_generic(
                    &mut req,
                    self.cost_delta(o.controller, def, spell, &self.cost_changers()),
                );
                self.spell_payable(o.controller, spell, &req)
            })
            .map(|(i, _)| Choice::Mode(i))
            .collect()
    }

    /// CR 602.2b, 601.2g–h: pay the ability's costs. The source is tapped
    /// first so that it is not also tapped for mana; sacrifices come last,
    /// so a "sacrifice ~" source is still there to be tapped.
    #[allow(clippy::too_many_lines)] // one step per kind of cost
    fn finish_activation(&mut self, obj: ObjId, sacrifice: Option<ObjId>, discard: Option<ObjId>) {
        let o = &self.objects[obj];
        let (controller, def) = (o.controller, o.def);
        let sa = o.ability.expect("ability object");
        let pool = std::sync::Arc::clone(&self.pool);
        let AbilityDef::Activated(ab) = pool.ability(sa.ability) else {
            unreachable!("checked by finish_ability_targets");
        };
        if ab.cost.tap {
            self.tap(sa.source);
        }
        // "Tap two untapped Beasts you control": before the mana.
        let mut sacrifice = sacrifice;
        let mut tapped_paid: SmallVec<[ObjId; 2]> = SmallVec::new();
        if let (true, Some(SacCost::Filter(f))) = (ab.cost.taps_chosen(), &ab.cost.sacrifice) {
            let sources = self.ability_sources(controller, sa.source);
            let n = ab.cost.chosen_count();
            for c in self
                .tap_picks(
                    controller,
                    f,
                    sa.source,
                    ab.cost.tap,
                    n,
                    sacrifice,
                    &sources,
                )
                .unwrap_or_default()
            {
                self.tap(c);
                tapped_paid.push(c);
            }
            sacrifice = None;
        }
        let mut sources = self.ability_sources(controller, sa.source);
        let req = self.activation_mana_x(sa.source, ab, self.x_of(obj));
        // CR 701.67a: a {T} source is tapped already, so it doesn't help.
        if ab.cost.waterbend {
            sources.extend(self.waterbend_sources(controller, req.generic, None));
        }
        self.pay_from(controller, &req, &sources, Some(sa.source));
        if ab.cost.crew > 0 {
            for c in self.crew_choice(controller, sa.source, ab.cost.crew, ab.saddles()) {
                self.tap(c);
                self.emit(Event::Crewed {
                    crew: c,
                    vehicle: sa.source,
                });
            }
        }
        // CR 702.49a, c: return the unblocked attacker; the Ninja will
        // attack what it was attacking.
        if ab.keyword == Some(ActivatedKeyword::Ninjutsu)
            && let Some(s) = sacrifice
        {
            // An earlier activation that never resolved (countered) left
            // its entry behind.
            self.ninjutsu.retain(|(c, _)| *c != sa.source);
            if let Some(at) = self.combat.as_ref().and_then(|cb| cb.attacker(s)) {
                self.ninjutsu.push((sa.source, at.attacking));
            }
            if self.objects.contains_key(s) {
                self.move_object(s, Zone::Hand, false);
            }
        }
        if ab.cost.exile_self && self.objects[sa.source].zone == Zone::Graveyard
            || ab.cost.exile_this && self.objects[sa.source].zone == Zone::Battlefield
        {
            self.move_object(sa.source, Zone::Exile, false);
        }
        let mut cycled = None;
        if ab.cost.discard_self && self.objects[sa.source].zone == Zone::Hand {
            let card_def = self.objects[sa.source].def;
            self.discard_card(controller, sa.source);
            if ab.cycling {
                cycled = self.current_object(sa.source).map(|c| (c, card_def));
            }
        }
        self.pay_costs_recorded(obj, |s| {
            // "the tapped creature's power" (station, CR 702.184a).
            for c in tapped_paid {
                s.note_tapped_for_cost(c);
            }
            // "Exile X artifact cards from your graveyard": as many as the
            // X announced (CR 602.2b, 601.2b).
            if let Some(g) = &ab.cost.exile_cards
                && g.x
            {
                let mut cost = ab.cost.clone();
                if let Some(g) = &mut cost.exile_cards {
                    g.n = s.x_of(obj);
                }
                s.pay_other_costs(controller, &cost, sa.source, sacrifice, discard);
            } else {
                s.pay_other_costs(controller, &ab.cost, sa.source, sacrifice, discard);
            }
        });
        self.emit(Event::Activated {
            obj,
            source: sa.source,
            def,
        });
        // CR 702.29d: "when you cycle" triggers as the cycling ability is
        // activated, so they resolve before it.
        if let Some((card, def)) = cycled {
            let x = self.x_of(obj);
            self.emit(Event::Cycled {
                player: controller,
                card,
                def,
                x,
            });
        }
        // CR 605.3b: a mana ability doesn't use the stack.
        if ab.is_mana_ability() {
            self.resolve_top();
            // CR 605.4a: mana abilities it triggered by tapping its source
            // for mana resolve right after it.
            if ab.cost.tap && self.mana_trigger_sources > 0 {
                self.fire_mana_triggers(sa.source);
            }
        }
        self.give_priority(controller);
    }

    /// Strive: whether `spell` could still be paid for with one more
    /// target.
    pub(super) fn strive_payable(&self, spell: ObjId) -> bool {
        let o = &self.objects[spell];
        let Some(s) = &self.pool.get(o.def).strive else {
            return true;
        };
        let mut req = self.spell_requirements(spell, self.x_of(spell));
        req.add(&s.requirements(0));
        let sources = self.spell_sources(spell);
        let ps = self.player(o.controller);
        self.payable(o.controller, &sources, &ps.mana_pool, &req, ps.life, None)
    }

    /// Whether `spell` could be paid for with `t` as its first target.
    pub(super) fn target_payable(&self, spell: ObjId, t: Target) -> bool {
        let o = &self.objects[spell];
        let def = self.pool.get(o.def);
        let Some(tc) = &def.target_cost else {
            return true;
        };
        let mut req = self.spell_requirements(spell, self.x_of(spell));
        if self.target_fits(&tc.1, t, o.controller, spell) {
            adjust_generic(&mut req, tc.0);
        }
        let sources = self.spell_sources(spell);
        let ps = self.player(o.controller);
        self.payable(o.controller, &sources, &ps.mana_pool, &req, ps.life, None)
    }

    /// Whether `t` matches a target-cost filter (a permanent's current
    /// characteristics and state).
    pub(super) fn target_fits(
        &self,
        f: &crate::card::ObjFilter,
        t: Target,
        p: PlayerId,
        source: ObjId,
    ) -> bool {
        let Target::Object(id) = t else {
            return false;
        };
        let Some(o) = self.objects.get(id) else {
            return false;
        };
        self.characteristics(id)
            .is_some_and(|c| f.matches(id, o.controller, &c, Some(source), Some(p)))
            && self.status_ok(f, id)
    }

    /// Legal targets for `spec` chosen by `caster`, for a spell or an
    /// ability of `source`.
    pub(super) fn legal_targets(
        &self,
        caster: PlayerId,
        spec: TargetSpec,
        source: Option<ObjId>,
    ) -> Vec<Target> {
        let spec = self.pool.base_spec(spec);
        let mut out = Vec::new();
        if spec.is_graveyard() {
            for p in self.players_in_game() {
                for id in &self.player(p).graveyard {
                    let t = Target::Object(*id);
                    // A spell cast from a graveyard is on the stack by the
                    // time it targets (CR 601.2a, 601.2c).
                    if Some(*id) != source && self.is_legal_target_for(caster, spec, t, source) {
                        out.push(t);
                    }
                }
            }
            return out;
        }
        if spec.is_spell() || matches!(spec, TargetSpec::SpellOr(_)) {
            for id in &self.stack {
                let t = Target::Object(*id);
                if Some(*id) != source && self.is_legal_target_for(caster, spec, t, source) {
                    out.push(t);
                }
            }
            if spec.is_spell() {
                return out;
            }
        }
        for id in &self.battlefield {
            let t = Target::Object(*id);
            if self.is_legal_target_for(caster, spec, t, source) {
                out.push(t);
            }
        }
        for p in self.players_in_game() {
            let t = Target::Player(p);
            if self.is_legal_target_for(caster, spec, t, source) {
                out.push(t);
            }
        }
        out
    }

    /// Untapped mana sources `p` controls, as (object, mana produced): one
    /// entry per kind of mana a permanent can make. Basic land types give
    /// their mana intrinsically (CR 305.6); other mana abilities are listed
    /// on the card. Both are read from current characteristics, so a land
    /// turned into a Mountain taps for red.
    pub(super) fn mana_sources(&self, p: PlayerId) -> Sources {
        let key = (self.cversion, self.tap_epoch, self.turn_number);
        let slot = p.index().min(1);
        {
            let cache = self.mana_cache.borrow();
            let (cv, taps, turn, cached) = &cache[slot];
            if (*cv, *taps, *turn) == key && *cv != 0 {
                // A copy of the slice, not `clone` (which pushes entry by
                // entry): this runs at every priority.
                return Sources::from_slice(cached);
            }
        }
        let fresh = self.mana_sources_uncached(p);
        self.mana_cache.borrow_mut()[slot] = (key.0, key.1, key.2, Sources::from_slice(&fresh));
        fresh
    }

    fn mana_sources_uncached(&self, p: PlayerId) -> Sources {
        let mut out = Sources::new();
        // Sources with other uses (creatures, permanents with activated
        // abilities such as Mishra's Factory) are listed last so that the
        // canonical payment plan taps plain lands first.
        let mut later = Sources::new();
        let mut any_sacrificed = false;
        for id in self.permanents_of(p) {
            let o = &self.objects[id];
            if o.tapped {
                continue;
            }
            let Some(c) = self.characteristics(id) else {
                continue;
            };
            // CR 302.6: a creature's {T} abilities, mana abilities included,
            // need it to have been controlled since the turn began.
            if c.is_creature() && !o.controlled_since_turn_start && !c.has(Keyword::Haste) {
                continue;
            }
            // CR 602.5: no activated abilities, mana abilities included
            // (Encrust on a mana artifact; Voidstone Gargoyle's chosen
            // name).
            if c.has(Keyword::CantActivate)
                || (self.player_static_sources > 0 && self.name_cant_activate(id, true))
            {
                continue;
            }
            let other_use =
                c.is_creature() || (self.activated_sources > 0 && self.flags_of(o).has_activated());
            let list = if other_use { &mut later } else { &mut out };
            if c.is_land() {
                for s in &c.subtypes {
                    if let Some(color) = s.basic_color() {
                        list.push((id, Mana::Colored(color)));
                    }
                }
            }
            for a in &c.abilities {
                match self.pool.ability(*a) {
                    // The color chosen as it entered; none, no mana.
                    AbilityDef::TapForMana(Mana::Chosen) => {
                        if let Some(c) = o.chosen.color() {
                            list.push((id, Mana::Colored(c)));
                        }
                    }
                    AbilityDef::TapForMana(m) => list.push((id, *m)),
                    // Sources that sacrifice themselves go after the rest.
                    AbilityDef::SacForMana(_) => any_sacrificed = true,
                    _ => {}
                }
            }
        }
        out.extend(later);
        if any_sacrificed {
            for id in self.permanents_of(p) {
                let o = &self.objects[id];
                if o.tapped {
                    continue;
                }
                let Some(c) = self.characteristics(id) else {
                    continue;
                };
                if (c.is_creature() && !o.controlled_since_turn_start && !c.has(Keyword::Haste))
                    || c.has(Keyword::CantActivate)
                    || (self.player_static_sources > 0 && self.name_cant_activate(id, true))
                {
                    continue;
                }
                for a in &c.abilities {
                    match self.pool.ability(*a) {
                        AbilityDef::SacForMana(Mana::Chosen) => {
                            if let Some(c) = o.chosen.color() {
                                out.push((id, Mana::Colored(c)));
                            }
                        }
                        AbilityDef::SacForMana(m) => out.push((id, *m)),
                        _ => {}
                    }
                }
            }
        }
        out
    }

    /// Whether `id` makes mana by sacrificing itself (and has no plain
    /// `{T}` mana ability the plan could have used instead). Read from the
    /// printed definition: no effect grants a sacrifice mana ability.
    #[inline]
    pub(super) fn sacrifices_for_mana(&self, id: ObjId) -> bool {
        let def = self.pool.get(self.objects[id].def);
        let mut sac = false;
        for a in &def.ability_ids {
            match self.pool.ability(*a) {
                AbilityDef::TapForMana(_) => return false,
                AbilityDef::SacForMana(_) => sac = true,
                _ => {}
            }
        }
        sac
    }

    /// Whether `p` could pay `req` with their mana sources (see `payable`).
    pub(super) fn can_pay(&self, p: PlayerId, req: &ManaRequirement) -> bool {
        let ps = self.player(p);
        self.payable(p, &self.mana_sources(p), &ps.mana_pool, req, ps.life, None)
    }

    /// Whether `p` could pay `req` from `sources` and `pool`: the
    /// canonical plan (`payment_plan_from`), else one that activates
    /// `p`'s mana abilities (all but `exclude`'s, the source of an ability
    /// being paid for; see `ability_mana`). `pay_from` pays the same way.
    #[allow(clippy::inline_always)] // measured: out of line costs 0.7% instructions/game
    #[inline(always)]
    pub(super) fn payable(
        &self,
        p: PlayerId,
        sources: &[(ObjId, Mana)],
        pool: &crate::mana::ManaPool,
        req: &ManaRequirement,
        life: i32,
        exclude: Option<ObjId>,
    ) -> bool {
        if req.snow > 0 {
            return self.snow_payment(sources, pool, req, life).is_some();
        }
        payment_plan_from(sources, pool, req, life).is_some()
            || (self.ability_mana_possible()
                && self
                    .ability_plan(p, sources, pool, req, life, exclude)
                    .is_some())
    }

    /// Untapped sources of restricted mana `p` controls (CR 106.6) whose
    /// mana may be spent on `spend`. Empty without any in play.
    fn restricted_sources(&self, p: PlayerId, spend: Spend) -> Sources {
        let mut out = Sources::new();
        if !self.restricted_mana_possible() {
            return out;
        }
        for id in self.permanents_of(p) {
            let o = &self.objects[id];
            if o.tapped || !self.flags_of(o).has_restricted_mana() {
                continue;
            }
            let Some(c) = self.characteristics(id) else {
                continue;
            };
            // CR 302.6 and 602.5, as for the other sources.
            if (c.is_creature() && !o.controlled_since_turn_start && !c.has(Keyword::Haste))
                || c.has(Keyword::CantActivate)
            {
                continue;
            }
            for a in &c.abilities {
                let AbilityDef::RestrictedMana { mana, restriction } = self.pool.ability(*a) else {
                    continue;
                };
                // "a creature spell of the chosen type": the type this
                // permanent's controller chose as it entered.
                let filled;
                let restriction = if restriction.spells.as_ref().is_some_and(|f| f.chosen)
                    || restriction
                        .ability_source
                        .as_ref()
                        .is_some_and(|f| f.chosen)
                {
                    let fill = |f: &crate::card::ObjFilter| {
                        if f.chosen {
                            f.with_chosen(o.chosen)
                        } else {
                            f.clone()
                        }
                    };
                    let mut r = restriction.clone();
                    r.spells = restriction.spells.as_ref().map(fill);
                    r.ability_source = restriction.ability_source.as_ref().map(fill);
                    filled = r;
                    &filled
                } else {
                    restriction
                };
                let allowed = match spend {
                    Spend::Spell(sc) => restriction.allows_spell(sc),
                    Spend::Ability(src) => restriction.allows_abilities(src),
                    Spend::Foretell => restriction.foretell,
                };
                if !allowed {
                    continue;
                }
                match *mana {
                    Mana::Chosen => {
                        if let Some(c) = o.chosen.color() {
                            out.push((id, Mana::Colored(c)));
                        }
                    }
                    m => out.push((id, m)),
                }
            }
        }
        if !self.restricted_pool.is_empty() {
            self.pool_units_for(p, spend, &mut out);
        }
        out
    }

    /// Whether payment needs to look for restricted mana (CR 106.6): a
    /// permanent with a restricted mana ability is in play, or restricted
    /// mana is in a pool. Two loads on every payment check.
    #[inline]
    pub(super) fn restricted_mana_possible(&self) -> bool {
        self.restricted_mana_sources != 0 || !self.restricted_pool.is_empty()
    }

    /// `p`'s restricted mana in the pool that may be spent on `spend`, as
    /// sources (`pool_unit_id`), so that payment spends it after the
    /// unrestricted pool and before tapping permanents.
    #[cold]
    #[inline(never)]
    fn pool_units_for(&self, p: PlayerId, spend: Spend, out: &mut Sources) {
        let mut units = Sources::new();
        for (i, u) in self.restricted_pool.iter().enumerate() {
            if u.player != p {
                continue;
            }
            let r = &u.restriction;
            let allowed = match spend {
                Spend::Spell(sc) => r.allows_spell(sc),
                Spend::Ability(src) => r.allows_abilities(src),
                Spend::Foretell => r.foretell,
            };
            if allowed {
                units.push((pool_unit_id(i), u.mana));
            }
        }
        units.extend_from_slice(out);
        *out = units;
    }

    /// Spend the restricted pool mana `plan` names (CR 106.6): each unit
    /// moves into the pool, to be paid from there with the rest. The rest
    /// of the plan.
    #[cold]
    #[inline(never)]
    fn take_pool_units(&mut self, p: PlayerId, plan: &Sources) -> Sources {
        let mut taken: SmallVec<[usize; 4]> = SmallVec::new();
        let mut rest = Sources::new();
        for &(id, mana) in plan {
            match pool_unit_index(id) {
                Some(i) if i < self.restricted_pool.len() => {
                    taken.push(i);
                    self.player_mut(p).mana_pool.add(mana);
                }
                Some(_) => {}
                None => rest.push((id, mana)),
            }
        }
        taken.sort_unstable();
        for i in taken.into_iter().rev() {
            self.restricted_pool.remove(i);
        }
        rest
    }

    /// The chosen values in `r` filled in from `source` ("a creature spell
    /// of the chosen type").
    pub(super) fn filled_restriction(
        &self,
        r: &crate::card::ManaRestriction,
        source: ObjId,
    ) -> crate::card::ManaRestriction {
        let chosen = r.spells.as_ref().is_some_and(|f| f.chosen)
            || r.ability_source.as_ref().is_some_and(|f| f.chosen);
        let mut out = r.clone();
        if chosen {
            let c = self
                .objects
                .get(source)
                .map(|o| o.chosen)
                .unwrap_or_default();
            let fill = |f: &crate::card::ObjFilter| {
                if f.chosen {
                    f.with_chosen(c)
                } else {
                    f.clone()
                }
            };
            out.spells = r.spells.as_ref().map(fill);
            out.ability_source = r.ability_source.as_ref().map(fill);
        }
        out
    }

    /// `sources` with the restricted sources whose mana may be spent on
    /// `spend` in front, so that the canonical plan spends restricted mana
    /// before mana that could pay for anything; `None` when there are
    /// none (the common case, at the cost of one counter check).
    fn with_restricted(
        &self,
        p: PlayerId,
        sources: &[(ObjId, Mana)],
        spend: Spend,
    ) -> Option<Sources> {
        if !self.restricted_mana_possible() {
            return None;
        }
        let mut all = self.restricted_sources(p, spend);
        if all.is_empty() {
            return None;
        }
        all.extend_from_slice(sources);
        Some(all)
    }

    /// Whether `p` could pay `req` from `sources` and the pool, restricted
    /// mana included where `spend` allows it.
    fn payable_for(
        &self,
        p: PlayerId,
        sources: &[(ObjId, Mana)],
        req: &ManaRequirement,
        spend: Spend,
    ) -> bool {
        let ps = self.player(p);
        match self.with_restricted(p, sources, spend) {
            Some(all) => self.payable(p, &all, &ps.mana_pool, req, ps.life, None),
            None => self.payable(p, sources, &ps.mana_pool, req, ps.life, None),
        }
    }

    /// Whether `p` could pay `req` to cast `card` (its current
    /// characteristics), restricted mana included.
    pub(super) fn spell_payable(&self, p: PlayerId, card: ObjId, req: &ManaRequirement) -> bool {
        if !self.restricted_mana_possible() {
            return self.can_pay(p, req);
        }
        let Some(c) = self.characteristics(card) else {
            return false;
        };
        self.payable_for(p, &self.mana_sources(p), req, Spend::Spell(&c))
    }

    /// The sources `p` may tap to activate an ability: restricted mana that
    /// allows it first (CR 106.6).
    fn ability_sources(&self, p: PlayerId, source: ObjId) -> Sources {
        let sources = self.mana_sources(p);
        if !self.restricted_mana_possible() {
            return sources;
        }
        let c = self.characteristics(source);
        self.with_restricted(p, &sources, Spend::Ability(c.as_ref()))
            .unwrap_or(sources)
    }

    /// Exile `card` and make it plotted (CR 702.170a, 702.170c): its owner
    /// may cast it free as a sorcery from a later turn on. The exiled card.
    pub(super) fn plot(&mut self, card: ObjId) -> ObjId {
        let new = self.move_object(card, Zone::Exile, true);
        self.objects[new].alt = AltCast::Plotted;
        let owner = self.objects[new].owner;
        self.player_mut(owner).adventures_in_exile += 1;
        self.exile_casts
            .get_or_insert_with(Box::default)
            .foretold_this_turn
            .push(new);
        // "When this card becomes plotted".
        let def = self.objects[new].def;
        let pool = std::sync::Arc::clone(&self.pool);
        for a in &pool.get(def).ability_ids {
            if let AbilityDef::Triggered(t) = pool.ability(*a)
                && t.trigger == crate::card::TriggerSpec::BecomesPlotted
            {
                self.queue_builtin(owner, *a, new, def);
            }
        }
        new
    }

    /// The sources `p` may tap to foretell a card (CR 702.143a): restricted
    /// mana that allows it first.
    pub(super) fn foretell_sources(&self, p: PlayerId) -> Sources {
        let sources = self.mana_sources(p);
        self.with_restricted(p, &sources, Spend::Foretell)
            .unwrap_or(sources)
    }

    /// CR 601.2g–h: activate mana abilities per the plan, then pay from the
    /// pool.
    /// The non-mana parts of a cost (CR 118.3): life, a sacrifice ("~" is
    /// `this`), a discard. The choices were made as the spell or ability was
    /// announced.
    #[allow(clippy::too_many_lines)] // one block per cost part
    fn pay_other_costs(
        &mut self,
        p: PlayerId,
        cost: &Cost,
        this: ObjId,
        sacrifice: Option<ObjId>,
        discard: Option<ObjId>,
    ) {
        if cost.life > 0 {
            self.change_life(p, -i32::try_from(cost.life).unwrap_or(i32::MAX));
        }
        // CR 107.3: "pay X life", X announced as the spell was cast.
        if cost.life_x {
            let x = self.x_of(this);
            self.change_life(p, -i32::try_from(x).unwrap_or(i32::MAX));
        }
        if let Some((k, n)) = cost.remove_counters {
            // "Remove a +1/+1 counter from a creature you control".
            let from = if cost.remove_from_creature {
                self.counter_cost_creature(p, k, n)
            } else {
                Some(this)
            };
            if let Some(from) = from {
                self.remove_counters(from, k, n);
            }
        }
        // CR 701.43a: an exerted permanent won't untap during its
        // controller's next untap step.
        if cost.exert
            && let Some(o) = self.objects.get_mut(this)
        {
            o.skip_untap = true;
        }
        if cost.discard_hand {
            let hand: Vec<ObjId> = self.player(p).hand.clone();
            for c in hand {
                self.discard_card(p, c);
            }
        }
        // "Put a -1/-1 counter on this creature" (CR 118.3).
        if let Some((k, n)) = cost.put_counters {
            self.add_counters(this, k, n);
        }
        if cost.reveal_hand {
            self.reveal_hand(p);
        }
        // CR 702.57a: "Reveal this card from your hand".
        if cost.reveal_self
            && let Some(o) = self.objects.get_mut(this)
        {
            o.revealed = u8::MAX;
        }
        if cost.energy > 0 {
            let e = &mut self.player_mut(p).energy;
            *e = e.saturating_sub(cost.energy);
        }
        if let Some(b) = &cost.behold {
            self.pay_behold(p, b, this);
        }
        if cost.blight > 0
            && let Some(c) = self.blight_pick(p)
        {
            self.add_counters(c, CounterKind::MinusOneMinusOne, cost.blight);
        }
        if cost.evidence > 0 {
            self.collect_evidence(p, cost.evidence, this);
        }
        // A processor's cost (CR 400.3: to the owner's graveyard).
        if cost.process > 0 {
            for id in self
                .process_candidates(p)
                .into_iter()
                .take(cost.process as usize)
            {
                self.move_object(id, Zone::Graveyard, false);
            }
        }
        if cost.forage {
            self.forage(p, this, None);
        }
        // CR 702.167a: craft's materials, a graveyard card first.
        if let Some(f) = cost.craft.as_deref()
            && let Some(m) = self.craft_candidates(p, f, this).first().copied()
        {
            self.move_object(m, Zone::Exile, true);
        }
        // CR 606.4: loyalty costs are counters put on or removed from the
        // planeswalker.
        if cost.loyalty > 0 {
            self.add_counters(this, CounterKind::Loyalty, cost.loyalty.unsigned_abs());
        } else if cost.loyalty < 0 {
            self.remove_counters(this, CounterKind::Loyalty, cost.loyalty.unsigned_abs());
        }
        match cost.sacrifice {
            Some(SacCost::This) => self.sacrifice(this),
            Some(SacCost::Filter(ref f)) => {
                if let Some(first) = sacrifice {
                    // "Sacrifice two Goblins": the chosen one, then the
                    // engine's pick of the rest.
                    let mut picks: SmallVec<[ObjId; 4]> = smallvec::smallvec![first];
                    if cost.count > 1 {
                        let rest = self.sacrifice_candidates(p, f, this);
                        picks.extend(
                            rest.into_iter()
                                .filter(|c| *c != first)
                                .take(cost.chosen_count() - 1),
                        );
                    }
                    for s in picks {
                        match cost.chosen {
                            crate::card::ChosenCost::Sacrifice => self.sacrifice(s),
                            crate::card::ChosenCost::Return => {
                                self.move_object(s, Zone::Hand, false);
                            }
                            crate::card::ChosenCost::Tap => {
                                self.tap(s);
                                self.note_tapped_for_cost(s);
                            }
                        }
                    }
                }
            }
            None => {}
        }
        // "Exile two cards from your graveyard": the oldest that match,
        // sparing what the spell or ability being paid for (the top of the
        // stack) targets.
        if let Some(g) = &cost.exile_cards {
            let mut cands = self.exile_candidates(p, g.filter.as_ref(), this);
            if g.top {
                cands.reverse();
            }
            if let Some(top) = self.stack.last().and_then(|t| self.objects.get(*t)) {
                cands.sort_by_key(|c| top.targets.contains(&Target::Object(*c)));
            }
            let gone: SmallVec<[ObjId; 8]> = cands.into_iter().take(g.n as usize).collect();
            for id in gone {
                self.move_object(id, Zone::Exile, true);
            }
        }
        if cost.discard_random {
            // CR 701.9b: the cards are chosen at random from the hand,
            // drawing on the game's random number generator.
            for _ in 0..cost.discard {
                let n = self.player(p).hand.len();
                if n == 0 {
                    break;
                }
                let i = rand::Rng::random_range(&mut self.rng, 0..n);
                let card = self.player(p).hand[i];
                self.discard_card(p, card);
            }
        } else if cost.discard > 0
            && let Some(c) = discard
        {
            self.discard_card(p, c);
            // "Discard two cards": the chosen card, then the rest in hand
            // order (the engine picks them).
            for _ in 1..cost.discard {
                let next = self.player(p).hand.iter().copied().find(|id| {
                    cost.discard_filter.as_ref().is_none_or(|f| {
                        let o = &self.objects[*id];
                        let ch = self.base_characteristics(o);
                        f.matches(*id, o.owner, &ch, None, Some(p)) && self.status_ok(f, *id)
                    })
                });
                let Some(n) = next else { break };
                self.discard_card(p, n);
            }
        }
    }

    /// Remember a permanent tapped to pay a cost, for "the tapped
    /// creature's power" as the ability resolves (`ThisWay::Tapped`).
    fn note_tapped_for_cost(&mut self, id: ObjId) {
        let Some(owner) = self.objects.get(id).map(|o| o.owner) else {
            return;
        };
        if let Some(log) = &mut self.this_way {
            log.moved.push(crate::state::Moved {
                old: id,
                new: id,
                from: Zone::Battlefield,
                to: Zone::Battlefield,
                owner,
                sacrificed: false,
                destroyed: false,
            });
        }
    }

    /// Pay costs for the stack object `obj`, remembering what they
    /// sacrificed or discarded for "the sacrificed creature" and "If a
    /// Saproling was sacrificed this way" as it resolves.
    fn pay_costs_recorded(&mut self, obj: ObjId, pay: impl FnOnce(&mut Self)) {
        let outer = self.this_way.replace(crate::state::ThisWayLog::default());
        pay(self);
        let paid = std::mem::replace(&mut self.this_way, outer).map(|l| l.moved);
        if let Some(paid) = paid
            && !paid.is_empty()
        {
            self.paid_costs.push((obj, paid));
        }
    }

    /// Start recording what the resolution of stack object `obj` does
    /// "this way" (CR 608.2c), beginning with what its costs paid; returns
    /// the record of a resolution this one interrupts (a mana ability's),
    /// for `end_this_way`.
    pub(super) fn begin_this_way(&mut self, obj: ObjId) -> Option<crate::state::ThisWayLog> {
        let mut log = crate::state::ThisWayLog::default();
        if !self.paid_costs.is_empty()
            && let Some(i) = self.paid_costs.iter().position(|(o, _)| *o == obj)
        {
            log.moved = self.paid_costs.swap_remove(i).1;
        }
        self.this_way.replace(log)
    }

    pub(super) fn end_this_way(&mut self, outer: Option<crate::state::ThisWayLog>) {
        self.this_way = outer;
    }

    pub(super) fn pay(&mut self, p: PlayerId, req: &ManaRequirement) {
        let sources = self.mana_sources(p);
        self.pay_from(p, req, &sources, None);
    }

    /// `pay` from these sources (a spell's, with convoke or improvise).
    /// Restricted mana (CR 106.6) is in `sources` only where it may be
    /// spent, and the plan taps a source only once the pool can't cover
    /// that part of the cost, so all the mana it makes is spent here and
    /// none is left in the pool to be spent on something else.
    /// `exclude` is the source of an ability being paid for, whose own
    /// mana abilities are not activated (see `payable`).
    pub(super) fn pay_from(
        &mut self,
        p: PlayerId,
        req: &ManaRequirement,
        sources: &Sources,
        exclude: Option<ObjId>,
    ) {
        let ps = self.player(p);
        if req.snow > 0 {
            let (plan, req, life) = self
                .snow_payment(sources, &ps.mana_pool, req, ps.life)
                .expect("cast was offered only when payable");
            self.tap_for_plan(p, &plan);
            self.finish_payment(p, &req, life);
            return;
        }
        let Some((plan, req, life)) = resolve_payment(sources, &ps.mana_pool, req, ps.life) else {
            let plan = self
                .ability_plan(p, sources, &ps.mana_pool, req, ps.life, exclude)
                .expect("cast was offered only when payable");
            self.pay_with_abilities(p, plan);
            return;
        };
        self.tap_for_plan(p, &plan);
        self.finish_payment(p, &req, life);
    }

    /// A payment of `req`, which has `{S}` symbols (CR 107.4h): each paid
    /// with the mana of a different snow source (a snow permanent) that the
    /// plan taps, the rest planned as usual from the other sources and the
    /// pool. Returns the plan, the requirement with `{S}` turned into
    /// generic mana (paid from the pool once the snow sources' mana is
    /// in it), and the life to pay. Mana already in the pool pays no
    /// `{S}`: the pool doesn't remember where its mana came from.
    #[cold]
    #[inline(never)]
    fn snow_payment(
        &self,
        sources: &[(ObjId, Mana)],
        pool: &crate::mana::ManaPool,
        req: &ManaRequirement,
        life: i32,
    ) -> Option<(Sources, ManaRequirement, u32)> {
        let k = usize::try_from(req.snow).ok()?;
        let mut rest = *req;
        rest.snow = 0;
        // Each snow source once, with the first mana it's listed for.
        let mut snow: SmallVec<[(ObjId, Mana); 8]> = SmallVec::new();
        for &(id, mana) in sources {
            if snow.iter().any(|(s, _)| *s == id) {
                continue;
            }
            let is_snow = self
                .objects
                .get(id)
                .is_some_and(|o| o.zone == Zone::Battlefield)
                && self
                    .characteristics(id)
                    .is_some_and(|c| c.supertypes.contains(crate::card::Supertype::Snow));
            if is_snow {
                snow.push((
                    id,
                    if mana == Mana::Any {
                        Mana::Colorless
                    } else {
                        mana
                    },
                ));
            }
        }
        if snow.len() < k {
            return None;
        }
        // Every way to choose `k` of them, in order, until the rest of the
        // cost can be paid without them.
        let mut pick: SmallVec<[usize; 4]> = (0..k).collect();
        loop {
            let chosen = |id: ObjId| pick.iter().any(|i| snow[*i].0 == id);
            let others: Sources = sources
                .iter()
                .copied()
                .filter(|(id, _)| !chosen(*id))
                .collect();
            if let Some((plan, mut r, l)) = resolve_payment(&others, pool, &rest, life) {
                let mut full: Sources = pick.iter().map(|i| snow[*i]).collect();
                full.extend_from_slice(&plan);
                r.generic += req.snow;
                return Some((full, r, l));
            }
            // The next combination in lexicographic order.
            let n = snow.len();
            let mut i = k;
            loop {
                if i == 0 {
                    return None;
                }
                i -= 1;
                if pick[i] < n - k + i {
                    break;
                }
            }
            pick[i] += 1;
            for j in i + 1..k {
                pick[j] = pick[j - 1] + 1;
            }
        }
    }

    /// Whether `id` is a Treasure (CR 111.10a), for "mana from a Treasure".
    #[cold]
    fn is_treasure(&self, id: ObjId) -> bool {
        self.characteristics(id)
            .is_some_and(|c| c.has_subtype(crate::card::Subtype::TREASURE))
    }

    /// Pay for `spell`, whose text asks about the mana spent to cast it,
    /// and record what was spent (CR 601.2h). The payment is the canonical
    /// one, except that it spends what the text wants where it can: mana
    /// of the colors a condition names (generic mana paid with them), as
    /// many colors as it can for converge, Treasures first for "mana from
    /// a Treasure". Every rider of this kind is a bonus, so the player
    /// would choose that payment.
    #[cold]
    #[inline(never)]
    fn pay_recorded(
        &mut self,
        spell: ObjId,
        p: PlayerId,
        req: &ManaRequirement,
        sources: &Sources,
        want: crate::card::SpentUse,
    ) {
        let outer = self.spending.replace(crate::state::SpentMana::default());
        let ps = self.player(p);
        let (pool, life) = (ps.mana_pool, ps.life);
        let mut sources = sources.clone();
        if want.treasure > 0 {
            // Treasures first, so generic mana is paid with them.
            let mut first: Sources = sources
                .iter()
                .copied()
                .filter(|(id, _)| self.is_treasure(*id))
                .collect();
            if want.treasure == 1 {
                first.truncate(1);
            }
            sources.retain(|e| !first.contains(e));
            first.extend_from_slice(&sources);
            sources = first;
        }
        // Turn generic mana into the wanted colors, one at a time, while
        // the cost stays payable.
        let mut r = *req;
        let fits = |r: &ManaRequirement| resolve_payment(&sources, &pool, r, life).is_some();
        if fits(&r) {
            let wants: SmallVec<[(usize, u32); 5]> = if want.converge {
                (0..5).map(|i| (i, 1)).collect()
            } else {
                (0..5)
                    .filter(|i| want.colors[*i] > 0)
                    .map(|i| (i, u32::from(want.colors[i])))
                    .collect()
            };
            for (ci, n) in wants {
                while r.colored[ci] < n && r.generic > 0 {
                    let mut next = r;
                    next.generic -= 1;
                    next.colored[ci] += 1;
                    if !fits(&next) {
                        break;
                    }
                    r = next;
                }
            }
        }
        self.pay_from(p, &r, &sources, None);
        let spent = std::mem::replace(&mut self.spending, outer).unwrap_or_default();
        self.spent_mana.push((spell, spent));
    }

    /// Tap (or, for delve, exile) the sources of a plan, adding their mana.
    #[allow(clippy::inline_always)] // measured: out of line costs 0.7% instructions/game
    #[inline(always)]
    fn tap_for_plan(&mut self, p: PlayerId, plan: &Sources) {
        let rest;
        let plan = if self.restricted_pool.is_empty() {
            plan
        } else {
            rest = self.take_pool_units(p, plan);
            &rest
        };
        let boosted = if self.mana_trigger_sources > 0 {
            self.tapped_for_mana(p, plan)
        } else {
            SmallVec::new()
        };
        for &(id, mana) in plan {
            // Delve's cards are exiled; everything else is tapped.
            if self
                .objects
                .get(id)
                .is_some_and(|o| o.zone == Zone::Graveyard)
            {
                self.move_object(id, Zone::Exile, true);
                self.player_mut(p).mana_pool.add(mana);
                continue;
            }
            self.tap(id);
            self.player_mut(p).mana_pool.add(mana);
            if self.sacrifices_for_mana(id) {
                if self.spending.is_some()
                    && self.is_treasure(id)
                    && let Some(s) = &mut self.spending
                {
                    s.treasure = s.treasure.saturating_add(1);
                }
                self.sacrifice(id);
            }
        }
        for id in boosted {
            self.fire_mana_triggers(id);
        }
    }

    /// The sources of `plan` tapped for mana (not tapped for convoke or
    /// improvise, CR 702.51, 702.126) whose tapping triggers mana
    /// abilities (CR 605.1b).
    #[cold]
    #[inline(never)]
    fn tapped_for_mana(&self, p: PlayerId, plan: &Sources) -> SmallVec<[ObjId; 2]> {
        let sources = self.mana_sources(p);
        let mut out = SmallVec::new();
        for (id, _) in plan {
            let Some(o) = self.objects.get(*id) else {
                continue;
            };
            if o.zone == Zone::Battlefield
                && !out.contains(id)
                && (sources.iter().any(|(s, _)| s == id) || self.flags_of(o).has_restricted_mana())
                && !self.mana_triggers_on(*id).is_empty()
            {
                out.push(*id);
            }
        }
        out
    }

    /// The effects of mana ability `a` of `id` besides its mana, applied
    /// after the mana as payment activates it ("If there are no depletion
    /// counters on this land, sacrifice it.").
    #[cold]
    #[inline(never)]
    fn mana_ability_rest(&mut self, p: PlayerId, id: ObjId, a: crate::card::AbilityId) {
        let pool = std::sync::Arc::clone(&self.pool);
        let AbilityDef::Activated(ab) = pool.ability(a) else {
            return;
        };
        let mut ctx = EffectCtx {
            source: id,
            controller: p,
            targets: Vec::new(),
            division: Vec::new(),
            legal: Vec::new(),
            trigger_obj: None,
            trigger_player: None,
            trigger_amount: 0,
            created: None,
            revealed: Vec::new(),
            x: 0,
            did: true,
            chosen: Chosen::None,
            couldnt: 0,
            created_tokens: SmallVec::new(),
            counts: pool.counts_owner(a),
        };
        for e in ab.effects.iter().filter(|e| !e.adds_mana()) {
            self.apply_effect(&mut ctx, e);
        }
    }

    /// Pay the "exile cards from your graveyard" cost of mana ability `a`
    /// of `id` as payment activates it: the oldest that match, sparing
    /// what the spell or ability being paid for targets (as
    /// `pay_other_costs`).
    #[cold]
    fn exile_for_cost(&mut self, p: PlayerId, id: ObjId, a: crate::card::AbilityId) {
        let pool = std::sync::Arc::clone(&self.pool);
        let AbilityDef::Activated(ab) = pool.ability(a) else {
            return;
        };
        let Some(g) = &ab.cost.exile_cards else {
            return;
        };
        let mut cands = self.exile_candidates(p, g.filter.as_ref(), id);
        if g.top {
            cands.reverse();
        }
        if let Some(top) = self.stack.last().and_then(|t| self.objects.get(*t)) {
            cands.sort_by_key(|c| top.targets.contains(&Target::Object(*c)));
        }
        for c in cands.into_iter().take(g.n as usize) {
            self.move_object(c, Zone::Exile, true);
        }
    }

    /// CR 601.2g–h with mana abilities activated along the way: tap the
    /// plain sources, activate each ability (its mana cost from the pool,
    /// then {T}, life, the mana it adds, and last its sacrifice), then pay.
    /// The plan named every color, so nothing is asked (CR 106.1b, 605.3b).
    #[cold]
    #[inline(never)]
    fn pay_with_abilities(&mut self, p: PlayerId, plan: crate::engine::ability_mana::AbilityPlan) {
        self.tap_for_plan(p, &plan.taps);
        for act in plan.acts {
            let pool = &mut self.player_mut(p).mana_pool;
            for (have, spent) in pool.colored.iter_mut().zip(act.spend.colored) {
                *have = have.saturating_sub(spent);
            }
            pool.colorless = pool.colorless.saturating_sub(act.spend.colorless);
            if act.tap {
                self.tap(act.id);
            }
            if act.life > 0 {
                self.change_life(p, -i32::try_from(act.life).unwrap_or(i32::MAX));
            }
            if act.exile {
                self.exile_for_cost(p, act.id, act.ability);
            }
            if let Some((k, n)) = act.remove {
                self.remove_counters(act.id, k, n);
            }
            if let Some((k, n)) = act.put {
                self.add_counters(act.id, k, n);
            }
            for m in act.mana {
                self.player_mut(p).mana_pool.add(m);
            }
            if act.rest {
                self.mana_ability_rest(p, act.id, act.ability);
            }
            if act.per_turn {
                self.activated_once.push((act.id, act.ability));
            }
            if act.once_ever {
                self.activated_ever.push((act.id, act.ability));
            }
            if act.sac {
                self.sacrifice(act.id);
            }
        }
        self.finish_payment(p, &plan.req, plan.life);
    }

    /// Pay a plain `req` from the pool, which covers it, and `life` for
    /// Phyrexian symbols.
    #[allow(clippy::inline_always)] // measured: out of line costs 0.7% instructions/game
    #[inline(always)]
    fn finish_payment(&mut self, p: PlayerId, req: &ManaRequirement, life: u32) {
        let req = *req;
        let pool = self.player(p).mana_pool;
        let left = pool
            .payment_plan(&req)
            .expect("pool covers the cost after activating mana abilities");
        if let Some(s) = &mut self.spending {
            s.add_paid(&pool, &left);
        }
        self.player_mut(p).mana_pool = left;
        // CR 107.4f: Phyrexian symbols the mana could not cover cost 2 life.
        if life > 0 {
            self.change_life(p, -i32::try_from(life).unwrap_or(i32::MAX));
        }
        self.bump();
    }

    /// CR 601.2f–i: pay and finish casting; caster gets priority.
    #[allow(clippy::too_many_lines)] // one block per kind of cost
    pub(super) fn finish_cast(
        &mut self,
        spell: ObjId,
        sacrifice: Option<ObjId>,
        discard: Option<ObjId>,
    ) {
        let obj = &self.objects[spell];
        let (caster, def_ref) = (obj.controller, obj.def);
        let x = self.x_of(spell);
        let flashback = obj.cast.flashback();
        let targets: Vec<Target> = obj.targets.iter().copied().collect();
        let pool = std::sync::Arc::clone(&self.pool);
        let def = pool.get(def_ref);
        let mut req = self.spell_requirements(spell, x);
        // CR 702.119a: emerge's cost is reduced by the sacrificed
        // creature's mana value (generic mana only, CR 601.2f).
        if def.emerge
            && obj.alt == AltCast::AltCost
            && let Some(c) = sacrifice.and_then(|c| self.characteristics(c))
        {
            let mv = pool.get(c.def).mana_cost.mana_value();
            adjust_generic(&mut req, -i32::try_from(mv).unwrap_or(i32::MAX));
        }
        // "Tap three untapped white creatures you control": tapped before
        // the mana is paid, so they don't also tap for it (CR 601.2h).
        let mut sacrifice = sacrifice;
        if let (true, Some(SacCost::Filter(f))) = (
            self.additional_of(spell).taps_chosen(),
            &self.additional_of(spell).sacrifice,
        ) {
            let n = self.additional_of(spell).chosen_count();
            let sources = self.spell_sources(spell);
            let f = f.clone();
            for c in self
                .tap_picks(caster, &f, spell, false, n, sacrifice, &sources)
                .unwrap_or_default()
            {
                self.tap(c);
            }
            sacrifice = None;
        }
        // CR 702.180b: the creature chosen for harmonize is tapped as the
        // total cost is paid, before the mana.
        if let Some(i) = self.harmonized.iter().position(|(s, ..)| *s == spell) {
            let (_, c, _) = self.harmonized.swap_remove(i);
            if self
                .objects
                .get(c)
                .is_some_and(|o| o.zone == Zone::Battlefield && !o.tapped)
            {
                self.tap(c);
            }
        }
        // Expend (CR 700.14): the mana spent to cast spells this turn,
        // counted as the cost's mana symbols.
        let i = caster.index().min(3);
        let before = self.mana_spent_this_turn[i];
        self.mana_spent_before_cast[i] = before;
        self.mana_spent_this_turn[i] =
            before.saturating_add(u16::try_from(req.total()).unwrap_or(u16::MAX));
        let sources = self.spell_sources(spell);
        if def.spent.record || self.spent_watchers > 0 {
            self.pay_recorded(spell, caster, &req, &sources, def.spent);
        } else {
            self.pay_from(caster, &req, &sources, None);
        }
        let mut discard = discard;
        if let (Some(fb), true) = (&def.flashback, flashback) {
            // A permanent chosen for the flashback cost (`additional_of`);
            // one to tap was tapped above. Likewise the card discarded for
            // jump-start or retrace (CR 702.133a, 702.81a).
            let pick = if fb.sacrifice.is_some() {
                sacrifice.take()
            } else {
                None
            };
            let card = if fb.discard > 0 { discard.take() } else { None };
            self.pay_other_costs(caster, fb, spell, pick, card);
        }
        // CR 702.33a: a kicker cost other than mana, its permanent chosen
        // after targets (`additional_of`).
        if let (Some(k), true) = (&def.kicker_cost, self.objects[spell].cast.kicked()) {
            let pick = if k.sacrifice.is_some() {
                sacrifice.take()
            } else {
                None
            };
            let card = if k.discard > 0 { discard.take() } else { None };
            self.pay_other_costs(caster, k, spell, pick, card);
        }
        // CR 702.47a: splice costs other than mana ("Sacrifice two
        // Mountains", "Return a blue creature you control to its owner's
        // hand"), the engine choosing what pays them, sparing the spell's
        // targets where it can.
        if !self.splices.is_empty() {
            self.pay_splice_costs(spell, caster);
        }
        // CR 702.138a: exile the other cards escape asks for (the oldest).
        if self.objects[spell].alt == AltCast::Escape
            && let Some(&(_, n)) = def.escape.as_deref()
        {
            let gone: Vec<ObjId> = self
                .player(caster)
                .graveyard
                .iter()
                .copied()
                .take(n as usize)
                .collect();
            for id in gone {
                self.move_object(id, Zone::Exile, true);
            }
        }
        // CR 118.9: the alternative cost's permanents, the chosen one and
        // (for "sacrifice two Mountains") the engine's pick of the rest.
        let alt = def
            .alt_cost
            .as_deref()
            .filter(|_| self.objects[spell].alt == AltCast::AltCost);
        // CR 702.190a: sneak returns the chosen unblocked attacker; the
        // permanent will attack what it was attacking.
        let sneak = self.objects[spell].alt == AltCast::Sneak;
        if sneak && let Some(c) = sacrifice {
            self.ninjutsu.retain(|(s, _)| *s != spell);
            if let Some(at) = self.combat.as_ref().and_then(|cb| cb.attacker(c)) {
                self.ninjutsu.push((spell, at.attacking));
            }
            if self.objects.contains_key(c) {
                self.move_object(c, Zone::Hand, false);
            }
        }
        // CR 702.188a: web-slinging returns the chosen tapped creature.
        let web = self.objects[spell].alt == AltCast::WebSlinging;
        if web
            && let Some(c) = sacrifice
            && self.objects.contains_key(c)
        {
            self.move_object(c, Zone::Hand, false);
        }
        let add_sacrifice = if alt.is_some() || sneak || web {
            None
        } else {
            sacrifice
        };
        let other = self.objects[spell].cast.other_cost();
        self.pay_costs_recorded(spell, |s| {
            let chosen = match &def.additional_or {
                Some(c) if other => c,
                _ => &def.additional,
            };
            let x_cost = chosen.discard_x || chosen.exile_cards.as_ref().is_some_and(|g| g.x);
            if x_cost {
                // "Discard X cards", "exile X cards from your graveyard": as
                // many as the X announced (CR 601.2b).
                let mut add = chosen.clone();
                if add.discard_x {
                    add.discard = x;
                }
                if let Some(g) = &mut add.exile_cards
                    && g.x
                {
                    g.n = x;
                }
                s.pay_other_costs(caster, &add, spell, add_sacrifice, discard);
            } else {
                s.pay_other_costs(caster, chosen, spell, add_sacrifice, discard);
            }
            if let Some(alt) = alt {
                let mut chosen: Vec<ObjId> = sacrifice.into_iter().collect();
                if let Some(SacCost::Filter(f)) = &alt.cost.sacrifice {
                    let rest: Vec<ObjId> = s
                        .sacrifice_candidates(caster, f, spell)
                        .into_iter()
                        .filter(|c| !chosen.contains(c))
                        .take((alt.count as usize).saturating_sub(chosen.len()))
                        .collect();
                    chosen.extend(rest);
                }
                let mut one = alt.cost.clone();
                one.mana = crate::mana::ManaCost::default();
                if chosen.is_empty() {
                    // "You may pay 4 life rather than pay this spell's
                    // mana cost": nothing chosen.
                    s.pay_other_costs(caster, &one, spell, None, None);
                }
                for c in chosen {
                    s.pay_other_costs(caster, &one, spell, Some(c), None);
                }
                // "have each other player gain 6 life".
                if alt.others_gain > 0 {
                    let others: Vec<PlayerId> =
                        s.players_in_game().filter(|q| *q != caster).collect();
                    for q in others {
                        s.gain_life(q, alt.others_gain);
                    }
                }
            }
        });
        // CR 702.166a: bargain's sacrifice (and buyback's, "Buyback—
        // Sacrifice a land.", kept in the same list).
        if !self.bargains.is_empty()
            && let Some(i) = self.bargains.iter().position(|(s, _)| *s == spell)
        {
            let (_, s) = self.bargains.remove(i);
            if self
                .objects
                .get(s)
                .is_some_and(|o| o.zone == Zone::Battlefield)
            {
                self.sacrifice(s);
            }
        }
        // Teamwork taps creatures with the costs.
        if self.objects[spell].cast.teamwork() {
            let n = def.teamwork;
            if let Some(cs) = self.teamwork_tappers(caster, n) {
                for c in cs {
                    self.tap(c);
                }
            } else {
                self.objects[spell].cast.set_teamwork(false);
            }
        }
        // The optional additional cost ("you may collect evidence 6", "you
        // may blight 1"), paid with the others.
        if self.objects[spell].cast.optional_paid()
            && let Some(c) = def.optional_cost.as_deref()
        {
            if self.other_costs_payable(caster, c, spell) {
                let c = c.clone();
                self.pay_other_costs(caster, &c, spell, None, None);
            } else {
                self.objects[spell].cast.set_optional_paid(false);
            }
        }
        // CR 702.78a: conspire taps two creatures with the costs.
        if self.objects[spell].cast.conspired() {
            let two: Vec<ObjId> = self.conspirators(spell).into_iter().take(2).collect();
            if two.len() == 2 {
                for c in two {
                    self.tap(c);
                }
            } else {
                self.objects[spell].cast.set_conspired(false);
            }
        }
        let n = &mut self.spells_this_turn[caster.index().min(3)];
        *n = n.saturating_add(1);
        // "The next spell you cast this turn costs {1} less": used up by
        // this spell, its total cost paid (CR 601.2f-h).
        self.next_spell_less[caster.index().min(3)] = 0;
        self.cast_this_turn.push((caster, def_ref));
        if self.objects[spell].cast.cast_from() == Some(Zone::Hand) {
            self.turn_log.cast_from_hand |= 1 << caster.index().min(7);
        }
        // Void: "a spell was warped this turn" (CR 702.185).
        if self.objects[spell].alt == crate::state::AltCast::Warp && !self.turn_log.warped {
            self.turn_log.warped = true;
            self.history_bump();
        }
        // "As long as you've cast an instant or sorcery spell this turn":
        // the cast is recorded after the card left its zone, so a
        // hand-watching static is refreshed now.
        if self.turn_static_sources > 0
            && self
                .battlefield
                .iter()
                .any(|b| self.pool.get(self.objects[*b].def).hand_static)
        {
            self.cbump();
        }
        self.emit(Event::Cast {
            player: caster,
            spell,
            def: def_ref,
            targets,
        });
        // CR 702.60a: ripple triggers as the spell is cast.
        if def.ripple > 0 {
            let ability = self.pool.builtin(crate::card::Builtin::Ripple);
            self.queue_builtin(caster, ability, spell, def_ref);
        }
        // CR 702.85a: cascade triggers as the spell is cast (its own, or
        // given: "Instant and sorcery spells you cast have cascade").
        let (keywords, storm) = self.spell_keywords(caster, def, spell);
        if keywords.contains(Keyword::Cascade) {
            let ability = self.pool.builtin(crate::card::Builtin::Cascade);
            self.queue_builtin(caster, ability, spell, def_ref);
        }
        // CR 702.78a, 702.153a: "When you do, copy it" / "copy this
        // spell". CR 702.40a: storm copies it once for each spell cast
        // before it this turn; CR 702.56a: replicate once for each time
        // its cost was paid. (Each copy is its own trigger here.)
        let mut copies = self.objects.get(spell).map_or(0, |o| {
            usize::from(o.cast.conspired() || o.cast.casualty()) + o.cast.replicated() as usize
        });
        if storm {
            copies += self.cast_this_turn.len().saturating_sub(1);
        }
        if copies > 0 {
            let ability = self.pool.builtin(crate::card::Builtin::Conspire);
            for _ in 0..copies {
                self.queue_builtin(caster, ability, spell, def_ref);
            }
        }
        self.emit_targeted(spell);
        self.give_priority(caster);
    }

    // ----- resolution -------------------------------------------------------------

    /// CR 701.5: a countered spell goes to its owner's graveyard, or where
    /// the effect says ("exile it instead", "put it on top of its owner's
    /// library instead"); flashback's exile still wins (CR 702.34a).
    pub(super) fn counter_spell(&mut self, id: ObjId, dest: crate::card::CounteredTo) {
        use crate::card::CounteredTo;
        let Some(o) = self.objects.get(id) else {
            return;
        };
        if o.zone != Zone::Stack {
            return;
        }
        // CR 701.5a: a countered ability is removed from the stack.
        if o.kind == ObjKind::Ability {
            self.emit(Event::Countered {
                spell: id,
                def: o.def,
            });
            self.remove_from_game(id);
            return;
        }
        let def = o.def;
        // "This spell can't be countered.", "Spells you control can't be
        // countered."
        if self.pool.get(def).uncounterable
            || (self.cost_change_sources > 0 && self.uncounterable_granted(id))
        {
            return;
        }
        let flashback = o.cast.flashback() && !self.pool.get(def).retrace;
        self.emit(Event::Countered { spell: id, def });
        if self.objects[id].cast.copy() {
            self.remove_from_game(id);
            return;
        }
        match dest {
            CounteredTo::Exile if !flashback => {
                self.move_object(id, Zone::Exile, true);
            }
            CounteredTo::LibraryTop if !flashback => {
                self.move_object(id, Zone::Library, true);
            }
            _ => self.leave_stack(id),
        }
    }

    /// Scry (CR 701.18) or surveil (CR 701.42) `n`: the player chooses
    /// which of the top `n` cards go down (to the bottom, or the graveyard),
    /// in their order; the rest stay on top in their order.
    pub(super) fn scry_or_surveil(&mut self, p: PlayerId, n: u32, graveyard: bool) {
        // "If you've surveilled this turn" (CR 701.25c: surveil 0 is no
        // surveil).
        if graveyard && n > 0 && self.turn_log.surveilled & (1 << p.index().min(7)) == 0 {
            self.turn_log.surveilled |= 1 << p.index().min(7);
            self.history_bump();
        }
        let lib = &self.player(p).library;
        let n = (n as usize).min(lib.len()).min(4);
        if n == 0 {
            return;
        }
        // Top of the library is the end of the vector.
        let top: Vec<ObjId> = lib[lib.len() - n..].iter().rev().copied().collect();
        let mut options: Vec<Choice> = Vec::new();
        for mask in 0..(1u32 << n) {
            let chosen: Vec<ObjId> = top
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, id)| *id)
                .collect();
            options.push(if graveyard {
                Choice::SurveilGraveyard(chosen)
            } else {
                Choice::ScryBottom(chosen)
            });
        }
        let kind = if graveyard {
            DecisionKind::Surveil {
                count: u32::try_from(n).unwrap_or(0),
            }
        } else {
            DecisionKind::Scry {
                count: u32::try_from(n).unwrap_or(0),
            }
        };
        // The scrying player knows these cards where they end up.
        for id in &top {
            self.objects[*id].revealed |= 1 << p.index().min(7);
        }
        let k = self.choose_mid(p, kind, options.clone());
        let down = match &options[k] {
            Choice::ScryBottom(ids) | Choice::SurveilGraveyard(ids) => ids.clone(),
            _ => Vec::new(),
        };
        for id in &down {
            if graveyard {
                self.move_object(*id, Zone::Graveyard, true);
            } else {
                let lib = &mut self.player_mut(p).library;
                lib.retain(|x| x != id);
                lib.insert(0, *id);
            }
        }
        self.emit(Event::Scried {
            player: p,
            looked: u32::try_from(n).unwrap_or(0),
            down: u32::try_from(down.len()).unwrap_or(0),
            surveil: graveyard,
        });
        self.bump();
    }

    /// CR 608: resolve the top object of the stack.
    #[allow(clippy::too_many_lines)]
    pub(super) fn resolve_top(&mut self) {
        let Some(spell) = self.stack.last().copied() else {
            return;
        };
        let obj = self.objects[spell].clone();
        if obj.kind == ObjKind::Ability {
            self.resolve_ability(spell);
            return;
        }
        let pool = std::sync::Arc::clone(&self.pool);
        let def = pool.get(obj.def);
        if let Some(spec) = def.enchant() {
            // CR 303.4f, 608.2b: an Aura spell whose target is illegal is
            // countered; otherwise it enters attached to the target.
            let target = obj.targets.first().copied();
            let legal = target
                .is_some_and(|t| self.is_legal_target_for(obj.controller, spec, t, Some(spell)));
            if !legal && self.pool.get(obj.card).bestow_face == Some(obj.def) {
                // CR 702.103e (608.3b): a bestowed Aura spell with an
                // illegal target stops being bestowed and resolves as the
                // enchantment creature spell it is.
                self.objects[spell].def = obj.card;
                self.cbump();
                self.emit(Event::Resolve {
                    spell,
                    def: obj.card,
                });
                self.choose_copy_on_entering(spell);
                self.put_onto_battlefield(spell, obj.controller);
                return;
            }
            if !legal {
                self.emit(Event::Fizzle {
                    spell,
                    def: obj.def,
                });
                self.leave_stack(spell);
                return;
            }
            self.emit(Event::Resolve {
                spell,
                def: obj.def,
            });
            let new = self.put_onto_battlefield(spell, obj.controller);
            if let Some(Target::Object(t)) = target
                && self.objects.contains_key(new)
            {
                self.objects[new].attached_to = Some(t);
                // Before the event: its triggers read characteristics.
                self.cbump();
                self.emit(Event::Attached { obj: new, to: t });
            }
            self.cbump();
            return;
        }
        let parts: Option<(&[TargetSpec], &[Effect])> =
            (def.spell_ability().is_some() || def.modes().is_some()).then(|| {
                (
                    crate::state::targets_in(&pool, &obj),
                    crate::state::effects_in(&pool, &obj),
                )
            });
        // CR 702.47a: the spliced cards' effects (and targets) follow the
        // spell's own.
        let spliced = if self.splices.is_empty() {
            None
        } else {
            self.splices.iter().find(|s| s.spell == spell).cloned()
        };
        // CR 702.96a: an overloaded spell's "target" reads "each".
        let overloaded = (obj.alt == AltCast::Overload)
            .then(|| pool.get(obj.card))
            .filter(|c| c.overload_face == Some(obj.def))
            .and_then(|c| c.cast_targets().first().copied());
        if let Some((specs, effects)) = parts {
            let specs = spliced.as_ref().map_or(specs, |s| s.targets.as_slice());
            // CR 608.2b: targets that are no longer legal are ignored; if all
            // are illegal the spell does not resolve.
            let legal = self.targets_legal(obj.controller, specs, &obj.targets, Some(spell), None);
            // A spell whose optional targets were all declined has none.
            if all_illegal(&legal, &obj.targets) {
                self.emit(Event::Fizzle {
                    spell,
                    def: obj.def,
                });
                self.leave_stack(spell);
                return;
            }
            self.emit(Event::Resolve {
                spell,
                def: obj.def,
            });
            let mut ctx = EffectCtx {
                source: spell,
                controller: obj.controller,
                targets: obj.targets.to_vec(),
                division: self.division_of(spell),
                legal,
                trigger_obj: None,
                trigger_player: None,
                trigger_amount: 0,
                created: None,
                revealed: Vec::new(),
                x: self.x_of(spell),
                did: true,
                chosen: Chosen::None,
                couldnt: 0,
                created_tokens: SmallVec::new(),
                counts: None,
            };
            let split = self.mode_splits_of(spell);
            let outer = self.begin_this_way(spell);
            // "The damage can't be prevented" covers the spell's damage
            // dealt before the sentence too (CR 615.12).
            if effects.contains(&Effect::DamageUnpreventable)
                && let Some(log) = &mut self.this_way
            {
                log.unpreventable = true;
            }
            if effects.contains(&Effect::ExcessToController)
                && let Some(log) = &mut self.this_way
            {
                log.excess_to_controller = true;
            }
            // CR 702.131a: ascend on an instant or sorcery, first.
            if def.marks.has(crate::card::Marks::ASCEND) {
                self.ascend(obj.controller);
            }
            // CR 702.174j: a promised gift, before the spell's other
            // effects.
            if let Some(gift) = &def.gift
                && obj.cast.kicked()
            {
                self.apply_effects(&mut ctx, gift, &[]);
            }
            match overloaded {
                Some(spec) => self.apply_overloaded(&mut ctx, spec, effects),
                None => self.apply_effects(&mut ctx, effects, &split),
            }
            if let Some(sp) = &spliced {
                self.apply_spliced(&ctx, sp, crate::state::targets_in(&pool, &obj).len());
            }
            self.end_this_way(outer);
        } else {
            self.emit(Event::Resolve {
                spell,
                def: obj.def,
            });
        }
        if let Some(o) = self.objects.get(spell)
            && o.zone == Zone::Stack
        {
            if def.is_permanent_card() {
                // CR 608.3f, 707.10f: a copy of a permanent spell becomes a
                // token as it enters (not "created").
                if obj.cast.copy() {
                    self.objects[spell].kind = ObjKind::Token;
                }
                self.choose_copy_on_entering(spell);
                if obj.alt == AltCast::Sneak {
                    self.enter_sneaking(spell, obj.controller);
                } else if obj.alt == AltCast::Impending
                    && let Some(n) = def.impending.as_deref().map(|i| i.0)
                {
                    // CR 702.176a: "it enters with N time counters".
                    self.put_onto_battlefield_with(
                        spell,
                        obj.controller,
                        super::replace::Enter {
                            tapped: false,
                            counters: Some((CounterKind::Time, n)),
                            more: None,
                            haste: false,
                            prepared: false,
                        },
                    );
                } else {
                    self.put_onto_battlefield(spell, obj.controller);
                }
            } else if obj.cast.copy() {
                // CR 707.10a: a copy ceases to exist off the stack (no
                // buyback to hand, no adventure in exile).
                self.leave_stack(spell);
            } else if obj.cast.adventure() {
                self.finish_adventure(spell);
            } else if obj.cast.bought_back() && !obj.cast.flashback() {
                // CR 702.27a: to its owner's hand as it resolves.
                self.move_object(spell, Zone::Hand, false);
            } else if obj.alt == AltCast::Hand && def.keywords.contains(Keyword::Rebound) {
                self.rebound(spell);
            } else if def.keywords.contains(Keyword::Paradigm) {
                self.paradigm(spell);
            } else if !(def.keywords.contains(Keyword::Cipher)
                && !obj.cast.copy()
                && self.encode(spell, obj.controller))
            {
                self.leave_stack(spell);
                // CR 702.55a: haunt on an instant or sorcery triggers as it
                // is put into a graveyard during its resolution.
                if let Some(ability) = def.haunt_spell
                    && let Some(card) = self.current_object(spell)
                    && self.objects[card].zone == Zone::Graveyard
                {
                    let timestamp = self.tick();
                    self.pending_triggers.push(crate::state::PendingTrigger {
                        controller: obj.controller,
                        ability: StackAbility {
                            ability,
                            source: card,
                            def: self.objects[card].def,
                            trigger_obj: None,
                            trigger_player: None,
                            trigger_amount: 0,
                        },
                        timestamp,
                    });
                }
            }
        }
    }

    /// CR 702.190b: a permanent spell whose sneak cost was paid enters
    /// tapped and attacking what the returned creature was attacking.
    #[cold]
    #[inline(never)]
    fn enter_sneaking(&mut self, spell: ObjId, controller: PlayerId) {
        let at = self
            .ninjutsu
            .iter()
            .position(|(s, _)| *s == spell)
            .map(|i| self.ninjutsu.remove(i).1);
        let new = self.put_onto_battlefield_with(
            spell,
            controller,
            super::replace::Enter {
                tapped: true,
                counters: None,
                more: None,
                haste: false,
                prepared: false,
            },
        );
        if self
            .objects
            .get(new)
            .is_some_and(|o| o.zone == Zone::Battlefield)
        {
            let defender = at.unwrap_or_else(|| {
                Target::Player(
                    self.players_in_game()
                        .find(|q| *q != controller)
                        .unwrap_or(controller),
                )
            });
            self.enter_attacking(new, defender);
        }
    }

    /// CR 702.96a: an overloaded spell's effects, once for each object
    /// or player its target spec describes (no targeting, so hexproof and
    /// protection don't matter, CR 702.96b), as though that were the
    /// target: players in turn order, then permanents in timestamp order.
    #[cold]
    #[inline(never)]
    fn apply_overloaded(&mut self, ctx: &mut EffectCtx, spec: TargetSpec, effects: &[Effect]) {
        let spec = self.pool.base_spec(spec);
        let mut each: Vec<Target> = self
            .players_in_game()
            .map(Target::Player)
            .filter(|t| self.spec_matches(ctx.controller, spec, *t, Some(ctx.source), None, false))
            .collect();
        each.extend(
            self.battlefield
                .iter()
                .map(|id| Target::Object(*id))
                .filter(|t| {
                    self.spec_matches(ctx.controller, spec, *t, Some(ctx.source), None, false)
                }),
        );
        for t in each {
            ctx.targets = vec![t];
            ctx.legal = vec![true];
            ctx.created = None;
            ctx.did = true;
            self.apply_effects(ctx, effects, &[]);
        }
    }

    /// `Effect::CastCopy` (CR 707.12): a copy of each recipient card is
    /// made in exile, and its controller may cast it as the resolution
    /// finishes (free, or for its mana cost if they can pay it); a copy not
    /// cast ceases to exist. "If you do" sees whether one will be.
    #[cold]
    #[inline(never)]
    fn cast_copy(&mut self, ctx: &mut EffectCtx, what: Recipient, free: bool) {
        let p = ctx.controller;
        let alt = if free { AltCast::Free } else { AltCast::None };
        let mut any = false;
        for t in self.recipients(ctx, what) {
            let Target::Object(card) = t else {
                continue;
            };
            let Some(o) = self.objects.get(card) else {
                continue;
            };
            if matches!(o.zone, Zone::Battlefield | Zone::Stack) || o.kind != ObjKind::Card {
                continue;
            }
            let copy = self.new_object(o.def, p, Zone::Exile);
            self.objects[copy].cast = self.objects[copy].cast.copied();
            self.player_mut(p).exile.push(copy);
            // The decision names the card as it was before this resolution
            // moved it ("exile target card ... and copy it"), an object
            // that exists while the decision is asked.
            let shown = match what {
                Recipient::Target(i) => match ctx.targets.get(i) {
                    Some(Target::Object(id)) => *id,
                    _ => card,
                },
                _ => card,
            };
            if self.offer_exile_cast_shown(copy, alt, p, shown) {
                any = true;
            } else {
                self.remove_from_game(copy);
            }
        }
        ctx.did = any;
    }

    /// CR 707.10: a copy of the source spell on the stack (as it last
    /// existed there if it has left), controlled by the ability's
    /// controller, its controller may choose new targets (CR 707.10c).
    #[cold]
    #[inline(never)]
    fn copy_spell(&mut self, ctx: &EffectCtx) {
        self.copy_spell_object(ctx.source, ctx.controller, true, None);
    }

    /// `Effect::CopySpells`: `times` copies of each recipient spell. "Copy
    /// it" and "copy that spell" (the source or the triggering spell) copy
    /// it as it last existed on the stack if it has left (CR 707.10); a
    /// target spell must still be a legal target (CR 608.2b).
    #[cold]
    #[inline(never)]
    fn copy_spells(
        &mut self,
        ctx: &mut EffectCtx,
        what: Recipient,
        times: crate::card::Num,
        new_targets: bool,
        except: bool,
    ) {
        let spells: SmallVec<[ObjId; 2]> = match what {
            Recipient::Source => smallvec::smallvec![ctx.source],
            Recipient::TriggeringObject => ctx.trigger_obj.into_iter().collect(),
            other => self
                .recipients(ctx, other)
                .into_iter()
                .filter_map(|t| match t {
                    Target::Object(id) => Some(id),
                    Target::Player(_) => None,
                })
                .collect(),
        };
        let x = except.then(|| self.resolving_def(ctx));
        for spell in spells {
            // "Copy it X times": the copied spell's X.
            let n = if times == crate::card::Num::X {
                self.x_chosen(spell).unwrap_or(ctx.x)
            } else {
                self.eval_num(times, ctx)
            };
            for _ in 0..n {
                if self
                    .copy_spell_object(spell, ctx.controller, new_targets, x)
                    .is_none()
                {
                    ctx.did = false;
                    break;
                }
            }
        }
    }

    /// CR 707.10, 707.10b: a copy of the ability object `orig` on the
    /// stack, with its source, mode, X, and targets, controlled by
    /// `controller` (who may choose new targets with `new_targets`). A
    /// copy of an activated ability isn't activated.
    #[cold]
    #[inline(never)]
    fn copy_ability_object(&mut self, of: ObjId, controller: PlayerId, new_targets: bool) -> ObjId {
        let orig = self.objects[of].clone();
        let timestamp = self.tick();
        let copy = self.objects.insert(crate::state::GameObject {
            controller,
            owner: controller,
            timestamp,
            ..orig
        });
        self.stack.push(copy);
        if let Some(x) = self.x_chosen(of) {
            self.x_values.push((copy, x));
        }
        self.cbump();
        if new_targets {
            let source = orig.ability.map_or(copy, |sa| sa.source);
            self.choose_copy_targets(copy, controller, source);
        }
        self.emit_targeted_objects(copy);
        copy
    }

    /// CR 707.10c: the copy's controller may change each of its targets to
    /// another one legal for its spec (judged from `source`), or keep it.
    fn choose_copy_targets(&mut self, copy: ObjId, controller: PlayerId, source: ObjId) {
        let specs: Vec<TargetSpec> =
            crate::state::targets_in(&self.pool, &self.objects[copy]).to_vec();
        for i in 0..self.objects[copy].targets.len() {
            let Some(spec) = crate::card::spec_for(&specs, i) else {
                break;
            };
            let current = self.objects[copy].targets[i];
            // A declined target stays declined (CR 707.10c changes targets,
            // not their number).
            if current.is_none() {
                continue;
            }
            let mut options = vec![Choice::Target(current)];
            options.extend(
                self.legal_targets(controller, spec, Some(source))
                    .into_iter()
                    .filter(|t| *t != current && !self.objects[copy].targets.contains(t))
                    .map(Choice::Target),
            );
            if options.len() < 2 {
                continue;
            }
            let k = self.choose_mid(
                controller,
                DecisionKind::CopyTarget {
                    spell: copy,
                    index: i,
                },
                options.clone(),
            );
            if let Some(Choice::Target(t)) = options.get(k) {
                self.objects[copy].targets[i] = *t;
            }
        }
    }

    /// CR 707.10: a copy of the spell `of` on the stack (as it last
    /// existed there if it has left), controlled by `controller`, with the
    /// spell's choices: mode, X, targets, division, how it was cast
    /// (overloaded). With `new_targets`, its controller may change each
    /// target to another legal one (CR 707.10c). `except` is a definition
    /// whose copy exceptions apply to the copy (CR 707.9b).
    #[cold]
    #[inline(never)]
    pub(super) fn copy_spell_object(
        &mut self,
        of: ObjId,
        controller: PlayerId,
        new_targets: bool,
        except: Option<crate::card::CardRef>,
    ) -> Option<ObjId> {
        let orig = match self.objects.get(of) {
            // CR 707.10: an activated or triggered ability on the stack is
            // copied the same way ("Copy target triggered ability you
            // control"); the copy has the same source (CR 707.10b).
            Some(o) if o.zone == Zone::Stack && o.kind == ObjKind::Ability => {
                return Some(self.copy_ability_object(of, controller, new_targets));
            }
            Some(o) if o.zone == Zone::Stack && o.kind == ObjKind::Card => o.clone(),
            Some(_) => return None,
            None => match self.lki.get(&of) {
                Some(l) if l.object.zone == Zone::Stack && l.object.kind == ObjKind::Card => {
                    l.object.clone()
                }
                _ => return None,
            },
        };
        let copy = self.new_object(orig.def, controller, Zone::Stack);
        self.stack.push(copy);
        {
            let o = &mut self.objects[copy];
            o.card = orig.card;
            o.except = except.or(orig.except);
            o.controller = controller;
            o.cast = orig.cast.copied();
            o.alt = if orig.alt == AltCast::Overload {
                AltCast::Overload
            } else {
                AltCast::None
            };
            o.targets.clone_from(&orig.targets);
        }
        if let Some(x) = self.x_chosen(of) {
            self.x_values.push((copy, x));
        }
        if let Some(d) = self.divisions.iter().find(|(s, _)| *s == of) {
            let d = d.1.clone();
            self.divisions.push((copy, d));
        }
        self.cbump();
        if new_targets {
            self.choose_copy_targets(copy, controller, copy);
        }
        let def = self.objects[copy].def;
        self.emit(Event::SpellCopied {
            player: controller,
            spell: copy,
            def,
        });
        self.emit_targeted_objects(copy);
        Some(copy)
    }

    /// The parts of `spell`'s splice costs other than mana (CR 702.47a).
    #[cold]
    #[inline(never)]
    fn pay_splice_costs(&mut self, spell: ObjId, caster: PlayerId) {
        let Some(sp) = self.splices.iter().find(|s| s.spell == spell) else {
            return;
        };
        let pool = std::sync::Arc::clone(&self.pool);
        let costs: Vec<&Cost> = sp
            .cards
            .iter()
            .filter_map(|(_, d)| pool.get(*d).splice.as_deref())
            .collect();
        for cost in costs {
            let pick = match &cost.sacrifice {
                Some(SacCost::Filter(f)) => {
                    let mut cands = self.sacrifice_candidates(caster, f, spell);
                    let targets = &self.objects[spell].targets;
                    cands.sort_by_key(|c| targets.contains(&Target::Object(*c)));
                    cands.first().copied()
                }
                _ => None,
            };
            self.pay_other_costs(caster, cost, spell, pick, None);
        }
    }

    /// CR 702.47a: a spliced card's effects, as part of the resolving
    /// spell, each card with its own slice of the spell's targets (from
    /// `start`, after the spell's own).
    fn apply_spliced(&mut self, ctx: &EffectCtx, sp: &crate::state::Spliced, mut start: usize) {
        let pool = std::sync::Arc::clone(&self.pool);
        for (_, d) in &sp.cards {
            let def = pool.get(*d);
            let n = def.cast_targets().len();
            let end = (start + n).min(ctx.targets.len());
            let from = start.min(end);
            let mut part = EffectCtx {
                source: ctx.source,
                controller: ctx.controller,
                targets: ctx.targets[from..end].to_vec(),
                division: Vec::new(),
                legal: ctx.legal.get(from..end).unwrap_or(&[]).to_vec(),
                trigger_obj: None,
                trigger_player: None,
                trigger_amount: 0,
                created: None,
                revealed: Vec::new(),
                x: 0,
                did: true,
                chosen: Chosen::None,
                couldnt: 0,
                created_tokens: SmallVec::new(),
                // A spliced card's counts are its own.
                counts: Some(*d),
            };
            start += n;
            let effects = def.spell_ability().map_or(&[][..], |(_, e)| e);
            self.apply_effects(&mut part, effects, &[]);
        }
    }

    /// CR 702.88a: a rebound spell cast from hand is exiled as it resolves;
    /// at its owner's next upkeep it may be cast free.
    fn rebound(&mut self, spell: ObjId) {
        let controller = self.objects[spell].controller;
        let new = self.move_object(spell, Zone::Exile, true);
        let Some(o) = self.objects.get_mut(new) else {
            return;
        };
        o.alt = AltCast::Rebounding;
        let def = o.def;
        let ability = self.pool.builtin(crate::card::Builtin::Rebound);
        self.delayed_triggers.push(crate::state::DelayedTrigger {
            controller,
            ability: crate::state::StackAbility {
                ability,
                source: new,
                def,
                trigger_obj: None,
                trigger_player: None,
                trigger_amount: 0,
            },
            this_turn: false,
            forever: false,
            turn: self.turn_number,
        });
    }

    /// CR 702.192a: a resolved paradigm spell is exiled; the first time a
    /// spell its controller controls with its name resolves, a delayed
    /// trigger copies it at the beginning of each of their precombat main
    /// phases for the rest of the game.
    fn paradigm(&mut self, spell: ObjId) {
        let controller = self.objects[spell].controller;
        let card = self.objects[spell].card;
        let name = self.pool.get(card).name.clone();
        let first = !self
            .paradigms
            .iter()
            .any(|(p, c, _)| *p == controller && self.pool.get(*c).name == name);
        let new = self.move_object(spell, Zone::Exile, true);
        if !first {
            return;
        }
        self.paradigms.push((controller, card, new));
        let ability = self.pool.builtin(crate::card::Builtin::Paradigm);
        self.delayed_triggers.push(crate::state::DelayedTrigger {
            controller,
            ability: crate::state::StackAbility {
                ability,
                source: new,
                def: card,
                trigger_obj: None,
                trigger_player: None,
                trigger_amount: 0,
            },
            this_turn: false,
            forever: true,
            turn: self.turn_number,
        });
    }

    /// A resolved adventure goes on an adventure in exile (CR 715.4); a
    /// resolved omen is shuffled into its owner's library.
    fn finish_adventure(&mut self, spell: ObjId) {
        let o = &self.objects[spell];
        let owner = o.owner;
        if self.pool.get(o.card).layout == Layout::Omen {
            self.move_object(spell, Zone::Library, true);
            self.shuffle_library(owner);
            return;
        }
        let new = self.move_object(spell, Zone::Exile, true);
        self.objects[new].cast.set_adventure(true);
        self.player_mut(owner).adventures_in_exile += 1;
    }

    /// The color `source` chose as it entered (CR 607.2a), if it is still
    /// that object.
    pub(super) fn chosen_color_of(&self, source: ObjId) -> Option<Color> {
        self.objects.get(source).and_then(|o| o.chosen.color())
    }

    /// The player chooses a color, creature type, or basic land type (as
    /// a permanent enters, CR 614.12, or as an effect resolves, CR 608.2d).
    pub(super) fn choose_kind(
        &mut self,
        p: PlayerId,
        kind: ChoiceKind,
        def: crate::card::CardRef,
    ) -> Chosen {
        let (decision, options): (DecisionKind, Vec<Choice>) = match kind {
            // "Choose Khans or Dragons": `choose "1"` or `"2"` in a
            // scenario.
            ChoiceKind::Named(n) => (
                DecisionKind::ChooseOption { def },
                (0..usize::from(n)).map(Choice::Mode).collect(),
            ),
            ChoiceKind::Colors => (
                DecisionKind::ChooseColor,
                Color::ALL.into_iter().map(Choice::Color).collect(),
            ),
            ChoiceKind::Color { except } => (
                DecisionKind::ChooseColor,
                Color::ALL
                    .into_iter()
                    .filter(|c| Some(*c) != except)
                    .map(Choice::Color)
                    .collect(),
            ),
            ChoiceKind::CreatureType { except } => (
                DecisionKind::ChooseType,
                (0..self.pool.subtype_count())
                    .filter_map(|i| u16::try_from(i).ok().map(Subtype))
                    .filter(|s| s.is_creature_type() && Some(*s) != except)
                    .map(Choice::Subtype)
                    .collect(),
            ),
            // "choose a card name": the names of the cards in the game, in
            // name order (`choose "Lightning Bolt"` in a scenario).
            ChoiceKind::CardName { nonland } => {
                let mut seen: Vec<(&str, ObjId, crate::card::CardRef)> = Vec::new();
                for (id, o) in &self.objects {
                    if o.kind != ObjKind::Card {
                        continue;
                    }
                    let d = self.pool.get(o.card);
                    if d.name.is_empty() || (nonland && d.is_land()) {
                        continue;
                    }
                    if !seen.iter().any(|(n, _, _)| *n == d.name) {
                        seen.push((d.name.as_str(), id, o.card));
                    }
                }
                // The entering card itself, which is between zones (no
                // object stands for it; the choice is read by its name).
                let own = self.pool.get(def);
                if !own.name.is_empty()
                    && (!nonland || !own.is_land())
                    && !seen.iter().any(|(n, _, _)| *n == own.name)
                {
                    seen.push((own.name.as_str(), ObjId::default(), def));
                }
                seen.sort_by(|a, b| a.0.cmp(b.0));
                (
                    DecisionKind::ChooseCard,
                    seen.into_iter()
                        .map(|(_, card, def)| Choice::Discard { card, def })
                        .collect(),
                )
            }
            ChoiceKind::LandType(set) => (
                DecisionKind::ChooseType,
                set.iter()
                    .map(|c| Choice::Subtype(Subtype::basic(c)))
                    .collect(),
            ),
        };
        if options.is_empty() {
            return Chosen::None;
        }
        let k = if options.len() == 1 {
            0
        } else {
            self.choose_mid(p, decision, options.clone())
        };
        match options[k.min(options.len() - 1)] {
            Choice::Color(c) => Chosen::Color(c),
            Choice::Subtype(s) => Chosen::Subtype(s),
            Choice::Mode(i) => Chosen::Option(u8::try_from(i).unwrap_or(0)),
            Choice::Discard { def, .. } => Chosen::card(def),
            _ => Chosen::None,
        }
    }

    /// CR 707.?: "you may have ~ enter as a copy of ...". The controller
    /// chooses among the matching permanents (or nothing) as the spell
    /// resolves; the copy effect is set on the spell so that it enters with
    /// the copied values (and the copied card's "enters" effects, CR
    /// 614.12).
    fn choose_copy_on_entering(&mut self, spell: ObjId) {
        let o = &self.objects[spell];
        let (controller, card) = (o.controller, o.card);
        let Some(filter) = self.pool.get(card).enters_as_copy().cloned() else {
            return;
        };
        let candidates: Vec<ObjId> = self
            .battlefield
            .iter()
            .copied()
            .filter(|id| {
                let t = &self.objects[*id];
                self.characteristics(*id).is_some_and(|c| {
                    filter.matches(*id, t.controller, &c, Some(spell), Some(controller))
                }) && self.status_ok(&filter, *id)
            })
            .collect();
        if candidates.is_empty() {
            return;
        }
        let mut options: Vec<Choice> = candidates
            .iter()
            .map(|id| Choice::CopyOf {
                obj: *id,
                def: self.objects[*id].def,
            })
            .collect();
        options.push(Choice::NoCopy);
        let k = self.choose_mid(controller, DecisionKind::ChooseCopy, options);
        let Some(of) = candidates.get(k).copied() else {
            return;
        };
        let (def, except, sized) = {
            let t = &self.objects[of];
            (t.def, t.except, t.sized)
        };
        // The copying card's own exceptions (Phantasmal Image) replace
        // those of what it copies.
        let except = if self.pool.get(card).copy_except.is_some() {
            Some(card)
        } else {
            except
        };
        let s = &mut self.objects[spell];
        s.def = def;
        s.except = except;
        s.sized = sized;
        self.bump();
        self.emit(Event::BecameCopy {
            obj: spell,
            of: def,
        });
    }

    /// Per chosen target, whether it is still legal for its spec (CR
    /// 608.2b); a variable group's spec covers the rest of the list.
    pub(super) fn targets_legal(
        &self,
        controller: PlayerId,
        specs: &[TargetSpec],
        targets: &[Target],
        source: Option<ObjId>,
        trigger_player: Option<PlayerId>,
    ) -> Vec<bool> {
        targets
            .iter()
            .enumerate()
            .map_while(|(i, t)| {
                crate::card::spec_for(specs, i).map(|spec| {
                    !t.is_none()
                        && self.is_legal_target_for(controller, spec, *t, source)
                        && self.that_player_ok(spec, *t, trigger_player, targets)
                })
            })
            .collect()
    }

    /// The still-legal targets of a divided effect's group with what each
    /// was assigned (CR 601.2d).
    fn divided(ctx: &EffectCtx, to: Recipient) -> SmallVec<[(Target, u32); 4]> {
        let first = match to {
            Recipient::Targets(a, _) => usize::from(a),
            Recipient::Target(i) => i,
            _ => return SmallVec::new(),
        };
        let count = ctx.targets.len().saturating_sub(first);
        (0..count)
            .filter(|k| ctx.legal.get(first + k).copied().unwrap_or(false))
            .filter_map(|k| {
                let n = ctx.division.get(k).copied().unwrap_or(0);
                ctx.targets.get(first + k).map(|t| (*t, n))
            })
            .filter(|(_, n)| *n > 0)
            .collect()
    }

    /// The creatures blocking the triggering object, or else the source
    /// ("each creature blocking it", CR 509.1h).
    fn blocking_it(&self, ctx: &EffectCtx) -> SmallVec<[ObjId; 4]> {
        let attacker = ctx
            .trigger_obj
            .and_then(|o| self.current_object(o))
            .or_else(|| self.current_object(ctx.source));
        let Some(a) = attacker.and_then(|a| self.combat.as_ref()?.attacker(a)) else {
            return SmallVec::new();
        };
        a.blockers
            .iter()
            .copied()
            .filter(|b| {
                self.objects
                    .get(*b)
                    .is_some_and(|o| o.zone == Zone::Battlefield)
            })
            .collect()
    }

    /// Remember the objects a group recipient denoted, for "them" and
    /// "those creatures" later in the resolution (`Recipient::ThoseObjects`).
    fn note_group(&mut self, out: &[Target]) {
        if let Some(log) = &mut self.this_way {
            log.group.clear();
            log.group.extend(out.iter().filter_map(|t| match t {
                Target::Object(id) => Some(*id),
                Target::Player(_) => None,
            }));
        }
    }

    /// Who or what a `Recipient` denotes while `ctx`'s effects resolve.
    #[allow(clippy::too_many_lines)] // one arm per recipient kind
    fn recipients(&mut self, ctx: &EffectCtx, r: Recipient) -> SmallVec<[Target; 2]> {
        let mut out = SmallVec::new();
        match r {
            Recipient::Target(i) => {
                if ctx.legal.get(i).copied().unwrap_or(false)
                    && let Some(t) = ctx.targets.get(i)
                {
                    // "Exile target creature, then return it": the effect
                    // that moved it finds the new object (CR 400.7).
                    match *t {
                        Target::Object(id)
                            if !self.objects.contains_key(id)
                                && ctx.created.is_some()
                                && self.current_object(id) == ctx.created =>
                        {
                            out.push(Target::Object(ctx.created.unwrap_or(id)));
                        }
                        _ => out.push(*t),
                    }
                }
            }
            Recipient::Targets(first, count) => {
                for i in usize::from(first)..usize::from(first) + usize::from(count) {
                    if ctx.legal.get(i).copied().unwrap_or(false)
                        && let Some(t) = ctx.targets.get(i)
                    {
                        out.push(*t);
                    }
                }
            }
            Recipient::ControllerOf(i) | Recipient::OwnerOf(i) => {
                // "that player or that permanent's controller" (Chain
                // Lightning): a player target is that player.
                if let Some(Target::Player(p)) = ctx.targets.get(i)
                    && matches!(r, Recipient::ControllerOf(_))
                {
                    out.push(Target::Player(*p));
                }
                if let Some(Target::Object(id)) = ctx.targets.get(i) {
                    let owner = matches!(r, Recipient::OwnerOf(_));
                    let p = self
                        .objects
                        .get(*id)
                        .map(|o| if owner { o.owner } else { o.controller })
                        .or_else(|| {
                            self.lki.get(id).map(|l| {
                                if owner {
                                    l.object.owner
                                } else {
                                    l.object.controller
                                }
                            })
                        });
                    if let Some(p) = p {
                        out.push(Target::Player(p));
                    }
                }
            }
            Recipient::You => out.push(Target::Player(ctx.controller)),
            Recipient::BlockingIt => {
                out.extend(self.blocking_it(ctx).into_iter().map(Target::Object));
            }
            Recipient::EachOpponent => {
                out.extend(
                    self.players_in_game()
                        .filter(|p| *p != ctx.controller)
                        .map(Target::Player),
                );
            }
            Recipient::Source => {
                if let Some(id) = self.current_object(ctx.source) {
                    out.push(Target::Object(id));
                }
            }
            Recipient::TriggeringObject => {
                if let Some(id) = ctx.trigger_obj.and_then(|o| self.current_object(o)) {
                    out.push(Target::Object(id));
                }
            }
            Recipient::TriggeringPlayer => {
                if let Some(p) = ctx.trigger_player {
                    out.push(Target::Player(p));
                }
            }
            Recipient::TriggeringCard => {
                if let Some(id) = ctx
                    .trigger_obj
                    .and_then(|o| self.lki.get(&o)?.moved_to)
                    .filter(|id| self.objects.contains_key(*id))
                {
                    out.push(Target::Object(id));
                }
            }
            Recipient::TriggeringController => {
                let p = ctx.trigger_obj.and_then(|o| match self.objects.get(o) {
                    Some(x) if x.zone == Zone::Battlefield => Some(x.controller),
                    x => self
                        .lki
                        .get(&o)
                        .map(|l| l.object.controller)
                        .or(x.map(|x| x.controller)),
                });
                if let Some(p) = p {
                    out.push(Target::Player(p));
                }
            }
            Recipient::DefendingPlayer => {
                let attacked = ctx
                    .trigger_obj
                    .and_then(|a| self.combat.as_ref()?.attacker(a).map(|at| at.attacking));
                // If the attacker already left combat, the player it was
                // attacking is the only opponent in a two-player game.
                out.push(attacked.unwrap_or(Target::Player(self.opponent(ctx.controller))));
            }
            Recipient::EachCreature => {
                out.extend(
                    self.battlefield
                        .iter()
                        .copied()
                        .filter(|id| self.is_creature_on_battlefield(*id))
                        .map(Target::Object),
                );
                self.note_group(&out);
            }
            Recipient::EachPlayer => out.extend(self.players_in_game().map(Target::Player)),
            Recipient::EachCreatureYouControl => {
                out.extend(self.creatures_of(ctx.controller).map(Target::Object));
                self.note_group(&out);
            }
            Recipient::EachPermanentYouControl => {
                out.extend(self.permanents_of(ctx.controller).map(Target::Object));
                self.note_group(&out);
            }
            // "Untap them": what the last group denoted (CR 608.2c).
            Recipient::ThoseObjects => {
                if let Some(log) = &self.this_way {
                    out.extend(
                        log.group
                            .iter()
                            .filter(|id| {
                                self.objects
                                    .get(**id)
                                    .is_some_and(|o| o.zone == Zone::Battlefield)
                            })
                            .map(|id| Target::Object(*id)),
                    );
                }
            }
            Recipient::SameObject => {
                if let Some(id) = ctx.trigger_obj.filter(|id| self.objects.contains_key(*id)) {
                    out.push(Target::Object(id));
                }
            }
            Recipient::CreatedObject => {
                if let Some(id) = ctx.created.filter(|id| self.objects.contains_key(*id)) {
                    out.push(Target::Object(id));
                }
            }
            Recipient::CreatedTokens => {
                for id in &ctx.created_tokens {
                    if self
                        .objects
                        .get(*id)
                        .is_some_and(|o| o.zone == Zone::Battlefield)
                    {
                        out.push(Target::Object(*id));
                    }
                }
            }
            Recipient::Attached => {
                // "Sacrifice this Equipment: Equipped creature can't be
                // blocked this turn": a source that has left uses what it
                // was attached to as it last existed (CR 608.2h).
                let attached = match self.objects.get(ctx.source) {
                    Some(o) if o.zone == Zone::Battlefield => o.attached_to,
                    _ => self.lki.get(&ctx.source).and_then(|l| l.object.attached_to),
                };
                if let Some(t) = attached.filter(|t| self.objects.contains_key(*t)) {
                    out.push(Target::Object(t));
                } else if let Some(p) = self.attached_player_of(ctx.source) {
                    // "enchanted player" (CR 303.4).
                    out.push(Target::Player(p));
                }
            }
            Recipient::AttachedController => {
                if let Some(p) = self.attached_player_of(ctx.source) {
                    out.push(Target::Player(p));
                    return out;
                }
                let attached = match self.objects.get(ctx.source) {
                    Some(o) if o.zone == Zone::Battlefield => o.attached_to,
                    _ => self.lki.get(&ctx.source).and_then(|l| l.object.attached_to),
                };
                let p = attached.and_then(|t| match self.objects.get(t) {
                    Some(o) if o.zone == Zone::Battlefield => Some(o.controller),
                    _ => self.lki.get(&t).map(|l| l.object.controller),
                });
                if let Some(p) = p {
                    out.push(Target::Player(p));
                }
            }
            Recipient::Group(f) => {
                self.group_into(ctx, f, &mut out);
                self.note_group(&out);
            }
            Recipient::ExiledCard => {
                // CR 607.2a: what the source's linked ability exiled, while
                // it's still in exile.
                let objects = &self.objects;
                self.linked_exiles
                    .retain(|l| objects.get(l.card).is_some_and(|o| o.zone == Zone::Exile));
                out.extend(
                    self.linked_exiles
                        .iter()
                        .filter(|l| l.source == ctx.source)
                        .map(|l| Target::Object(l.card)),
                );
            }
            Recipient::ExiledWith(f) => {
                // CR 607.2a: what the source's abilities exiled, still in
                // exile, matching the filter as cards there.
                let objects = &self.objects;
                self.linked_exiles
                    .retain(|l| objects.get(l.card).is_some_and(|o| o.zone == Zone::Exile));
                let filter = self.pool.filter(f);
                let you = ctx.controller;
                out.extend(
                    self.exiled_with(ctx.source)
                        .filter(|id| {
                            let o = &self.objects[*id];
                            let c = self.base_characteristics(o);
                            filter.matches(*id, o.owner, &c, None, Some(you))
                        })
                        .map(Target::Object),
                );
                self.note_group(&out);
            }
            Recipient::DamagedThisWay => {
                if let Some(log) = &self.this_way {
                    out.extend(
                        log.damaged
                            .iter()
                            .filter(|id| {
                                self.objects
                                    .get(**id)
                                    .is_some_and(|o| o.zone == Zone::Battlefield)
                            })
                            .map(|id| Target::Object(*id)),
                    );
                }
            }
            Recipient::OpponentsWithout(f) => {
                let filter = self.pool.filter(f);
                let source = self.current_object(ctx.source);
                let players: SmallVec<[PlayerId; 4]> = self
                    .players_in_game()
                    .filter(|q| *q != ctx.controller)
                    .collect();
                for q in players {
                    let controls = self.permanents_of(q).any(|id| {
                        self.characteristics(id).is_some_and(|c| {
                            filter.matches(id, q, &c, source, Some(ctx.controller))
                        }) && self.status_ok(filter, id)
                    });
                    if !controls {
                        out.push(Target::Player(q));
                    }
                }
            }
            Recipient::RandomOf(f) => {
                let mut all = SmallVec::new();
                self.group_into(ctx, f, &mut all);
                if !all.is_empty() {
                    let i = rand::Rng::random_range(&mut self.rng, 0..all.len());
                    out.push(all[i]);
                }
            }
            // "Target creature and all other creatures with the same name as
            // that creature": the target, if still legal, and the group
            // judged against it as it is now (CR 608.2b, 608.2h).
            Recipient::TargetAndGroup(i, f) => {
                if !ctx.legal.get(i).copied().unwrap_or(false) {
                    return out;
                }
                let Some(t) = ctx.targets.get(i) else {
                    return out;
                };
                out.push(*t);
                // "target creature card and all other cards with the same
                // name as that card from your graveyard": a card's group is
                // in its graveyard.
                if let Target::Object(id) = *t
                    && let Some(o) = self.objects.get(id)
                    && o.zone == Zone::Graveyard
                {
                    let filter = &*self.chosen_filter(self.pool.filter(f), ctx);
                    let graveyard = &self.player(o.owner).graveyard;
                    out.extend(
                        graveyard
                            .iter()
                            .copied()
                            .filter(|card| *card != id)
                            .filter(|card| {
                                let owner = self.objects[*card].owner;
                                self.characteristics(*card).is_some_and(|ch| {
                                    filter.matches(*card, owner, &ch, None, Some(ctx.controller))
                                })
                            })
                            .map(Target::Object),
                    );
                    return out;
                }
                let mut group = SmallVec::new();
                self.group_into(ctx, f, &mut group);
                out.extend(group.into_iter().filter(|g| g != t));
            }
        }
        out
    }

    /// Attach `obj` to `host` (CR 701.3a). An Aura goes only where its
    /// enchant ability allows, an Equipment only onto a creature, and an
    /// Equipment that is a creature doesn't equip (CR 301.5c); an object
    /// can't be attached to itself. Otherwise, or if it is already attached
    /// to `host`, nothing happens (CR 701.3b, 701.3c).
    pub(crate) fn attach(&mut self, obj: ObjId, host: ObjId) {
        if obj == host || self.objects[obj].attached_to == Some(host) {
            return;
        }
        let Some(c) = self.characteristics(obj) else {
            return;
        };
        let legal = if c.has_subtype(crate::card::Subtype::AURA) {
            let def = self.objects[obj].def;
            let spec = self.pool.get(def).enchant().unwrap_or(TargetSpec::Creature);
            self.is_legal_attachment(spec, host)
        } else if c.has_subtype(crate::card::Subtype::EQUIPMENT) {
            // CR 702.151b: an Equipment with reconfigure attaches, and
            // stops being a creature while attached.
            (!c.is_creature() || self.pool.get(self.objects[obj].def).reconfigure)
                && self.is_legal_attachment(TargetSpec::Creature, host)
        } else {
            false
        };
        if !legal {
            return;
        }
        self.objects[obj].attached_to = Some(host);
        self.cbump();
        self.emit(Event::Attached { obj, to: host });
    }

    /// What a filter's "attached to" names as `ctx`'s effects resolve: the
    /// source, what it's attached to, or a target still legal.
    fn attached_host(&self, ctx: &EffectCtx, a: crate::card::AttachedTo) -> Option<ObjId> {
        use crate::card::AttachedTo;
        match a {
            AttachedTo::Source => self.current_object(ctx.source),
            AttachedTo::SourceHost => self
                .objects
                .get(ctx.source)
                .and_then(|o| o.attached_to)
                .or_else(|| self.lki.get(&ctx.source)?.object.attached_to),
            AttachedTo::Target(n) => {
                let n = usize::from(n);
                match ctx.targets.get(n) {
                    Some(Target::Object(id)) if ctx.legal.get(n).copied().unwrap_or(false) => {
                        Some(*id)
                    }
                    _ => None,
                }
            }
            AttachedTo::TriggeringObject => ctx.trigger_obj.filter(|id| {
                self.objects
                    .get(*id)
                    .is_some_and(|o| o.zone == Zone::Battlefield)
            }),
        }
    }

    /// Every battlefield object in the pool's `f`th filter, as `ctx`'s
    /// effects resolve.
    fn group_into(&self, ctx: &EffectCtx, f: u16, out: &mut SmallVec<[Target; 2]>) {
        let filter = &*self.chosen_filter(self.pool.filter(f), ctx);
        // "all Auras attached to it": nothing if what it names is gone.
        let host = filter.attached_to.map(|a| self.attached_host(ctx, a));
        if host == Some(None) {
            return;
        }
        let Some(that) = self.that_player_in(filter, ctx) else {
            return;
        };
        let source = self.current_object(ctx.source);
        // "Target creature you control gets +3/+3 and each other creature
        // you control gets +1/+1": a spell's "other" is other than its
        // targets.
        let other_than_targets = filter.relation == crate::card::Relation::Other
            && self
                .objects
                .get(ctx.source)
                .is_some_and(|o| o.zone == Zone::Stack);
        out.extend(
            self.battlefield
                .iter()
                .copied()
                .filter(|id| !other_than_targets || !ctx.targets.contains(&Target::Object(*id)))
                .filter(|id| host.is_none_or(|h| self.objects[*id].attached_to == h))
                .filter(|id| that.is_none_or(|p| self.objects[*id].controller == p))
                .filter(|id| {
                    let o = &self.objects[*id];
                    self.characteristics(*id).is_some_and(|c| {
                        filter.matches(*id, o.controller, &c, source, Some(ctx.controller))
                    }) && self.status_ok(filter, *id)
                        && self.combat_with_ok(filter.combat_with, source, *id)
                })
                .map(Target::Object),
        );
    }

    /// The player a group's or count's filter is held to
    /// (`ObjFilter::that_player`, "creatures target player controls"):
    /// `Some(None)` for no such relation, `None` if that player is not to
    /// be had (an illegal target, CR 608.2b), so nothing matches.
    #[allow(clippy::option_option)] // "no relation" differs from "no player"
    pub(super) fn that_player_in(
        &self,
        f: &crate::card::ObjFilter,
        ctx: &EffectCtx,
    ) -> Option<Option<PlayerId>> {
        use crate::card::ThatPlayer;
        let Some(rel) = f.that_player else {
            return Some(None);
        };
        let target = |n: u8| {
            let n = usize::from(n);
            ctx.targets
                .get(n)
                .copied()
                .filter(|_| ctx.legal.get(n).copied().unwrap_or(true))
        };
        match rel {
            ThatPlayer::Triggering => ctx.trigger_player.map(Some),
            ThatPlayer::Target(n) => match target(n)? {
                Target::Player(p) => Some(Some(p)),
                Target::Object(_) => None,
            },
            ThatPlayer::TargetOrController { n, .. } => match target(n)? {
                Target::Player(p) => Some(Some(p)),
                Target::Object(id) => self
                    .objects
                    .get(id)
                    .map(|o| o.controller)
                    .or_else(|| self.lki.get(&id).map(|l| l.object.controller))
                    .map(Some),
            },
            ThatPlayer::ControllerOf(n) => match target(n)? {
                Target::Object(id) => self
                    .objects
                    .get(id)
                    .map(|o| o.controller)
                    .or_else(|| self.lki.get(&id).map(|l| l.object.controller))
                    .map(Some),
                Target::Player(_) => None,
            },
        }
    }

    /// What "the chosen color" or "that type" refers to as `ctx`'s effects
    /// resolve: the choice made earlier in the resolution (CR 608.2d), or
    /// else the one the source made as it entered (CR 607.2a), as it last
    /// existed if it has left.
    /// The definition of the resolving spell or ability's source.
    fn source_def(&self, ctx: &EffectCtx) -> crate::card::CardRef {
        self.objects
            .get(ctx.source)
            .map(|o| o.def)
            .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
            .unwrap_or(crate::card::CardRef(u32::MAX))
    }

    fn resolution_chosen(&self, ctx: &EffectCtx) -> Chosen {
        if ctx.chosen != Chosen::None {
            return ctx.chosen;
        }
        self.objects
            .get(ctx.source)
            .map(|o| o.chosen)
            .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.chosen))
            .unwrap_or_default()
    }

    /// `f` with "of the chosen type" filled in (`ObjFilter::with_chosen`);
    /// other filters as they are.
    fn chosen_filter<'f>(
        &self,
        f: &'f crate::card::ObjFilter,
        ctx: &EffectCtx,
    ) -> std::borrow::Cow<'f, crate::card::ObjFilter> {
        if !f.chosen
            && f.same.is_none()
            && !f.status.mana_value_x
            && f.status.bound.is_none()
            && !f.enchanted_player
        {
            return std::borrow::Cow::Borrowed(f);
        }
        // "each creature enchanted player controls" (CR 303.4).
        let f = &if f.enchanted_player {
            f.with_enchanted_player(self.attached_player_of(ctx.source))
        } else {
            f.clone()
        };
        let mut g = if f.chosen {
            f.with_chosen(self.resolution_chosen(ctx))
        } else {
            f.clone()
        };
        // "each creature with power less than or equal to the number of
        // ...": the count as the effect happens.
        if let Some(b) = g.status.bound {
            let v = self.eval_num(b.n, ctx);
            GameState::fill_bound(&mut g, v);
        }
        // "Destroy each artifact with mana value X or less": X of the
        // resolving spell or ability (CR 107.3); unfilled, the filter
        // wouldn't restrict the mana value at all.
        if g.status.mana_value_x {
            g.status.mana_value_x = false;
            g.mana_value = g
                .mana_value
                .map(|(cmp, _)| (cmp, u8::try_from(ctx.x).unwrap_or(u8::MAX)));
        }
        if let Some(same) = g.same {
            // "named Food Fight" in an ability Food Fight grants names the
            // granting card, not the permanent that has the ability.
            if let (crate::card::SameOf::Source, Some(d)) = (same.of, ctx.counts)
                && matches!(
                    same.what,
                    crate::card::SameWhat::Name | crate::card::SameWhat::NotName
                )
            {
                return std::borrow::Cow::Owned(g.with_same(
                    Some(d),
                    crate::mana::ColorSet::default(),
                    None,
                    crate::card::TypeSet::default(),
                ));
            }
            let of = match same.of {
                crate::card::SameOf::Source => Some(ctx.source),
                crate::card::SameOf::Target(n) => match ctx.targets.get(usize::from(n)) {
                    Some(Target::Object(id)) => Some(*id),
                    _ => None,
                },
            };
            g = self.with_same_of(&g, of);
        }
        std::borrow::Cow::Owned(g)
    }

    /// "Each player loses 2 life unless they discard a card" / "... unless
    /// they sacrifice a creature, artifact, or land of their choice": each
    /// player in turn order, from the controller, chooses (CR 101.4).
    fn punish_each(
        &mut self,
        ctx: &mut EffectCtx,
        life: u32,
        unless: Option<&crate::card::ObjFilter>,
    ) {
        let def = self
            .objects
            .get(ctx.source)
            .or_else(|| self.lki.get(&ctx.source).map(|l| &l.object))
            .map(|o| o.def);
        let n = self.num_players();
        let first = ctx.controller.index();
        let saved = ctx.controller;
        for k in 0..n {
            let p = PlayerId(u8::try_from((first + k) % n).unwrap_or(0));
            if self.player(p).lost.is_some() {
                continue;
            }
            let can = match unless {
                None => !self.player(p).hand.is_empty(),
                Some(f) => self.permanents_of(p).any(|id| {
                    self.characteristics(id)
                        .is_some_and(|c| f.matches_characteristics(&c))
                }),
            };
            // `Mode(0)` avoids the life loss, `Mode(1)` takes it.
            let avoid = can
                && def.is_some_and(|def| {
                    self.choose_mid(
                        p,
                        DecisionKind::ChooseOption { def },
                        vec![Choice::Mode(0), Choice::Mode(1)],
                    ) == 0
                });
            ctx.controller = p;
            let e = match (avoid, unless) {
                (true, None) => Effect::Discard {
                    count: crate::card::Num::N(1),
                    who: Recipient::You,
                },
                (true, Some(f)) => Effect::SacrificeChosen {
                    who: Recipient::You,
                    filter: f.clone(),
                    count: crate::card::Num::N(1),
                },
                (false, _) => Effect::LoseLife {
                    amount: crate::card::Num::N(life),
                    who: Recipient::You,
                },
            };
            self.apply_effect(ctx, &e);
            ctx.controller = saved;
        }
    }

    /// CR 608.2g: "you may cast <a card> without paying its mana cost" as
    /// the effect resolves: the card `what` names, or one from the
    /// controller's hand matching `filter` with mana value at most
    /// `max_mv`, cast as the resolution finishes.
    fn cast_free(
        &mut self,
        ctx: &mut EffectCtx,
        what: Option<Recipient>,
        filter: Option<&crate::card::ObjFilter>,
        max_mv: Option<crate::card::Num>,
        exile: bool,
    ) {
        let p = ctx.controller;
        let alt = if exile {
            AltCast::FreeExile
        } else {
            AltCast::Free
        };
        let cap = max_mv.map(|n| self.eval_num(n, ctx));
        let source = ctx.source;
        let fits = |st: &Self, id: ObjId| -> bool {
            let Some(o) = st.objects.get(id) else {
                return false;
            };
            if !matches!(o.zone, Zone::Hand | Zone::Graveyard | Zone::Exile) {
                return false;
            }
            let def = st.pool.get(o.def);
            let mv = def.mana_cost.mana_value();
            if cap.is_some_and(|c| mv > c) {
                return false;
            }
            if let Some(f) = filter {
                let chars = st.base_characteristics(o);
                // The mana value isn't a characteristic `matches` sees.
                if !(f
                    .mana_value
                    .is_none_or(|(cmp, n)| cmp.holds(mv, u32::from(n)))
                    && f.matches(id, p, &chars, Some(source), Some(p)))
                {
                    return false;
                }
            }
            st.exile_castable(p, id, alt)
        };
        // "You may cast a card exiled with this": one of them, chosen.
        if let Some(r @ Recipient::ExiledWith(_)) = what {
            let castable: Vec<ObjId> = self
                .recipients(ctx, r)
                .into_iter()
                .filter_map(|t| match t {
                    Target::Object(id) if fits(self, id) => Some(id),
                    _ => None,
                })
                .collect();
            if castable.is_empty() {
                return;
            }
            let mut options: Vec<Choice> = castable
                .iter()
                .map(|id| Choice::Target(Target::Object(*id)))
                .collect();
            options.push(Choice::Done);
            let k = self.choose_mid(p, DecisionKind::ChooseCard, options);
            if let Some(&card) = castable.get(k) {
                self.exile_casts
                    .get_or_insert_with(Box::default)
                    .pending
                    .push((p, card, alt));
            }
            return;
        }
        if let Some(r) = what {
            for t in self.recipients(ctx, r) {
                if let Target::Object(id) = t
                    && fits(self, id)
                {
                    self.offer_exile_cast_by(id, alt, p);
                }
            }
            return;
        }
        let castable: Vec<ObjId> = self
            .player(p)
            .hand
            .iter()
            .copied()
            .filter(|id| fits(self, *id))
            .collect();
        if castable.is_empty() {
            return;
        }
        let mut options: Vec<Choice> = castable
            .iter()
            .map(|id| Choice::Discard {
                card: *id,
                def: self.objects[*id].def,
            })
            .collect();
        options.push(Choice::Done);
        let k = self.choose_mid(p, DecisionKind::ChooseCard, options);
        if let Some(&card) = castable.get(k) {
            self.exile_casts
                .get_or_insert_with(Box::default)
                .pending
                .push((p, card, alt));
        }
    }

    /// Mad Wizard's Lair: "Draw three cards and reveal them. You may cast
    /// one of them without paying its mana cost."
    fn draw_cast_one(&mut self, ctx: &mut EffectCtx, count: u32) {
        let p = ctx.controller;
        let before = self.player(p).hand.clone();
        self.apply_effect(
            ctx,
            &Effect::DrawCards {
                count: crate::card::Num::N(count),
                who: Recipient::You,
            },
        );
        let drawn: Vec<ObjId> = self
            .player(p)
            .hand
            .iter()
            .copied()
            .filter(|id| !before.contains(id))
            .collect();
        for &id in &drawn {
            self.reveal_in_hand(id);
        }
        let castable: Vec<ObjId> = drawn
            .into_iter()
            .filter(|id| self.exile_castable(p, *id, AltCast::Free))
            .collect();
        if castable.is_empty() {
            return;
        }
        let mut options: Vec<Choice> = castable
            .iter()
            .map(|id| Choice::Discard {
                card: *id,
                def: self.objects[*id].def,
            })
            .collect();
        options.push(Choice::Done);
        let k = self.choose_mid(p, DecisionKind::ChooseCard, options);
        if let Some(&card) = castable.get(k) {
            self.exile_casts
                .get_or_insert_with(Box::default)
                .pending
                .push((p, card, AltCast::Free));
        }
    }

    /// Search `p`'s library (CR 701.19) for `count` cards matching
    /// `filter` and put them where `to` says, then shuffle. With `may`,
    /// declining before anything is found is not searching at all, and
    /// the library isn't shuffled. With `reveal`, a card put into a hand
    /// is known to every player (CR 701.20) until it moves.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)] // one arm per destination
    fn search_library(
        &mut self,
        ctx: &mut EffectCtx,
        p: PlayerId,
        filter: &crate::card::ObjFilter,
        count: crate::card::Num,
        to: SearchTo,
        may: bool,
        reveal: bool,
        (graveyard, name): (bool, Option<&str>),
    ) {
        let to = &to;
        let filter = &*self.chosen_filter(filter, ctx);
        let mut found = Vec::new();
        let mut declined = false;
        for _ in 0..self.eval_num(count, ctx) {
            // One option per distinct card (copies are alike) in each
            // zone searched.
            let mut seen = Vec::new();
            let mut options = Vec::new();
            let ps = self.player(p);
            let graveyard_cards = if graveyard {
                ps.graveyard.as_slice()
            } else {
                &[]
            };
            for id in ps.library.iter().rev().chain(graveyard_cards.iter().rev()) {
                if found.contains(id) {
                    continue;
                }
                let o = &self.objects[*id];
                let c = self.base_characteristics(o);
                if filter.matches(*id, p, &c, Some(ctx.source), Some(p))
                    && self.status_ok(filter, *id)
                    && name.is_none_or(|n| self.pool.get(o.def).name == n)
                    && !seen.contains(&(o.def, o.zone))
                {
                    seen.push((o.def, o.zone));
                    options.push(Choice::Target(Target::Object(*id)));
                }
            }
            if options.is_empty() {
                break;
            }
            options.push(Choice::Decline);
            let k = self.choose_mid(p, DecisionKind::SearchLibrary, options.clone());
            if let Choice::Target(Target::Object(id)) = options[k] {
                found.push(id);
            } else {
                declined = true;
                break;
            }
        }
        // "may search": declining at once is choosing not to search.
        if may && declined && found.is_empty() {
            return;
        }
        // "If you search your library this way, shuffle." (and/or
        // graveyard): searched if a card was found there, or if none was
        // found at all.
        let from_library = !graveyard
            || found.is_empty()
            || found.iter().any(|id| {
                self.objects
                    .get(*id)
                    .is_some_and(|o| o.zone == Zone::Library)
            });
        let mut top = Vec::new();
        // Cultivate: the searcher chooses the one that goes onto the
        // battlefield tapped; the other goes to the hand (one found
        // goes where they choose).
        let mut to_field = None;
        if *to == SearchTo::BattlefieldAndHand && !found.is_empty() {
            let mut options: Vec<Choice> = found
                .iter()
                .map(|id| Choice::Target(Target::Object(*id)))
                .collect();
            if found.len() == 1 {
                options.push(Choice::Decline);
            }
            let k = self.choose_mid(p, DecisionKind::SearchLibrary, options.clone());
            if let Choice::Target(Target::Object(id)) = options[k] {
                to_field = Some(id);
            }
        }
        for id in found {
            let to = match to {
                SearchTo::BattlefieldAndHand if to_field == Some(id) => {
                    &SearchTo::Battlefield { tapped: true }
                }
                SearchTo::BattlefieldAndHand => &SearchTo::Hand,
                other => other,
            };
            match to {
                SearchTo::Hand | SearchTo::BattlefieldAndHand => {
                    let new = self.move_object(id, Zone::Hand, true);
                    if reveal {
                        self.reveal_in_hand(new);
                    }
                }
                SearchTo::Battlefield { tapped } => {
                    let new = self.put_onto_battlefield_with(
                        id,
                        p,
                        super::replace::Enter {
                            tapped: *tapped,
                            counters: None,
                            ..super::replace::Enter::default()
                        },
                    );
                    ctx.created = Some(new);
                }
                SearchTo::TopOfLibrary => top.push(id),
                SearchTo::Graveyard => {
                    self.move_object(id, Zone::Graveyard, true);
                }
                SearchTo::HandOrGraveyard => {
                    let def = self
                        .objects
                        .get(ctx.source)
                        .map(|o| o.def)
                        .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
                        .unwrap_or_default();
                    let k = self.choose_mid(
                        p,
                        DecisionKind::ChooseOption { def },
                        vec![Choice::Mode(0), Choice::Mode(1)],
                    );
                    let zone = if k == 0 { Zone::Hand } else { Zone::Graveyard };
                    let new = self.move_object(id, zone, true);
                    if reveal && zone == Zone::Hand {
                        self.reveal_in_hand(new);
                    }
                }
            }
        }
        if from_library {
            self.shuffle_library(p);
        }
        for id in top {
            let lib = &mut self.player_mut(p).library;
            lib.retain(|x| *x != id);
            lib.push(id);
        }
    }

    /// Throne of the Dead Three: "Reveal the top ten cards of your
    /// library. Put a creature card from among them onto the battlefield
    /// with three +1/+1 counters on it. ... Then shuffle."
    fn reveal_put_creature(&mut self, ctx: &mut EffectCtx, count: u32, counters: u32) {
        let p = ctx.controller;
        let top: Vec<ObjId> = self
            .player(p)
            .library
            .iter()
            .rev()
            .take(count as usize)
            .copied()
            .collect();
        for &id in &top {
            let def = self.objects[id].def;
            self.emit(Event::Revealed { obj: id, def });
        }
        let creatures: Vec<ObjId> = top
            .into_iter()
            .filter(|id| self.pool.get(self.objects[*id].def).is_creature())
            .collect();
        let pick = match creatures.len() {
            0 => None,
            1 => Some(creatures[0]),
            _ => {
                let options = creatures
                    .iter()
                    .map(|id| Choice::Discard {
                        card: *id,
                        def: self.objects[*id].def,
                    })
                    .collect();
                Some(creatures[self.choose_mid(p, DecisionKind::ChooseCard, options)])
            }
        };
        if let Some(id) = pick {
            let new = self.put_onto_battlefield_with(
                id,
                p,
                super::replace::Enter {
                    tapped: false,
                    counters: Some((CounterKind::PlusOnePlusOne, counters)),
                    ..super::replace::Enter::default()
                },
            );
            if self
                .objects
                .get(new)
                .is_some_and(|o| o.zone == Zone::Battlefield)
            {
                ctx.created = Some(new);
            }
        }
        self.shuffle_library(p);
    }

    /// CR 611.2b: inside an `Effect::Lasting`, an effect that would last
    /// until end of turn lasts its duration instead; `id` is the object it
    /// affects (the source's, for an effect on no object).
    #[inline]
    fn lasting_or(&mut self, duration: Duration, ctx: &EffectCtx, id: ObjId) -> Duration {
        let Some(l) = self.lasting else {
            return duration;
        };
        if duration != Duration::UntilEndOfTurn {
            return duration;
        }
        let d = match l {
            crate::card::Lasting::YourNextTurn => Duration::UntilYourNextTurn(ctx.controller),
            crate::card::Lasting::SourceOnBattlefield => Duration::WhileOnBattlefield(ctx.source),
            crate::card::Lasting::WhileTapped => Duration::WhileTapped(id),
            crate::card::Lasting::YouControlSource => {
                Duration::WhileControlled(ctx.source, ctx.controller)
            }
        };
        if d.linked() {
            self.linked_durations += 1;
        }
        d
    }

    /// CR 611.2b: the effects of `Effect::Lasting`. A duration linked to
    /// the source does nothing if the source has already left the
    /// battlefield (or its controller no longer controls it).
    fn apply_lasting(
        &mut self,
        ctx: &mut EffectCtx,
        lasting: crate::card::Lasting,
        effects: &[Effect],
    ) {
        let source_ok = || {
            self.objects
                .get(ctx.source)
                .is_some_and(|o| o.zone == Zone::Battlefield)
        };
        match lasting {
            crate::card::Lasting::SourceOnBattlefield if !source_ok() => return,
            crate::card::Lasting::YouControlSource
                if !source_ok() || self.objects[ctx.source].controller != ctx.controller =>
            {
                return;
            }
            _ => {}
        }
        let outer = self.lasting.replace(lasting);
        for e in effects {
            self.apply_effect(ctx, e);
        }
        self.lasting = outer;
    }

    /// Store an "until end of turn" effect on one object (CR 611.2a, 611.2c).
    fn effect_until_end_of_turn(
        &mut self,
        ctx: &EffectCtx,
        id: ObjId,
        parts: SmallVec<[Modification; 2]>,
    ) {
        self.effect_on(ctx, id, parts, Duration::UntilEndOfTurn);
    }

    /// CR 603.10a: several permanents leaving the battlefield at once
    /// ("Destroy all creatures") are one event, and leaves-the-battlefield
    /// triggers look back at the battlefield as it was before it, as for
    /// state-based actions. Whether this call started the look-back, for
    /// `end_together`.
    fn begin_together(&mut self, n: usize) -> bool {
        if n > 1 && self.lookback.is_none() {
            self.lookback = Some(self.snapshot_battlefield());
            true
        } else {
            false
        }
    }

    fn end_together(&mut self, started: bool) {
        if started {
            self.lookback = None;
        }
    }

    /// Player `p` gains `keywords` until end of turn, or until the
    /// controller's next turn inside "until your next turn" (CR 611.2b).
    fn grant_player(&mut self, ctx: &EffectCtx, p: PlayerId, keywords: KeywordSet) {
        let duration = match self.lasting {
            Some(crate::card::Lasting::YourNextTurn) => Duration::UntilYourNextTurn(ctx.controller),
            _ => Duration::UntilEndOfTurn,
        };
        self.player_grants.push(crate::state::PlayerGrant {
            player: p,
            keywords,
            duration,
        });
        self.bump();
    }

    /// Store an effect on one object for `duration` (CR 611.2a, 611.2c):
    /// until end of turn, or indefinitely.
    fn effect_on(
        &mut self,
        ctx: &EffectCtx,
        id: ObjId,
        parts: SmallVec<[Modification; 2]>,
        duration: Duration,
    ) {
        let duration = self.lasting_or(duration, ctx, id);
        // An ability gained this way ("gains undying until end of turn",
        // a granted triggered ability) can trigger: trigger detection
        // looks past definitions while grants exist (`grants_exist`).
        if parts.iter().any(|m| match m {
            Modification::AddAbility(_) => true,
            Modification::AddKeywords(ks) => ks.intersects(KeywordSet::TRIGGERED),
            _ => false,
        }) {
            self.eot_grants += 1;
            if duration != Duration::UntilEndOfTurn {
                self.lasting_grants += 1;
            }
        }
        let timestamp = self.tick();
        self.effects.push(ContinuousEffect {
            source: Some(ctx.source),
            controller: ctx.controller,
            timestamp,
            duration,
            affected: Affected::Objects(smallvec::smallvec![id]),
            parts,
        });
        self.cbump();
    }

    #[allow(clippy::too_many_lines)] // one arm per effect kind
    pub(super) fn apply_effect(&mut self, ctx: &mut EffectCtx, effect: &Effect) {
        // CR 603.2c: each instruction is its own event for "one or more"
        // triggers ("discard a card, then ... discards a card": two).
        self.batch_from = self.next_timestamp;
        let outer = self.effect_controller.replace(ctx.controller);
        self.apply_effect_now(ctx, effect);
        self.effect_controller = outer;
    }

    #[allow(clippy::too_many_lines)]
    fn apply_effect_now(&mut self, ctx: &mut EffectCtx, effect: &Effect) {
        let on_battlefield = |st: &GameState, t: Target| match t {
            Target::Object(id) => st
                .objects
                .get(id)
                .is_some_and(|o| o.zone == Zone::Battlefield)
                .then_some(id),
            Target::Player(_) => None,
        };
        match effect {
            Effect::DealDamage { amount, to } => {
                let excess_away = self
                    .this_way
                    .as_ref()
                    .is_some_and(|l| l.excess_to_controller);
                for t in self.recipients(ctx, *to) {
                    let n = self.eval_num(*amount, ctx);
                    // CR 120.4a: "Excess damage is dealt to that
                    // creature's controller instead" (a spell's damage,
                    // never deathtouch).
                    if excess_away
                        && let Target::Object(id) = t
                        && let Some(c) = self.characteristics(id)
                        && c.is_creature()
                        && self
                            .objects
                            .get(id)
                            .is_some_and(|o| o.zone == Zone::Battlefield)
                    {
                        let lethal = self.lethal_left(id, &c, false);
                        let ctl = self.objects[id].controller;
                        self.deal_damage(ctx.source, t, n.min(lethal), false);
                        if n > lethal {
                            self.deal_damage(ctx.source, Target::Player(ctl), n - lethal, false);
                        }
                        continue;
                    }
                    self.deal_damage(ctx.source, t, n, false);
                }
            }
            Effect::Pump {
                power,
                toughness,
                to,
            } => {
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    self.effect_until_end_of_turn(
                        ctx,
                        id,
                        smallvec::smallvec![Modification::ModifyPt {
                            power: *power,
                            toughness: *toughness,
                        }],
                    );
                }
            }
            Effect::Destroy { to } => {
                let ts = self.recipients(ctx, *to);
                let together = self.begin_together(ts.len());
                for t in ts {
                    if let Some(id) = on_battlefield(self, t) {
                        self.destroy(id);
                    }
                }
                self.end_together(together);
            }
            Effect::GainLife { amount, who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.gain_life(p, self.eval_num(*amount, ctx));
                    }
                }
            }
            Effect::LoseLife { amount, who } => {
                let mut lost = 0u32;
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        let n = self.eval_num(*amount, ctx);
                        lost = lost.saturating_add(n);
                        self.lose_life(p, n);
                    }
                }
                // "Target opponent loses 1 life for each attacking creature
                // you control. You gain that much life."
                ctx.trigger_amount = lost;
            }
            // CR 104.3e: "You lose the game."
            Effect::LoseGame { who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t
                        && self.player(p).lost.is_none()
                        && !self.cant_lose(p)
                    {
                        let reason = crate::state::LossReason::Effect;
                        self.player_mut(p).lost = Some(reason);
                        self.emit(Event::PlayerLost { player: p, reason });
                        self.cbump();
                    }
                }
            }
            // CR 107.1a: rounded up.
            Effect::LoseHalfLife { who } => {
                let mut lost = 0u32;
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        let life = self.player(p).life.max(0);
                        let n = life - life / 2;
                        lost = lost.saturating_add(n.unsigned_abs());
                        self.lose_life(p, n.unsigned_abs());
                    }
                }
                ctx.trigger_amount = lost;
            }
            Effect::DrawCards { count, who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        for _ in 0..self.eval_num(*count, ctx) {
                            self.draw(p);
                        }
                    }
                }
            }
            Effect::AddCounters { kind, count, to } => {
                // CR 608.2h: the number is determined once, before any
                // counter is put ("put X +1/+1 counters on each creature
                // you control, where X is this creature's power").
                let n = self.eval_num(*count, ctx);
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.add_counters(id, *kind, n);
                    }
                }
            }
            Effect::AddCountersCapped {
                kind,
                count,
                to,
                cap,
            } => {
                let n = self.eval_num(*count, ctx);
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        let room = cap.saturating_sub(self.objects[id].counters(*kind));
                        if n.min(room) > 0 {
                            self.add_counters(id, *kind, n.min(room));
                        }
                    }
                }
            }
            // CR 601.2d, 608.2b: the division announced on casting holds;
            // what an illegal target was assigned is not dealt.
            Effect::DealDamageDivided { to, .. } => {
                for (t, n) in Self::divided(ctx, *to) {
                    self.deal_damage(ctx.source, t, n, false);
                }
            }
            Effect::DistributeCounters { kind, to, .. } => {
                for (t, n) in Self::divided(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.add_counters(id, *kind, n);
                    }
                }
            }
            Effect::ReturnToHand { what } => {
                if *what == crate::card::Recipient::Source && self.source_left_graveyard(ctx) {
                    return;
                }
                let ts = self.recipients(ctx, *what);
                let together = self.begin_together(ts.len());
                for t in ts {
                    let Target::Object(id) = t else {
                        continue;
                    };
                    // "return Psychic Miasma to its owner's hand": the
                    // resolving spell itself, from the stack (it then
                    // doesn't go to the graveyard, CR 608.2n).
                    // "return the exiled card to its owner's hand" (CR
                    // 607.2a): a linked card, from exile.
                    let in_zone = self.objects.get(id).is_some_and(|o| {
                        matches!(o.zone, Zone::Battlefield | Zone::Graveyard)
                            || (o.zone == Zone::Exile
                                && *what == crate::card::Recipient::ExiledCard)
                            || (o.zone == Zone::Stack
                                && (id == ctx.source
                                    || matches!(what, crate::card::Recipient::Target(_)))
                                && o.kind != ObjKind::Ability)
                    });
                    if in_zone {
                        self.move_object(id, Zone::Hand, false);
                    }
                }
                self.end_together(together);
            }
            Effect::RemoveCounters { kind, count, to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.remove_counters(id, *kind, self.eval_num(*count, ctx));
                    }
                }
            }
            Effect::RemoveCounterChosen { to, up_to } => {
                let def = self
                    .objects
                    .get(ctx.source)
                    .or_else(|| self.lki.get(&ctx.source).map(|l| &l.object))
                    .map(|o| o.def);
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    // "up to three": a counter at a time, the controller
                    // choosing its kind or to stop (`Choice::Done`, last).
                    for _ in 0..(*up_to).max(1) {
                        let kinds: SmallVec<[CounterKind; 4]> = self.objects[id]
                            .counters
                            .iter()
                            .filter(|(_, n)| *n > 0)
                            .map(|(k, _)| *k)
                            .collect();
                        let n = kinds.len();
                        if n == 0 {
                            break;
                        }
                        let i = match def {
                            Some(def) if n > 1 || *up_to > 0 => {
                                let mut options: Vec<Choice> = (0..n).map(Choice::Mode).collect();
                                if *up_to > 0 {
                                    options.push(Choice::Done);
                                }
                                self.choose_mid(
                                    ctx.controller,
                                    DecisionKind::ChooseOption { def },
                                    options,
                                )
                            }
                            _ => 0,
                        };
                        if i >= n {
                            break;
                        }
                        self.remove_counters(id, kinds[i], 1);
                    }
                }
            }
            Effect::RemoveAllCounters { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.remove_all_counters(id);
                    }
                }
            }
            Effect::Regenerate { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.regenerate(ctx, id);
                    }
                }
            }
            Effect::DestroyNoRegen { to } => {
                let ts = self.recipients(ctx, *to);
                let together = self.begin_together(ts.len());
                for t in ts {
                    if let Some(id) = on_battlefield(self, t) {
                        self.destroy_with(id, false);
                    }
                }
                self.end_together(together);
            }
            Effect::Mill { count, who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.mill(p, self.eval_num(*count, ctx));
                    }
                }
            }
            Effect::ExileTop { who, count } => {
                let n = self.eval_num(*count, ctx);
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        for _ in 0..n {
                            let Some(top) = self.player(p).library.last().copied() else {
                                break;
                            };
                            let new = self.move_object(top, Zone::Exile, true);
                            ctx.created = Some(new);
                        }
                    }
                }
            }
            Effect::CreateEmblem { emblem } => {
                self.create_emblem(*emblem, ctx.controller);
            }
            Effect::Counter { to, dest } => {
                for t in self.recipients(ctx, *to) {
                    if let Target::Object(id) = t {
                        self.counter_spell(id, *dest);
                    }
                }
            }
            Effect::CounterUnlessPays { to, mana, x, dest } => {
                // "{1} for each card in your hand": counted as it resolves.
                let x = self.eval_num(*x, ctx);
                for t in self.recipients(ctx, *to) {
                    let Target::Object(id) = t else {
                        continue;
                    };
                    let Some(o) = self.objects.get(id) else {
                        continue;
                    };
                    if o.zone != Zone::Stack {
                        continue;
                    }
                    let (ctl, def) = (o.controller, o.def);
                    let req = mana.requirements(x);
                    let can_pay = self.can_pay(ctl, &req);
                    let pays = can_pay
                        && self.choose_mid(
                            ctl,
                            DecisionKind::PayOrCounter { def },
                            vec![Choice::Accept, Choice::Decline],
                        ) == 0;
                    // "If they do, ..." sees the payment.
                    ctx.did = pays;
                    if pays {
                        self.pay(ctl, &req);
                    } else {
                        self.counter_spell(id, *dest);
                    }
                }
            }
            Effect::Scry { count, who } => {
                let n = self.eval_num(*count, ctx);
                if *who == Recipient::You {
                    self.scry_or_surveil(ctx.controller, n, false);
                } else {
                    // "Each player may scry 1": in turn order (CR 101.4),
                    // each their own library.
                    for t in self.recipients(ctx, *who) {
                        if let Target::Player(p) = t {
                            self.scry_or_surveil(p, n, false);
                        }
                    }
                }
            }
            Effect::Search {
                filter,
                count,
                to,
                who,
                may,
                reveal,
                graveyard,
                name,
            } => {
                // "Its controller may search their library": each searcher
                // searches their own library.
                let searchers: SmallVec<[PlayerId; 2]> = if *who == Recipient::You {
                    smallvec::smallvec![ctx.controller]
                } else {
                    self.recipients(ctx, *who)
                        .into_iter()
                        .filter_map(|t| match t {
                            Target::Player(p) => Some(p),
                            Target::Object(_) => None,
                        })
                        .collect()
                };
                for p in searchers {
                    self.search_library(
                        ctx,
                        p,
                        filter,
                        *count,
                        *to,
                        *may,
                        *reveal,
                        (*graveyard, name.as_deref()),
                    );
                }
            }
            Effect::Meld { partner, into } => {
                self.meld(ctx.source, ctx.controller, partner, into);
            }
            Effect::Flip { what } => {
                for t in self.recipients(ctx, *what) {
                    if let Target::Object(id) = t {
                        self.flip_now(id);
                    }
                }
            }
            Effect::Transform { what } => {
                for t in self.recipients(ctx, *what) {
                    let Target::Object(id) = t else {
                        continue;
                    };
                    // CR 702.145b, e: a daybound or nightbound permanent
                    // transforms only by those abilities.
                    if self.objects.get(id).is_some_and(|o| {
                        let m = self.pool.get(o.def).marks;
                        m.has(crate::card::Marks::DAYBOUND) || m.has(crate::card::Marks::NIGHTBOUND)
                    }) {
                        continue;
                    }
                    self.transform_now(id);
                }
            }
            Effect::AddMana(mana) => {
                let p = ctx.controller;
                for m in mana {
                    let m = match *m {
                        Mana::Any => {
                            let options: Vec<Choice> =
                                Color::ALL.into_iter().map(Choice::Color).collect();
                            let k = self.choose_mid(p, DecisionKind::ChooseColor, options);
                            Mana::Colored(Color::ALL[k.min(4)])
                        }
                        // Ruling: no chosen color, no mana.
                        Mana::Chosen => match self.chosen_color_of(ctx.source) {
                            Some(c) => Mana::Colored(c),
                            None => continue,
                        },
                        m => m,
                    };
                    self.player_mut(p).mana_pool.add(m);
                }
                self.bump();
            }
            Effect::AddManaOneOf(mana) => {
                let p = ctx.controller;
                let chosen = self.chosen_color_of(ctx.source);
                let options: Vec<Choice> = mana
                    .iter()
                    .filter_map(|m| match m {
                        Mana::Colored(c) => Some(Choice::Color(*c)),
                        Mana::Chosen => chosen.map(Choice::Color),
                        _ => None,
                    })
                    .collect();
                let m = if options.len() == mana.len() && !options.is_empty() {
                    let k = self.choose_mid(p, DecisionKind::ChooseColor, options.clone());
                    match options[k.min(options.len() - 1)] {
                        Choice::Color(c) => Mana::Colored(c),
                        _ => unreachable!("color options"),
                    }
                } else {
                    mana.first().copied().unwrap_or(Mana::Colorless)
                };
                self.player_mut(p).mana_pool.add(m);
                self.bump();
            }
            Effect::AddManaTimes { mana, count } => {
                let p = ctx.controller;
                let n = self.eval_num(*count, ctx);
                let chosen = self.chosen_color_of(ctx.source);
                if n > 0 && (*mana != Mana::Chosen || chosen.is_some()) {
                    let m = match *mana {
                        Mana::Any => {
                            let options: Vec<Choice> =
                                Color::ALL.into_iter().map(Choice::Color).collect();
                            let k = self.choose_mid(p, DecisionKind::ChooseColor, options);
                            Mana::Colored(Color::ALL[k.min(4)])
                        }
                        Mana::Chosen => Mana::Colored(chosen.unwrap_or(Color::White)),
                        m => m,
                    };
                    for _ in 0..n.min(1000) {
                        self.player_mut(p).mana_pool.add(m);
                    }
                    self.bump();
                }
            }
            Effect::AddManaLandCould { whose, basic } => {
                // CR 106.7: the colors those lands could produce now.
                let p = ctx.controller;
                let k = self.lands_could_produce(p, *whose, *basic);
                let options: Vec<Choice> = Color::ALL
                    .into_iter()
                    .filter(|c| k & (1 << (*c as u8)) != 0)
                    .map(Choice::Color)
                    .collect();
                let c = match options.len() {
                    0 => None,
                    1 => Some(options[0].clone()),
                    n => {
                        let i = self.choose_mid(p, DecisionKind::ChooseColor, options.clone());
                        Some(options[i.min(n - 1)].clone())
                    }
                };
                if let Some(Choice::Color(c)) = c {
                    self.player_mut(p).mana_pool.add(Mana::Colored(c));
                    self.bump();
                }
            }
            Effect::AddManaCombination {
                count,
                colors,
                different,
            } => {
                // CR 106.1b: each mana's color is chosen as it is added.
                let p = ctx.controller;
                let mut left = *colors;
                let n = self.eval_num(*count, ctx).min(1000);
                for _ in 0..n {
                    let options: Vec<Choice> = left.iter().map(Choice::Color).collect();
                    let Some(&Choice::Color(c)) = (match options.len() {
                        0 => None,
                        1 => options.first(),
                        n => {
                            let k = self.choose_mid(p, DecisionKind::ChooseColor, options.clone());
                            options.get(k.min(n - 1))
                        }
                    }) else {
                        break;
                    };
                    if *different {
                        left = ColorSet::of(left.iter().filter(|x| *x != c));
                    }
                    self.player_mut(p).mana_pool.add(Mana::Colored(c));
                }
                self.bump();
            }
            Effect::Surveil { count } => {
                self.scry_or_surveil(ctx.controller, self.eval_num(*count, ctx), true);
            }
            Effect::Discard { count, who } => {
                ctx.couldnt = 0;
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    if self.player(p).hand.is_empty() {
                        ctx.did = false;
                    }
                    let n = self.eval_num(*count, ctx);
                    if (self.player(p).hand.len() as u64) < u64::from(n) {
                        ctx.couldnt += 1;
                    }
                    self.discard_n(p, n);
                }
            }
            Effect::HandToLibrary {
                count,
                who,
                bottom,
                shuffle,
            } => {
                let place = if *bottom {
                    crate::card::LibraryPlace::Bottom
                } else {
                    crate::card::LibraryPlace::Top
                };
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    // "If you do" (Fire Prophecy): a card was put back.
                    if self.player(p).hand.is_empty() {
                        ctx.did = false;
                    }
                    for _ in 0..self.eval_num(*count, ctx) {
                        let options: Vec<Choice> = self
                            .player(p)
                            .hand
                            .iter()
                            .map(|id| Choice::Discard {
                                card: *id,
                                def: self.objects[*id].def,
                            })
                            .collect();
                        if options.is_empty() {
                            break;
                        }
                        let k = self.choose_mid(p, DecisionKind::ChooseCard, options.clone());
                        if let Choice::Discard { card, .. } = options[k] {
                            self.put_in_library(card, place);
                        }
                    }
                    if *shuffle {
                        self.shuffle_library(p);
                    }
                }
            }
            Effect::ShuffleIntoLibrary {
                who,
                hand,
                both,
                filter,
            } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    let ps = self.player(p);
                    let (zone, also): (&[ObjId], &[ObjId]) = if *both {
                        (&ps.hand, &ps.graveyard)
                    } else if *hand {
                        (&ps.hand, &[])
                    } else {
                        (&ps.graveyard, &[])
                    };
                    let cards: Vec<ObjId> = zone
                        .iter()
                        .chain(also)
                        .copied()
                        .filter(|id| {
                            filter.as_ref().is_none_or(|f| {
                                let o = &self.objects[*id];
                                let c = self.base_characteristics(o);
                                f.matches(*id, p, &c, Some(ctx.source), Some(ctx.controller))
                                    && self.status_ok(f, *id)
                            })
                        })
                        .collect();
                    for c in cards {
                        self.move_object(c, Zone::Library, true);
                    }
                    // CR 701.24: shuffled even if nothing moved.
                    self.shuffle_library(p);
                }
            }
            Effect::Fight { a, b } => {
                let x = self.recipients(ctx, *a).first().copied();
                let y = self.recipients(ctx, *b).first().copied();
                if let (Some(x), Some(y)) = (
                    x.and_then(|t| on_battlefield(self, t)),
                    y.and_then(|t| on_battlefield(self, t)),
                ) && x != y
                {
                    let px = self.characteristics(x).map_or(0, |c| c.power.max(0));
                    let py = self.characteristics(y).map_or(0, |c| c.power.max(0));
                    self.deal_damage(x, Target::Object(y), u32::try_from(px).unwrap_or(0), false);
                    self.deal_damage(y, Target::Object(x), u32::try_from(py).unwrap_or(0), false);
                }
            }
            // "Each creature deals damage to itself equal to its power"
            // (Solar Blaze): each one its own recipient.
            Effect::Bite { source, to, twice } if source == to => {
                let sources: SmallVec<[ObjId; 2]> = self
                    .recipients(ctx, *source)
                    .into_iter()
                    .filter_map(|t| on_battlefield(self, t))
                    .collect();
                for x in sources {
                    let p = self.characteristics(x).map_or(0, |c| c.power.max(0));
                    let p = u32::try_from(p).unwrap_or(0);
                    let p = if *twice { p.saturating_mul(2) } else { p };
                    self.deal_damage(x, Target::Object(x), p, false);
                }
            }
            Effect::Bite { source, to, twice } => {
                // CR 608.2b: an illegal target is unaffected. If the
                // creature is no longer a legal target it deals no damage;
                // if the recipient is illegal there is nothing to damage.
                let to_all = self.recipients(ctx, *to);
                if to_all.is_empty() {
                    return;
                }
                // "Target creature you control deals damage equal to its
                // power to each other creature": "other" is other than the
                // creature dealing the damage.
                let other = matches!(to, Recipient::Group(n)
                    if self.pool.filter(*n).relation == crate::card::Relation::Other);
                let sources: SmallVec<[ObjId; 2]> = self
                    .recipients(ctx, *source)
                    .into_iter()
                    .filter_map(|t| on_battlefield(self, t))
                    .collect();
                for x in sources {
                    let p = self.characteristics(x).map_or(0, |c| c.power.max(0));
                    let p = u32::try_from(p).unwrap_or(0);
                    let p = if *twice { p.saturating_mul(2) } else { p };
                    for &y in &to_all {
                        if other && y == Target::Object(x) {
                            continue;
                        }
                        self.deal_damage(x, y, p, false);
                    }
                }
            }
            Effect::DamageFrom { source, amount, to } => {
                // CR 608.2h: a source that has left the battlefield still
                // deals the damage, as it last existed there ("When this
                // creature dies, it deals 1 damage to target player").
                let left = match source {
                    Recipient::TriggeringObject => ctx.trigger_obj,
                    _ => None,
                };
                let Some(x) = self
                    .recipients(ctx, *source)
                    .first()
                    .copied()
                    .and_then(|t| on_battlefield(self, t))
                    .or_else(|| {
                        left.filter(|o| {
                            self.lki.contains_key(o)
                                && self
                                    .objects
                                    .get(*o)
                                    .is_none_or(|x| x.zone != Zone::Battlefield)
                        })
                    })
                else {
                    return;
                };
                let n = self.eval_num(*amount, ctx);
                for y in self.recipients(ctx, *to) {
                    self.deal_damage(x, y, n, false);
                }
            }
            Effect::ExileGraveyard { who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        let cards = self.player(p).graveyard.clone();
                        for id in cards {
                            self.move_object(id, Zone::Exile, true);
                        }
                    }
                }
            }
            Effect::ExileAllGraveyards => {
                let cards: Vec<ObjId> = self
                    .players
                    .iter()
                    .flat_map(|p| p.graveyard.iter().copied())
                    .collect();
                for id in cards {
                    self.move_object(id, Zone::Exile, true);
                }
            }
            Effect::CreateReplacement(r) => self.create_replacement(ctx, r),
            Effect::EachPutsFromGraveyard { who, filter } => {
                let mut chosen: SmallVec<[(ObjId, PlayerId); 2]> = SmallVec::new();
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    let options: Vec<Choice> = self
                        .player(p)
                        .graveyard
                        .iter()
                        .copied()
                        .filter(|id| {
                            let c = self.base_characteristics(&self.objects[*id]);
                            filter.matches(*id, p, &c, Some(ctx.source), Some(p))
                                && self.status_ok(filter, *id)
                        })
                        .map(|id| Choice::Discard {
                            card: id,
                            def: self.objects[id].def,
                        })
                        .collect();
                    if options.is_empty() {
                        continue;
                    }
                    let k = self.choose_mid(p, DecisionKind::ChooseCard, options.clone());
                    if let Choice::Discard { card, .. } = options[k] {
                        chosen.push((card, p));
                    }
                }
                for (id, p) in chosen {
                    if self
                        .objects
                        .get(id)
                        .is_some_and(|o| o.zone == Zone::Graveyard)
                    {
                        self.put_onto_battlefield(id, p);
                    }
                }
            }
            Effect::ForEachCouldnt(effects) => {
                for _ in 0..ctx.couldnt {
                    for e in effects {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::PlayerRule { who, kind } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.player_rules.push(crate::state::PlayerRuleEffect {
                            player: p,
                            kind: *kind,
                        });
                    }
                }
                self.bump();
            }
            Effect::Rule(kind) => {
                let duration = self.lasting_or(Duration::UntilEndOfTurn, ctx, ctx.source);
                self.rules.push(RuleEffect {
                    kind: *kind,
                    duration,
                });
                self.bump();
            }
            Effect::Tap { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.tap_by(id, ctx.controller);
                    }
                }
            }
            Effect::ExtraLandPlays(n) => {
                // CR 305.2: additional land plays this turn.
                let p = ctx.controller;
                self.player_mut(p).extra_land_plays += *n;
            }
            Effect::NextSpellLess(n) => {
                let i = ctx.controller.index().min(3);
                self.next_spell_less[i] =
                    self.next_spell_less[i].saturating_add(u16::try_from(*n).unwrap_or(u16::MAX));
            }
            Effect::TapOrUntap { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        // Accept taps, Decline untaps; asked as the source's
                        // choice.
                        let def = self
                            .objects
                            .get(ctx.source)
                            .map(|o| o.def)
                            .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
                            .unwrap_or(self.objects[id].def);
                        let k = self.choose_mid(
                            ctx.controller,
                            DecisionKind::YouMay { def },
                            vec![Choice::Accept, Choice::Decline],
                        );
                        if k == 0 {
                            self.tap_by(id, ctx.controller);
                        } else {
                            self.untap(id);
                        }
                    }
                }
            }
            Effect::Untap { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.untap(id);
                    }
                }
            }
            Effect::Grant { keywords, to } => {
                for t in self.recipients(ctx, *to) {
                    // "You gain shroud until end of turn" (CR 702.18c,
                    // 702.11c, 702.16).
                    if let Target::Player(p) = t {
                        self.grant_player(ctx, p, *keywords);
                        continue;
                    }
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::AddKeywords(*keywords)],
                        );
                    }
                }
            }
            Effect::GainControl { of, forever } => {
                let duration = if *forever {
                    Duration::Permanent
                } else {
                    Duration::UntilEndOfTurn
                };
                for t in self.recipients(ctx, *of) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_on(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::Control],
                            duration,
                        );
                        self.control_effects += 1;
                    }
                }
                // Later effects of the same spell ("untap it") see the new
                // controller.
                self.sync_control();
            }
            Effect::PumpVar {
                power,
                toughness,
                times,
                to,
            } => {
                let n = i32::try_from(self.eval_num(*times, ctx)).unwrap_or(i32::MAX);
                let (p, t) = (power.saturating_mul(n), toughness.saturating_mul(n));
                for r in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, r) {
                        self.effect_until_end_of_turn(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::ModifyPt {
                                power: p,
                                toughness: t
                            }],
                        );
                    }
                }
            }
            Effect::GrantAbility { ability, to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::AddAbility(*ability)],
                        );
                    }
                }
            }
            Effect::BlocksIfAble { blocker, attacker } => {
                let attackers: SmallVec<[ObjId; 2]> = self
                    .recipients(ctx, *attacker)
                    .into_iter()
                    .filter_map(|t| on_battlefield(self, t))
                    .collect();
                for t in self.recipients(ctx, *blocker) {
                    if let Some(b) = on_battlefield(self, t) {
                        for a in &attackers {
                            self.block_demands.push((b, *a));
                        }
                    }
                }
            }
            Effect::CantBeRegenerated { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.no_regen.push(id);
                    }
                }
            }
            Effect::DamageUnpreventable => {
                if let Some(log) = &mut self.this_way {
                    log.unpreventable = true;
                }
            }
            Effect::ExcessToController => {
                if let Some(log) = &mut self.this_way {
                    log.excess_to_controller = true;
                }
            }
            Effect::AssignsNoCombatDamage { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.no_combat_damage.push(id);
                    }
                }
            }
            Effect::CantBlockThis { blocker, attacker } => {
                let attackers: SmallVec<[ObjId; 2]> = self
                    .recipients(ctx, *attacker)
                    .into_iter()
                    .filter_map(|t| on_battlefield(self, t))
                    .collect();
                for t in self.recipients(ctx, *blocker) {
                    if let Some(b) = on_battlefield(self, t) {
                        for a in &attackers {
                            self.block_bans.push((b, *a));
                        }
                    }
                }
            }
            Effect::SkipNext { who, what } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    let ps = self.player_mut(p);
                    match what {
                        crate::card::SkipWhat::DrawStep => ps.skip_draw += 1,
                        crate::card::SkipWhat::CombatPhase => ps.skip_combat += 1,
                        crate::card::SkipWhat::CombatPhaseThisTurn => {
                            ps.skip_combat_this_turn += 1;
                        }
                    }
                }
            }
            // CR 500.7: the most recently created extra turn comes first,
            // so each goes on top of the stack of turns to come.
            Effect::ExtraTurns { who, n } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        for _ in 0..*n {
                            self.extra_turns.push(p);
                        }
                    }
                }
            }
            // CR 500.8: added directly after the current phase; phases
            // added later come before those added earlier.
            Effect::AdditionalPhases { combats, main } => {
                let mut added: SmallVec<[crate::turn::Step; 4]> = SmallVec::new();
                for _ in 0..*combats {
                    added.push(crate::turn::Step::BeginningOfCombat);
                }
                if *main {
                    added.push(crate::turn::Step::PostcombatMain);
                }
                self.extra_phases.splice(0..0, added);
            }
            Effect::SkipNextUntap { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.objects[id].skip_untap = true;
                    }
                }
            }
            Effect::CreatureTypes { to, all } => {
                let part = if *all {
                    Modification::AllCreatureTypes
                } else {
                    Modification::NoCreatureTypes
                };
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(ctx, id, smallvec::smallvec![part.clone()]);
                    }
                }
            }
            Effect::GrantFlashback { to, harmonize } => {
                for t in self.recipients(ctx, *to) {
                    if let Target::Object(id) = t
                        && self
                            .objects
                            .get(id)
                            .is_some_and(|o| o.zone == Zone::Graveyard)
                    {
                        if *harmonize {
                            self.granted_harmonize.push(id);
                        } else {
                            self.granted_flashback.push(id);
                        }
                    }
                }
            }
            Effect::SwitchPt { to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::SwitchPt],
                        );
                    }
                }
            }
            Effect::PutFromHand { filter, tapped } => {
                let p = ctx.controller;
                let mut options: Vec<Choice> = self
                    .player(p)
                    .hand
                    .iter()
                    .filter(|id| {
                        let o = &self.objects[**id];
                        let c = self.base_characteristics(o);
                        filter.matches(**id, p, &c, None, Some(p)) && self.status_ok(filter, **id)
                    })
                    .map(|id| Choice::Discard {
                        card: *id,
                        def: self.objects[*id].def,
                    })
                    .collect();
                if !options.is_empty() {
                    options.push(Choice::Done);
                    let k = self.choose_mid(p, DecisionKind::PutFromHand, options.clone());
                    if let Some(Choice::Discard { card, .. }) = options.get(k) {
                        let card = *card;
                        let new = self.put_onto_battlefield(card, p);
                        if *tapped && self.objects.contains_key(new) {
                            self.tap(new);
                        }
                        ctx.created = Some(new);
                    }
                }
            }
            Effect::Strip { keywords, to } => {
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::RemoveKeywords(*keywords)],
                        );
                    }
                }
            }
            Effect::PutInLibrary { what, place } => {
                for t in self.recipients(ctx, *what) {
                    let Target::Object(id) = t else {
                        continue;
                    };
                    if !self.objects.get(id).is_some_and(|o| {
                        matches!(o.zone, Zone::Battlefield | Zone::Graveyard | Zone::Stack)
                    }) {
                        continue;
                    }
                    self.put_in_library(id, *place);
                }
            }
            Effect::SacrificeChosen { who, filter, count }
            | Effect::ReturnChosen { who, filter, count }
            | Effect::ExileChosen { who, filter, count } => {
                let bounce = matches!(effect, Effect::ReturnChosen { .. });
                let exile = matches!(effect, Effect::ExileChosen { .. });
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    for _ in 0..self.eval_num(*count, ctx) {
                        let options: Vec<Choice> = self
                            .permanents_of(p)
                            .filter(|id| {
                                self.characteristics(*id).is_some_and(|c| {
                                    filter.matches(*id, p, &c, Some(ctx.source), Some(p))
                                }) && self.status_ok(filter, *id)
                                    // "a creature of their choice other
                                    // than enchanted creature".
                                    && !(filter.not_attached
                                        && self.objects.get(ctx.source).and_then(|s| s.attached_to)
                                            == Some(*id))
                            })
                            .map(Choice::Sacrifice)
                            .collect();
                        if options.is_empty() {
                            // "You may sacrifice a creature. When you do,
                            // ...": nothing was sacrificed.
                            ctx.did = false;
                            break;
                        }
                        let k = self.choose_mid(p, DecisionKind::Edict, options.clone());
                        if let Choice::Sacrifice(id) = options[k] {
                            if bounce {
                                self.move_object(id, Zone::Hand, false);
                            } else if exile {
                                self.move_object(id, Zone::Exile, false);
                            } else {
                                self.sacrifice(id);
                            }
                        }
                    }
                }
            }
            Effect::ExileFromHand {
                count,
                who,
                graveyard,
            } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    let ps = self.player(p);
                    if if *graveyard { &ps.graveyard } else { &ps.hand }.is_empty() {
                        ctx.did = false;
                    }
                    for _ in 0..self.eval_num(*count, ctx) {
                        let ps = self.player(p);
                        let options: Vec<Choice> =
                            if *graveyard { &ps.graveyard } else { &ps.hand }
                                .iter()
                                .map(|id| Choice::Discard {
                                    card: *id,
                                    def: self.objects[*id].def,
                                })
                                .collect();
                        if options.is_empty() {
                            break;
                        }
                        let k = self.choose_mid(p, DecisionKind::ChooseCard, options.clone());
                        if let Choice::Discard { card, .. } = options[k] {
                            self.move_object(card, Zone::Exile, false);
                        }
                    }
                }
            }
            Effect::DrawDiscardDrawn { draw, discard } => {
                let p = ctx.controller;
                let before = self.player(p).hand.clone();
                let n = self.eval_num(*draw, ctx);
                self.apply_effect(
                    ctx,
                    &Effect::DrawCards {
                        count: crate::card::Num::N(n),
                        who: Recipient::You,
                    },
                );
                for _ in 0..self.eval_num(*discard, ctx) {
                    let options: Vec<Choice> = self
                        .player(p)
                        .hand
                        .iter()
                        .filter(|id| !before.contains(id))
                        .map(|id| Choice::Discard {
                            card: *id,
                            def: self.objects[*id].def,
                        })
                        .collect();
                    if options.is_empty() {
                        break;
                    }
                    let k = self.choose_mid(p, DecisionKind::Discard, options.clone());
                    if let Choice::Discard { card, .. } = options[k] {
                        self.discard_card(p, card);
                    }
                }
            }
            Effect::ExileFromGraveyardOrHand { who, filter } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    // CR 701.20a: the hand is revealed first.
                    self.reveal_hand(p);
                    let ps = self.player(p);
                    let options: Vec<Choice> = ps
                        .graveyard
                        .iter()
                        .chain(ps.hand.iter())
                        .filter(|id| {
                            let o = &self.objects[**id];
                            let c = self.base_characteristics(o);
                            filter.matches(**id, p, &c, Some(ctx.source), Some(ctx.controller))
                                && self.status_ok(filter, **id)
                        })
                        .map(|id| Choice::Discard {
                            card: *id,
                            def: self.objects[*id].def,
                        })
                        .collect();
                    if options.is_empty() {
                        continue;
                    }
                    let k =
                        self.choose_mid(ctx.controller, DecisionKind::ChooseCard, options.clone());
                    if let Choice::Discard { card, .. } = options[k] {
                        self.move_object(card, Zone::Exile, false);
                    }
                }
            }
            Effect::RevealHand { who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.reveal_hand(p);
                    }
                }
            }
            Effect::RevealRandom { who } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    let n = self.player(p).hand.len();
                    ctx.created = None;
                    if n == 0 {
                        continue;
                    }
                    let i = rand::Rng::random_range(&mut self.rng, 0..n);
                    let card = self.player(p).hand[i];
                    self.reveal_in_hand(card);
                    // "then loses life equal to that card's mana value".
                    ctx.created = Some(card);
                }
            }
            Effect::Vote { options } => {
                let def = self.resolving_def(ctx);
                let choices: Vec<Choice> = (0..usize::from(*options)).map(Choice::Mode).collect();
                let tally = self.vote(ctx, DecisionKind::ChooseOption { def }, &choices);
                let get =
                    |i: usize| u8::try_from(tally.get(i).copied().unwrap_or(0)).unwrap_or(u8::MAX);
                ctx.chosen = Chosen::Votes([get(0), get(1)]);
            }
            Effect::VoteReturn { filter } => {
                let you = ctx.controller;
                let cards: Vec<ObjId> = self
                    .player(you)
                    .graveyard
                    .iter()
                    .copied()
                    .filter(|id| {
                        let o = &self.objects[*id];
                        let c = self.base_characteristics(o);
                        filter.matches(*id, o.owner, &c, Some(ctx.source), Some(you))
                            && self.status_ok(filter, *id)
                    })
                    .collect();
                if cards.is_empty() {
                    return;
                }
                let choices: Vec<Choice> = cards
                    .iter()
                    .map(|id| Choice::Discard {
                        card: *id,
                        def: self.objects[*id].def,
                    })
                    .collect();
                let tally = self.vote(ctx, DecisionKind::ChooseCard, &choices);
                let most = tally.iter().copied().max().unwrap_or(0);
                if most == 0 {
                    return;
                }
                for (id, n) in cards.into_iter().zip(tally) {
                    if n == most {
                        ctx.created = Some(self.move_object(id, Zone::Hand, false));
                    }
                }
            }
            Effect::FlipCoin { won } => {
                // CR 705.2: the flipper calls heads or tails; the call
                // matches the result half the time.
                let win = rand::Rng::random_bool(&mut self.rng, 0.5);
                self.emit(Event::FlippedCoin {
                    player: ctx.controller,
                    won: win,
                });
                if win {
                    for e in won {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::Clash { won } => {
                // "Otherwise, ..." follows whether the clash was won.
                ctx.did = self.clash(ctx.controller);
                if ctx.did {
                    for e in won {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::RollDie { sides, rows } => {
                let result = self.roll_die(ctx.controller, *sides);
                // CR 706.3a: the row whose range holds the result.
                if let Some(row) = rows.iter().find(|r| (r.lo..=r.hi).contains(&result)) {
                    for e in &row.effects {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::DiscardRandom { count, who } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    // "unless you discard a card at random": not with an
                    // empty hand.
                    if self.player(p).hand.is_empty() {
                        ctx.did = false;
                    }
                    for _ in 0..self.eval_num(*count, ctx) {
                        let n = self.player(p).hand.len();
                        if n == 0 {
                            break;
                        }
                        let i = rand::Rng::random_range(&mut self.rng, 0..n);
                        let card = self.player(p).hand[i];
                        self.discard_card(p, card);
                    }
                }
            }
            Effect::DiscardChosen {
                who,
                filter,
                optional,
                exile,
                until,
            } => {
                // "If you do" / "If you don't": whether a card was chosen.
                ctx.did = false;
                // CR 610.3a: "until this leaves the battlefield" after it
                // already has: nothing is exiled.
                let gone = *until
                    && !self
                        .objects
                        .get(ctx.source)
                        .is_some_and(|o| o.zone == Zone::Battlefield);
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    self.reveal_hand(p);
                    if gone {
                        continue;
                    }
                    let mut options: Vec<Choice> = self
                        .player(p)
                        .hand
                        .iter()
                        .filter(|id| {
                            filter.as_ref().is_none_or(|f| {
                                let c = self.base_characteristics(&self.objects[**id]);
                                f.matches(**id, p, &c, None, None) && self.status_ok(f, **id)
                            })
                        })
                        .map(|id| Choice::Discard {
                            card: *id,
                            def: self.objects[*id].def,
                        })
                        .collect();
                    if options.is_empty() {
                        continue;
                    }
                    if *optional {
                        options.push(Choice::Done);
                    }
                    let k = self.choose_mid(ctx.controller, DecisionKind::Discard, options.clone());
                    if let Choice::Discard { card, .. } = options[k] {
                        if *exile {
                            let new = self.move_object(card, Zone::Exile, false);
                            if *until
                                && self.objects.get(new).is_some_and(|o| o.zone == Zone::Exile)
                            {
                                self.link_exile(ctx.source, new, true);
                                // CR 610.3: back to the hand it left.
                                if let Some(l) = self.linked_exiles.last_mut() {
                                    l.to_hand = true;
                                }
                            }
                        } else {
                            self.discard_card(p, card);
                        }
                        ctx.did = true;
                    }
                }
            }
            Effect::Sacrifice { what } => {
                let ts = self.recipients(ctx, *what);
                let together = self.begin_together(ts.len());
                for t in ts {
                    if let Some(id) = on_battlefield(self, t) {
                        self.sacrifice(id);
                    }
                }
                self.end_together(together);
            }
            Effect::CreateToken {
                token,
                count,
                who,
                tapped,
                attached,
            } => {
                // CR 303.4f: an Aura token enters attached to what it was
                // created attached to; with nothing it could enchant there
                // (no target chosen, or one now illegal), it isn't created.
                // "For each Rat you control, create a Wicked Role token
                // attached to that Rat": one creation for each object.
                let hosts: SmallVec<[Option<ObjId>; 2]> = if let Some(r) = attached {
                    {
                        let spec = self
                            .pool
                            .get(*token)
                            .enchant()
                            .unwrap_or(TargetSpec::Creature);
                        let mut hosts: SmallVec<[Option<ObjId>; 2]> = SmallVec::new();
                        for t in self.recipients(ctx, *r) {
                            if let Some(h) = on_battlefield(self, t)
                                && self.is_legal_attachment(spec, h)
                                && !hosts.contains(&Some(h))
                            {
                                hosts.push(Some(h));
                            }
                        }
                        if hosts.is_empty() {
                            return;
                        }
                        hosts
                    }
                } else {
                    smallvec::smallvec![None]
                };
                ctx.created_tokens.clear();
                let sized = self.token_pt.take();
                for host in hosts {
                    for t in self.recipients(ctx, *who) {
                        if let Target::Player(p) = t {
                            let n = self.eval_num(*count, ctx);
                            // CR 614.1a: token-creation replacement effects.
                            for (def, k) in self.tokens_to_create(p, *token, n) {
                                for _ in 0..k {
                                    let id =
                                        self.create_token_with(def, p, *tapped && def == *token);
                                    if def == *token
                                        && let Some(h) = host
                                    {
                                        self.attach(id, h);
                                    }
                                    // CR 111.3: "an X/X ... token", X part of
                                    // its copiable values (CR 707.2).
                                    if def == *token
                                        && let Some(v) = sized
                                        && self.objects[id].zone == Zone::Battlefield
                                    {
                                        self.objects[id].sized =
                                            u8::try_from(v.max(0)).unwrap_or(u8::MAX);
                                        self.cbump();
                                    }
                                    ctx.created = Some(id);
                                    ctx.created_tokens.push(id);
                                }
                            }
                        }
                    }
                }
            }
            Effect::Attach { what, to } => {
                let Some(host) = self
                    .recipients(ctx, *to)
                    .into_iter()
                    .find_map(|t| on_battlefield(self, t))
                else {
                    return;
                };
                let objs: SmallVec<[ObjId; 2]> = if *what == Recipient::Source {
                    self.current_object(ctx.source)
                        .filter(|s| self.objects[*s].zone == Zone::Battlefield)
                        .into_iter()
                        .collect()
                } else {
                    self.recipients(ctx, *what)
                        .into_iter()
                        .filter_map(|t| on_battlefield(self, t))
                        .collect()
                };
                for obj in objs {
                    self.attach(obj, host);
                }
            }
            Effect::Unattach { what } => {
                for t in self.recipients(ctx, *what) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    if self.objects[id].attached_to.take().is_some() {
                        self.cbump();
                        self.emit(Event::Unattached { obj: id });
                    }
                }
            }
            Effect::Animate {
                to,
                types,
                subtypes,
                pt,
                colors,
                keywords,
                indefinite,
                loses_abilities,
                in_addition,
            } => {
                let mut parts: SmallVec<[Modification; 2]> = SmallVec::new();
                if !types.is_empty() {
                    parts.push(if *in_addition {
                        Modification::AddTypes(*types)
                    } else {
                        Modification::BecomeTypes(*types)
                    });
                }
                // CR 205.1a: the first creature type replaces the old ones,
                // the rest join it ("a 4/4 Phyrexian Eye artifact creature");
                // "in addition to its other ... types" (CR 205.1b) adds them
                // all.
                for (i, s) in subtypes.iter().enumerate() {
                    parts.push(if i == 0 && s.is_creature_type() && !*in_addition {
                        Modification::SetCreatureType(*s)
                    } else {
                        Modification::AddSubtype(*s)
                    });
                }
                if let Some(c) = colors {
                    parts.push(if *in_addition {
                        Modification::AddColors(*c)
                    } else {
                        Modification::SetColors(*c)
                    });
                }
                // CR 613.1f: in layer 6, in the order written: it loses its
                // abilities, then gains the effect's keywords.
                if *loses_abilities {
                    parts.push(Modification::RemoveAllAbilities);
                }
                if !keywords.is_empty() {
                    parts.push(Modification::AddKeywords(*keywords));
                }
                if let Some((power, toughness)) = pt {
                    parts.push(Modification::SetPt {
                        power: PtValue::Fixed(*power),
                        toughness: PtValue::Fixed(*toughness),
                    });
                }
                let duration = if *indefinite {
                    Duration::Permanent
                } else {
                    Duration::UntilEndOfTurn
                };
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    self.effect_on(ctx, id, parts.clone(), duration);
                }
            }
            Effect::BecomeColor {
                to,
                colors,
                indefinite,
            } => {
                let duration = if *indefinite {
                    Duration::Permanent
                } else {
                    Duration::UntilEndOfTurn
                };
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_on(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::SetColors(*colors)],
                            duration,
                        );
                    }
                }
            }
            Effect::BecomeLandType {
                to,
                land,
                indefinite,
            } => {
                let duration = if *indefinite {
                    Duration::Permanent
                } else {
                    Duration::UntilEndOfTurn
                };
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_on(
                            ctx,
                            id,
                            smallvec::smallvec![Modification::SetLandType(*land)],
                            duration,
                        );
                    }
                }
            }
            // CR 608.2d: chosen as the effect resolves, for what follows.
            Effect::Choose(kind) => {
                let def = self.source_def(ctx);
                ctx.chosen = self.choose_kind(ctx.controller, *kind, def);
            }
            Effect::ProtectionFromChosen { to } => {
                let Some(c) = self.resolution_chosen(ctx).color() else {
                    return;
                };
                let part = Modification::AddKeywords(KeywordSet::of([Keyword::protection_from(c)]));
                for t in self.recipients(ctx, *to) {
                    if let Target::Player(p) = t {
                        self.grant_player(ctx, p, KeywordSet::of([Keyword::protection_from(c)]));
                        continue;
                    }
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(ctx, id, smallvec::smallvec![part.clone()]);
                    }
                }
            }
            Effect::BecomeChosen { to } => {
                let part = match self.resolution_chosen(ctx) {
                    Chosen::Color(c) => Modification::SetColors(ColorSet::of([c])),
                    // CR 305.7: a basic land type replaces the land's others.
                    Chosen::Subtype(s) => match s.basic_color() {
                        Some(c) => Modification::SetLandType(c),
                        None => Modification::SetCreatureType(s),
                    },
                    Chosen::None | Chosen::Votes(_) | Chosen::Option(_) | Chosen::Card(_) => return,
                };
                for t in self.recipients(ctx, *to) {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(ctx, id, smallvec::smallvec![part.clone()]);
                    }
                }
            }
            Effect::OfChoice {
                to,
                kind,
                protection,
                in_addition,
            } => {
                let recipients = self.recipients(ctx, *to);
                // "You gain protection from the color of your choice until
                // end of turn" (CR 702.16): a player.
                if *protection && let [Target::Player(p)] = recipients[..] {
                    let def = self.source_def(ctx);
                    if let Chosen::Color(c) = self.choose_kind(ctx.controller, *kind, def) {
                        self.grant_player(ctx, p, KeywordSet::of([Keyword::protection_from(c)]));
                    }
                    return;
                }
                // CR 608.2b: with no legal recipient left, nothing to choose.
                if !recipients
                    .iter()
                    .any(|t| on_battlefield(self, *t).is_some())
                {
                    return;
                }
                // "the color or colors of your choice": a color at a time,
                // until the player is done (at least one; never colorless).
                if *kind == ChoiceKind::Colors {
                    let mut set = ColorSet::default();
                    loop {
                        let mut options: Vec<Choice> = Color::ALL
                            .into_iter()
                            .filter(|c| !set.contains(*c))
                            .map(Choice::Color)
                            .collect();
                        // Done first: by default, one color.
                        if set != ColorSet::default() {
                            options.insert(0, Choice::Done);
                        }
                        let k = match options.len() {
                            0 => break,
                            1 => 0,
                            _ => self.choose_mid(
                                ctx.controller,
                                DecisionKind::ChooseColor,
                                options.clone(),
                            ),
                        };
                        match options[k.min(options.len() - 1)] {
                            Choice::Color(c) => set = set.with(c),
                            _ => break,
                        }
                    }
                    for t in recipients {
                        if let Some(id) = on_battlefield(self, t) {
                            self.effect_until_end_of_turn(
                                ctx,
                                id,
                                smallvec::smallvec![Modification::SetColors(set)],
                            );
                        }
                    }
                    return;
                }
                let def = self.source_def(ctx);
                let part = match (self.choose_kind(ctx.controller, *kind, def), *protection) {
                    (Chosen::Color(c), true) => {
                        Modification::AddKeywords(KeywordSet::of([Keyword::protection_from(c)]))
                    }
                    (Chosen::Color(c), false) => Modification::SetColors(ColorSet::of([c])),
                    (Chosen::Subtype(s), _) => match s.basic_color() {
                        Some(c) if matches!(kind, ChoiceKind::LandType(_)) => {
                            Modification::SetLandType(c)
                        }
                        _ if *in_addition => Modification::AddSubtype(s),
                        _ => Modification::SetCreatureType(s),
                    },
                    (Chosen::None | Chosen::Votes(_) | Chosen::Option(_) | Chosen::Card(_), _) => {
                        return;
                    }
                };
                for t in recipients {
                    if let Some(id) = on_battlefield(self, t) {
                        self.effect_until_end_of_turn(ctx, id, smallvec::smallvec![part.clone()]);
                    }
                }
            }
            Effect::IfCondition(cond, inner) => {
                let c = self.pool.condition(*cond).clone();
                // CR 701.38: "If death gets more votes", the tally of this
                // resolution's vote.
                let holds = match c {
                    crate::card::StaticCondition::MoreVotes { index, ties } => match &ctx.chosen {
                        Chosen::Votes(v) => {
                            let mine = v.get(usize::from(index)).copied().unwrap_or(0);
                            let best = v
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| *i != usize::from(index))
                                .map(|(_, n)| *n)
                                .max()
                                .unwrap_or(0);
                            mine > best || (ties && mine == best)
                        }
                        _ => false,
                    },
                    _ => self.condition_for(&c, ctx.controller, ctx.source, ctx.trigger_obj),
                };
                if holds {
                    // For an "Otherwise" after it; optional actions inside
                    // set it again.
                    ctx.did = true;
                    for e in inner {
                        self.apply_effect(ctx, e);
                    }
                } else {
                    // "If you control a Raccoon, you may discard a card. If
                    // you do, draw a card.": nothing was done.
                    ctx.did = false;
                }
            }
            Effect::IfThisWay { cond, then } => {
                if self.this_way_holds(ctx, cond) {
                    for e in then {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::IfExcessDamage(then) => {
                if self.this_way.as_ref().is_some_and(|l| l.excess > 0) {
                    for e in then {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::ChooseAct {
                act,
                filter,
                count,
                up_to,
            } => {
                let n = self.eval_num(*count, ctx) as usize;
                let did = self.choose_act(ctx, *act, filter, n, *up_to);
                if did == 0 {
                    ctx.did = false;
                }
            }
            Effect::OneOf(options) => {
                let doable: Vec<usize> = (0..options.len())
                    .filter(|i| self.can_do(ctx, &options[*i]))
                    .collect();
                let Some(&first) = doable.first() else {
                    ctx.did = false;
                    return;
                };
                let def = self
                    .objects
                    .get(ctx.source)
                    .map(|o| o.def)
                    .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
                    .expect("a resolving spell or its last known information exists");
                let k = if doable.len() == 1 {
                    first
                } else {
                    let choices: Vec<Choice> = doable.iter().map(|i| Choice::Mode(*i)).collect();
                    let k = self.choose_mid(
                        ctx.controller,
                        DecisionKind::ChooseOption { def },
                        choices,
                    );
                    doable[k]
                };
                self.apply_effect(ctx, &options[k]);
            }
            Effect::DiscardUnless { who, count, filter } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    // CR 608.2d: the player chooses: one matching card, or
                    // `count` cards.
                    let mut options: Vec<Choice> = self
                        .player(p)
                        .hand
                        .iter()
                        .filter(|id| {
                            let o = &self.objects[**id];
                            let c = self.base_characteristics(o);
                            filter.matches(**id, p, &c, None, Some(p))
                                && self.status_ok(filter, **id)
                        })
                        .map(|id| Choice::Discard {
                            card: *id,
                            def: self.objects[*id].def,
                        })
                        .collect();
                    let n = self.eval_num(*count, ctx);
                    if options.is_empty() {
                        self.discard_n(p, n);
                        continue;
                    }
                    options.push(Choice::Done);
                    let k = self.choose_mid(p, DecisionKind::Discard, options.clone());
                    match options[k] {
                        Choice::Discard { card, .. } => self.discard_card(p, card),
                        _ => self.discard_n(p, n),
                    }
                }
            }
            Effect::DiscardAll { who, filter } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    if filter.is_some() {
                        self.reveal_hand(p);
                    }
                    let cards: Vec<ObjId> = self
                        .player(p)
                        .hand
                        .iter()
                        .copied()
                        .filter(|id| {
                            filter.as_ref().is_none_or(|f| {
                                let o = &self.objects[*id];
                                let c = self.base_characteristics(o);
                                f.matches(*id, p, &c, None, Some(p)) && self.status_ok(f, *id)
                            })
                        })
                        .collect();
                    for c in cards {
                        self.discard_card(p, c);
                    }
                }
            }
            Effect::IfTarget { n, filter, then } => {
                if self.nth_target_matches(ctx, *n, filter) {
                    for e in then {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::Instead { cond, base, alt } => {
                // CR 608.2c: the later "instead" text modifies the earlier
                // instruction; exactly one of them happens.
                let holds = match cond {
                    EffectCond::Kicked => self.was_kicked(ctx.source),
                    EffectCond::Static(i) => {
                        let c = self.pool.condition(*i).clone();
                        self.condition_for(&c, ctx.controller, ctx.source, ctx.trigger_obj)
                    }
                    EffectCond::Target { n, filter } => self.nth_target_matches(ctx, *n, filter),
                    EffectCond::ThisWay(c) => self.this_way_holds(ctx, c),
                };
                // "you may put it on top of its owner's library instead":
                // declined, the earlier instruction happens.
                if holds && let [Effect::YouMay(_)] = alt.as_slice() {
                    self.apply_effect(ctx, &alt[0]);
                    if !ctx.did {
                        for e in base {
                            self.apply_effect(ctx, e);
                        }
                    }
                } else {
                    for e in if holds { alt } else { base } {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::IfKicked(inner) => {
                if self.was_kicked(ctx.source) {
                    for e in inner {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::SetBasePt {
                to,
                power,
                toughness,
            } => {
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    self.effect_until_end_of_turn(
                        ctx,
                        id,
                        smallvec::smallvec![Modification::SetPt {
                            power: PtValue::Fixed(*power),
                            toughness: PtValue::Fixed(*toughness),
                        }],
                    );
                }
            }
            Effect::SetBasePower { to, n } => {
                // CR 608.2h: the value is locked in as the effect resolves.
                let v = i32::try_from(self.eval_num(*n, ctx)).unwrap_or(i32::MAX);
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    self.effect_until_end_of_turn(
                        ctx,
                        id,
                        smallvec::smallvec![Modification::SetPt {
                            power: PtValue::Fixed(v),
                            toughness: PtValue::Unchanged,
                        }],
                    );
                }
            }
            Effect::SetBasePtVar { to, n } => {
                // CR 608.2h: the value is locked in as the effect resolves.
                let v = i32::try_from(self.eval_num(*n, ctx)).unwrap_or(i32::MAX);
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    self.effect_until_end_of_turn(
                        ctx,
                        id,
                        smallvec::smallvec![Modification::SetPt {
                            power: PtValue::Fixed(v),
                            toughness: PtValue::Fixed(v),
                        }],
                    );
                }
            }
            Effect::YouMay(inner) => {
                // "Do this only once each turn.": done already, it isn't
                // offered (`TurnLimit::Do`).
                let once = self.doing_once.take();
                if let Some(key) = once
                    && self.limited_this_turn.contains(&key)
                {
                    ctx.did = false;
                    return;
                }
                // "You may tap two untapped creatures you control": with
                // fewer than two it can't be done, so it isn't offered and
                // "if you do" fails (CR 608.2c), where a required "tap two"
                // taps what it can (CR 101.3).
                if inner
                    .iter()
                    .any(|e| matches!(e, Effect::ChooseAct { .. }) && !self.can_do(ctx, e))
                {
                    ctx.did = false;
                    return;
                }
                // "You may remove a counter from this creature. When you
                // do, ...": with no counter to remove it can't be done
                // (Leatherhead, Swamp Stalker).
                let no_counter = inner.iter().any(|e| {
                    let (to, kind, n) = match e {
                        Effect::RemoveCounterChosen { to, .. } => (*to, None, 1),
                        Effect::RemoveCounters { kind, count, to } => {
                            (*to, Some(*kind), self.eval_num(*count, ctx).max(1))
                        }
                        _ => return false,
                    };
                    !self.recipients(ctx, to).into_iter().any(|t| {
                        on_battlefield(self, t).is_some_and(|id| {
                            self.objects[id]
                                .counters
                                .iter()
                                .any(|(k, c)| kind.is_none_or(|w| w == *k) && *c >= n)
                        })
                    })
                });
                if no_counter {
                    ctx.did = false;
                    return;
                }
                let def = self
                    .objects
                    .get(ctx.source)
                    .map(|o| o.def)
                    .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
                    .expect("a resolving spell or its last known information exists");
                let k = self.choose_mid(
                    ctx.controller,
                    DecisionKind::YouMay { def },
                    vec![Choice::Accept, Choice::Decline],
                );
                ctx.did = k == 0;
                if k == 0 {
                    if let Some(key) = once {
                        self.limited_this_turn.push(key);
                    }
                    for e in inner {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            // "That player may pay {4}. If the player does, ...": the
            // player decides (CR 608.2d) and acts as the one doing it; an
            // act they can't complete in full isn't offered (CR 101.3).
            Effect::PlayerMay { who, effects } => {
                // "Each player may discard their hand and draw seven
                // cards": each player in turn order decides and acts
                // (CR 101.4; an approximation of simultaneous actions).
                let players: SmallVec<[PlayerId; 4]> = self
                    .recipients(ctx, *who)
                    .into_iter()
                    .filter_map(|t| match t {
                        Target::Player(p) => Some(p),
                        Target::Object(_) => None,
                    })
                    .collect();
                if players.is_empty() {
                    ctx.did = false;
                    return;
                }
                let def = self
                    .objects
                    .get(ctx.source)
                    .map(|o| o.def)
                    .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
                    .expect("a resolving spell or its last known information exists");
                let you = ctx.controller;
                let mut any = false;
                for p in players {
                    ctx.controller = p;
                    let possible = effects.iter().all(|e| match e {
                        Effect::Pay { mana, life } => {
                            (*life == 0
                                || self.player(p).life >= i32::try_from(*life).unwrap_or(i32::MAX))
                                && self.can_pay(p, &mana.requirements(0))
                        }
                        e => self.can_do(ctx, e),
                    });
                    let did = possible
                        && self.choose_mid(
                            p,
                            DecisionKind::YouMay { def },
                            vec![Choice::Accept, Choice::Decline],
                        ) == 0;
                    if did {
                        ctx.did = true;
                        for e in effects {
                            self.apply_effect(ctx, e);
                        }
                    }
                    any |= did;
                }
                ctx.did = any;
                ctx.controller = you;
            }
            // CR 701.67a: pay it, tapping artifacts and creatures for
            // generic mana where mana runs short.
            Effect::Waterbend(mana) => {
                if self.can_do(ctx, effect) {
                    let p = ctx.controller;
                    let req = mana.requirements(0);
                    let mut sources = self.mana_sources(p);
                    sources.extend(self.waterbend_sources(p, req.generic, None));
                    self.pay_from(p, &req, &sources, None);
                } else {
                    ctx.did = false;
                }
            }
            Effect::Pay { mana, life } => {
                let p = ctx.controller;
                // "You may pay {X}. When you do, ...": the player chooses
                // X as they pay (CR 107.1c, 107.3), any value they can
                // pay; later "X" in the resolution, and in the reflexive
                // ability, is that value.
                let x = if mana.has_x() {
                    let max = (0u32..=99)
                        .take_while(|&n| self.can_pay(p, &mana.requirements(n)))
                        .last()
                        .unwrap_or(0);
                    let pick = self.choose_mid(
                        p,
                        DecisionKind::ChooseX { spell: ctx.source },
                        (0..=max).map(Choice::X).collect(),
                    );
                    let x = u32::try_from(pick).unwrap_or(0).min(max);
                    ctx.x = x;
                    x
                } else {
                    0
                };
                let req = mana.requirements(x);
                let life_ok =
                    *life == 0 || self.player(p).life >= i32::try_from(*life).unwrap_or(i32::MAX);
                if life_ok && self.can_pay(p, &req) {
                    self.pay(p, &req);
                    if *life > 0 {
                        self.change_life(p, -i32::try_from(*life).unwrap_or(i32::MAX));
                    }
                } else {
                    ctx.did = false;
                }
            }
            // "Sacrifice it unless you sacrifice a land": the controller
            // decides whether to act (CR 608.2d); an act they can't
            // complete doesn't count (CR 101.3).
            Effect::UnlessYou { act, effects } => {
                if act.iter().all(|e| self.can_do(ctx, e)) {
                    self.apply_effect(ctx, &Effect::YouMay(act.clone()));
                } else {
                    ctx.did = false;
                }
                if !ctx.did {
                    for e in effects {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::Otherwise(inner) => {
                if !ctx.did {
                    for e in inner {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::IfYouDo(inner) => {
                if ctx.did {
                    for e in inner {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            // CR 603.12: the reflexive ability triggers now and goes on the
            // stack the next time a player would receive priority.
            Effect::Reflexive(ability) => {
                if ctx.did {
                    let def = self
                        .objects
                        .get(ctx.source)
                        .map(|o| o.def)
                        .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
                        .expect("a resolving spell or its last known information exists");
                    let timestamp = self.tick();
                    // "You may pay {X}. When you do, put X +1/+1 counters
                    // on this creature": the reflexive ability's X is the
                    // resolution's.
                    if ctx.x > 0 {
                        self.reflexive_x.push((timestamp, ctx.x));
                    }
                    // "When you do, Bolg deals damage equal to that
                    // creature's power": the creature sacrificed this way.
                    if let Some(log) = &self.this_way
                        && !log.moved.is_empty()
                    {
                        let moved = log.moved.clone();
                        self.reflexive_moved.push((timestamp, moved));
                    }
                    self.pending_triggers.push(crate::state::PendingTrigger {
                        controller: ctx.controller,
                        ability: StackAbility {
                            ability: *ability,
                            source: ctx.source,
                            def,
                            // "Create an Axe token. When you do, attach it
                            // to target creature you control": the
                            // reflexive ability carries what the ability
                            // just created as its object ("that token").
                            trigger_obj: ctx.created.or(ctx.trigger_obj),
                            trigger_player: ctx.trigger_player,
                            trigger_amount: ctx.trigger_amount,
                        },
                        timestamp,
                    });
                }
            }
            Effect::UnlessPays {
                payer,
                mana,
                life,
                discard,
                waterbend,
                discard_random,
                effects,
            } => {
                let def = self
                    .objects
                    .get(ctx.source)
                    .map(|o| o.def)
                    .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
                    .expect("a resolving spell or its last known information exists");
                // "unless any player pays {2}": each player in turn, from
                // the active player (CR 101.4), may pay, until one does.
                let payers: SmallVec<[PlayerId; 2]> = if *payer == Recipient::EachPlayer {
                    let mut p = self.active_player;
                    let mut out = SmallVec::new();
                    for _ in 0..self.num_players() {
                        if self.player(p).lost.is_none() {
                            out.push(p);
                        }
                        p = self.next_player(p);
                    }
                    out
                } else {
                    self.recipients(ctx, *payer)
                        .into_iter()
                        .find_map(|t| match t {
                            Target::Player(p) => Some(p),
                            Target::Object(_) => None,
                        })
                        .into_iter()
                        .collect()
                };
                // "unless any player pays {X}": X of the spell (CR 107.3).
                let req = mana.requirements(ctx.x);
                let life_n = i32::try_from(*life).unwrap_or(i32::MAX);
                // "pays {1} or 1 life": either, the payer's choice.
                let either = *life > 0 && *mana != crate::mana::ManaCost::default();
                let mut paid = false;
                for p in payers {
                    // "Ward—Waterbend {4}": artifacts and creatures tapped
                    // for the generic mana (CR 701.67a).
                    let bend = waterbend.then(|| {
                        let mut sources = self.mana_sources(p);
                        sources.extend(self.waterbend_sources(p, req.generic, None));
                        sources
                    });
                    if let Some(sources) = &bend {
                        let ps = self.player(p);
                        let ok = self.payable(p, sources, &ps.mana_pool, &req, ps.life, None);
                        let mut options = Vec::new();
                        if ok {
                            options.push(Choice::Accept);
                        }
                        if options.is_empty() {
                            continue;
                        }
                        options.push(Choice::Decline);
                        let i =
                            self.choose_mid(p, DecisionKind::PayOrCounter { def }, options.clone());
                        if options.get(i) != Some(&Choice::Accept) {
                            continue;
                        }
                        self.pay_from(p, &req, sources, None);
                        paid = true;
                        break;
                    }
                    let mana_ok = (either || *life == 0) && self.can_pay(p, &req);
                    let life_ok = *life > 0 && self.player(p).life >= life_n;
                    let mut options = Vec::new();
                    if either {
                        if mana_ok {
                            options.push(Choice::Mode(0));
                        }
                        if life_ok {
                            options.push(Choice::Mode(1));
                        }
                    } else if (*life == 0 || life_ok)
                        && self.can_pay(p, &req)
                        && self.player(p).hand.len() >= *discard as usize
                    {
                        options.push(Choice::Accept);
                    }
                    if options.is_empty() {
                        continue;
                    }
                    // The effect's own controller is asked too ("any
                    // player"), but declining is listed first for them.
                    if *payer == Recipient::EachPlayer && p == ctx.controller {
                        options.insert(0, Choice::Decline);
                    } else {
                        options.push(Choice::Decline);
                    }
                    let i = self.choose_mid(p, DecisionKind::PayOrCounter { def }, options.clone());
                    match options.get(i) {
                        Some(Choice::Mode(0)) => self.pay(p, &req),
                        Some(Choice::Mode(_)) => self.change_life(p, -life_n),
                        Some(Choice::Accept) => {
                            self.pay(p, &req);
                            if *life > 0 {
                                self.change_life(p, -life_n);
                            }
                            // CR 701.9: the payer chooses what to discard,
                            // or it is chosen at random (CR 701.9b).
                            if *discard > 0 && *discard_random {
                                for _ in 0..*discard {
                                    let n = self.player(p).hand.len();
                                    if n == 0 {
                                        break;
                                    }
                                    let i = rand::Rng::random_range(&mut self.rng, 0..n);
                                    let card = self.player(p).hand[i];
                                    self.discard_card(p, card);
                                }
                            } else if *discard > 0 {
                                self.discard_n(p, *discard);
                            }
                        }
                        _ => continue,
                    }
                    paid = true;
                    break;
                }
                if !paid {
                    for e in effects {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::UnlessSacrifices {
                payer,
                filter,
                or_discard,
                effects,
            } => {
                let def = self
                    .objects
                    .get(ctx.source)
                    .map(|o| o.def)
                    .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
                    .unwrap_or_default();
                let payers: SmallVec<[PlayerId; 2]> = self
                    .recipients(ctx, *payer)
                    .into_iter()
                    .filter_map(|t| match t {
                        Target::Player(p) => Some(p),
                        Target::Object(_) => None,
                    })
                    .collect();
                for p in payers {
                    // CR 608.2d: the player chooses as it resolves: `Mode(0)`
                    // sacrifices, `Mode(1)` discards, `Mode(2)` takes the
                    // penalty.
                    let sacrificable: Vec<Choice> = self
                        .permanents_of(p)
                        .filter(|id| {
                            self.characteristics(*id).is_some_and(|c| {
                                filter.matches(*id, p, &c, Some(ctx.source), Some(p))
                            }) && self.status_ok(filter, *id)
                        })
                        .map(Choice::Sacrifice)
                        .collect();
                    let mut options = Vec::new();
                    if !sacrificable.is_empty() {
                        options.push(Choice::Mode(0));
                    }
                    if *or_discard && !self.player(p).hand.is_empty() {
                        options.push(Choice::Mode(1));
                    }
                    options.push(Choice::Mode(2));
                    let k = if options.len() == 1 {
                        0
                    } else {
                        self.choose_mid(p, DecisionKind::ChooseOption { def }, options.clone())
                    };
                    match options[k] {
                        Choice::Mode(0) => {
                            let k = self.choose_mid(p, DecisionKind::Edict, sacrificable.clone());
                            if let Choice::Sacrifice(id) = sacrificable[k] {
                                self.sacrifice(id);
                            }
                        }
                        Choice::Mode(1) => self.discard_n(p, 1),
                        _ => {
                            for e in effects {
                                self.apply_effect(ctx, e);
                            }
                        }
                    }
                }
            }
            // CR 702.55a: "exile it haunting target creature": the card,
            // still in its owner's graveyard, is exiled; its "When the
            // creature it haunts dies" ability watches that creature from
            // exile (702.55c).
            // CR 702.72a: the source still on the battlefield, another
            // permanent of its controller's matching the filter is exiled
            // (the first option; declining sacrifices the source).
            Effect::Champion(filter) => {
                let p = ctx.controller;
                let Some(me) = self
                    .objects
                    .get(ctx.source)
                    .filter(|o| o.zone == Zone::Battlefield)
                    .map(|_| ctx.source)
                else {
                    return;
                };
                let cands: Vec<ObjId> = self
                    .permanents_of(p)
                    .filter(|id| {
                        *id != me
                            && self
                                .characteristics(*id)
                                .is_some_and(|c| filter.matches(*id, p, &c, Some(me), Some(p)))
                    })
                    .collect();
                let mut options: Vec<Choice> =
                    cands.iter().map(|id| Choice::Sacrifice(*id)).collect();
                options.push(Choice::Done);
                let k = self.choose_mid(p, DecisionKind::ChooseCard, options.clone());
                if let Choice::Sacrifice(id) = options[k] {
                    let new = self.move_object(id, Zone::Exile, true);
                    if self.objects.get(new).is_some_and(|o| o.zone == Zone::Exile) {
                        self.link_exile(me, new, false);
                    }
                } else {
                    self.sacrifice(me);
                }
            }
            Effect::ExileHaunting => {
                let Some(card) = self
                    .current_object(ctx.source)
                    .filter(|id| self.objects[*id].zone == Zone::Graveyard)
                else {
                    return;
                };
                let Some(Target::Object(creature)) = self
                    .recipients(ctx, crate::card::Recipient::Target(0))
                    .first()
                    .copied()
                else {
                    return;
                };
                let (def, owner) = (self.objects[card].def, self.objects[card].owner);
                let new = self.move_object(card, Zone::Exile, true);
                if let Some(ability) = self.pool.get(def).haunted
                    && self.objects.get(new).is_some_and(|o| o.zone == Zone::Exile)
                {
                    self.delayed_triggers.push(DelayedTrigger {
                        controller: owner,
                        ability: StackAbility {
                            ability,
                            source: new,
                            def,
                            trigger_obj: Some(creature),
                            trigger_player: None,
                            trigger_amount: 0,
                        },
                        this_turn: false,
                        forever: false,
                        turn: self.turn_number,
                    });
                }
            }
            Effect::Delayed {
                ability,
                about,
                this_turn,
            } => {
                let def = self
                    .objects
                    .get(ctx.source)
                    .map(|o| o.def)
                    .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
                    .expect("a resolving spell or its last known information exists");
                for t in self.recipients(ctx, *about) {
                    let Target::Object(id) = t else {
                        continue;
                    };
                    // "That card": the object it became after the effect
                    // that moved it (CR 400.7 exceptions).
                    let Some(id) = self.current_object(id) else {
                        continue;
                    };
                    self.delayed_triggers.push(DelayedTrigger {
                        controller: ctx.controller,
                        ability: StackAbility {
                            ability: *ability,
                            source: ctx.source,
                            def,
                            trigger_obj: Some(id),
                            trigger_player: None,
                            trigger_amount: 0,
                        },
                        this_turn: *this_turn,
                        forever: false,
                        turn: self.turn_number,
                    });
                    self.emit(Event::DelayedTriggerCreated {
                        source: ctx.source,
                        def,
                        about: id,
                    });
                }
            }
            Effect::Exile { what } => {
                let ts = self.recipients(ctx, *what);
                let together = self.begin_together(ts.len());
                for t in ts {
                    // A permanent, or a card in a graveyard ("exile target
                    // card from a graveyard").
                    // "Exile Flood of Recollection.": the resolving spell
                    // itself, instead of its graveyard.
                    let id = on_battlefield(self, t).or_else(|| match t {
                        Target::Object(id)
                            if self.objects.get(id).is_some_and(|o| {
                                o.zone == Zone::Graveyard
                                    || (o.zone == Zone::Stack && id == ctx.source)
                            }) =>
                        {
                            Some(id)
                        }
                        _ => None,
                    });
                    if let Some(id) = id {
                        let new = self.move_object(id, Zone::Exile, true);
                        ctx.created = Some(new);
                    }
                }
                self.end_together(together);
            }
            Effect::ExileLinked { what, until } => {
                // CR 610.3a: "until this leaves the battlefield" after it
                // already has: nothing is exiled.
                if *until
                    && !self
                        .objects
                        .get(ctx.source)
                        .is_some_and(|o| o.zone == Zone::Battlefield)
                {
                    return;
                }
                for t in self.recipients(ctx, *what) {
                    if let Some(id) = on_battlefield(self, t) {
                        let new = self.move_object(id, Zone::Exile, true);
                        ctx.created = Some(new);
                        if self.objects.get(new).is_some_and(|o| o.zone == Zone::Exile) {
                            self.link_exile(ctx.source, new, *until);
                        }
                    }
                }
            }
            Effect::ReturnToBattlefield {
                what,
                counters,
                tapped,
                under_you,
                attached,
                more,
                if_creature,
            } => {
                if *what == crate::card::Recipient::Source && self.source_left_graveyard(ctx) {
                    return;
                }
                // CR 303.4f: "attached to that creature": the host must
                // still be on the battlefield (CR 303.4g).
                let host = match attached {
                    Some(r) => {
                        let Some(h) = self
                            .recipients(ctx, *r)
                            .into_iter()
                            .find_map(|t| on_battlefield(self, t))
                        else {
                            return;
                        };
                        Some(h)
                    }
                    None => None,
                };
                for t in self.recipients(ctx, *what) {
                    let Target::Object(id) = t else {
                        continue;
                    };
                    // "Exile two target creatures you control, then return
                    // those cards": each target the resolution has moved
                    // is found as the card it became (CR 400.7); the
                    // targets were legal as it began.
                    let id = if self.objects.contains_key(id) {
                        id
                    } else {
                        match self.current_object(id) {
                            Some(c) => c,
                            None => continue,
                        }
                    };
                    let Some(o) = self.objects.get(id) else {
                        continue;
                    };
                    if !matches!(o.zone, Zone::Exile | Zone::Graveyard) {
                        continue;
                    }
                    // CR 303.4g: an Aura that can't legally enchant the
                    // host stays where it is.
                    if let Some(h) = host {
                        let spec = self
                            .pool
                            .get(o.def)
                            .enchant()
                            .unwrap_or(TargetSpec::Creature);
                        if !self.is_legal_attachment(spec, h) {
                            continue;
                        }
                    }
                    let controller = if *under_you { ctx.controller } else { o.owner };
                    // "If it enters as a creature": its card's types.
                    let with = !*if_creature
                        || self
                            .pool
                            .get(o.def)
                            .types
                            .contains(crate::card::CardType::Creature);
                    let new = self.put_onto_battlefield_with(
                        id,
                        controller,
                        super::replace::Enter {
                            tapped: *tapped,
                            counters: counters.filter(|_| with),
                            more: *more,
                            ..super::replace::Enter::default()
                        },
                    );
                    if let Some(h) = host
                        && self
                            .objects
                            .get(new)
                            .is_some_and(|o| o.zone == Zone::Battlefield)
                        && self.objects.contains_key(h)
                    {
                        self.objects[new].attached_to = Some(h);
                        self.cbump();
                    }
                    ctx.created = Some(new);
                }
            }
            Effect::BecomeCopy {
                of,
                except,
                who,
                until,
            } => {
                // The exceptions are the source card's (`except` block).
                let x = if *except {
                    self.objects
                        .get(ctx.source)
                        .map(|o| o.card)
                        .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.card))
                } else {
                    None
                };
                let copiers: SmallVec<[ObjId; 4]> = match who {
                    None => self
                        .current_object(ctx.source)
                        .filter(|s| self.objects[*s].zone == Zone::Battlefield)
                        .into_iter()
                        .collect(),
                    Some(w) => self
                        .recipients(ctx, *w)
                        .into_iter()
                        .filter_map(|t| on_battlefield(self, t))
                        .collect(),
                };
                // What is copied: a permanent, or a card in a graveyard or
                // exile ("a creature card exiled with it").
                let of_obj = self.recipients(ctx, *of).into_iter().find_map(|t| {
                    on_battlefield(self, t).or_else(|| match t {
                        Target::Object(id)
                            if self.objects.get(id).is_some_and(|o| {
                                matches!(o.zone, Zone::Graveyard | Zone::Exile)
                            }) =>
                        {
                            Some(id)
                        }
                        _ => None,
                    })
                });
                let Some(of_obj) = of_obj else {
                    return;
                };
                for id in copiers {
                    if id == of_obj {
                        continue;
                    }
                    let (def, before, sized) = {
                        let o = &self.objects[id];
                        (o.def, o.except, o.sized)
                    };
                    self.become_copy(id, of_obj, x);
                    let next_turn_of = match until {
                        crate::card::CopyUntil::Permanent => continue,
                        crate::card::CopyUntil::EndOfTurn => None,
                        crate::card::CopyUntil::YourNextTurn => Some(ctx.controller),
                    };
                    let copied = self.objects[id].def;
                    self.temp_copies.push(crate::state::TempCopy {
                        obj: id,
                        def,
                        except: before,
                        sized,
                        copied,
                        next_turn_of,
                    });
                }
            }
            Effect::AddEnergy { count, who } => {
                let n = self.eval_num(*count, ctx);
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.player_mut(p).energy += n;
                        self.bump();
                    }
                }
            }
            Effect::PayEnergy(n) => {
                let p = ctx.controller;
                if self.player(p).energy >= *n {
                    self.player_mut(p).energy -= n;
                    self.bump();
                } else {
                    ctx.did = false;
                }
            }
            Effect::AddPoison { count, who } => {
                let n = self.eval_num(*count, ctx);
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t
                        && n > 0
                    {
                        self.add_poison(p, n);
                    }
                }
            }
            Effect::Proliferate => self.proliferate(ctx.controller),
            Effect::BecomeStage(stage) => self.become_stage(ctx.source, *stage),
            Effect::BecomeMonarch { who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.become_monarch(p);
                    }
                }
            }
            Effect::TakeInitiative { who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.take_initiative(p);
                    }
                }
            }
            Effect::Venture { undercity } => self.venture(ctx.controller, *undercity),
            Effect::RingTempts => self.ring_tempts(ctx.controller),
            Effect::IncreaseSpeed => self.increase_speed(ctx.controller),
            Effect::Ascend => {
                self.ascend(ctx.controller);
            }
            Effect::UntilYourNextTurn { to, parts } => {
                let parts: SmallVec<[Modification; 2]> = parts.iter().cloned().collect();
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    let d = Duration::UntilYourNextTurn(ctx.controller);
                    self.effect_on(ctx, id, parts.clone(), d);
                }
            }
            Effect::Lasting { lasting, effects } => self.apply_lasting(ctx, *lasting, effects),
            Effect::CopySpell => self.copy_spell(ctx),
            Effect::CopySpells {
                what,
                times,
                new_targets,
                except,
            } => self.copy_spells(ctx, *what, *times, *new_targets, *except),
            Effect::FaceDownSpellsLess(n) => {
                let i = ctx.controller.index().min(3);
                let less = &mut self.turn_log.face_down_less[i];
                *less = less.saturating_add(u8::try_from(*n).unwrap_or(u8::MAX));
            }
            Effect::TurnFaceUp { what } => {
                for t in self.recipients(ctx, *what) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    let o = &self.objects[id];
                    if !self.pool.is_face_down(o.def) {
                        continue;
                    }
                    if self.pool.get(o.card).is_permanent_card() {
                        self.turn_face_up(id, false);
                    } else {
                        // CR 701.40g: revealed, it stays face down.
                        self.objects[id].revealed = u8::MAX;
                    }
                }
            }
            Effect::TurnFaceDown { what } => {
                for t in self.recipients(ctx, *what) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    let o = &self.objects[id];
                    // CR 708.2b, 712.16: nothing happens.
                    if self.pool.is_face_down(o.def)
                        || self.pool.get(o.card).layout == Layout::Transform
                    {
                        continue;
                    }
                    self.turn_face_down(id);
                }
            }
            Effect::LookAtFaceDown { what } => {
                for t in self.recipients(ctx, *what) {
                    if let Some(id) = on_battlefield(self, t)
                        && self.pool.is_face_down(self.objects[id].def)
                    {
                        self.objects[id].revealed |= 1 << ctx.controller.index().min(7);
                    }
                }
            }
            Effect::LookAtPlayer { who } => {
                let bit = 1 << ctx.controller.index().min(7);
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    let ps = self.player(p);
                    let seen: Vec<ObjId> = ps
                        .hand
                        .iter()
                        .chain(ps.library.last())
                        .copied()
                        .chain(
                            self.permanents_of(p)
                                .filter(|id| self.pool.is_face_down(self.objects[*id].def)),
                        )
                        .collect();
                    for id in seen {
                        self.objects[id].revealed |= bit;
                    }
                }
            }
            Effect::Goad { what } => {
                for t in self.recipients(ctx, *what) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    let d = Duration::UntilYourNextTurn(ctx.controller);
                    let must =
                        Modification::AddKeywords(KeywordSet::NONE.with(Keyword::MustAttack));
                    self.effect_on(ctx, id, smallvec::smallvec![must], d);
                    if !self.goads.contains(&(id, ctx.controller)) {
                        self.goads.push((id, ctx.controller));
                    }
                }
            }
            Effect::PunishEach { life, unless } => self.punish_each(ctx, *life, unless.as_ref()),
            Effect::DrawCastOne { count } => self.draw_cast_one(ctx, *count),
            Effect::CastFree {
                what,
                filter,
                max_mv,
                exile,
            } => self.cast_free(ctx, *what, filter.as_ref(), *max_mv, *exile),
            Effect::RevealPutCreature { count, counters } => {
                self.reveal_put_creature(ctx, *count, *counters);
            }
            Effect::PhaseOut { what } => {
                for t in self.recipients(ctx, *what) {
                    if let Target::Object(id) = t {
                        self.phase_out(id, false);
                    }
                }
            }
            Effect::Explore { what } => {
                for t in self.recipients(ctx, *what) {
                    if let Target::Object(id) = t {
                        self.explore(ctx, id);
                        // CR 701.44c: it explored, whatever was revealed
                        // and even if it has left the battlefield.
                        self.emit(Event::Explored { obj: id });
                    }
                }
            }
            Effect::Endure { what, count, token } => {
                let n = self.eval_num(*count, ctx);
                // A source that has left the battlefield still endures:
                // its last controller creates the token.
                if *what == Recipient::Source
                    && self
                        .objects
                        .get(ctx.source)
                        .is_none_or(|o| o.zone != Zone::Battlefield)
                {
                    let id = ctx.source;
                    self.endure(ctx, id, n, *token);
                    return;
                }
                for t in self.recipients(ctx, *what) {
                    if let Target::Object(id) = t {
                        self.endure(ctx, id, n, *token);
                    }
                }
            }
            Effect::Amass { count, token } => {
                let n = self.eval_num(*count, ctx);
                let p = ctx.controller;
                let army = self.pool.subtype_lookup("Army");
                let mut target = army.and_then(|a| {
                    self.permanents_of(p).find(|id| {
                        self.characteristics(*id)
                            .is_some_and(|c| c.is_creature() && c.has_subtype(a))
                    })
                });
                if target.is_none() {
                    // CR 614.1a: an Army made twice is two Armies; one of
                    // them gets the counters (CR 701.47a).
                    for (d, k) in self.tokens_to_create(p, *token, 1) {
                        for _ in 0..k {
                            let new = self.create_token(d, p);
                            if self.objects.contains_key(new) && d == *token && target.is_none() {
                                target = Some(new);
                                ctx.created = Some(new);
                            }
                        }
                    }
                }
                if let Some(id) = target {
                    self.add_counters(id, CounterKind::PlusOnePlusOne, n);
                    // "attach this Equipment to the amassed Army".
                    ctx.created = Some(id);
                }
            }
            Effect::EmpowerJace { count, token } => {
                let n = self.eval_num(*count, ctx);
                let p = ctx.controller;
                let jace = self.pool.subtype_lookup("Jace");
                let mut target = jace.and_then(|j| {
                    self.permanents_of(p).find(|id| {
                        self.objects[*id].kind == ObjKind::Token
                            && self
                                .characteristics(*id)
                                .is_some_and(|c| c.is_planeswalker() && c.has_subtype(j))
                    })
                });
                if target.is_none() {
                    for (d, k) in self.tokens_to_create(p, *token, 1) {
                        for _ in 0..k {
                            let new = self.create_token(d, p);
                            if self.objects.contains_key(new) && d == *token && target.is_none() {
                                target = Some(new);
                            }
                        }
                    }
                }
                if let Some(id) = target {
                    self.add_counters(id, CounterKind::Loyalty, n);
                }
            }
            // Modes resolve through the chosen mode (`effects_of`).
            Effect::Modes(_) => {}
            // Looking at the top cards keeps their order; the controller
            // knows them where they are.
            Effect::LookAtTop { count, who, looker } => {
                let n = self.eval_num(*count, ctx) as usize;
                let mut bits = 0u8;
                for t in self.recipients(ctx, *looker) {
                    if let Target::Player(p) = t {
                        bits |= 1 << p.index().min(7);
                    }
                }
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    let top: Vec<_> = self
                        .player(p)
                        .library
                        .iter()
                        .rev()
                        .take(n)
                        .copied()
                        .collect();
                    for id in top {
                        self.objects[id].revealed |= bits;
                    }
                }
            }
            Effect::RevealTop { who, look } => {
                // "Each player reveals the top card of their library"
                // (parley): every card revealed is the resolution's.
                ctx.revealed.clear();
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    let top = self.player(p).library.last().copied();
                    ctx.created = top;
                    let Some(top) = top else {
                        continue;
                    };
                    ctx.revealed.push(top);
                    if *look {
                        self.objects[top].revealed |= 1 << ctx.controller.index().min(7);
                    } else {
                        self.reveal_in_library(top);
                    }
                }
            }
            Effect::ExileUntilCard { who, filter } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    ctx.created = None;
                    while let Some(top) = self.player(p).library.last().copied() {
                        let new = self.move_object(top, Zone::Exile, true);
                        let Some(o) = self.objects.get(new) else {
                            break;
                        };
                        if o.zone != Zone::Exile {
                            continue;
                        }
                        let c = self.base_characteristics(o);
                        if filter.matches(new, o.owner, &c, Some(ctx.source), Some(ctx.controller))
                            && self.status_ok(filter, new)
                        {
                            ctx.created = Some(new);
                            break;
                        }
                    }
                }
            }
            Effect::MayPlayCard { until, cast, free } => {
                let p = ctx.controller;
                let own_ends = if self.active_player == p { 2 } else { 1 };
                if let Some(id) = ctx.created
                    && self.objects.get(id).is_some_and(|o| o.zone == Zone::Exile)
                {
                    self.exile_casts
                        .get_or_insert_with(Box::default)
                        .playable
                        .push(crate::state::PlayableExile {
                            card: id,
                            player: p,
                            until: *until,
                            own_ends,
                            cast_only: *cast,
                            free: *free,
                            source: ctx.source,
                        });
                }
            }
            Effect::RevealUntil { who, filter } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else {
                        continue;
                    };
                    ctx.created = None;
                    ctx.revealed.clear();
                    // CR 701.20: from the top down; the cards stay in the
                    // library until an instruction moves them.
                    let lib = self.player(p).library.clone();
                    for id in lib.iter().rev() {
                        self.reveal_in_library(*id);
                        ctx.revealed.push(*id);
                        let o = &self.objects[*id];
                        let c = self.base_characteristics(o);
                        if filter.matches(
                            *id,
                            o.controller,
                            &c,
                            Some(ctx.source),
                            Some(ctx.controller),
                        ) && self.status_ok(filter, *id)
                        {
                            ctx.created = Some(*id);
                            break;
                        }
                    }
                }
            }
            Effect::IfCard {
                filter,
                negated,
                shares_type,
                then,
            } => {
                let source = self.characteristics_or_lki(ctx.source);
                let holds = ctx
                    .created
                    .filter(|id| self.objects.contains_key(*id))
                    .is_some_and(|id| {
                        let o = &self.objects[id];
                        let c = self
                            .characteristics(id)
                            .unwrap_or_else(|| self.base_characteristics(o));
                        let shares = !*shares_type
                            || source.as_ref().is_some_and(|s| {
                                c.subtypes
                                    .iter()
                                    .any(|t| t.is_creature_type() && s.has_subtype(*t))
                                    || s.subtypes
                                        .iter()
                                        .any(|t| t.is_creature_type() && c.has_subtype(*t))
                            });
                        (shares
                            && filter.matches(
                                id,
                                o.controller,
                                &c,
                                Some(ctx.source),
                                Some(ctx.controller),
                            ))
                            != *negated
                    });
                // For an "Otherwise" after it; optional actions inside set
                // it again.
                ctx.did = holds;
                if holds {
                    for e in then {
                        self.apply_effect(ctx, e);
                    }
                }
            }
            Effect::PutCard { what, to, random } => self.put_card(ctx, *what, *to, *random),
            Effect::RevealCard => {
                if let Some(id) = ctx.created.filter(|id| self.objects.contains_key(*id)) {
                    if self.objects[id].zone == Zone::Hand {
                        self.reveal_in_hand(id);
                    } else {
                        self.reveal_in_library(id);
                    }
                }
            }
            Effect::DrawReveal => {
                let p = ctx.controller;
                let before = self.player(p).hand.len();
                self.draw(p);
                let hand = &self.player(p).hand;
                ctx.created = (hand.len() > before).then(|| hand[hand.len() - 1]);
                if let Some(id) = ctx.created {
                    self.reveal_in_hand(id);
                }
            }
            Effect::MoveCounters {
                from,
                to,
                kind,
                count,
            } => self.move_counters(ctx, *from, *to, *kind, *count),
            Effect::ShuffleLibrary { who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        self.shuffle_library(p);
                    }
                }
            }
            Effect::CastParadigmCopy => {
                // CR 702.192a, 707.12: the copy is made in exile and may be
                // cast free; not cast, it ceases to exist.
                let Some(&(_, card, _)) = self.paradigms.iter().find(|(_, _, s)| *s == ctx.source)
                else {
                    return;
                };
                let p = ctx.controller;
                let copy = self.new_object(card, p, Zone::Exile);
                self.objects[copy].cast = self.objects[copy].cast.copied();
                self.player_mut(p).exile.push(copy);
                // The decision names the exiled card while it is there (the
                // copy is new to this resolution).
                let shown = if self.objects.contains_key(ctx.source) {
                    ctx.source
                } else {
                    copy
                };
                if !self.offer_exile_cast_shown(copy, AltCast::Free, p, shown) {
                    self.remove_from_game(copy);
                }
            }
            Effect::CastCipherCopy => {
                // CR 707.12: the copy is made in exile and cast from there;
                // not cast, it ceases to exist.
                let Some(card) = self
                    .current_object(ctx.source)
                    .filter(|c| self.objects[*c].zone == Zone::Exile)
                else {
                    return;
                };
                let p = ctx.controller;
                let copy = self.new_object(self.objects[card].def, p, Zone::Exile);
                self.objects[copy].cast = self.objects[copy].cast.copied();
                self.player_mut(p).exile.push(copy);
                if !self.offer_exile_cast_shown(copy, AltCast::Free, p, card) {
                    self.remove_from_game(copy);
                }
            }
            Effect::CastCopy { what, free } => self.cast_copy(ctx, *what, *free),
            Effect::CastExiled(kind) => {
                // CR 702.94a: a miracle card is cast from the hand.
                let from = if *kind == crate::card::ExileCast::Miracle {
                    Zone::Hand
                } else {
                    Zone::Exile
                };
                let Some(card) = self
                    .current_object(ctx.source)
                    .filter(|c| self.objects[*c].zone == from)
                else {
                    return;
                };
                let alt = match kind {
                    crate::card::ExileCast::Miracle => {
                        self.offer_exile_cast(card, AltCast::Miracle);
                        return;
                    }
                    crate::card::ExileCast::Madness => AltCast::Madness,
                    crate::card::ExileCast::Free => AltCast::Free,
                    crate::card::ExileCast::Transformed => {
                        // CR 310.12b: "you" (the Siege's controller) may
                        // cast it transformed.
                        let d = self.pool.get(self.objects[card].def);
                        if d.layout == Layout::Transform
                            && let Some(back) = d.faces.first().copied()
                            && self.offer_exile_cast_as(
                                card,
                                AltCast::Free,
                                ctx.controller,
                                back,
                                card,
                            )
                        {
                            self.exile_casts
                                .get_or_insert_with(Box::default)
                                .transformed
                                .push((card, back));
                        }
                        return;
                    }
                };
                self.objects[card].alt = AltCast::None;
                if !self.offer_exile_cast(card, alt) && *kind == crate::card::ExileCast::Madness {
                    self.move_object(card, Zone::Graveyard, true);
                }
            }
            Effect::SuspendTick => {
                let Some(card) = self
                    .current_object(ctx.source)
                    .filter(|c| self.objects[*c].zone == Zone::Exile)
                else {
                    return;
                };
                let o = &mut self.objects[card];
                if o.alt != AltCast::Suspended {
                    return;
                }
                o.remove_counters(CounterKind::Time, 1);
                if o.counters(CounterKind::Time) == 0 {
                    // CR 702.62a: as the last is removed, it may be cast;
                    // either way it's no longer suspended.
                    o.alt = AltCast::None;
                    self.suspended_count = self.suspended_count.saturating_sub(1);
                    self.offer_exile_cast(card, AltCast::Suspend);
                }
            }
            Effect::Cascade => self.cascade(ctx, None),
            Effect::Ripple => self.ripple(ctx),
            Effect::Discover(n) => self.cascade(ctx, Some(*n)),
            Effect::Exploit => self.exploit(ctx),
            Effect::MoveSourceCounters { to } => {
                let n = self
                    .objects
                    .get(ctx.source)
                    .map(|o| o.counters(CounterKind::PlusOnePlusOne))
                    .or_else(|| {
                        self.lki
                            .get(&ctx.source)
                            .map(|l| l.object.counters(CounterKind::PlusOnePlusOne))
                    })
                    .unwrap_or(0);
                if n > 0 {
                    for t in self.recipients(ctx, *to) {
                        if let Some(id) = on_battlefield(self, t) {
                            self.add_counters(id, CounterKind::PlusOnePlusOne, n);
                        }
                    }
                }
            }
            Effect::Soulbond { triggering } => self.soulbond(ctx, *triggering),
            Effect::CumulativeUpkeep(cost) => {
                let Some(id) = self
                    .current_object(ctx.source)
                    .filter(|s| self.objects[*s].zone == Zone::Battlefield)
                else {
                    return;
                };
                self.add_counters(id, CounterKind::Age, 1);
                let n = self.objects[id].counters(CounterKind::Age);
                let mut req = ManaRequirement::default();
                for _ in 0..n {
                    req.add(&cost.requirements(0));
                }
                let p = ctx.controller;
                let paid = self.can_pay(p, &req)
                    && self.choose_mid(
                        p,
                        DecisionKind::YouMay {
                            def: self.objects[id].def,
                        },
                        vec![Choice::Accept, Choice::Decline],
                    ) == 0;
                if paid {
                    self.pay(p, &req);
                } else {
                    self.sacrifice(id);
                }
            }
            Effect::Adapt(n) => {
                if let Some(id) = self.current_object(ctx.source)
                    && self.objects[id].zone == Zone::Battlefield
                    && self.objects[id].counters(CounterKind::PlusOnePlusOne) == 0
                {
                    self.add_counters(id, CounterKind::PlusOnePlusOne, *n);
                }
            }
            Effect::Bolster(n) => {
                let n = self.eval_num(*n, ctx);
                self.bolster(ctx, n);
            }
            Effect::Populate => self.populate(ctx),
            Effect::ExchangeControl { a, b } => {
                let mut objs: SmallVec<[ObjId; 2]> = SmallVec::new();
                for r in std::iter::once(*a).chain(*b) {
                    for t in self.recipients(ctx, r) {
                        match on_battlefield(self, t) {
                            Some(id) => objs.push(id),
                            // CR 701.12a: if the whole exchange can't be
                            // completed, none of it happens.
                            None => return,
                        }
                    }
                }
                let [x, y] = objs[..] else {
                    return;
                };
                let (cx, cy) = (self.objects[x].controller, self.objects[y].controller);
                // CR 701.12b: the same controller: nothing happens.
                if cx == cy {
                    return;
                }
                for (id, to) in [(x, cy), (y, cx)] {
                    let timestamp = self.tick();
                    self.effects.push(ContinuousEffect {
                        source: Some(ctx.source),
                        controller: to,
                        timestamp,
                        duration: Duration::Permanent,
                        affected: Affected::Objects(smallvec::smallvec![id]),
                        parts: smallvec::smallvec![Modification::Control],
                    });
                    self.control_effects += 1;
                }
                self.cbump();
                self.sync_control();
            }
            Effect::TokenPtX(n) => {
                self.token_pt = Some(i32::try_from(self.eval_num(*n, ctx)).unwrap_or(i32::MAX));
            }
            Effect::ExchangeLife { with } => {
                ctx.did = false;
                let me = ctx.controller;
                let Some(them) = self
                    .recipients(ctx, *with)
                    .into_iter()
                    .find_map(|t| match t {
                        Target::Player(o) if o != me && self.player(o).lost.is_none() => Some(o),
                        _ => None,
                    })
                else {
                    return;
                };
                let (mine, theirs) = (self.player(me).life, self.player(them).life);
                // CR 701.12c: each gains or loses the life necessary to
                // equal the other's former total.
                let (loser, gainer, diff) = match mine.cmp(&theirs) {
                    std::cmp::Ordering::Equal => return,
                    std::cmp::Ordering::Greater => (me, them, mine - theirs),
                    std::cmp::Ordering::Less => (them, me, theirs - mine),
                };
                self.change_life(loser, -diff);
                self.gain_life(gainer, diff.unsigned_abs());
                ctx.did = loser == me;
            }
            Effect::GiveControl { of, to } => {
                ctx.did = false;
                let Some(p) = self.recipients(ctx, *to).into_iter().find_map(|t| match t {
                    Target::Player(p) if self.player(p).lost.is_none() => Some(p),
                    _ => None,
                }) else {
                    return;
                };
                let mut given = false;
                for t in self.recipients(ctx, *of) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    if self.objects[id].controller == p {
                        continue;
                    }
                    let timestamp = self.tick();
                    self.effects.push(ContinuousEffect {
                        source: Some(ctx.source),
                        controller: p,
                        timestamp,
                        duration: Duration::Permanent,
                        affected: Affected::Objects(smallvec::smallvec![id]),
                        parts: smallvec::smallvec![Modification::Control],
                    });
                    self.control_effects += 1;
                    given = true;
                }
                if given {
                    self.cbump();
                    self.sync_control();
                }
                // "If they do, you draw a card.": whether it changed hands.
                ctx.did = given;
            }
            Effect::EndTurn => {
                // CR 724.1a: triggered abilities not yet on the stack cease
                // to exist.
                self.pending_triggers.clear();
                // CR 724.1b: every object on the stack is exiled, the
                // resolving one included; an ability or a copy of a spell
                // ceases to exist.
                for id in self.stack.clone().into_iter().rev() {
                    let o = &self.objects[id];
                    if o.kind == ObjKind::Ability || o.cast.copy() {
                        self.remove_from_game(id);
                    } else {
                        self.move_object(id, Zone::Exile, true);
                    }
                }
                self.end_turn = true;
            }
            Effect::Incubate { count, token } => {
                // CR 701.53a: it enters with the counters (CR 122.6).
                let n = self.eval_num(*count, ctx);
                // CR 614.1a: token-creation replacement effects apply; each
                // Incubator enters with the counters.
                for (d, k) in self.tokens_to_create(ctx.controller, *token, 1) {
                    for _ in 0..k {
                        let counters =
                            (d == *token && n > 0).then_some((CounterKind::PlusOnePlusOne, n));
                        let new = self.create_token_entering(
                            d,
                            ctx.controller,
                            super::replace::Enter {
                                tapped: false,
                                counters,
                                ..super::replace::Enter::default()
                            },
                        );
                        if self.objects.contains_key(new) {
                            ctx.created = Some(new);
                        }
                    }
                }
            }
            Effect::Fabricate { count, token } => self.fabricate(ctx, *count, *token),
            Effect::BecomeRenowned => {
                // CR 702.112b: only a permanent; one that has left the
                // battlefield doesn't become renowned.
                if let Some(o) = self.objects.get_mut(ctx.source)
                    && o.zone == Zone::Battlefield
                {
                    o.marks |= crate::state::OBJ_RENOWNED;
                }
            }
            Effect::BecomePrepared { what } => {
                for t in self.recipients(ctx, *what) {
                    if let Target::Object(id) = t {
                        self.become_prepared(id);
                    }
                }
            }
            Effect::Suspect { what, on } => {
                for t in self.recipients(ctx, *what) {
                    if let Some(id) = on_battlefield(self, t) {
                        // CR 701.60b: a permanent already suspected can't
                        // become suspected again; nothing changes.
                        if *on {
                            self.objects[id].marks |= crate::state::OBJ_SUSPECTED;
                        } else {
                            self.objects[id].marks &= !crate::state::OBJ_SUSPECTED;
                        }
                    }
                }
                self.cbump();
            }
            Effect::BlightBy { who, n } => {
                for t in self.recipients(ctx, *who) {
                    let Target::Player(p) = t else { continue };
                    let options: Vec<Choice> = self
                        .creatures_of(p)
                        .map(|id| Choice::Target(Target::Object(id)))
                        .collect();
                    if options.is_empty() {
                        continue;
                    }
                    let k = self.choose_mid(p, DecisionKind::ChooseCard, options.clone());
                    if let Choice::Target(Target::Object(id)) = options[k] {
                        self.add_counters(id, CounterKind::MinusOneMinusOne, *n);
                    }
                }
            }
            Effect::DoublePt { to, toughness } => {
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    let Some(c) = self.characteristics(id) else {
                        continue;
                    };
                    // CR 701.10b, c: +X/+0 with X its power, which may be
                    // negative.
                    let power = i32::from(c.power);
                    let tough = if *toughness {
                        i32::from(c.toughness)
                    } else {
                        0
                    };
                    self.effect_until_end_of_turn(
                        ctx,
                        id,
                        smallvec::smallvec![Modification::ModifyPt {
                            power,
                            toughness: tough,
                        }],
                    );
                }
            }
            Effect::DoubleCounters { kind, to } => {
                for t in self.recipients(ctx, *to) {
                    let Some(id) = on_battlefield(self, t) else {
                        continue;
                    };
                    // CR 701.10e: as many as it already has, each kind.
                    let have: SmallVec<[(CounterKind, u32); 2]> = self.objects[id]
                        .counters
                        .iter()
                        .copied()
                        .filter(|(k, n)| *n > 0 && kind.is_none_or(|w| w == *k))
                        .collect();
                    for (k, n) in have {
                        self.add_counters(id, k, n);
                    }
                }
            }
            Effect::DoubleLife { who } => {
                for t in self.recipients(ctx, *who) {
                    if let Target::Player(p) = t {
                        // CR 701.10d: gain or lose so the total is twice
                        // what it was.
                        let life = self.player(p).life;
                        if life > 0 {
                            self.gain_life(p, life.unsigned_abs());
                        } else if life < 0 {
                            self.change_life(p, life);
                        }
                    }
                }
            }
            Effect::SpendOnly {
                effects,
                restriction,
            } => {
                let p = ctx.controller;
                let before = self.player(p).mana_pool;
                for e in effects {
                    self.apply_effect(ctx, e);
                }
                let r = Box::new(self.filled_restriction(restriction, ctx.source));
                let after = self.player(p).mana_pool;
                let mut made: SmallVec<[Mana; 8]> = SmallVec::new();
                for (c, color) in Color::ALL.into_iter().enumerate() {
                    for _ in 0..after.colored[c].saturating_sub(before.colored[c]) {
                        made.push(Mana::Colored(color));
                    }
                }
                for _ in 0..after.colorless.saturating_sub(before.colorless) {
                    made.push(Mana::Colorless);
                }
                // The mana goes into the pool restricted (CR 106.6).
                self.player_mut(p).mana_pool = before.min_each(&after);
                for mana in made {
                    self.restricted_pool.push(crate::state::RestrictedMana {
                        player: p,
                        mana,
                        restriction: r.clone(),
                    });
                }
                self.bump();
            }
            Effect::KeepMana(inner) => {
                let before = self.player(ctx.controller).mana_pool;
                for e in inner {
                    self.apply_effect(ctx, e);
                }
                let ps = self.player_mut(ctx.controller);
                let after = ps.mana_pool;
                for c in 0..5 {
                    ps.turn_mana.colored[c] += after.colored[c].saturating_sub(before.colored[c]);
                }
                ps.turn_mana.colorless += after.colorless.saturating_sub(before.colorless);
            }
            Effect::Firebend(n) => {
                let n = self.eval_num(*n, ctx);
                let ps = self.player_mut(ctx.controller);
                ps.mana_pool.colored[crate::mana::Color::Red as usize] += n;
                ps.combat_mana += n;
                self.bump();
            }
            Effect::GraveyardToBattlefield { filter, tapped } => {
                let p = ctx.controller;
                // "each creature card with mana value X": X of the spell.
                let filter = &*self.chosen_filter(filter, ctx);
                let cards: Vec<ObjId> = self
                    .player(p)
                    .graveyard
                    .iter()
                    .copied()
                    .filter(|id| {
                        let o = &self.objects[*id];
                        let c = self.base_characteristics(o);
                        // "with mana value 2 or less": the mana value
                        // `matches` leaves to the caller.
                        filter.matches(*id, p, &c, Some(ctx.source), Some(ctx.controller))
                            && self.status_ok(filter, *id)
                    })
                    .collect();
                for id in cards {
                    let owner = self.objects[id].owner;
                    // CR 303.4f: an Aura put onto the battlefield this way
                    // is attached to an object its owner chooses that it
                    // could enchant; with none, it stays in the graveyard.
                    let host = match self.pool.get(self.objects[id].def).enchant() {
                        Some(spec) => {
                            let options: Vec<Choice> = self
                                .battlefield
                                .iter()
                                .copied()
                                .filter(|h| self.is_legal_attachment(spec, *h))
                                .map(|h| Choice::Target(Target::Object(h)))
                                .collect();
                            if options.is_empty() {
                                continue;
                            }
                            let k =
                                self.choose_mid(owner, DecisionKind::ChooseCard, options.clone());
                            match options[k] {
                                Choice::Target(Target::Object(h)) => Some(h),
                                _ => None,
                            }
                        }
                        None => None,
                    };
                    let new = self.put_onto_battlefield_with(
                        id,
                        owner,
                        super::replace::Enter {
                            tapped: *tapped,
                            ..super::replace::Enter::default()
                        },
                    );
                    if let Some(h) = host
                        && self
                            .objects
                            .get(new)
                            .is_some_and(|o| o.zone == Zone::Battlefield)
                        && self.objects.contains_key(h)
                    {
                        self.objects[new].attached_to = Some(h);
                        self.cbump();
                    }
                    ctx.created = Some(new);
                }
            }
            Effect::CloakTop { of } => {
                // CR 701.58a: face down as a 2/2 creature with ward {2}.
                for t in self.recipients(ctx, *of) {
                    let Target::Player(p) = t else { continue };
                    let Some(&top) = self.player(p).library.last() else {
                        continue;
                    };
                    self.objects[top].def = self.pool.face_down(true);
                    let new = self.put_onto_battlefield(top, ctx.controller);
                    ctx.created = Some(new);
                }
            }
            Effect::Airbend { what } => {
                // CR 701.65a: exile the permanents and/or spells; each card
                // exiled this way may be cast by its owner for {2} rather
                // than its mana cost for as long as it remains exiled. A
                // token or a copy of a spell ceases to exist (CR 111.7,
                // 707.10a).
                for t in self.recipients(ctx, *what) {
                    let Target::Object(id) = t else { continue };
                    let Some(o) = self.objects.get(id) else {
                        continue;
                    };
                    if !matches!(o.zone, Zone::Battlefield | Zone::Stack) {
                        continue;
                    }
                    let card = o.kind == crate::state::ObjKind::Card
                        && !(o.zone == Zone::Stack && o.cast.copy());
                    let new = self.move_object(id, Zone::Exile, true);
                    ctx.created = Some(new);
                    if card
                        && let Some(n) = self.objects.get_mut(new)
                        && n.zone == Zone::Exile
                    {
                        n.alt = AltCast::Airbent;
                        let owner = n.owner;
                        self.player_mut(owner).adventures_in_exile += 1;
                    }
                }
            }
            Effect::Recruit { token } => {
                // CR 701.70a: draw, then discard; a 1/1 white Human
                // Soldier if the discarded card was a nonland card.
                let effects = [
                    Effect::DrawCards {
                        count: crate::card::Num::N(1),
                        who: crate::card::Recipient::You,
                    },
                    Effect::Discard {
                        count: crate::card::Num::N(1),
                        who: crate::card::Recipient::You,
                    },
                    Effect::IfThisWay {
                        cond: crate::card::ThisWayCond {
                            verb: crate::card::ThisWay::Discarded,
                            filter: Some(Box::new(crate::card::ObjFilter {
                                not_types: crate::card::TypeSet::of([crate::card::CardType::Land]),
                                ..crate::card::ObjFilter::default()
                            })),
                            negated: false,
                            by_you: true,
                        },
                        then: vec![Effect::CreateToken {
                            token: *token,
                            count: crate::card::Num::N(1),
                            who: crate::card::Recipient::You,
                            tapped: false,
                            attached: None,
                        }],
                    },
                ];
                for e in &effects {
                    self.apply_effect(ctx, e);
                }
            }
            Effect::Earthbend { count, to } => {
                // CR 701.66a: the land becomes a 0/0 creature with haste
                // in addition to its other types, gets N +1/+1 counters,
                // and returns tapped when it dies or is put into exile.
                let effects = [
                    Effect::Animate {
                        to: *to,
                        types: crate::card::TypeSet::of([crate::card::CardType::Creature]),
                        subtypes: crate::card::SubtypeSet::default(),
                        pt: Some((0, 0)),
                        colors: None,
                        keywords: crate::card::KeywordSet::NONE.with(Keyword::Haste),
                        indefinite: true,
                        loses_abilities: false,
                        in_addition: true,
                    },
                    Effect::AddCounters {
                        kind: CounterKind::PlusOnePlusOne,
                        count: *count,
                        to: *to,
                    },
                    Effect::Delayed {
                        ability: self.pool.builtin(crate::card::Builtin::EarthbendReturn),
                        about: *to,
                        this_turn: false,
                    },
                ];
                for e in &effects {
                    self.apply_effect(ctx, e);
                }
            }
            Effect::Monstrosity(n) => {
                // CR 701.37a: only a permanent that isn't monstrous; one
                // that has left the battlefield does nothing.
                let n = self.eval_num(*n, ctx);
                if let Some(id) = self.current_object(ctx.source)
                    && self.objects[id].zone == Zone::Battlefield
                    && !self.objects[id].monstrous()
                {
                    self.add_counters(id, CounterKind::PlusOnePlusOne, n);
                    self.objects[id].marks |= crate::state::OBJ_MONSTROUS;
                    // "As long as this creature is monstrous" (CR 701.37b).
                    self.cbump();
                }
            }
            Effect::Connive { what } => {
                for t in self.recipients(ctx, *what) {
                    let Target::Object(id) = t else { continue };
                    let p = self.objects[id].controller;
                    self.draw(p);
                    let options: Vec<Choice> = self
                        .player(p)
                        .hand
                        .iter()
                        .map(|c| Choice::Discard {
                            card: *c,
                            def: self.objects[*c].def,
                        })
                        .collect();
                    if options.is_empty() {
                        continue;
                    }
                    let k = self.choose_mid(p, DecisionKind::Discard, options.clone());
                    if let Choice::Discard { card, def } = options[k] {
                        let nonland = !self.pool.get(def).is_land();
                        self.discard_card(p, card);
                        if nonland && self.objects.contains_key(id) {
                            self.add_counters(id, CounterKind::PlusOnePlusOne, 1);
                        }
                    }
                }
            }
            Effect::Dig {
                count,
                filter,
                optional,
                all,
                rest_graveyard,
                take,
                also,
                random,
                top: back_on_top,
                onto,
            } => {
                let p = ctx.controller;
                let n = self.eval_num(*count, ctx) as usize;
                let lib = &self.player(p).library;
                let top: Vec<ObjId> = lib.iter().rev().take(n).copied().collect();
                let matches = |s: &Self, f: Option<&crate::card::ObjFilter>, id: ObjId| {
                    f.is_none_or(|f| {
                        let c = s.base_characteristics(&s.objects[id]);
                        f.matches(id, p, &c, Some(ctx.source), Some(p)) && s.status_ok(f, id)
                    })
                };
                let mut taken: Vec<ObjId> = Vec::new();
                if *all {
                    taken = top
                        .iter()
                        .copied()
                        .filter(|id| matches(self, filter.as_ref(), *id))
                        .collect();
                } else {
                    // Up to `take` cards matching the filter, one at a time,
                    // then one matching `also` ("a creature card and/or a
                    // land card").
                    let picks: Vec<(Option<&crate::card::ObjFilter>, bool)> =
                        std::iter::repeat_n(filter.as_ref(), (*take as usize).min(top.len()))
                            .map(|f| (f, *optional))
                            .chain(also.as_ref().map(|f| (Some(f), true)))
                            .collect();
                    for (f, may) in picks {
                        let mut options: Vec<Choice> = top
                            .iter()
                            .copied()
                            .filter(|id| !taken.contains(id) && matches(self, f, *id))
                            .map(|id| Choice::Target(Target::Object(id)))
                            .collect();
                        if options.is_empty() {
                            if also.is_some() {
                                continue;
                            }
                            break;
                        }
                        if may {
                            options.push(Choice::Decline);
                        }
                        let k = self.choose_mid(p, DecisionKind::SearchLibrary, options.clone());
                        match options[k] {
                            Choice::Target(Target::Object(id)) => taken.push(id),
                            _ if also.is_some() => {}
                            _ => break,
                        }
                    }
                }
                // "back on top of your library": the taken cards stay where
                // they are, on top once the rest have gone.
                if let Some(tapped) = *onto {
                    // CR 110.2a: under the controller's control.
                    for id in &taken {
                        let new = self.put_onto_battlefield_with(
                            *id,
                            p,
                            super::replace::Enter {
                                tapped,
                                counters: None,
                                ..super::replace::Enter::default()
                            },
                        );
                        ctx.created = Some(new);
                    }
                } else if !*back_on_top {
                    for id in &taken {
                        self.move_object(*id, Zone::Hand, false);
                    }
                }
                let mut top = top;
                if *random && !*rest_graveyard {
                    use rand::seq::SliceRandom;
                    top.shuffle(&mut self.rng);
                }
                for id in top {
                    if taken.contains(&id)
                        || !self
                            .objects
                            .get(id)
                            .is_some_and(|o| o.zone == Zone::Library)
                    {
                        continue;
                    }
                    if *rest_graveyard {
                        self.move_object(id, Zone::Graveyard, false);
                    } else {
                        self.move_object(id, Zone::Library, false);
                    }
                }
            }
            // CR 702.185a: the warped permanent is exiled; its owner may
            // cast it from exile on a later turn.
            Effect::WarpExile => {
                for t in self.recipients(ctx, crate::card::Recipient::SameObject) {
                    let Target::Object(id) = t else {
                        continue;
                    };
                    if !self
                        .objects
                        .get(id)
                        .is_some_and(|o| o.zone == Zone::Battlefield)
                    {
                        continue;
                    }
                    let owner = self.objects[id].owner;
                    let new = self.move_object(id, Zone::Exile, false);
                    if self.objects.get(new).is_some_and(|o| o.zone == Zone::Exile) {
                        self.exile_casts
                            .get_or_insert_with(Box::default)
                            .playable
                            .push(crate::state::PlayableExile {
                                card: new,
                                player: owner,
                                until: crate::card::PlayUntil::Forever,
                                own_ends: crate::state::WARP_LATER,
                                cast_only: true,
                                free: false,
                                source: ObjId::default(),
                            });
                    }
                }
            }
            Effect::ExilePlayable {
                count,
                until,
                choose,
                from,
            } => {
                let p = ctx.controller;
                // "Target opponent exiles the top card of their library":
                // their library; the permission is still the controller's.
                let lib = match from {
                    Some(w) => match self.recipients(ctx, *w).first() {
                        Some(Target::Player(q)) => *q,
                        _ => return,
                    },
                    None => p,
                };
                // "Until the end of your next turn": through this turn too
                // when it's already the controller's.
                let own_ends = if self.active_player == p { 2 } else { 1 };
                let n = self.eval_num(*count, ctx) as usize;
                let top: Vec<ObjId> = self
                    .player(lib)
                    .library
                    .iter()
                    .rev()
                    .take(n)
                    .copied()
                    .collect();
                // "Choose one of them.": only that one may be played. The
                // choice names the cards as they were before they moved,
                // the objects the resolution began with; they are exiled
                // all the same.
                let chosen = if *choose && top.len() > 1 {
                    let options: Vec<Choice> = top
                        .iter()
                        .map(|id| Choice::Target(Target::Object(*id)))
                        .collect();
                    Some(top[self.choose_mid(p, DecisionKind::ChooseCard, options)])
                } else {
                    None
                };
                let mut exiled: Vec<ObjId> = Vec::new();
                for id in top {
                    let new = self.move_object(id, Zone::Exile, true);
                    if self.objects.get(new).is_some_and(|o| o.zone == Zone::Exile)
                        && chosen.is_none_or(|c| c == id)
                    {
                        exiled.push(new);
                    }
                }
                for new in exiled {
                    self.exile_casts
                        .get_or_insert_with(Box::default)
                        .playable
                        .push(crate::state::PlayableExile {
                            card: new,
                            player: p,
                            until: *until,
                            own_ends,
                            cast_only: false,
                            free: false,
                            source: ctx.source,
                        });
                }
            }
            Effect::Manifest => {
                let p = ctx.controller;
                if let Some(top) = self.player(p).library.last().copied() {
                    // Face down before it moves, so nothing sees the card's
                    // own characteristics enter (CR 701.34a).
                    self.objects[top].def = self.pool.face_down(false);
                    let new = self.put_onto_battlefield(top, p);
                    ctx.created = Some(new);
                }
            }
            Effect::Blight(n) => {
                // CR 701.68: a creature of the controller's choice.
                let p = ctx.controller;
                let options: Vec<Choice> = self
                    .creatures_of(p)
                    .map(|id| Choice::Target(Target::Object(id)))
                    .collect();
                if options.is_empty() {
                    ctx.did = false;
                } else {
                    let k = self.choose_mid(p, DecisionKind::ChooseCard, options.clone());
                    if let Choice::Target(Target::Object(id)) = options[k] {
                        self.add_counters(id, CounterKind::MinusOneMinusOne, *n);
                    }
                }
            }
            Effect::CollectEvidence(n) => {
                ctx.did = self.collect_evidence(ctx.controller, *n, ctx.source);
            }
            Effect::Forage => {
                let def = self
                    .objects
                    .get(ctx.source)
                    .or_else(|| self.lki.get(&ctx.source).map(|l| &l.object))
                    .map(|o| o.def);
                ctx.did = self.forage(ctx.controller, ctx.source, def);
            }
            Effect::ReturnTransformed => {
                // CR 702.167a, 712.14: the card exiled to pay the cost
                // returns with its back face up, under its owner's control.
                if let Some(id) = self.current_object(ctx.source)
                    && self.objects[id].zone == Zone::Exile
                {
                    let card = self.objects[id].card;
                    let d = self.pool.get(card);
                    if d.layout == Layout::Transform
                        && let Some(back) = d.faces.first().copied()
                    {
                        let owner = self.objects[id].owner;
                        self.objects[id].def = back;
                        let new = self.put_onto_battlefield(id, owner);
                        ctx.created = Some(new);
                    }
                }
            }
            Effect::ManifestDread => {
                // CR 701.62a: look at the top two, manifest one, the rest
                // into the graveyard.
                let p = ctx.controller;
                let top: Vec<ObjId> = self
                    .player(p)
                    .library
                    .iter()
                    .rev()
                    .take(2)
                    .copied()
                    .collect();
                if let Some(&first) = top.first() {
                    let pick = if top.len() > 1 {
                        let options: Vec<Choice> = top
                            .iter()
                            .map(|id| Choice::Target(Target::Object(*id)))
                            .collect();
                        top[self.choose_mid(p, DecisionKind::ChooseCard, options)]
                    } else {
                        first
                    };
                    self.objects[pick].def = self.pool.face_down(false);
                    let new = self.put_onto_battlefield(pick, p);
                    ctx.created = Some(new);
                    for id in top {
                        if id != pick {
                            self.move_object(id, Zone::Graveyard, true);
                        }
                    }
                }
            }
            Effect::ClearEcho => {
                if let Some(id) = self.current_object(ctx.source)
                    && let Some(o) = self.objects.get_mut(id)
                {
                    o.marks &= !crate::state::OBJ_ECHO_DUE;
                }
            }
            Effect::BecomeSaddled => {
                if let Some(id) = self.current_object(ctx.source)
                    && self.objects[id].zone == Zone::Battlefield
                    && !self.saddled.contains(&id)
                {
                    self.saddled.push(id);
                    self.bump();
                    self.emit(Event::BecameSaddled { obj: id });
                }
            }
            Effect::BecomeSaddledTarget { what } => {
                for t in self.recipients(ctx, *what) {
                    if let Some(id) = on_battlefield(self, t)
                        && !self.saddled.contains(&id)
                    {
                        self.saddled.push(id);
                        self.bump();
                        self.emit(Event::BecameSaddled { obj: id });
                    }
                }
            }
            Effect::CreatedAttack | Effect::EnteredAttack => {
                let defender = self
                    .combat
                    .as_ref()
                    .and_then(|c| c.attacker(ctx.source))
                    .map(|a| a.attacking)
                    .or_else(|| {
                        self.players_in_game()
                            .find(|q| *q != ctx.controller)
                            .map(Target::Player)
                    });
                let entered: Vec<ObjId> = if matches!(effect, Effect::EnteredAttack) {
                    ctx.created.into_iter().collect()
                } else {
                    ctx.created_tokens.to_vec()
                };
                if let Some(d) = defender {
                    for id in entered {
                        if self
                            .objects
                            .get(id)
                            .is_some_and(|o| o.zone == Zone::Battlefield)
                        {
                            self.enter_attacking(id, d);
                        }
                    }
                }
            }
            Effect::EnterAttacking => {
                let at = self
                    .ninjutsu
                    .iter()
                    .position(|(s, _)| *s == ctx.source)
                    .map(|i| self.ninjutsu.remove(i).1);
                // CR 702.49: the card enters only if it's still in the hand.
                let Some(o) = self.objects.get(ctx.source) else {
                    return;
                };
                if o.zone != Zone::Hand {
                    return;
                }
                let new = self.put_onto_battlefield_with(
                    ctx.source,
                    ctx.controller,
                    super::replace::Enter {
                        tapped: true,
                        counters: None,
                        ..super::replace::Enter::default()
                    },
                );
                ctx.created = Some(new);
                let defender = at.unwrap_or_else(|| {
                    Target::Player(
                        self.players_in_game()
                            .find(|q| *q != ctx.controller)
                            .unwrap_or(ctx.controller),
                    )
                });
                self.enter_attacking(new, defender);
            }
            Effect::CreateTokenCopy { of, except, count } => {
                let x = except.then(|| {
                    self.objects
                        .get(ctx.source)
                        .map(|o| o.card)
                        .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.card))
                        .expect("a resolving spell or its last known information exists")
                });
                let n = self.eval_num(*count, ctx);
                ctx.created_tokens.clear();
                // "When this creature dies, ... create a token that's a copy
                // of it": the copiable values it last had on the
                // battlefield (CR 111.12, 707.2).
                let left = match of {
                    Recipient::Source => Some(ctx.source),
                    Recipient::TriggeringObject => ctx.trigger_obj,
                    _ => None,
                }
                .filter(|id| {
                    self.objects
                        .get(*id)
                        .is_none_or(|o| o.zone != Zone::Battlefield)
                });
                if let Some(left) = left {
                    if let Some(l) = self
                        .lki
                        .get(&left)
                        .filter(|l| l.object.zone == Zone::Battlefield)
                    {
                        let (def, was, sized) = (l.object.def, l.object.except, l.object.sized);
                        for (d, k) in self.tokens_to_create(ctx.controller, def, n) {
                            for _ in 0..k {
                                let new = if d == def {
                                    self.create_token_copy_of(def, x.or(was), sized, ctx.controller)
                                } else {
                                    self.create_token(d, ctx.controller)
                                };
                                ctx.created = Some(new);
                                ctx.created_tokens.push(new);
                            }
                        }
                    }
                    return;
                }
                let recipients = self.recipients(ctx, *of);
                for t in recipients {
                    if let Some(id) = on_battlefield(self, t) {
                        // CR 614.1a: token-creation replacement effects
                        // ("twice that many of those tokens").
                        let def = self.objects[id].def;
                        for (d, k) in self.tokens_to_create(ctx.controller, def, n) {
                            for _ in 0..k {
                                let new = if d == def {
                                    self.create_token_copy(id, ctx.controller, x)
                                } else {
                                    self.create_token(d, ctx.controller)
                                };
                                ctx.created = Some(new);
                                ctx.created_tokens.push(new);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Proliferate (CR 701.27) with the choices a player would make: one
    /// more of each kind of counter on their own permanents (+1/+1,
    /// loyalty), on opponents' permanents with stun counters or counters
    /// that lower power and toughness (-1/-1, -0/-1), and on
    /// opponents with poison counters.
    fn proliferate(&mut self, p: PlayerId) {
        let ids: Vec<ObjId> = self.battlefield.clone();
        for id in ids {
            let o = &self.objects[id];
            let mine = o.controller == p;
            let kinds: SmallVec<[CounterKind; 2]> = o
                .counters
                .iter()
                .filter(|(k, n)| {
                    *n > 0
                        && if *k == CounterKind::Stun || k.pt().is_some_and(|(p, t)| p + t < 0) {
                            !mine
                        } else {
                            mine
                        }
                })
                .map(|(k, _)| *k)
                .collect();
            for k in kinds {
                self.add_counters(id, k, 1);
            }
        }
        let opponents: SmallVec<[PlayerId; 2]> =
            self.players_in_game().filter(|q| *q != p).collect();
        for q in opponents {
            if self.player(q).poison > 0 {
                self.add_poison(q, 1);
            }
        }
    }

    /// The creature `id` explores (CR 701.44).
    /// Clash with an opponent (CR 701.23): `p` and an opponent each reveal
    /// the top card of their library, then each, starting with the active
    /// player (CR 101.4), puts theirs on the top or bottom. `p` wins if
    /// their card's mana value is greater than the opponent's (or the
    /// opponent revealed nothing, CR 701.23b, c).
    pub(super) fn clash(&mut self, p: PlayerId) -> bool {
        let mut opp = self.next_player(p);
        while opp != p && self.player(opp).lost.is_some() {
            opp = self.next_player(opp);
        }
        let mut revealed: SmallVec<[(PlayerId, ObjId, u32); 2]> = SmallVec::new();
        for who in [p, opp] {
            if who == p && !revealed.is_empty() {
                continue;
            }
            if let Some(top) = self.player(who).library.last().copied() {
                let def = self.objects[top].def;
                self.emit(Event::Revealed { obj: top, def });
                revealed.push((who, top, self.pool.get(def).mana_cost.mana_value()));
            }
        }
        let mine = revealed.iter().find(|r| r.0 == p).map(|r| r.2);
        let theirs = revealed.iter().find(|r| r.0 != p).map(|r| r.2);
        let won = match (mine, theirs) {
            (Some(m), Some(t)) => m > t,
            (Some(_), None) => opp != p,
            (None, _) => false,
        };
        revealed.sort_by_key(|r| r.0 != self.active_player);
        for (who, card, _) in revealed {
            let def = self.objects[card].def;
            let top = self.choose_mid(
                who,
                DecisionKind::TopOrBottom { def },
                vec![Choice::Accept, Choice::Decline],
            ) == 0;
            if !top {
                let lib = &mut self.player_mut(who).library;
                if let Some(i) = lib.iter().position(|c| *c == card) {
                    let c = lib.remove(i);
                    lib.insert(0, c);
                }
            }
        }
        won
    }

    /// CR 701.63a: `id` endures `n`: its controller chooses `n` +1/+1
    /// counters on it (`choose "1"`) or an `n`/`n` white Spirit token
    /// (`choose "2"`). One that has left the battlefield gets no
    /// counters; its last controller creates the token. Endure 0 does
    /// nothing (CR 701.63b).
    #[cold]
    #[inline(never)]
    fn endure(&mut self, ctx: &mut EffectCtx, id: ObjId, n: u32, token: crate::card::CardRef) {
        if n == 0 {
            return;
        }
        let here = self
            .objects
            .get(id)
            .filter(|o| o.zone == Zone::Battlefield)
            .map(|o| o.controller);
        let Some(p) = here.or_else(|| self.lki.get(&id).map(|l| l.object.controller)) else {
            return;
        };
        let def = self
            .objects
            .get(ctx.source)
            .or_else(|| self.lki.get(&ctx.source).map(|l| &l.object))
            .map_or(token, |o| o.def);
        let counters = here.is_some()
            && self.choose_mid(
                p,
                DecisionKind::ChooseOption { def },
                vec![Choice::Mode(0), Choice::Mode(1)],
            ) == 0;
        if counters {
            self.add_counters(id, CounterKind::PlusOnePlusOne, n);
            return;
        }
        // CR 614.1a: token-creation replacement effects apply.
        for (d, k) in self.tokens_to_create(p, token, 1) {
            for _ in 0..k {
                let new = self.create_token_with(d, p, false);
                // An X/X Spirit: the 0/0 made that size.
                if d == token
                    && self.objects.contains_key(new)
                    && self
                        .pool
                        .get(token)
                        .printed
                        .is_some_and(|c| u32::try_from(c.power).ok() != Some(n))
                {
                    let size = i32::try_from(n).unwrap_or(i32::MAX);
                    self.effect_on(
                        ctx,
                        new,
                        smallvec::smallvec![Modification::SetPt {
                            power: PtValue::Fixed(size),
                            toughness: PtValue::Fixed(size),
                        }],
                        Duration::Permanent,
                    );
                }
                ctx.created = Some(new);
            }
        }
    }

    fn explore(&mut self, ctx: &EffectCtx, id: ObjId) {
        let Some(o) = self.objects.get(id) else {
            return;
        };
        let p = o.controller;
        let Some(top) = self.player(p).library.last().copied() else {
            // CR 701.44b: nothing revealed is a nonland card.
            self.add_counters(id, CounterKind::PlusOnePlusOne, 1);
            return;
        };
        let land = self.pool.get(self.objects[top].def).is_land();
        let top_def = self.objects[top].def;
        self.emit(Event::Revealed {
            obj: top,
            def: top_def,
        });
        if land {
            // The revealed land is known in its new hand too.
            let new = self.move_object(top, Zone::Hand, false);
            if let Some(o) = self.objects.get_mut(new)
                && o.zone == Zone::Hand
            {
                o.revealed = u8::MAX;
            }
            return;
        }
        self.add_counters(id, CounterKind::PlusOnePlusOne, 1);
        // A nonland card left on top stays known there (CR 701.20).
        self.objects[top].revealed = u8::MAX;
        let def = self.objects[top].def;
        let k = self.choose_mid(
            p,
            DecisionKind::YouMay { def },
            vec![Choice::Accept, Choice::Decline],
        );
        if k == 0
            && self
                .objects
                .get(top)
                .is_some_and(|o| o.zone == Zone::Library)
        {
            self.move_object(top, Zone::Graveyard, false);
        }
        let _ = ctx;
    }

    /// `Effect::MoveCounters` (CR 122.5).
    fn move_counters(
        &mut self,
        ctx: &EffectCtx,
        from: Recipient,
        to: Recipient,
        kind: Option<CounterKind>,
        count: Option<u32>,
    ) {
        let on_battlefield = |st: &GameState, t: Target| match t {
            Target::Object(id) => st
                .objects
                .get(id)
                .is_some_and(|o| o.zone == Zone::Battlefield)
                .then_some(id),
            Target::Player(_) => None,
        };
        let dest: Vec<ObjId> = self
            .recipients(ctx, to)
            .into_iter()
            .filter_map(|t| on_battlefield(self, t))
            .collect();
        let src = self
            .recipients(ctx, from)
            .into_iter()
            .find_map(|t| on_battlefield(self, t));
        let counters = match src {
            Some(id) => self.objects[id].counters.clone(),
            // "put its counters on ...": as the source last existed.
            None if from == Recipient::Source => match self.lki.get(&ctx.source) {
                Some(l) => l.object.counters.clone(),
                None => return,
            },
            // "Whenever another creature you control dies, ... put its
            // counters on this creature": as that creature last existed.
            None if from == Recipient::TriggeringObject => {
                match ctx.trigger_obj.and_then(|o| self.lki.get(&o)) {
                    Some(l) => l.object.counters.clone(),
                    None => return,
                }
            }
            None => return,
        };
        if dest.is_empty() {
            return;
        }
        // "Move any number of +1/+1 counters": how many, chosen now (most
        // first, so the engine's default moves them all).
        let count = if count == Some(crate::card::MOVE_ANY_NUMBER) {
            let have: u32 = counters
                .iter()
                .filter(|(k, _)| kind.is_none_or(|x| x == *k))
                .map(|(_, n)| *n)
                .sum();
            let options: Vec<Choice> = (0..=have).rev().map(Choice::X).collect();
            let k = self.choose_mid(
                ctx.controller,
                DecisionKind::ChooseX { spell: ctx.source },
                options,
            );
            Some(have.saturating_sub(u32::try_from(k).unwrap_or(0)))
        } else {
            count
        };
        let mut left = count.unwrap_or(u32::MAX);
        for (k, n) in counters {
            if kind.is_some_and(|x| x != k) {
                continue;
            }
            let m = n.min(left);
            if m == 0 {
                continue;
            }
            left -= m;
            if let Some(id) = src {
                self.remove_counters(id, k, m);
            }
            for d in &dest {
                self.add_counters(*d, k, m);
            }
        }
    }

    /// Reveal a card where it is in a library (CR 701.20): every player
    /// knows it until it moves or the library is shuffled
    /// (`GameObject::revealed`, kept by `determinize`).
    pub(super) fn reveal_in_library(&mut self, id: ObjId) {
        let Some(o) = self.objects.get_mut(id) else {
            return;
        };
        o.revealed = u8::MAX;
        let def = o.def;
        self.emit(Event::Revealed { obj: id, def });
    }

    /// `Effect::PutCard`: move the resolution's card, or its revealed
    /// cards still in their library, to `to`.
    fn put_card(&mut self, ctx: &mut EffectCtx, what: CardSet, to: CardDest, random: bool) {
        let cards: Vec<ObjId> = match what {
            CardSet::It => ctx.created.into_iter().collect(),
            CardSet::Revealed => ctx.revealed.clone(),
            CardSet::OtherRevealed => ctx
                .revealed
                .iter()
                .copied()
                .filter(|id| Some(*id) != ctx.created)
                .collect(),
        };
        let mut cards = cards;
        if random {
            rand::seq::SliceRandom::shuffle(cards.as_mut_slice(), &mut self.rng);
        }
        let mut last = None;
        for id in cards {
            let Some(o) = self.objects.get(id) else {
                continue;
            };
            // "it" may be a card the previous effect moved elsewhere
            // ("Draw a card and reveal it"); revealed cards move only from
            // their library.
            if what != CardSet::It && o.zone != Zone::Library {
                continue;
            }
            let (from, owner, seen) = (o.zone, o.owner, o.revealed);
            let new = match to {
                CardDest::Hand => {
                    if from == Zone::Hand {
                        continue;
                    }
                    let new = self.move_object(id, Zone::Hand, false);
                    // What players saw of it they still know in the hand.
                    if let Some(o) = self.objects.get_mut(new)
                        && o.zone == Zone::Hand
                    {
                        o.revealed = seen;
                    }
                    new
                }
                CardDest::Graveyard => {
                    if from == Zone::Graveyard {
                        continue;
                    }
                    if from == Zone::Hand {
                        // "If it isn't a land card, discard it" (CR 701.9).
                        self.discard_card(owner, id);
                        self.current_object(id).unwrap_or(id)
                    } else {
                        self.move_object(id, Zone::Graveyard, false)
                    }
                }
                CardDest::Battlefield { tapped } => {
                    if from == Zone::Battlefield {
                        continue;
                    }
                    self.put_onto_battlefield_with(
                        id,
                        ctx.controller,
                        super::replace::Enter {
                            tapped,
                            ..super::replace::Enter::default()
                        },
                    )
                }
                CardDest::LibraryBottom => {
                    if from != Zone::Library {
                        continue;
                    }
                    let lib = &mut self.player_mut(owner).library;
                    lib.retain(|x| *x != id);
                    // Each later card goes under the ones before it.
                    lib.insert(0, id);
                    self.bump();
                    id
                }
            };
            last = Some(new);
        }
        if what == CardSet::It {
            ctx.created = last;
        }
        if to != CardDest::LibraryBottom {
            ctx.revealed.retain(|id| {
                self.objects
                    .get(*id)
                    .is_some_and(|o| o.zone == Zone::Library)
            });
        }
    }

    /// "If that creature was a Human" / "If it's a Human": whether the
    /// `n`th target matches, judged as it last existed if it has left
    /// (CR 608.2h).
    fn nth_target_matches(&self, ctx: &EffectCtx, n: u8, filter: &crate::card::ObjFilter) -> bool {
        // What only the permanent's state on the battlefield answers
        // ("If it's an enchanted creature", "if it's attacking").
        fn live_state(f: &crate::card::ObjFilter) -> bool {
            f.attacking
                || f.blocking
                || f.tapped.is_some()
                || f.status.counter.is_some()
                || f.status.suspected.is_some()
                || f.status.enchanted.is_some()
                || f.status.equipped
                || f.status.modified
                || f.status.on.is_some()
        }
        // Converge: "if its mana value is less than or equal to the number
        // of colors of mana spent to cast this spell".
        let filled;
        let filter = if filter.status.spent_colors || filter.status.bound.is_some() {
            let mut g = filter.clone();
            if g.status.spent_colors {
                GameState::fill_spent_colors(&mut g, self.spent_on(ctx.source).colors());
            }
            // "if its toughness is less than or equal to the number of
            // cards in your graveyard": the count as the effect happens.
            if let Some(b) = g.status.bound {
                let v = self.eval_num(b.n, ctx);
                GameState::fill_bound(&mut g, v);
            }
            filled = g;
            &filled
        } else {
            filter
        };
        let Some(Target::Object(id)) = ctx.targets.get(usize::from(n)).copied() else {
            return false;
        };
        // A declined "up to one" target (`Target::none()`) or one that
        // left with no last known information matches nothing.
        let controller = match (self.lki.get(&id), self.objects.get(id)) {
            (Some(l), _) if self.characteristics(id).is_none() => l.object.controller,
            (_, Some(o)) => o.controller,
            (Some(l), None) => l.object.controller,
            (None, None) => return false,
        };
        // "If it was dealt damage this turn": the damage marked on it, as
        // it last existed if it has left (CR 608.2h).
        if filter.damaged {
            let damaged = |o: &crate::state::GameObject| o.damage > 0 || o.deathtouch_damage;
            let on_battlefield = self
                .objects
                .get(id)
                .is_some_and(|o| o.zone == Zone::Battlefield);
            let was = if on_battlefield {
                damaged(&self.objects[id])
            } else {
                self.lki.get(&id).is_some_and(|l| damaged(&l.object))
            };
            if !was {
                return false;
            }
        }
        // "If its mana value was 3 or less": as it last existed (CR
        // 608.2h). A copy's or a face-down permanent's isn't tracked; the
        // card's own is used.
        if let Some((cmp, k)) = filter.mana_value
            && !filter.status.mana_value_x
        {
            let def = match self.objects.get(id) {
                // "Counter target spell if its mana value is ...": on the stack.
                Some(o) if matches!(o.zone, Zone::Battlefield | Zone::Stack) => Some(o.def),
                _ => self.lki.get(&id).map(|l| l.object.def),
            };
            let Some(def) = def else {
                return false;
            };
            if !cmp.holds(self.pool.get(def).mana_cost.mana_value(), u32::from(k)) {
                return false;
            }
        }
        self.characteristics_or_lki(id).is_some_and(|c| {
            filter.matches(id, controller, &c, Some(ctx.source), Some(ctx.controller))
                // "If that creature has toxic" (CR 702.164), as it last
                // existed if it has left.
                && (!filter.status.toxic
                    || c.abilities.iter().any(|a| {
                        matches!(self.pool.ability(*a), crate::card::AbilityDef::Toxic(_))
                    }))
        }) && (!(live_state(filter)
            || filter.or.as_deref().is_some_and(live_state))
            // "If that creature is attacking", "If that creature has a
            // +1/+1 counter on it": its state while it is still on the
            // battlefield.
            || (self
                .objects
                .get(id)
                .is_some_and(|o| o.zone == Zone::Battlefield)
                && self.status_ok(filter, id))
            // "If it was tapped", "If it was attacking": as it last
            // existed on the battlefield (CR 608.2h).
            || (filter.status.counter.is_none()
                && filter.status.suspected.is_none()
                && self
                    .objects
                    .get(id)
                    .is_none_or(|o| o.zone != Zone::Battlefield)
                && self.lki.get(&id).is_some_and(|l| {
                    l.object.zone == Zone::Battlefield
                        && filter.tapped.is_none_or(|t| l.object.tapped == t)
                        && (!filter.attacking || l.attacking)
                        && (!filter.blocking || l.blocking)
                })))
    }

    /// Apply a resolving object's effects in order. For a "both" mode
    /// (CR 700.2), the second mode's effects see their own targets.
    pub(super) fn apply_effects(
        &mut self,
        ctx: &mut EffectCtx,
        effects: &[Effect],
        splits: &[(usize, usize)],
    ) {
        for (i, effect) in effects.iter().enumerate() {
            // A spree mode's third part sees the targets from its own
            // start (CR 702.172a).
            match splits.iter().rev().find(|(e, _)| i >= *e).copied() {
                Some((_, t)) => {
                    let mut second = EffectCtx {
                        source: ctx.source,
                        controller: ctx.controller,
                        targets: ctx.targets.get(t..).unwrap_or(&[]).to_vec(),
                        division: ctx.division.clone(),
                        legal: ctx.legal.get(t..).unwrap_or(&[]).to_vec(),
                        trigger_obj: ctx.trigger_obj,
                        trigger_player: ctx.trigger_player,
                        trigger_amount: ctx.trigger_amount,
                        created: ctx.created,
                        revealed: std::mem::take(&mut ctx.revealed),
                        x: ctx.x,
                        did: ctx.did,
                        chosen: ctx.chosen,
                        couldnt: ctx.couldnt,
                        created_tokens: std::mem::take(&mut ctx.created_tokens),
                        counts: ctx.counts,
                    };
                    self.apply_effect(&mut second, effect);
                    ctx.created = second.created;
                    ctx.created_tokens = second.created_tokens;
                    ctx.revealed = second.revealed;
                    ctx.did = second.did;
                    ctx.chosen = second.chosen;
                }
                _ => self.apply_effect(ctx, effect),
            }
        }
    }

    /// Store a replacement or prevention effect until end of turn (CR 614.3,
    /// 615.3). One aimed at a spell's target is locked to that target
    /// (CR 611.2c) and is not created if the target became illegal.
    /// A regeneration shield on `id` until end of turn (CR 701.15): a
    /// stored replacement of its destruction, used once.
    fn regenerate(&mut self, ctx: &EffectCtx, id: ObjId) {
        let def = self
            .objects
            .get(ctx.source)
            .map(|o| o.def)
            .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
            .expect("a resolving spell or its last known information exists");
        let timestamp = self.tick();
        self.replacements.push(ReplacementEffect {
            source: Some(ctx.source),
            def,
            controller: ctx.controller,
            timestamp,
            duration: Duration::UntilEndOfTurn,
            ability: crate::card::ReplacementAbility {
                replaces: Replaces::Destroy {
                    to: Affects::Target(0),
                },
                replacement: Replacement::Regenerate,
                then: Vec::new(),
                condition: None,
            },
            target: Some(Target::Object(id)),
            remaining: 1,
            chosen: None,
            redirect: None,
        });
        self.bump();
    }

    fn create_replacement(&mut self, ctx: &EffectCtx, r: &crate::card::ReplacementAbility) {
        // A group of targets: one effect per still-legal target, locked to
        // it, a divided shield holding that target's share (CR 601.2d).
        if let Replaces::Damage {
            to: Affects::Targets(first, count),
            ..
        }
        | Replaces::Damage {
            from: Some(Affects::Targets(first, count)),
            ..
        } = r.replaces
        {
            let first = usize::from(first);
            let end = ctx
                .targets
                .len()
                .min(first.saturating_add(usize::from(count)));
            for i in first..end {
                if !ctx.legal.get(i).copied().unwrap_or(false) {
                    continue;
                }
                let share = if count == u8::MAX && !ctx.division.is_empty() {
                    let n = ctx.division.get(i - first).copied().unwrap_or(0);
                    if n == 0 {
                        continue;
                    }
                    Some(n)
                } else {
                    None
                };
                self.store_replacement(ctx, r, Some(ctx.targets[i]), share);
            }
            return;
        }
        // "If a creature dealt damage this way would die this turn" (CR
        // 608.2c): one effect locked to each permanent dealt damage so far.
        if let Replaces::Dies {
            to: Affects::DamagedThisWay,
        } = r.replaces
        {
            let damaged = self
                .this_way
                .as_ref()
                .map_or_else(Vec::new, |log| log.damaged.clone());
            for id in damaged {
                self.store_replacement(ctx, r, Some(Target::Object(id)), None);
            }
            return;
        }
        // The one target the effect is locked to (CR 608.2b: an illegal
        // target means that part of the spell does nothing).
        let target = match &r.replaces {
            Replaces::Damage {
                to: Affects::Target(i),
                ..
            }
            | Replaces::Damage {
                from: Some(Affects::Target(i) | Affects::OtherThanTarget(_, i)),
                ..
            }
            | Replaces::DamageToAndBy {
                what: Affects::Target(i),
                ..
            }
            | Replaces::Dies {
                to: Affects::Target(i),
            } => {
                if !ctx.legal.get(*i).copied().unwrap_or(false) {
                    return;
                }
                ctx.targets.get(*i).copied()
            }
            _ => None,
        };
        self.store_replacement(ctx, r, target, None);
    }

    /// Store `r` until end of turn, locked to `target`; `share` is a
    /// divided shield's part (CR 601.2d).
    fn store_replacement(
        &mut self,
        ctx: &EffectCtx,
        r: &crate::card::ReplacementAbility,
        target: Option<Target>,
        share: Option<u32>,
    ) {
        // CR 614.9 and 608.2b: a redirection to a target that became
        // illegal does nothing.
        let redirect = match r.replacement {
            Replacement::Redirect {
                to: crate::card::RedirectTo::Target(i),
                ..
            } => {
                if !ctx.legal.get(i).copied().unwrap_or(false) {
                    return;
                }
                ctx.targets.get(i).copied()
            }
            _ => None,
        };
        // CR 609.7a: a source of the controller's choice is chosen as the
        // effect is created.
        let chosen = match &r.replaces {
            Replaces::Damage {
                from: Some(Affects::Chosen(f)),
                ..
            } => match self.choose_damage_source(ctx, f) {
                Some(id) => Some(id),
                None => return,
            },
            _ => None,
        };
        let remaining = match r.replacement {
            Replacement::PreventNext(n) => self.eval_num(n, ctx),
            Replacement::Redirect { next: Some(n), .. } => n,
            Replacement::PreventNextTime | Replacement::DestroyNextTime => 1,
            _ => 0,
        };
        let remaining = share.unwrap_or(remaining);
        let def = self
            .objects
            .get(ctx.source)
            .map(|o| o.def)
            .or_else(|| self.lki.get(&ctx.source).map(|l| l.object.def))
            .expect("a resolving spell or its last known information exists");
        let timestamp = self.tick();
        let duration = self.lasting_or(Duration::UntilEndOfTurn, ctx, ctx.source);
        self.replacements.push(ReplacementEffect {
            source: Some(ctx.source),
            def,
            controller: ctx.controller,
            timestamp,
            duration,
            ability: r.clone(),
            target,
            remaining,
            chosen,
            redirect,
        });
        self.bump();
    }

    /// CR 609.7a: the controller chooses a source of damage matching `f`:
    /// a permanent or a spell on the stack. `None` if there is none.
    fn choose_damage_source(
        &mut self,
        ctx: &EffectCtx,
        f: &crate::card::ObjFilter,
    ) -> Option<ObjId> {
        let f = self.chosen_filter(f, ctx);
        let spells = self
            .stack
            .iter()
            .copied()
            .filter(|id| self.objects.get(*id).is_some_and(|o| o.ability.is_none()));
        let cands: Vec<ObjId> = self
            .battlefield
            .iter()
            .copied()
            .chain(spells)
            .filter(|id| {
                let Some(o) = self.objects.get(*id) else {
                    return false;
                };
                self.characteristics(*id).is_some_and(|c| {
                    f.matches(
                        *id,
                        o.controller,
                        &c,
                        Some(ctx.source),
                        Some(ctx.controller),
                    )
                })
            })
            .collect();
        match cands.len() {
            0 => None,
            1 => Some(cands[0]),
            _ => {
                // Offered like other objects an effect acts on as it
                // resolves: a permanent, or a card (a spell on the stack).
                let options: Vec<Choice> = cands
                    .iter()
                    .map(|id| match self.objects[*id].zone {
                        Zone::Battlefield => Choice::Sacrifice(*id),
                        _ => Choice::Discard {
                            card: *id,
                            def: self.objects[*id].def,
                        },
                    })
                    .collect();
                let k = self.choose_mid(ctx.controller, DecisionKind::ChooseCard, options);
                cands.get(k).copied()
            }
        }
    }
}

/// What a resolving spell or ability knows while its effects apply.
pub(super) struct EffectCtx {
    /// The spell, or the ability's source (by the id it had when it
    /// triggered).
    pub source: ObjId,
    pub controller: PlayerId,
    pub targets: Vec<Target>,
    /// Per target of a variable group: the damage or counters the caster
    /// divided onto it (CR 601.2d); empty if nothing is divided.
    pub division: Vec<u32>,
    /// Per target: still legal on resolution (CR 608.2b).
    pub legal: Vec<bool>,
    pub trigger_obj: Option<ObjId>,
    pub trigger_player: Option<PlayerId>,
    /// The triggering event's amount, for "that much".
    pub trigger_amount: u32,
    /// The object the most recent effect of this resolution created or
    /// moved (a token, an exiled card), for "sacrifice it" clauses that
    /// follow (`Recipient::CreatedObject`).
    pub created: Option<ObjId>,
    /// The tokens the most recent token-creating effect of this resolution
    /// created ("those tokens", `Recipient::CreatedTokens`).
    pub created_tokens: SmallVec<[ObjId; 2]>,
    /// The cards revealed this way, in the order revealed, still in their
    /// library ("Put all cards revealed this way into your hand",
    /// `Effect::RevealUntil`).
    pub revealed: Vec<ObjId>,
    /// X of the resolving spell (CR 107.3); 0 for abilities.
    pub x: u32,
    /// Whether the last optional action ("you may pay {1}") happened, for
    /// "If you do"; true until one is declined, so "if you do" after a
    /// mandatory action ("Sacrifice this creature. If you do, ...")
    /// follows it.
    pub did: bool,
    /// What the controller chose as this resolution went ("Choose a
    /// creature type.", CR 608.2d), for the filters and effects that refer
    /// to it later in the resolution.
    pub chosen: Chosen,
    /// Players who couldn't do what the last per-player effect asked
    /// ("each opponent discards a card. For each opponent who can't, you
    /// draw a card"): those with too few cards in hand to discard.
    pub couldnt: u32,
    /// The definition whose counts `Num::Count` indexes when not the
    /// source's: an ability another card granted it (Galuf's Final Act's
    /// "equal to its power"), or a spliced card.
    pub counts: Option<crate::card::CardRef>,
}

/// The ways to divide `total` among `k` targets, each getting at least 1
/// (CR 601.2d), in lexicographic order. Past `MAX_SET_OPTIONS` ways only
/// the even split and those giving one target all but the minimum to the
/// others are offered.
pub(super) fn divisions(total: u32, k: usize) -> Vec<Vec<u32>> {
    fn rec(left: u32, k: usize, cur: &mut Vec<u32>, out: &mut Vec<Vec<u32>>, cap: usize) -> bool {
        if out.len() > cap {
            return false;
        }
        if k == 1 {
            cur.push(left);
            out.push(cur.clone());
            cur.pop();
            return true;
        }
        let k32 = u32::try_from(k).unwrap_or(u32::MAX);
        for n in 1..=left.saturating_sub(k32 - 1) {
            cur.push(n);
            let ok = rec(left - n, k - 1, cur, out, cap);
            cur.pop();
            if !ok {
                return false;
            }
        }
        true
    }
    let k32 = u32::try_from(k).unwrap_or(u32::MAX);
    if k == 0 || total < k32 {
        return Vec::new();
    }
    let mut out = Vec::new();
    if rec(
        total,
        k,
        &mut Vec::with_capacity(k),
        &mut out,
        super::MAX_SET_OPTIONS,
    ) {
        return out;
    }
    let mut out: Vec<Vec<u32>> = (0..k)
        .map(|j| {
            (0..k)
                .map(|i| if i == j { total - (k32 - 1) } else { 1 })
                .collect()
        })
        .collect();
    let even: Vec<u32> = (0..k32)
        .map(|i| total / k32 + u32::from(i < total % k32))
        .collect();
    if !out.contains(&even) {
        out.push(even);
    }
    out
}

/// A canonical payment plan: which sources to tap, and for what, so that
/// `pool` plus their mana covers `req`. With single-color sources a greedy
/// assignment (colored symbols first, then generic from whatever is left) is
/// exact; multi-type lands are assigned to the first color that needs them.
/// Mana sources, inline for typical board sizes.
pub(super) type Sources = SmallVec<[(ObjId, Mana); 12]>;

/// `sources` less the permanents in `gone` (tapped for a cost).
pub(super) fn without(sources: &Sources, gone: &[ObjId]) -> Sources {
    sources
        .iter()
        .filter(|(s, _)| !gone.contains(s))
        .copied()
        .collect()
}

/// The first index of the ids that stand for restricted mana in a pool
/// (`GameState::restricted_pool`) among payment sources: past any arena
/// slot, so no object has one.
const POOL_UNIT_BASE: u64 = 0xFFFF_0000;

/// The source id of the `i`th unit of `GameState::restricted_pool`.
pub(super) fn pool_unit_id(i: usize) -> ObjId {
    let i = u64::try_from(i).unwrap_or(0xFFFF);
    ObjId::from(slotmap::KeyData::from_ffi(
        (1u64 << 32) | (POOL_UNIT_BASE + i),
    ))
}

/// Which unit of the restricted pool a source id stands for, if any.
pub(super) fn pool_unit_index(id: ObjId) -> Option<usize> {
    use slotmap::Key;
    let idx = id.data().as_ffi() & 0xFFFF_FFFF;
    idx.checked_sub(POOL_UNIT_BASE)
        .and_then(|i| usize::try_from(i).ok())
}

/// What mana is being spent on, for restricted mana (CR 106.6).
#[derive(Copy, Clone)]
enum Spend<'a> {
    /// Casting a spell with these characteristics.
    Spell(&'a crate::state::Characteristics),
    /// Activating an ability of a source with these characteristics
    /// (`None`: not known).
    Ability(Option<&'a crate::state::Characteristics>),
    /// Foretelling a card (CR 702.143a).
    Foretell,
}

/// {3}: what a face-down spell costs (CR 702.37c).
fn three() -> ManaRequirement {
    ManaRequirement {
        generic: 3,
        ..ManaRequirement::default()
    }
}

/// Apply a cost change to the generic part (CR 601.2f): a reduction
/// takes off generic mana only, and never below zero.
fn adjust_generic(req: &mut ManaRequirement, delta: i32) {
    let g = i64::from(req.generic) + i64::from(delta);
    req.generic = u32::try_from(g.max(0)).unwrap_or(u32::MAX);
}

fn payment_plan_from(
    sources: &[(ObjId, Mana)],
    pool: &crate::mana::ManaPool,
    req: &ManaRequirement,
    life: i32,
) -> Option<Sources> {
    if req.is_plain() {
        return plain_payment_plan(sources, pool, req);
    }
    resolve_payment(sources, pool, req, life).map(|(plan, _, _)| plan)
}

/// The first expansion of `req` (hybrid and Phyrexian symbols resolved,
/// see `ManaRequirement::expansions`) that `sources` and `pool` can pay
/// with life to spare (CR 119.4): the plan, the plain requirement, and the
/// life to pay.
pub(super) fn resolve_payment(
    sources: &[(ObjId, Mana)],
    pool: &crate::mana::ManaPool,
    req: &ManaRequirement,
    life: i32,
) -> Option<(Sources, ManaRequirement, u32)> {
    if req.is_plain() {
        return plain_payment_plan(sources, pool, req).map(|p| (p, *req, 0));
    }
    // `{S}` needs to know which sources are snow: `snow_payment`.
    if req.snow > 0 {
        return None;
    }
    req.expansions().into_iter().find_map(|(r, l)| {
        if i64::from(l) > i64::from(life) {
            return None;
        }
        plain_payment_plan(sources, pool, &r).map(|p| (p, r, l))
    })
}

fn plain_payment_plan(
    sources: &[(ObjId, Mana)],
    pool: &crate::mana::ManaPool,
    req: &ManaRequirement,
) -> Option<Sources> {
    let mut pool = *pool;
    let mut sources: Sources = SmallVec::from_slice(sources);
    let mut tapped = Sources::new();
    let take = |sources: &mut Sources, pos: usize| -> (ObjId, Mana) {
        let (id, mana) = sources[pos];
        sources.retain(|(s, _)| *s != id);
        (id, mana)
    };
    for (ci, need) in req.colored.iter().enumerate() {
        let mut need = *need;
        let from_pool = need.min(pool.colored[ci]);
        pool.colored[ci] -= from_pool;
        need -= from_pool;
        while need > 0 {
            let pos = sources
                .iter()
                .position(|(_, m)| matches!(m, Mana::Colored(c) if *c as usize == ci))
                .or_else(|| sources.iter().position(|(_, m)| *m == Mana::Any))?;
            let (id, mana) = take(&mut sources, pos);
            // An any-color source pays as the color needed.
            let mana = if mana == Mana::Any {
                Mana::Colored(crate::mana::Color::ALL[ci])
            } else {
                mana
            };
            tapped.push((id, mana));
            need -= 1;
        }
    }
    let mut need = req.colorless;
    let from_pool = need.min(pool.colorless);
    pool.colorless -= from_pool;
    need -= from_pool;
    while need > 0 {
        let pos = sources.iter().position(|(_, m)| *m == Mana::Colorless)?;
        tapped.push(take(&mut sources, pos));
        need -= 1;
    }
    let mut generic = req.generic;
    let from_pool = generic.min(pool.total());
    generic -= from_pool;
    while generic > 0 {
        if sources.is_empty() {
            return None;
        }
        let (id, mana) = take(&mut sources, 0);
        tapped.push((
            id,
            if mana == Mana::Any {
                Mana::Colorless
            } else {
                mana
            },
        ));
        generic -= 1;
    }
    Some(tapped)
}

/// CR 608.2b: a spell or ability with targets, all of them now illegal,
/// doesn't resolve. Declined optional targets (`Target::none`) are not
/// targets; with none chosen it resolves.
pub(super) fn all_illegal(legal: &[bool], targets: &[Target]) -> bool {
    let mut any = false;
    for (l, t) in legal.iter().zip(targets) {
        if t.is_none() {
            continue;
        }
        if *l {
            return false;
        }
        any = true;
    }
    any
}

/// Where each part of a combined mode after the first begins among its
/// targets (`Mode::split`, `split2`, `more`).
pub(super) fn mode_target_starts(m: &crate::card::Mode) -> SmallVec<[usize; 4]> {
    m.split
        .into_iter()
        .chain(m.split2)
        .chain(m.more.iter().copied())
        .map(|(_, t)| usize::from(t))
        .collect()
}
