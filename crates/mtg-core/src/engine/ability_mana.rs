//! Mana abilities that payment activates for the player (CR 601.2g,
//! 605.3a) when the plain sources (`mana_sources`: `{T}: Add one mana`
//! and sacrifice-for-mana abilities) can't cover a cost: abilities that
//! need no tap ("Sacrifice this creature: Add {C}", usable even by a
//! summoning-sick creature, CR 302.6 being about {T} only), that cost mana
//! (filters, "{2}, {T}, Sacrifice this artifact: Add two mana"), that are
//! granted by another permanent ("Enchanted land has '{T}: Add two mana of
//! any one color'"), or that make several mana or ask for a color.
//!
//! The planner is the slow path behind the canonical one: it runs only
//! when the plain plan fails and some permanent has an activated ability
//! (`activated_sources`; a separate count of mana abilities cost more in
//! the hot path than it saved) or abilities may be granted, and it looks
//! only at permanents flagged `DefFlags::has_mana_ability`. It tries
//! activations cheapest first and, for each, matches the cost's symbols
//! to mana (a bipartite matching, so multi-color sources are assigned
//! exactly): the abilities' own mana costs from the pool and plain
//! sources, the cost being paid from those and the abilities' mana.

use smallvec::SmallVec;

use crate::card::{
    AbilityDef, AbilityId, ActivatedAbility, CounterKind, Effect, EffectCond, Keyword, SacCost, Who,
};
use crate::ids::{ObjId, PlayerId};
use crate::mana::{Color, ManaPool, ManaRequirement};
use crate::mana::{ColorSet, Mana};
use crate::state::{Chosen, GameState};

use super::casting::{EffectCtx, Sources};

/// The kinds of mana a unit may be, as bits: one per color in `Color`
/// order, then colorless.
type Kinds = u8;
const COLORLESS: Kinds = 1 << 5;
/// The kinds of each mana one activation adds.
type Outs = SmallVec<[Kinds; 4]>;
const ANY_COLOR: Kinds = 0b1_1111;

/// Most activations of one ability the planner considers.
const MAX_TIMES: u8 = 3;
/// Most combinations of activations tried for one cost.
const MAX_COMBOS: usize = 256;
/// Most mana one activation is taken to make ("add {G} for each Elf").
const MAX_OUT: u32 = 8;

/// The ability of a planned tap of a plain source whose mana triggered
/// mana abilities add to (`AbilityDef::ManaTrigger`): no ability of the
/// pool.
const BOOSTED_TAP: AbilityId = AbilityId(u32::MAX);

/// A triggered mana ability that triggers as a permanent is tapped for
/// mana (`GameState::mana_boosts`).
#[derive(Copy, Clone, Debug)]
pub(super) struct ManaBoost {
    pub source: ObjId,
    pub ability: AbilityId,
    /// The kinds of mana the permanent must have made for it to trigger
    /// ("Whenever you tap a land for {C}"); 0 if it names the chosen color
    /// and none is chosen.
    pub made: Option<Kinds>,
}

/// The kinds `m` names, "the chosen color" being `chosen`'s.
fn chosen_kind(m: Mana, chosen: Chosen) -> Kinds {
    match m {
        Mana::Chosen => chosen.color().map_or(0, |c| 1 << (c as u8)),
        m => kinds_of(m),
    }
}

fn kinds_of(m: Mana) -> Kinds {
    match m {
        Mana::Colored(c) => 1 << (c as u8),
        Mana::Colorless => COLORLESS,
        Mana::Any => ANY_COLOR,
        Mana::Chosen => 0,
    }
}

fn color_kinds(cs: ColorSet) -> Kinds {
    cs.iter().fold(0, |k, c| k | (1 << (c as u8)))
}

/// One mana of the lowest kind in `k` (a color before colorless).
fn some_mana(k: Kinds) -> Mana {
    match Color::ALL.into_iter().find(|c| k & (1 << (*c as u8)) != 0) {
        Some(c) => Mana::Colored(c),
        None => Mana::Colorless,
    }
}

/// What `GameState::mana_made` has found so far.
struct Made {
    out: Outs,
    tie: Tie,
    rest: bool,
}

/// Whether payment may apply `e`, an effect of a mana ability besides its
/// mana, right after the mana: one that asks nothing and needs no target
/// ("draw a card", "This land deals 1 damage to you", "If there are no
/// depletion counters on this land, sacrifice it", a delayed trigger).
fn rest_ok(e: &Effect) -> bool {
    match e {
        Effect::IfCondition(_, inner) => inner.iter().all(rest_ok),
        Effect::DrawCards { .. }
        | Effect::GainLife { .. }
        | Effect::LoseLife { .. }
        | Effect::Delayed { .. }
        | Effect::DealDamage { .. }
        | Effect::Sacrifice { .. }
        // "This land doesn't untap during your next untap step." (the
        // bounce-tap lands).
        | Effect::SkipNextUntap {
            to: crate::card::Recipient::Source,
        } => true,
        _ => false,
    }
}

/// The kinds of mana an effect could add, however much (CR 106.7).
fn effect_kinds(e: &Effect, chosen: Kinds) -> Kinds {
    let of = |m: Mana| {
        if m == Mana::Chosen {
            chosen
        } else {
            kinds_of(m)
        }
    };
    match e {
        Effect::AddMana(ms) | Effect::AddManaOneOf(ms) => ms.iter().fold(0, |k, m| k | of(*m)),
        Effect::AddManaTimes { mana, .. } => of(*mana),
        Effect::AddManaCombination { colors, .. } => color_kinds(*colors),
        Effect::AddManaTwoColors(_) => ANY_COLOR,
        Effect::Instead { base, alt, .. } => base
            .iter()
            .chain(alt)
            .fold(0, |k, e| k | effect_kinds(e, chosen)),
        // CR 106.7: what the mana may be spent on doesn't matter.
        Effect::SpendOnly { effects, .. } | Effect::SpendRider { effects, .. } => {
            effects.iter().fold(0, |k, e| k | effect_kinds(e, chosen))
        }
        _ => 0,
    }
}

/// How the mana one activation adds relates.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum Tie {
    /// Each mana's kind is its own ("Add {B}{R}", "Add one mana of any
    /// color").
    Free,
    /// "Two mana of any one color": all the same color.
    Same,
    /// "Two mana of different colors".
    Different,
}

/// An activated mana ability payment may activate.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)] // independent costs and limits
pub(super) struct AbilityMana {
    pub id: ObjId,
    pub ability: AbilityId,
    /// Its mana cost (no hybrid, Phyrexian, or X symbols).
    cost: ManaRequirement,
    life: u32,
    tap: bool,
    sac: bool,
    /// Recorded for "activate only once each turn" and friends.
    per_turn: bool,
    once_ever: bool,
    /// Counters removed from its source as a cost.
    remove: Option<(CounterKind, u32)>,
    /// Counters put on its source as a cost ("Put a -0/-1 counter on
    /// this creature: Add {G}.").
    put: Option<(CounterKind, u32)>,
    /// It does more than add mana ("If there are no depletion counters on
    /// this land, sacrifice it."): the rest is applied after the mana.
    rest: bool,
    /// It exiles cards from its controller's graveyard as a cost ("{T},
    /// Exile a card from your graveyard: Add one mana of any color."):
    /// the oldest that match, as when it is activated by hand.
    exile: bool,
    /// How many times it may be activated now.
    times: u8,
    /// The kinds each mana it adds may be.
    out: SmallVec<[Kinds; 4]>,
    tie: Tie,
    /// Preference: lower first. Tapping costs least, then sacrificing,
    /// then paying mana or life.
    weight: u32,
}

/// One activation in a plan.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)] // independent costs and limits
pub(super) struct Activation {
    pub id: ObjId,
    pub ability: AbilityId,
    /// The mana spent from the pool on its cost.
    pub spend: ManaPool,
    pub life: u32,
    pub tap: bool,
    pub sac: bool,
    pub per_turn: bool,
    pub once_ever: bool,
    pub remove: Option<(CounterKind, u32)>,
    pub put: Option<(CounterKind, u32)>,
    pub rest: bool,
    pub exile: bool,
    /// The mana it adds, colors chosen (CR 106.1b), triggered mana
    /// abilities' included.
    pub mana: SmallVec<[Mana; 4]>,
}

/// A payment that activates mana abilities: tap `taps` (as in the plain
/// plan), then activate `acts` in order, then pay `req` from the pool and
/// `life` for Phyrexian symbols.
#[derive(Clone, Debug)]
pub(super) struct AbilityPlan {
    pub taps: Sources,
    pub acts: SmallVec<[Activation; 4]>,
    pub req: ManaRequirement,
    pub life: u32,
}

impl GameState {
    /// Whether payment may need to look at activated mana abilities: some
    /// permanent has a printed activated ability, or abilities may be
    /// granted. One branch on the failure path of every payability check.
    #[inline]
    pub(super) fn ability_mana_possible(&self) -> bool {
        (self.activated_sources | self.grant_sources | self.eot_grants | self.mana_trigger_sources)
            != 0
    }

    /// The activated mana abilities of `p`'s permanents (other than
    /// `exclude`, the source of an ability being paid for) that payment
    /// may activate now.
    pub(super) fn ability_mana(&self, p: PlayerId, exclude: Option<ObjId>) -> Vec<AbilityMana> {
        let grants = self.grants_exist();
        let mut out = Vec::new();
        for id in self.permanents_of(p) {
            if Some(id) == exclude {
                continue;
            }
            let o = &self.objects[id];
            if !grants && !self.flags_of(o).has_mana_ability() {
                continue;
            }
            let Some(c) = self.characteristics(id) else {
                continue;
            };
            // CR 302.6: only a creature's {T} abilities need it to have
            // been controlled since the turn began.
            let sick = c.is_creature() && !o.controlled_since_turn_start && !c.has(Keyword::Haste);
            for a in &c.abilities {
                let AbilityDef::Activated(ab) = self.pool.ability(*a) else {
                    continue;
                };
                if ab.cost.tap && (o.tapped || sick) || ab.cost.untap {
                    continue;
                }
                if let Some(m) = self.usable_mana_ability(p, id, *a, ab) {
                    out.push(m);
                }
            }
        }
        if self.mana_trigger_sources > 0 {
            self.boosted_taps(p, exclude, &mut out);
        }
        out
    }

    /// Plain sources (`mana_sources`) whose mana triggered mana abilities
    /// add to (CR 605.1b: Wild Growth's {G} as the land is tapped), as
    /// activations making both: the planner may then count a land as two
    /// mana. Tapping it as a plain source adds the extra mana too, after
    /// the plan (`tap_for_plan`).
    #[cold]
    fn boosted_taps(&self, p: PlayerId, exclude: Option<ObjId>, out: &mut Vec<AbilityMana>) {
        let sources = self.mana_sources(p);
        let mut seen: SmallVec<[ObjId; 8]> = SmallVec::new();
        for (id, _) in &sources {
            if Some(*id) == exclude || seen.contains(id) || self.sacrifices_for_mana(*id) {
                continue;
            }
            seen.push(*id);
            let own = sources
                .iter()
                .filter(|(s, _)| s == id)
                .fold(0, |k, (_, m)| k | kinds_of(*m));
            let Some(extra) = self.boost_outs(p, *id, &[own]) else {
                continue;
            };
            let mut made: Outs = smallvec::smallvec![own];
            made.extend(extra);
            out.push(AbilityMana {
                id: *id,
                ability: BOOSTED_TAP,
                cost: ManaRequirement::default(),
                life: 0,
                tap: true,
                sac: false,
                per_turn: false,
                once_ever: false,
                remove: None,
                put: None,
                rest: false,
                exile: false,
                times: 1,
                out: made,
                tie: Tie::Free,
                weight: 1,
            });
        }
    }

    /// The triggered mana abilities (`AbilityDef::ManaTrigger`) that
    /// trigger when `tapped` is tapped for mana, and by how much mana
    /// replacements (`Replaces::TappedForMana`) multiply its own mana.
    /// Each trigger comes with its source, its controller (a delayed one's,
    /// "Until end of turn, whenever a player taps an Island for mana"),
    /// and the kind of mana it needs the permanent to have made ("for
    /// {C}"; the chosen color filled in).
    pub(super) fn mana_boosts(&self, tapped: ObjId) -> (SmallVec<[ManaBoost; 2]>, u32) {
        let mut out = SmallVec::new();
        let mut times = 1u32;
        let Some(t) = self.objects.get(tapped) else {
            return (out, times);
        };
        let Some(tc) = self.characteristics(tapped) else {
            return (out, times);
        };
        for src in &self.battlefield {
            let o = &self.objects[*src];
            if !self.flags_of(o).has_mana_trigger() {
                continue;
            }
            let Some(c) = self.characteristics(*src) else {
                continue;
            };
            for a in &c.abilities {
                match self.pool.ability(*a) {
                    AbilityDef::ManaTrigger { what, form, .. } => {
                        if (!what.enchanted || o.attached_to == Some(tapped))
                            && what.matches(
                                tapped,
                                t.controller,
                                &tc,
                                Some(*src),
                                Some(o.controller),
                            )
                            && self.status_ok(what, tapped)
                        {
                            out.push(ManaBoost {
                                source: *src,
                                ability: *a,
                                made: form.made.map(|m| chosen_kind(m, o.chosen)),
                            });
                        }
                    }
                    // CR 106.12, 614.1a: "it produces three times as much
                    // of that mana instead"; several multiply together.
                    AbilityDef::Replacement(r) => {
                        if let crate::card::Replaces::TappedForMana { what } = &r.replaces
                            && what.matches(
                                tapped,
                                t.controller,
                                &tc,
                                Some(*src),
                                Some(o.controller),
                            )
                            && self.status_ok(what, tapped)
                        {
                            times = times.saturating_mul(match r.replacement {
                                crate::card::Replacement::Triple => 3,
                                _ => 2,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        // CR 603.7c: "Until end of turn, whenever a player taps an Island
        // for mana, that player adds an additional {U}." (High Tide), a
        // delayed triggered mana ability, its controller's.
        if self.eot_mana_triggers > 0 {
            for dt in &self.delayed_triggers {
                let AbilityDef::ManaTrigger { what, form, .. } =
                    self.pool.ability(dt.ability.ability)
                else {
                    continue;
                };
                let src = dt.ability.source;
                if what.matches(tapped, t.controller, &tc, Some(src), Some(dt.controller))
                    && self.status_ok(what, tapped)
                {
                    out.push(ManaBoost {
                        source: src,
                        ability: dt.ability.ability,
                        made: form.made.map(|m| chosen_kind(m, Chosen::None)),
                    });
                }
            }
        }
        (out, times)
    }

    /// Whether tapping `tapped` for mana triggers a mana ability or has
    /// its mana multiplied.
    pub(super) fn mana_boosted(&self, tapped: ObjId) -> bool {
        let (triggers, times) = self.mana_boosts(tapped);
        !triggers.is_empty() || times > 1
    }

    /// The colors among the cards `source` exiled that are still in exile
    /// (CR 607.2a), each judged as the card is (CR 406.3: a face-down one
    /// has none).
    pub(super) fn exiled_colors(&self, source: ObjId) -> ColorSet {
        let mut seen = ColorSet::default();
        for id in self.exiled_with(source) {
            if self.objects[id].exiled_face_down() {
                continue;
            }
            for col in self.base_characteristics(&self.objects[id]).colors.iter() {
                seen = seen.with(col);
            }
        }
        seen
    }

    /// The colors among the permanents `p` controls matching `filter`
    /// ("For each color among permanents you control"), for `src`'s
    /// filter.
    pub(super) fn colors_among(
        &self,
        p: PlayerId,
        filter: &crate::card::ObjFilter,
        src: ObjId,
    ) -> crate::mana::ColorSet {
        let mut seen = crate::mana::ColorSet::default();
        for id in self.permanents_of(p) {
            if let Some(c) = self.characteristics(id)
                && filter.matches(id, p, &c, Some(src), Some(p))
                && self.status_ok(filter, id)
            {
                for col in c.colors.iter() {
                    seen = seen.with(col);
                }
            }
        }
        seen
    }

    /// The kinds of the extra mana triggered mana abilities add as
    /// `tapped` is tapped for mana, making `own` (the kinds of each of its
    /// mana), and the extra mana a mana replacement makes of it; `None`
    /// without any (or with any the planner can't foresee).
    fn boost_outs(&self, p: PlayerId, tapped: ObjId, own: &[Kinds]) -> Option<Outs> {
        let mut out = Outs::new();
        let all = own.iter().fold(0, |k, m| k | m);
        let (triggers, times) = self.mana_boosts(tapped);
        if times > 1 {
            // Three times as much of that mana: of a permanent making
            // mana of known kinds, as much again of the same kinds.
            if own.iter().any(|k| k.count_ones() != 1) {
                return None;
            }
            for _ in 1..times.min(MAX_OUT) {
                out.extend_from_slice(own);
            }
        }
        for b in triggers {
            // "Whenever you tap a land for {C}": only if it makes that
            // kind (a permanent that may make others isn't foreseen).
            if let Some(need) = b.made {
                if all & need == 0 {
                    continue;
                }
                if all != need {
                    return None;
                }
            }
            let effects = self.pool.ability(b.ability).effects();
            // "One mana of any type that land produced": of a permanent
            // making one kind, that kind.
            if matches!(effects, [Effect::AddManaProduced]) {
                if all.count_ones() != 1 {
                    return None;
                }
                out.push(all);
                continue;
            }
            let (made, tie, rest) = self.mana_made(p, b.source, effects)?;
            if (tie != Tie::Free && made.len() > 1) || rest {
                return None;
            }
            out.extend(made);
        }
        (!out.is_empty()).then_some(out)
    }

    /// CR 605.4a: the triggered mana abilities of `tapped`, just tapped for
    /// mana, resolve at once, adding their mana for its controller; first a
    /// mana replacement multiplies what it made (`tapped_produced`).
    #[cold]
    #[inline(never)]
    pub(super) fn fire_mana_triggers(&mut self, tapped: ObjId) {
        let Some(p) = self.objects.get(tapped).map(|o| o.controller) else {
            return;
        };
        let (triggers, times) = self.mana_boosts(tapped);
        let made = self.tapped_produced;
        for _ in 1..times {
            let pool = &mut self.player_mut(p).mana_pool;
            for c in 0..5 {
                pool.colored[c] = pool.colored[c].saturating_add(made.colored[c]);
            }
            pool.colorless = pool.colorless.saturating_add(made.colorless);
        }
        for b in triggers {
            if let Some(need) = b.made {
                let has = (0..5).any(|c| need & (1 << c) != 0 && made.colored[c] > 0)
                    || (need & COLORLESS != 0 && made.colorless > 0);
                if !has {
                    continue;
                }
            }
            let (src, a) = (b.source, b.ability);
            let pool = std::sync::Arc::clone(&self.pool);
            let mut ctx = EffectCtx {
                source: src,
                controller: p,
                targets: Vec::new(),
                division: Vec::new(),
                legal: Vec::new(),
                trigger_obj: Some(tapped),
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
            for e in pool.ability(a).effects() {
                self.apply_effect(&mut ctx, e);
            }
        }
    }

    /// The kinds of mana lands (`whose`, from `p`; `basic`: basic lands
    /// only) could produce (CR 106.7): their mana abilities' mana, whatever
    /// they cost and whether or not they could be activated. Abilities
    /// that themselves ask what lands could produce add nothing.
    pub(super) fn lands_could_produce(&self, p: PlayerId, whose: Who, basic: bool) -> Kinds {
        let mut kinds: Kinds = 0;
        for id in &self.battlefield {
            let o = &self.objects[*id];
            let theirs = match whose {
                Who::You => o.controller == p,
                Who::Opponents => o.controller != p,
                Who::Any => true,
            };
            if !theirs {
                continue;
            }
            let Some(ch) = self.characteristics(*id) else {
                continue;
            };
            if !ch.is_land() || (basic && !ch.supertypes.contains(crate::card::Supertype::Basic)) {
                continue;
            }
            for s in &ch.subtypes {
                if let Some(color) = s.basic_color() {
                    kinds |= 1 << (color as u8);
                }
            }
            let chosen = o.chosen.color().map_or(0, |c| 1 << (c as u8));
            let of = |m: Mana| {
                if m == Mana::Chosen {
                    chosen
                } else {
                    kinds_of(m)
                }
            };
            for a in &ch.abilities {
                match self.pool.ability(*a) {
                    AbilityDef::TapForMana(mana) | AbilityDef::SacForMana(mana) => {
                        kinds |= of(*mana);
                    }
                    AbilityDef::RestrictedMana { mana, .. } => kinds |= of(*mana),
                    AbilityDef::Activated(ab) if ab.is_mana_ability() => {
                        for e in &ab.effects {
                            kinds |= effect_kinds(e, chosen);
                        }
                    }
                    _ => {}
                }
            }
        }
        kinds
    }

    /// `ab` as an `AbilityMana`, if it is a mana ability whose costs the
    /// planner can pay (mana, {T}, sacrificing its source, life) and that
    /// may be activated now.
    #[allow(clippy::too_many_lines)] // one check per reason a mana ability can't be planned
    fn usable_mana_ability(
        &self,
        p: PlayerId,
        id: ObjId,
        a: AbilityId,
        ab: &ActivatedAbility,
    ) -> Option<AbilityMana> {
        let cost = &ab.cost;
        if !ab.is_mana_ability()
            || ab.from_hand
            || ab.from_graveyard
            || ab.sorcery_speed
            || ab.keyword.is_some()
            || cost.discard_self
            || cost.exile_self
            || cost.exile_this
            || cost.crew > 0
            || cost
                .remove_counters
                .is_some_and(|(k, n)| self.objects[id].counters(k) < n)
            || cost.energy > 0
            || cost.exert
            || cost.discard_hand
            || cost.life_x
            || cost.discard > 0
            || cost.discard_x
            || cost.exile_cards.as_ref().is_some_and(|g| {
                g.x || self.exile_candidates(p, g.filter.as_ref(), id).len() < g.n as usize
            })
            // CR 701.61a: foraging exiles cards or sacrifices a Food, which
            // paying for a spell doesn't do on the way.
            || cost.forage
            // Likewise blighting, collecting evidence, beholding, and
            // revealing: costs with a choice the planner doesn't make.
            || cost.blight > 0
            || cost.process > 0
            || cost.remove_from_creature
            || cost.remove_among.is_some()
            || cost.evidence > 0
            || cost.behold.is_some()
            || cost.reveal_hand
            || !matches!(cost.sacrifice, None | Some(SacCost::This))
            || cost.mana.has_x()
        {
            return None;
        }
        let req = self.activation_mana(id, ab, p);
        // A hybrid symbol in the ability's cost ("{U/B}, {T}: Add {U}{U},
        // {U}{B}, or {B}{B}", the filter lands) is matched to mana of
        // either color; the other kinds of symbol aren't planned.
        let plain_or_hybrid = req.two_hybrid == [0; 5]
            && req.phyrexian == [0; 5]
            && req.hybrid_phyrexian == [0; 10]
            && req.snow == 0;
        if !plain_or_hybrid || !self.activation_allowed(ab, a, p, id) {
            return None;
        }
        let used = if ab.per_turn > 0 {
            let n = self
                .activated_once
                .iter()
                .filter(|x| **x == (id, a))
                .count();
            if n >= usize::from(ab.per_turn) {
                return None;
            }
            u8::try_from(usize::from(ab.per_turn) - n).unwrap_or(MAX_TIMES)
        } else {
            MAX_TIMES
        };
        let sac = cost.sacrifice.is_some();
        let counters = cost.remove_counters.is_some() || cost.put_counters.is_some();
        let times = if cost.tap || sac || ab.once_ever || counters {
            1
        } else {
            used.min(MAX_TIMES)
        };
        // Each activation exiles its own cards.
        let times = match &cost.exile_cards {
            Some(g) if g.n > 0 => {
                let have = self.exile_candidates(p, g.filter.as_ref(), id).len() / g.n as usize;
                times.min(u8::try_from(have).unwrap_or(u8::MAX))
            }
            _ => times,
        };
        let (mut out, tie, rest) = self.mana_made(p, id, &ab.effects)?;
        // CR 605.1b: tapping it for mana triggers mana abilities that add
        // to what it makes.
        if cost.tap
            && self.mana_trigger_sources > 0
            && let Some(extra) = self.boost_outs(p, id, &out)
        {
            if tie != Tie::Free {
                return None;
            }
            out.extend(extra);
        }
        let weight = 1
            + if sac { 4 } else { 0 }
            + 3 * req.total()
            + 2 * cost.life
            + u32::from(!cost.tap && !sac && req.total() == 0)
            + 2 * u32::from(rest || counters || cost.exile_cards.is_some());
        Some(AbilityMana {
            id,
            ability: a,
            cost: req,
            life: cost.life,
            tap: cost.tap,
            sac,
            per_turn: ab.per_turn > 0 || ab.counted,
            once_ever: ab.once_ever,
            remove: cost.remove_counters,
            put: cost.put_counters,
            rest,
            exile: cost.exile_cards.is_some(),
            times,
            out,
            tie,
            weight,
        })
    }

    /// The mana these effects add, as `apply_effect` would (one entry per
    /// mana, the kinds it may be), how the entries relate, and whether the
    /// effects do more than add mana (applied after, see `rest_ok`);
    /// `None` if they add none, mix ties, or do something payment can't
    /// do on the way.
    fn mana_made(
        &self,
        p: PlayerId,
        id: ObjId,
        effects: &[Effect],
    ) -> Option<(SmallVec<[Kinds; 4]>, Tie, bool)> {
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
        let mut made = Made {
            out: SmallVec::new(),
            tie: Tie::Free,
            rest: false,
        };
        self.mana_made_into(&ctx, effects, &mut made)?;
        if made.out.is_empty() {
            return None;
        }
        Some((made.out, made.tie, made.rest))
    }

    #[allow(clippy::too_many_lines)] // one arm per way to add mana
    fn mana_made_into(&self, ctx: &EffectCtx, effects: &[Effect], made: &mut Made) -> Option<()> {
        let id = ctx.source;
        let chosen = self.chosen_color_of(id).map_or(0, |c| 1 << (c as u8));
        let set_tie = |t: Tie, tie: &mut Tie| {
            if t == Tie::Free || *tie == t {
                return true;
            }
            let ok = *tie == Tie::Free;
            *tie = t;
            ok
        };
        for e in effects {
            match e {
                Effect::AddMana(mana) => {
                    for m in mana {
                        let k = if *m == Mana::Chosen {
                            chosen
                        } else {
                            kinds_of(*m)
                        };
                        if k != 0 {
                            made.out.push(k);
                        }
                    }
                }
                Effect::AddManaOneOf(mana) => {
                    // As `apply_effect`: a choice among colors, else the
                    // first.
                    let colors: SmallVec<[Kinds; 4]> = mana
                        .iter()
                        .filter_map(|m| match m {
                            Mana::Colored(c) => Some(1 << (*c as u8)),
                            Mana::Chosen if chosen != 0 => Some(chosen),
                            _ => None,
                        })
                        .collect();
                    if colors.len() == mana.len() && !colors.is_empty() {
                        made.out.push(colors.iter().fold(0, |a, b| a | b));
                    } else {
                        made.out
                            .push(mana.first().map_or(COLORLESS, |m| kinds_of(*m)));
                    }
                }
                Effect::AddManaTimes { mana, count } => {
                    let n = self.eval_num(*count, ctx).min(MAX_OUT);
                    let k = match *mana {
                        Mana::Chosen => chosen,
                        m => kinds_of(m),
                    };
                    if n == 0 || k == 0 {
                        continue;
                    }
                    if k == ANY_COLOR && n > 1 && !set_tie(Tie::Same, &mut made.tie) {
                        return None;
                    }
                    for _ in 0..n {
                        made.out.push(k);
                    }
                }
                Effect::AddManaCombination {
                    count,
                    colors,
                    different,
                } => {
                    let k = color_kinds(*colors);
                    let count = self.eval_num(*count, ctx);
                    let n = if *different {
                        count.min(colors.count())
                    } else {
                        count
                    };
                    if *different && n > 1 && !set_tie(Tie::Different, &mut made.tie) {
                        return None;
                    }
                    for _ in 0..n.min(MAX_OUT) {
                        made.out.push(k);
                    }
                }
                Effect::AddManaLandCould {
                    whose,
                    basic,
                    any_type,
                } => {
                    let mut k = self.lands_could_produce(ctx.controller, *whose, *basic);
                    // "any type": colorless only when no color is possible
                    // (as it resolves).
                    if !*any_type || k & ANY_COLOR != 0 {
                        k &= ANY_COLOR;
                    }
                    if k != 0 {
                        made.out.push(k);
                    }
                }
                // CR 903.4f: no commander, no mana.
                Effect::AddManaCommander { .. } => {}
                Effect::AddManaOneOfTimes { colors, count } => {
                    let n = self.eval_num(*count, ctx).min(MAX_OUT);
                    let k = color_kinds(*colors);
                    if n == 0 || k == 0 {
                        continue;
                    }
                    if n > 1 && k.count_ones() > 1 && !set_tie(Tie::Same, &mut made.tie) {
                        return None;
                    }
                    for _ in 0..n {
                        made.out.push(k);
                    }
                }
                Effect::AddManaColorsAmong(f) => {
                    for c in self.colors_among(ctx.controller, f, id).iter() {
                        made.out.push(1 << (c as u8));
                    }
                }
                // "one mana of any of the exiled cards' colors".
                Effect::AddManaExiledColors => {
                    let k = color_kinds(self.exiled_colors(id));
                    if k != 0 {
                        made.out.push(k);
                    }
                }
                // "Add {C}. If you control an Urza's Mine and an Urza's
                // Power-Plant, add {C}{C} instead." (CR 608.2c): the branch
                // that would happen now.
                Effect::Instead {
                    cond: EffectCond::Static(i),
                    base,
                    alt,
                } if e.adds_mana() => {
                    let c = self.pool.condition(*i);
                    let branch = if self.condition_for(c, ctx.controller, id, None) {
                        alt
                    } else {
                        base
                    };
                    self.mana_made_into(ctx, branch, made)?;
                }
                e if rest_ok(e) => made.rest = true,
                _ => return None,
            }
        }
        Some(())
    }
    /// A payment of `req` from `sources`, `pool`, and `p`'s activated mana
    /// abilities (all but `exclude`'s); see the module comment. `None`
    /// when there are no such abilities, so that it never disagrees with
    /// the plain plan on boards without them.
    #[cold]
    #[inline(never)]
    pub(super) fn ability_plan(
        &self,
        p: PlayerId,
        sources: &[(ObjId, Mana)],
        pool: &ManaPool,
        req: &ManaRequirement,
        life: i32,
        exclude: Option<ObjId>,
    ) -> Option<AbilityPlan> {
        self.ability_plan_tapping(p, sources, pool, req, life, exclude, &[])
    }

    /// `ability_plan` once the permanents `gone` are tapped for a cost
    /// (CR 601.2h: "tap four untapped artifacts, creatures, and/or
    /// lands"): their {T} mana abilities can't pay the rest.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn ability_plan_tapping(
        &self,
        p: PlayerId,
        sources: &[(ObjId, Mana)],
        pool: &ManaPool,
        req: &ManaRequirement,
        life: i32,
        exclude: Option<ObjId>,
        gone: &[ObjId],
    ) -> Option<AbilityPlan> {
        // `{S}` (CR 107.4h) is paid only by tapping snow sources, in the
        // plain plan (`snow_payment`).
        if req.snow > 0 {
            return None;
        }
        let mut abilities = self.ability_mana(p, exclude);
        if !gone.is_empty() {
            abilities.retain(|a| !(a.tap && gone.contains(&a.id)));
        }
        if abilities.is_empty() {
            return None;
        }
        let mut base = self.mana_sources(p);
        if !gone.is_empty() {
            base.retain(|(id, _)| !gone.contains(id));
        }
        // Restricted mana in the pool isn't offered here: an ability's own
        // cost might be paid with it, which its restriction may not allow
        // (CR 106.6).
        if !self.restricted_pool.is_empty() {
            let plain: Sources = sources
                .iter()
                .copied()
                .filter(|(id, _)| super::casting::pool_unit_index(*id).is_none())
                .collect();
            return plan_with_abilities(&plain, &base, pool, req, life, &abilities);
        }
        plan_with_abilities(sources, &base, pool, req, life, &abilities)
    }
}

/// A unit of mana the matching may assign: from the pool, a plain
/// source, or an activation's output.
#[derive(Copy, Clone, Debug)]
struct Unit {
    kinds: Kinds,
    /// The kinds it may be when spent on an ability's cost (plain
    /// sources: only their own mana, not convoke or delve help; 0 for an
    /// activation's output, which pays only the cost being paid).
    cost_kinds: Kinds,
    from: From,
}

#[derive(Copy, Clone, Debug)]
enum From {
    Pool,
    /// Index into the plain units.
    Plain(usize),
    /// Activation, output index.
    Out(usize, usize),
}

/// A symbol to pay: one kind (or any, for generic), for the cost being
/// paid (`None`) or an activation's cost.
#[derive(Copy, Clone, Debug)]
struct Sym {
    need: Option<Kinds>,
    act: Option<usize>,
}

struct Plain {
    id: ObjId,
    kinds: Kinds,
    cost_kinds: Kinds,
    first: Mana,
}

/// Abilities of the same definition making the same mana, merged so that
/// combinations count activations instead of choosing objects.
struct Kind<'a> {
    rep: &'a AbilityMana,
    members: SmallVec<[&'a AbilityMana; 4]>,
    cap: u8,
}

#[allow(clippy::too_many_lines)] // setup, bound, combinations, search
fn plan_with_abilities(
    sources: &[(ObjId, Mana)],
    base: &[(ObjId, Mana)],
    pool: &ManaPool,
    req: &ManaRequirement,
    life: i32,
    abilities: &[AbilityMana],
) -> Option<AbilityPlan> {
    let mut plain: SmallVec<[Plain; 16]> = SmallVec::new();
    for (id, m) in sources {
        match plain.iter_mut().find(|u| u.id == *id) {
            Some(u) => u.kinds |= kinds_of(*m),
            None => plain.push(Plain {
                id: *id,
                kinds: kinds_of(*m),
                cost_kinds: 0,
                first: *m,
            }),
        }
    }
    for (id, m) in base {
        if let Some(u) = plain.iter_mut().find(|u| u.id == *id) {
            u.cost_kinds |= kinds_of(*m);
        }
    }
    let mut kinds: Vec<Kind> = Vec::new();
    for a in abilities {
        match kinds.iter_mut().find(|k| {
            k.rep.ability == a.ability
                && k.rep.out == a.out
                && k.rep.times == a.times
                && k.rep.tap == a.tap
        }) {
            Some(k) => {
                k.members.push(a);
                k.cap = k.cap.saturating_add(a.times);
            }
            None => kinds.push(Kind {
                rep: a,
                members: smallvec::smallvec![a],
                cap: a.times,
            }),
        }
    }
    kinds.sort_by_key(|k| k.rep.weight);
    kinds.truncate(8);
    for k in &mut kinds {
        k.cap = k.cap.min(4);
    }
    // Not enough mana even with every activation's net gain.
    let need = req.colored.iter().sum::<u32>()
        + req.colorless
        + req.generic
        + req.hybrid.iter().map(|n| u32::from(*n)).sum::<u32>()
        + req.two_hybrid.iter().map(|n| u32::from(*n)).sum::<u32>();
    let gain: usize = kinds
        .iter()
        .map(|k| usize::from(k.cap) * k.rep.out.len().saturating_sub(k.rep.cost.total() as usize))
        .sum();
    if (pool.total() as usize) + plain.len() + gain < need as usize {
        return None;
    }
    // Combinations of activation counts, cheapest first.
    let mut product: usize = kinds.iter().map(|k| usize::from(k.cap) + 1).product();
    while product > MAX_COMBOS {
        let Some(k) = kinds.iter_mut().filter(|k| k.cap > 1).max_by_key(|k| k.cap) else {
            break;
        };
        product = product / (usize::from(k.cap) + 1) * usize::from(k.cap);
        k.cap -= 1;
    }
    let mut combos: Vec<SmallVec<[u8; 8]>> = vec![SmallVec::new()];
    for k in &kinds {
        let mut next = Vec::with_capacity(combos.len() * (usize::from(k.cap) + 1));
        for c in &combos {
            for n in 0..=k.cap {
                let mut c = c.clone();
                c.push(n);
                next.push(c);
            }
        }
        combos = next;
        if combos.len() > MAX_COMBOS {
            combos.truncate(MAX_COMBOS);
        }
    }
    let weight = |c: &SmallVec<[u8; 8]>| -> (u32, u32) {
        let w = c
            .iter()
            .zip(&kinds)
            .map(|(n, k)| u32::from(*n) * k.rep.weight)
            .sum();
        (w, c.iter().map(|n| u32::from(*n)).sum())
    };
    combos.sort_by_key(weight);
    let expansions = req.expansions();
    for combo in &combos {
        // The activations, each on an object.
        let mut acts: SmallVec<[&AbilityMana; 4]> = SmallVec::new();
        let mut on: SmallVec<[ObjId; 4]> = SmallVec::new();
        for (n, k) in combo.iter().zip(&kinds) {
            let mut left = *n;
            for m in &k.members {
                let t = left.min(m.times);
                for _ in 0..t {
                    acts.push(m);
                    on.push(m.id);
                }
                left -= t;
                if left == 0 {
                    break;
                }
            }
        }
        if acts
            .iter()
            .any(|a| (a.tap || a.sac) && on.iter().filter(|id| **id == a.id).count() > 1)
        {
            continue;
        }
        let ability_life: u32 = acts.iter().map(|a| a.life).sum();
        for (r, l) in &expansions {
            if i64::from(*l) + i64::from(ability_life) > i64::from(life) {
                continue;
            }
            if let Some(plan) = match_payment(&plain, pool, r, *l, &acts) {
                return Some(plan);
            }
        }
    }
    None
}

/// Try the choices of color for the activations that tie their mana
/// together, then match.
fn match_payment(
    plain: &[Plain],
    pool: &ManaPool,
    req: &ManaRequirement,
    life: u32,
    acts: &[&AbilityMana],
) -> Option<AbilityPlan> {
    // The colors the cost asks for, most wanted first.
    let mut wanted: SmallVec<[Color; 5]> = Color::ALL
        .into_iter()
        .filter(|c| req.colored[*c as usize] > 0)
        .collect();
    wanted.sort_by_key(|c| std::cmp::Reverse(req.colored[*c as usize]));
    // Per activation, the candidate assignments of its outputs.
    let mut choices: SmallVec<[SmallVec<[Outs; 5]>; 4]> = SmallVec::new();
    for a in acts {
        let mut opts: SmallVec<[Outs; 5]> = SmallVec::new();
        match a.tie {
            Tie::Free => opts.push(a.out.clone()),
            Tie::Same => {
                let k = a.out.iter().fold(ANY_COLOR, |x, y| x & y);
                for c in wanted.iter().filter(|c| k & (1 << (**c as u8)) != 0) {
                    opts.push(a.out.iter().map(|_| 1 << (*c as u8)).collect());
                }
                if opts.is_empty() {
                    // Only generic to pay: any of its colors will do.
                    let one = k & k.wrapping_neg();
                    opts.push(a.out.iter().map(|_| one).collect());
                }
            }
            Tie::Different => {
                let k = a.out.iter().fold(ANY_COLOR, |x, y| x & y);
                let mut order: SmallVec<[Color; 5]> = wanted
                    .iter()
                    .copied()
                    .filter(|c| k & (1 << (*c as u8)) != 0)
                    .collect();
                for c in Color::ALL {
                    if k & (1 << (c as u8)) != 0 && !order.contains(&c) {
                        order.push(c);
                    }
                }
                opts.push(
                    order
                        .iter()
                        .take(a.out.len())
                        .map(|c| 1 << (*c as u8))
                        .collect(),
                );
            }
        }
        choices.push(opts);
    }
    let mut pick: SmallVec<[usize; 4]> = smallvec::smallvec![0; acts.len()];
    loop {
        let outs: SmallVec<[&[Kinds]; 4]> = choices
            .iter()
            .zip(&pick)
            .map(|(o, i)| o[*i].as_slice())
            .collect();
        if let Some(plan) = matching(plain, pool, req, life, acts, &outs) {
            return Some(plan);
        }
        // Next assignment (odometer).
        let mut i = 0;
        loop {
            if i == pick.len() {
                return None;
            }
            pick[i] += 1;
            if pick[i] < choices[i].len() {
                break;
            }
            pick[i] = 0;
            i += 1;
        }
    }
}

/// Match every symbol of `req` and of the activations' costs to a unit
/// of mana (Kuhn's augmenting paths); build the plan if all are matched.
#[allow(clippy::too_many_lines)] // units, symbols, matching, plan
fn matching(
    plain: &[Plain],
    pool: &ManaPool,
    req: &ManaRequirement,
    life: u32,
    acts: &[&AbilityMana],
    outs: &[&[Kinds]],
) -> Option<AbilityPlan> {
    let mut consumed: SmallVec<[ObjId; 4]> = SmallVec::new();
    for a in acts {
        if a.tap || a.sac {
            consumed.push(a.id);
        }
    }
    // Units: activations' mana first (it is made anyway), then the pool,
    // then plain sources in their canonical order.
    let mut units: SmallVec<[Unit; 24]> = SmallVec::new();
    for (ai, out) in outs.iter().enumerate() {
        for (k, kinds) in out.iter().enumerate() {
            units.push(Unit {
                kinds: *kinds,
                cost_kinds: 0,
                from: From::Out(ai, k),
            });
        }
    }
    for c in Color::ALL {
        for _ in 0..pool.colored[c as usize] {
            let k = 1 << (c as u8);
            units.push(Unit {
                kinds: k,
                cost_kinds: k,
                from: From::Pool,
            });
        }
    }
    for _ in 0..pool.colorless {
        units.push(Unit {
            kinds: COLORLESS,
            cost_kinds: COLORLESS,
            from: From::Pool,
        });
    }
    for (i, u) in plain.iter().enumerate() {
        if consumed.contains(&u.id) {
            continue;
        }
        units.push(Unit {
            kinds: u.kinds,
            cost_kinds: u.cost_kinds,
            from: From::Plain(i),
        });
    }
    // Symbols: the activations' costs, then the cost being paid; specific
    // kinds before generic.
    let mut syms: SmallVec<[Sym; 16]> = SmallVec::new();
    let push = |r: &ManaRequirement, act: Option<usize>, syms: &mut SmallVec<[Sym; 16]>| {
        for c in Color::ALL {
            for _ in 0..r.colored[c as usize] {
                syms.push(Sym {
                    need: Some(1 << (c as u8)),
                    act,
                });
            }
        }
        for _ in 0..r.colorless {
            syms.push(Sym {
                need: Some(COLORLESS),
                act,
            });
        }
        for (i, (a, b)) in crate::mana::HYBRID_PAIRS.iter().enumerate() {
            for _ in 0..r.hybrid[i] {
                syms.push(Sym {
                    need: Some((1 << (*a as u8)) | (1 << (*b as u8))),
                    act,
                });
            }
        }
    };
    for (i, a) in acts.iter().enumerate() {
        push(&a.cost, Some(i), &mut syms);
    }
    push(req, None, &mut syms);
    for (i, a) in acts.iter().enumerate() {
        for _ in 0..a.cost.generic {
            syms.push(Sym {
                need: None,
                act: Some(i),
            });
        }
    }
    for _ in 0..req.generic {
        syms.push(Sym {
            need: None,
            act: None,
        });
    }
    if syms.len() > units.len() {
        return None;
    }
    let fits = |s: &Sym, u: &Unit| -> bool {
        let k = if s.act.is_some() {
            u.cost_kinds
        } else {
            u.kinds
        };
        match s.need {
            Some(n) => k & n != 0,
            None => k != 0,
        }
    };
    let mut owner: SmallVec<[Option<usize>; 24]> = smallvec::smallvec![None; units.len()];
    for s in 0..syms.len() {
        let mut seen: SmallVec<[bool; 24]> = smallvec::smallvec![false; units.len()];
        if !augment(s, &syms, &units, &fits, &mut seen, &mut owner) {
            return None;
        }
    }
    // The mana each matched unit makes.
    let made = |u: &Unit, s: &Sym| -> Mana {
        match s.need {
            Some(COLORLESS) => Mana::Colorless,
            // A hybrid symbol: the color the unit makes.
            Some(n) => {
                let k = if s.act.is_some() {
                    u.cost_kinds
                } else {
                    u.kinds
                };
                some_mana(if k & n != 0 { k & n } else { n })
            }
            None => match u.from {
                From::Plain(i) if plain[i].first == Mana::Any => Mana::Colorless,
                From::Plain(i) => plain[i].first,
                _ => some_mana(u.kinds),
            },
        }
    };
    let mut taps = Sources::new();
    let mut plan_acts: SmallVec<[Activation; 4]> = acts
        .iter()
        .zip(outs)
        .map(|(a, out)| Activation {
            id: a.id,
            ability: a.ability,
            spend: ManaPool::default(),
            life: a.life,
            tap: a.tap,
            sac: a.sac,
            per_turn: a.per_turn,
            once_ever: a.once_ever,
            remove: a.remove,
            put: a.put,
            rest: a.rest,
            exile: a.exile,
            mana: out.iter().map(|k| some_mana(*k)).collect(),
        })
        .collect();
    for (ui, u) in units.iter().enumerate() {
        let Some(si) = owner[ui] else {
            continue;
        };
        let s = &syms[si];
        let m = made(u, s);
        match u.from {
            From::Plain(i) => taps.push((plain[i].id, m)),
            From::Out(ai, k) => plan_acts[ai].mana[k] = m,
            From::Pool => {}
        }
        if let Some(ai) = s.act {
            plan_acts[ai].spend.add(m);
        }
    }
    // Plain sources in their canonical order.
    taps.sort_by_key(|(id, _)| plain.iter().position(|p| p.id == *id));
    Some(AbilityPlan {
        taps,
        acts: plan_acts,
        req: *req,
        life,
    })
}

fn augment(
    s: usize,
    syms: &[Sym],
    units: &[Unit],
    fits: &dyn Fn(&Sym, &Unit) -> bool,
    seen: &mut [bool],
    owner: &mut [Option<usize>],
) -> bool {
    for u in 0..units.len() {
        if seen[u] || !fits(&syms[s], &units[u]) {
            continue;
        }
        seen[u] = true;
        if owner[u].is_none_or(|t| augment(t, syms, units, fits, seen, owner)) {
            owner[u] = Some(s);
            return true;
        }
    }
    false
}
