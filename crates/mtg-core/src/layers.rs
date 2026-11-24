//! The CR 613 layer system: derives every object's current characteristics
//! from printed values, counters, stored continuous effects, and the static
//! abilities of permanents on the battlefield.
//!
//! Within a layer, effects apply in timestamp order (CR 613.7) except that an
//! effect that depends on another applies after it (CR 613.8). An effect
//! that has started to apply keeps applying in later layers to the same
//! objects even if its ability is removed along the way (CR 613.6), and
//! effects of static abilities exist only while the ability does (CR 611.3).

use smallvec::SmallVec;

use crate::card::{
    AbilityDef, AbilityId, Layer, Modification, ObjFilter, PtValue, StaticCondition, Subtype,
};
use crate::ids::{IdMap, ObjId, PlayerId, Timestamp};
use crate::state::{Affected, Characteristics, Derived, GameState, Zone, pt16};

/// Where an effect comes from, for identity across layers.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
enum Origin {
    Stored(usize),
    Static { source: ObjId, ability: AbilityId },
}

struct Live {
    origin: Origin,
    source: Option<ObjId>,
    controller: Option<PlayerId>,
    timestamp: Timestamp,
    /// Locked once the effect starts applying (CR 613.6).
    affected: Option<SmallVec<[ObjId; 4]>>,
    /// Has applied in some layer; from then on it keeps applying (CR 613.6).
    started: bool,
    /// Its ability was gone by the time it would have started (CR 613.8
    /// existence dependency); it never applies.
    dead: bool,
    filter: Option<ObjFilter>,
    parts: SmallVec<[Modification; 2]>,
}

/// Working characteristics of every object plus per-object flags.
struct Work<'a> {
    state: &'a GameState,
    ids: Vec<ObjId>,
    index: IdMap<usize>,
    chars: Vec<Characteristics>,
    /// A permanent about to enter, judged as if on the battlefield under
    /// this controller (CR 614.12); it is the last of `ids`.
    entering: Option<(ObjId, PlayerId)>,
    /// CR 305.7: lost the abilities of its rules text.
    lost_printed: Vec<bool>,
    /// The characteristics a first pass ended with, in `ids` order. A
    /// condition on power or toughness that gates an effect in an earlier
    /// layer ("as long as this creature's power is 5 or greater, it can
    /// attack as though it didn't have defender") is judged against these,
    /// since the working power is not final until the last layers.
    settled: Option<Vec<Characteristics>>,
    /// Set while such a condition is being judged.
    use_settled: std::cell::Cell<bool>,
    /// A first pass met such a condition: run the second.
    needs_settled: std::cell::Cell<bool>,
}

impl Work<'_> {
    fn get(&self, id: ObjId) -> Option<&Characteristics> {
        self.index.get(&id).map(|i| &self.chars[*i])
    }

    /// The controller an object has, or will have as it enters.
    fn controller_of(&self, id: ObjId) -> PlayerId {
        match self.entering {
            Some((e, p)) if e == id => p,
            _ => self.state.objects[id].controller,
        }
    }

    fn on_battlefield(&self, id: ObjId) -> bool {
        self.state
            .objects
            .get(id)
            .is_some_and(|o| o.zone == Zone::Battlefield)
    }

    /// Objects a filter matches, judged for an effect from `source`
    /// controlled by `controller`.
    fn matching(
        &self,
        filter: &ObjFilter,
        source: Option<ObjId>,
        controller: Option<PlayerId>,
    ) -> SmallVec<[ObjId; 4]> {
        // Soulbond (CR 702.95b): the source and its partner, if paired.
        if filter.paired {
            return source
                .and_then(|s| Some((s, self.state.partner(s)?)))
                .filter(|(s, t)| self.on_battlefield(*s) && self.on_battlefield(*t))
                .map(|(s, t)| [s, t])
                .into_iter()
                .flatten()
                .collect();
        }
        if filter.enchanted {
            return source
                .and_then(|s| self.state.objects.get(s))
                .and_then(|o| o.attached_to)
                .filter(|t| self.on_battlefield(*t))
                .into_iter()
                .collect();
        }
        // "Creatures you control that are enchanted" (CR 303.4), "equipped
        // creatures": judged with the Auras' and Equipment's working
        // characteristics (`status_ok` would ask the cache being built).
        // "Modified" (CR 700.9) likewise asks what is attached (Upriser
        // Renegade's count of other modified creatures).
        let attached =
            filter.status.enchanted.is_some() || filter.status.equipped || filter.status.modified;
        let rest = if attached {
            let mut g = filter.clone();
            g.status.enchanted = None;
            g.status.equipped = false;
            g.status.modified = false;
            std::borrow::Cow::Owned(g)
        } else {
            std::borrow::Cow::Borrowed(filter)
        };
        self.ids
            .iter()
            .copied()
            .filter(|id| {
                let affectable = self.state.objects[*id].zone == Zone::Battlefield
                    || self.entering.is_some_and(|(e, _)| e == *id);
                let c = &self.chars[self.index[id]];
                affectable
                    && filter.matches(*id, self.controller_of(*id), c, source, controller)
                    // "Commander creatures you own" (Flaming Fist): the
                    // effect's controller owns it.
                    && (!filter.status.you_own
                        || controller.is_some_and(|p| self.state.objects[*id].owner == p))
                    && self.status_ok_working(&rest, *id, c)
                    && (!attached || self.attached_ok(filter, *id))
            })
            .collect()
    }

    /// `GameState::status_ok` for a filter with alternatives, each side
    /// matched against the working characteristics (`status_ok` itself
    /// would ask for the characteristics being built, and recurse):
    /// "Insects and Spiders you control get +1/+1".
    fn status_ok_working(&self, f: &ObjFilter, id: ObjId, c: &Characteristics) -> bool {
        let Some(alt) = &f.or else {
            return self.state.status_ok(f, id);
        };
        let base = ObjFilter {
            or: None,
            ..f.clone()
        };
        let side = |g: &ObjFilter| {
            g.matches(id, self.controller_of(id), c, None, None) && self.status_ok_working(g, id, c)
        };
        side(&base) || side(alt)
    }

    /// `status.enchanted` and `status.equipped` for `id`, from the working
    /// characteristics of what is attached to it.
    fn attached_ok(&self, filter: &ObjFilter, id: ObjId) -> bool {
        let has = |sub: Subtype| {
            self.ids.iter().any(|a| {
                self.on_battlefield(*a)
                    && self.state.objects[*a].attached_to == Some(id)
                    && self.chars[self.index[a]].has_subtype(sub)
            })
        };
        let is = |a: ObjId, sub: Subtype| {
            self.index
                .get(&a)
                .is_some_and(|i| self.chars[*i].has_subtype(sub))
        };
        filter
            .status
            .enchanted
            .is_none_or(|e| has(Subtype::AURA) == e)
            && (!filter.status.equipped || has(Subtype::EQUIPMENT))
            && (!filter.status.modified
                || self
                    .state
                    .modified(id, |a| is(a, Subtype::EQUIPMENT), |a| is(a, Subtype::AURA)))
    }
}

/// Compute the characteristics of every battlefield object.
pub fn compute_battlefield(state: &GameState) -> Derived {
    compute(state, None)
}

/// CR 614.12: the characteristics `obj` would have on the battlefield under
/// `controller`, given the continuous effects now in play (its own static
/// abilities included), to decide which of its "enters" replacement effects
/// apply.
pub fn compute_entering(state: &GameState, obj: ObjId, controller: PlayerId) -> Characteristics {
    match compute(state, Some((obj, controller))) {
        Derived::Printed => state.base_characteristics(&state.objects[obj]),
        Derived::Battlefield(map) => map[&obj],
    }
}

#[cfg(debug_assertions)]
thread_local! {
    /// Whether a layer pass is running on this thread (debug builds).
    static IN_PASS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Debug builds: fails at once, by name, when something inside the layer
/// pass asks for finished characteristics (`GameState::characteristics`)
/// while the cache is invalid. That call starts another pass, which asks
/// again: unbounded recursion and a stack overflow in a release build.
/// Code run by the pass must read the pass's own working characteristics
/// (`Work::chars`).
#[cfg(debug_assertions)]
struct PassGuard;

#[cfg(debug_assertions)]
impl PassGuard {
    fn enter() -> PassGuard {
        IN_PASS.with(|f| {
            assert!(
                !f.replace(true),
                "layers::compute re-entered: code inside the layer pass asked for finished \
                 characteristics; read the pass's working characteristics instead"
            );
        });
        PassGuard
    }
}

#[cfg(debug_assertions)]
impl Drop for PassGuard {
    fn drop(&mut self) {
        IN_PASS.with(|f| f.set(false));
    }
}

#[allow(clippy::too_many_lines)] // the layer loop
fn compute(state: &GameState, entering: Option<(ObjId, PlayerId)>) -> Derived {
    #[cfg(debug_assertions)]
    let _guard = PassGuard::enter();
    match pass(state, entering, None) {
        (Derived::Battlefield(first), true) => pass(state, entering, Some(first)).0,
        (d, _) => d,
    }
}

/// One pass over the layers; the flag says a condition wanted the final
/// power and toughness, which only a second pass given `settled` has.
#[allow(clippy::too_many_lines)]
fn pass(
    state: &GameState,
    entering: Option<(ObjId, PlayerId)>,
    settled: Option<crate::state::CharMap>,
) -> (Derived, bool) {
    // Fast path: nothing can modify anything.
    let plain = |id: &ObjId| {
        let o = &state.objects[*id];
        o.counters.is_empty()
            && o.except.is_none()
            && o.sized == 0
            && !o.riot_haste()
            && !o.suspected()
            && !matches!(
                o.alt,
                crate::state::AltCast::Suspend
                    | crate::state::AltCast::Dash
                    | crate::state::AltCast::Blitz
            )
            && !state.pool.get(o.def).has_static()
    };
    let simple = state.effects.is_empty()
        && state.emblems.is_empty()
        && state.desig.tempted == [0; 4]
        && state.battlefield.iter().all(plain)
        && entering.is_none_or(|(e, _)| plain(&e));
    if simple {
        return (Derived::Printed, false);
    }
    // Emblems (CR 114) are sources of static abilities but, being in the
    // command zone, never affected (`matching` skips them).
    let mut ids: Vec<ObjId> = state.battlefield.clone();
    ids.extend(state.emblems.iter().copied());
    if let Some((e, _)) = entering {
        ids.push(e);
    }
    let mut chars: Vec<Characteristics> = ids
        .iter()
        .map(|id| state.base_characteristics(&state.objects[*id]))
        .collect();
    let index: IdMap<usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let lost_printed = vec![false; ids.len()];
    // In `ids` order, as the working characteristics are.
    let settled: Option<Vec<Characteristics>> =
        settled.and_then(|mut m| ids.iter().map(|id| m.remove(id)).collect());
    let mut work = Work {
        state,
        ids,
        index,
        chars: std::mem::take(&mut chars),
        lost_printed,
        entering,
        settled,
        use_settled: std::cell::Cell::new(false),
        needs_settled: std::cell::Cell::new(false),
    };

    let mut live = stored_effects(state);

    for layer in Layer::ALL {
        // CR 305.7: a land that became a basic land type loses the abilities
        // of its rules text; this takes effect in the ability layer.
        if layer == Layer::L6Ability {
            for (i, lost) in work.lost_printed.iter().enumerate() {
                if *lost {
                    let printed = work
                        .state
                        .base_characteristics(&work.state.objects[work.ids[i]]);
                    let c = &mut work.chars[i];
                    c.keywords = c.keywords.difference(printed.keywords);
                    c.abilities.retain(|a| !printed.abilities.contains(a));
                }
            }
        }
        // CR 701.54c: "Your Ring-bearer is legendary", from The Ring
        // emblem, which entered the game before any permanent it affects.
        if layer == Layer::L4Type && state.desig.tempted != [0; 4] {
            for p in state.players_in_game() {
                if let Some(b) = state.ring_bearer(p)
                    && let Some(&wi) = work.index.get(&b)
                {
                    work.chars[wi].supertypes = work.chars[wi]
                        .supertypes
                        .with(crate::card::Supertype::Legendary);
                }
            }
        }
        gather_statics(&work, &mut live, layer);
        // A static ability granted in layer 6 ("has 'This creature has
        // hexproof as long as it's untapped'", Giant's Amulet) exists once
        // the grant has applied; its own layer-6 effect applies after the
        // grant in the same layer (CR 613.8a: it depends on the grant).
        // A later round applies only the effects it newly gathered.
        let mut from = 0;
        while apply_layer(&mut work, &mut live, from, layer) && layer == Layer::L6Ability {
            from = live.len();
            gather_statics(&work, &mut live, layer);
            if from == live.len() {
                break;
            }
        }
        apply_counters(&mut work, layer);
    }

    let again = work.needs_settled.get() && work.settled.is_none();
    (
        Derived::Battlefield(crate::state::CharMap::new(work.ids, work.chars)),
        again,
    )
}

/// Applies the live effects from index `from` on that have a part in
/// `layer`, in dependency and timestamp order. Returns whether one of them
/// granted a static ability with a layer-6 part.
fn apply_layer(work: &mut Work<'_>, live: &mut [Live], from: usize, layer: Layer) -> bool {
    let mut granted_static = false;
    {
        // Effects with a part in this layer.
        let mut here: Vec<usize> = live
            .iter()
            .enumerate()
            .skip(from)
            .filter(|(_, l)| !l.dead && l.parts.iter().any(|p| p.layer() == layer))
            .map(|(i, _)| i)
            .collect();
        if here.is_empty() {
            return false;
        }
        // Lock affected sets of effects starting now (CR 613.6).
        for &i in &here {
            if live[i].affected.is_none() {
                let l = &live[i];
                let set = work.matching(
                    l.filter.as_ref().expect("filter or locked set"),
                    l.source,
                    l.controller,
                );
                live[i].affected = Some(set);
            }
        }
        here.sort_by_key(|i| live[*i].timestamp);
        let (order, waited) = dependency_order(work, live, &here, layer);
        for i in order {
            // CR 613.6, 613.8: an effect that waited for one it depends on
            // finds what it applies to once that one has applied ("Vehicle
            // creatures you control are the chosen creature type" after the
            // Vehicle is crewed, Lifecraft Engine).
            if !live[i].started
                && waited.contains(&i)
                && let Some(filter) = &live[i].filter
            {
                let set = work.matching(filter, live[i].source, live[i].controller);
                live[i].affected = Some(set);
            }
            if !live[i].started {
                // A static ability's effect exists only if the ability still
                // does once the effects it depends on have applied.
                if let Origin::Static { source, ability } = live[i].origin {
                    let exists = work.index.get(&source).is_some_and(|wi| {
                        !work.lost_printed[*wi] && work.chars[*wi].abilities.contains(&ability)
                    });
                    if !exists {
                        live[i].dead = true;
                        continue;
                    }
                }
                live[i].started = true;
            }
            let l = &live[i];
            let targets = l.affected.clone().unwrap_or_default();
            for part in l.parts.iter().filter(|p| p.layer() == layer) {
                // "has all activated abilities of ...": the abilities those
                // objects have now, each granted (CR 613.1f).
                if let Modification::AddActivatedOf(from) = part {
                    let granted = activated_of(work, l.source, l.controller, from);
                    for t in &targets {
                        if let Some(&wi) = work.index.get(t) {
                            for a in &granted {
                                apply(work, wi, &Modification::AddAbility(*a));
                            }
                        }
                    }
                    continue;
                }
                // "for each ...": the count is the source's, once.
                let part = match part {
                    Modification::ModifyPtPer {
                        power,
                        toughness,
                        count,
                    } => {
                        let n = per_count(work, l.source, l.controller, count);
                        Modification::ModifyPt {
                            power: power.saturating_mul(n),
                            toughness: toughness.saturating_mul(n),
                        }
                    }
                    other => other.clone(),
                };
                if let Modification::AddAbility(a) = part
                    && let AbilityDef::Static(sa) = work.state.pool.ability(a)
                    && sa.parts.iter().any(|p| p.layer() == Layer::L6Ability)
                {
                    granted_static = true;
                }
                for t in &targets {
                    if let Some(&wi) = work.index.get(t) {
                        apply(work, wi, &part);
                    }
                }
            }
        }
    }
    granted_static
}

/// Whether a filter looks at power or toughness.
fn filter_reads_pt(f: &ObjFilter) -> bool {
    f.power.is_some()
        || f.toughness.is_some()
        || f.status.toughness_over_power
        || f.or.as_deref().is_some_and(filter_reads_pt)
}

/// Whether a condition looks at power or toughness.
fn condition_reads_pt(cond: &StaticCondition) -> bool {
    match cond {
        StaticCondition::SourceIs(f)
        | StaticCondition::AttachedIs(f)
        | StaticCondition::YouControlNone(f)
        | StaticCondition::YouControl { filter: f, .. }
        | StaticCondition::OpponentControls { filter: f, .. }
        | StaticCondition::OnBattlefield { filter: f, .. }
        | StaticCondition::YouControlAtMost { filter: f, .. } => filter_reads_pt(f),
        StaticCondition::YouControlEach(fs) => fs.iter().any(filter_reads_pt),
        StaticCondition::TotalPower(_)
        | StaticCondition::DifferentPowers(_)
        | StaticCondition::TotalToughness(_) => true,
        StaticCondition::And(a, b) | StaticCondition::Or(a, b) => {
            condition_reads_pt(a) || condition_reads_pt(b)
        }
        StaticCondition::Not(a) => condition_reads_pt(a),
        _ => false,
    }
}

/// Stored effects (from resolved spells and abilities) as live effects.
fn stored_effects(state: &GameState) -> Vec<Live> {
    state
        .effects
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let (affected, filter, controller) = match &e.affected {
                Affected::Objects(set) => (Some(set.iter().copied().collect()), None, None),
                Affected::Filter { filter, controller } => {
                    (None, Some(filter.clone()), Some(*controller))
                }
            };
            Live {
                origin: Origin::Stored(i),
                source: e.source,
                controller: Some(controller.unwrap_or(e.controller)),
                timestamp: e.timestamp,
                affected,
                filter,
                parts: e.parts.clone(),
                started: false,
                dead: false,
            }
        })
        .collect()
}

/// Whether a conditional static ability's condition holds (CR 611.3a),
/// judged from its source's controller with the characteristics worked
/// out so far.
#[allow(clippy::too_many_lines)] // one arm per condition
fn static_condition_holds(work: &Work<'_>, cond: &StaticCondition, source: ObjId) -> bool {
    let state = work.state;
    let you = work.controller_of(source);
    let chars: &[Characteristics] = match &work.settled {
        Some(s) if work.use_settled.get() => s,
        _ => &work.chars,
    };
    match cond {
        // A token waiting on the state-based check is not a card (CR
        // 111.8, 704.5d).
        StaticCondition::CardsInGraveyard(n) => state.cards_in_graveyard(you) >= *n as usize,
        StaticCondition::UnlockedDoors(n) => state.unlocked_doors(you) >= *n,
        StaticCondition::ExiledWith(n) => state.count_exiled_with(source, None, you) >= *n as usize,
        StaticCondition::TypesExiledWith(n) => state.types_exiled_with(source) >= *n,
        // Permanents still on the battlefield are judged with the
        // characteristics worked out so far, the rest as they last existed.
        StaticCondition::Happened { count, cmp, n }
            if count.who == crate::card::TurnWho::You
                && matches!(
                    count.kind,
                    crate::card::TurnEvent::Entered | crate::card::TurnEvent::Attacked
                ) =>
        {
            match &count.filter {
                Some(f) => cmp.holds(listed(work, chars, count.kind, f, you, source), *n),
                None => state.condition_history(cond, you, source),
            }
        }
        StaticCondition::CardTypesInGraveyard(n) => {
            let mut types = crate::card::TypeSet::default();
            for g in &state.player(you).graveyard {
                types = types.union(state.pool.get(state.objects[*g].def).types);
            }
            types.len() >= *n
        }
        StaticCondition::YouControl { filter, at_least } => {
            let n = work
                .ids
                .iter()
                .filter(|id| {
                    work.on_battlefield(**id)
                        && filter.matches(
                            **id,
                            work.controller_of(**id),
                            &chars[work.index[*id]],
                            Some(source),
                            Some(you),
                        )
                        // "a creature with three or more level counters
                        // on it".
                        && filter
                            .status
                            .counter
                            .is_none_or(|q| state.objects[**id].counters_match(q))
                        // "As long as you control a transformed permanent".
                        && (!filter.status.transformed
                            || state.is_transformed(&state.objects[**id]))
                        // "As long as you control a token" (Druid of the
                        // Spade).
                        && is_token_ok(state, filter, **id)
                })
                .count();
            n >= *at_least as usize
        }
        StaticCondition::YourTurn => state.active_player == you,
        StaticCondition::TopCard(_) => state.condition_now(cond, you, source),
        StaticCondition::SourceAttackedBattle => state.battle_attackers_this_turn.contains(&source),
        StaticCondition::NotYourTurn => state.active_player != you,
        StaticCondition::YourMainPhase => state.active_player == you && state.step.is_main(),
        StaticCondition::YourCombatPhase => {
            state.active_player == you && state.step.phase() == crate::turn::Phase::Combat
        }
        StaticCondition::FirstCombatPhase => {
            state.step.phase() == crate::turn::Phase::Combat && state.combats_this_turn <= 1
        }
        StaticCondition::SourceAttacking => state
            .combat
            .as_ref()
            .is_some_and(|c| c.attackers.iter().any(|a| a.id == source)),
        StaticCondition::SourceMonstrous => state
            .objects
            .get(source)
            .is_some_and(super::state::GameObject::monstrous),
        StaticCondition::SourceSuspected(want) => {
            state
                .objects
                .get(source)
                .is_some_and(super::state::GameObject::suspected)
                == *want
        }
        StaticCondition::SourceEnchanted | StaticCondition::SourceEquipped => {
            let want = if *cond == StaticCondition::SourceEnchanted {
                crate::card::Subtype::AURA
            } else {
                crate::card::Subtype::EQUIPMENT
            };
            work.ids.iter().any(|id| {
                work.on_battlefield(*id)
                    && state.objects[*id].attached_to == Some(source)
                    && chars[work.index[id]].has_subtype(want)
            })
        }
        StaticCondition::OpponentLifeAtMost(n) => state
            .players_in_game()
            .any(|p| p != you && state.player(p).life <= *n),
        StaticCondition::YourLife { n, at_least } => {
            let l = state.player(you).life;
            if *at_least { l >= *n } else { l <= *n }
        }
        // CR 103.4: the starting life total.
        StaticCondition::LifeVsStarting { at_least, below } => {
            let d = state.player(you).life - state.starting_life;
            if *below { d < 0 } else { d >= *at_least }
        }
        StaticCondition::AnyPlayerLifeAtMost(n) => {
            state.players_in_game().any(|p| state.player(p).life <= *n)
        }
        StaticCondition::MoreLifeThanOpponent => {
            let l = state.player(you).life;
            state
                .players_in_game()
                .any(|p| p != you && state.player(p).life < l)
        }
        StaticCondition::BasicLandTypes(n) => {
            let mut seen = crate::mana::ColorSet::default();
            for id in &work.ids {
                let c = &chars[work.index[id]];
                if work.on_battlefield(*id) && work.controller_of(*id) == you && c.is_land() {
                    for s in &c.subtypes {
                        if let Some(col) = s.basic_color() {
                            seen = seen.with(col);
                        }
                    }
                }
            }
            seen.iter().count() >= *n as usize
        }
        StaticCondition::SourceCounters { kind, n, at_least } => {
            let have = state.objects.get(source).map_or(0, |o| o.counters(*kind));
            if *at_least { have >= *n } else { have <= *n }
        }
        // CR 702.176a: cast for its impending cost, with a time counter.
        StaticCondition::Impending => state.objects.get(source).is_some_and(|o| {
            o.alt == crate::state::AltCast::Impending
                && o.counters(crate::card::CounterKind::Time) > 0
        }),
        StaticCondition::OpponentPoisonAtLeast(n) => state
            .players_in_game()
            .any(|p| p != you && state.player(p).poison >= *n),
        StaticCondition::Or(a, b) => {
            static_condition_holds(work, a, source) || static_condition_holds(work, b, source)
        }
        StaticCondition::Not(a) => !static_condition_holds(work, a, source),
        StaticCondition::YouControlEach(filters) => filters.iter().all(|filter| {
            work.ids.iter().any(|id| {
                work.on_battlefield(*id)
                    && filter.matches(
                        *id,
                        work.controller_of(*id),
                        &chars[work.index[id]],
                        Some(source),
                        Some(you),
                    )
            })
        }),
        // "as long as any player controls a black permanent": the filter
        // names no controller.
        StaticCondition::OnBattlefield { filter, at_least } => {
            work.ids
                .iter()
                .filter(|id| {
                    work.on_battlefield(**id)
                        && filter.matches(
                            **id,
                            work.controller_of(**id),
                            &chars[work.index[*id]],
                            Some(source),
                            Some(you),
                        )
                })
                .count()
                >= *at_least as usize
        }
        StaticCondition::YouControlAtMost { filter, n } => {
            work.ids
                .iter()
                .filter(|id| {
                    work.on_battlefield(**id)
                        && filter.matches(
                            **id,
                            work.controller_of(**id),
                            &chars[work.index[*id]],
                            Some(source),
                            Some(you),
                        )
                })
                .count()
                <= *n as usize
        }
        StaticCondition::YouControlNone(filter) => !work.ids.iter().any(|id| {
            work.on_battlefield(*id)
                && filter
                    .tapped
                    .is_none_or(|t| work.state.objects[*id].tapped == t)
                && filter.matches(
                    *id,
                    work.controller_of(*id),
                    &chars[work.index[id]],
                    Some(source),
                    Some(you),
                )
        }),
        // Judged as attackers are declared, or as a permanent enters; not
        // by a static ability.
        StaticCondition::DefenderControls { .. }
        | StaticCondition::DefenderPoisoned
        | StaticCondition::OthersInCombat { .. }
        | StaticCondition::MoreCreaturesThanOpponent { .. }
        | StaticCondition::Escaped => false,
        StaticCondition::AttackedLastTurn => state.attacked_last_turn.contains(&(you, source)),
        // Fixed as the permanent enters (CR 702.33c), so no cache bump.
        StaticCondition::Kicked => state.objects.get(source).is_some_and(|o| o.cast.kicked()),
        StaticCondition::Bargained => state
            .objects
            .get(source)
            .is_some_and(|o| o.cast.bargained()),
        StaticCondition::Saddled => state.saddled.contains(&source),
        StaticCondition::Teamwork => state.objects.get(source).is_some_and(|o| o.cast.teamwork()),
        StaticCondition::Beheld => state.objects.get(source).is_some_and(|o| o.cast.beheld()),
        StaticCondition::OptionalCostPaid => state
            .objects
            .get(source)
            .is_some_and(|o| o.cast.optional_paid()),
        StaticCondition::OffspringPaid => state
            .objects
            .get(source)
            .is_some_and(|o| o.cast.offspring()),
        StaticCondition::ActivatedThisTurn(n) => {
            state
                .activated_once
                .iter()
                .filter(|(id, _)| *id == source)
                .count()
                >= *n as usize
        }
        StaticCondition::EachHandEmpty => state
            .players_in_game()
            .all(|p| state.player(p).hand.is_empty()),
        StaticCondition::LibraryAtMost(n) => state
            .players_in_game()
            .any(|p| state.player(p).library.len() <= *n as usize),
        StaticCondition::CardsInHand { n, at_most } => {
            let h = state.player(you).hand.len();
            if *at_most {
                h <= *n as usize
            } else {
                h >= *n as usize
            }
        }
        StaticCondition::OpponentControls { filter, at_least } => {
            state.players_in_game().filter(|p| *p != you).any(|p| {
                work.ids
                    .iter()
                    .filter(|id| {
                        work.on_battlefield(**id)
                            && work.controller_of(**id) == p
                            && filter.matches(
                                **id,
                                p,
                                &chars[work.index[*id]],
                                Some(source),
                                Some(p),
                            )
                            && filter
                                .tapped
                                .is_none_or(|t| state.objects[**id].tapped == t)
                            && is_token_ok(state, filter, **id)
                    })
                    .count()
                    >= *at_least as usize
            })
        }
        StaticCondition::SourceIs(filter) => work.index.get(&source).is_some_and(|i| {
            filter.matches(
                source,
                work.controller_of(source),
                &chars[*i],
                Some(source),
                Some(you),
            ) && filter
                .tapped
                .is_none_or(|t| state.objects[source].tapped == t)
                // "As long as this creature has a counter on it" (Demon
                // Wall, CR 122.1).
                && filter
                    .status
                    .counter
                    .is_none_or(|q| state.objects[source].counters_match(q))
                // "As long as this creature entered this turn": a new turn
                // bumps `cversion`.
                && (!filter.status.entered_this_turn
                    || state.objects[source].timestamp >= state.turn_started_at)
                && is_token_ok(state, filter, source)
        }),
        StaticCondition::AttachedIs(filter) => state.objects[source]
            .attached_to
            .and_then(|host| work.index.get(&host).map(|i| (host, *i)))
            .is_some_and(|(host, i)| {
                work.on_battlefield(host)
                    && filter.matches(
                        host,
                        work.controller_of(host),
                        &chars[i],
                        Some(source),
                        Some(you),
                    )
                    && filter
                        .tapped
                        .is_none_or(|t| state.objects[host].tapped == t)
                    // "if it has a sleep counter on it" (Venarian Gold).
                    && filter
                        .status
                        .counter
                        .is_none_or(|q| state.objects[host].counters_match(q))
                    && is_token_ok(state, filter, host)
                    // "As long as enchanted creature has toxic" (CR 702.164).
                    && (!filter.status.toxic
                        || chars[i].abilities.iter().any(|a| {
                            matches!(state.pool.ability(*a), AbilityDef::Toxic(_))
                        }))
            }),
        StaticCondition::SourceModified => {
            let has = |id: ObjId, sub: crate::card::Subtype| {
                work.index
                    .get(&id)
                    .is_some_and(|i| chars[*i].has_subtype(sub))
            };
            state.modified(
                source,
                |id| has(id, crate::card::Subtype::EQUIPMENT),
                |id| has(id, crate::card::Subtype::AURA),
            )
        }
        StaticCondition::TotalPower(_) | StaticCondition::DifferentPowers(_) => {
            let powers = work.ids.iter().filter_map(|id| {
                let c = &chars[work.index[id]];
                (work.on_battlefield(*id) && work.controller_of(*id) == you && c.is_creature())
                    .then(|| c.power())
            });
            match cond {
                StaticCondition::TotalPower(n) => {
                    powers.map(|p| p.max(0)).sum::<i32>() >= i32::try_from(*n).unwrap_or(i32::MAX)
                }
                StaticCondition::DifferentPowers(n) => {
                    let mut v: Vec<i32> = powers.collect();
                    v.sort_unstable();
                    v.dedup();
                    v.len() >= *n as usize
                }
                _ => unreachable!(),
            }
        }
        StaticCondition::And(a, b) => {
            static_condition_holds(work, a, source) && static_condition_holds(work, b, source)
        }
        StaticCondition::TotalToughness(n) => {
            work.ids
                .iter()
                .filter_map(|id| {
                    let c = &chars[work.index[id]];
                    (work.on_battlefield(*id) && work.controller_of(*id) == you && c.is_creature())
                        .then(|| c.toughness().max(0))
                })
                .sum::<i32>()
                >= i32::try_from(*n).unwrap_or(i32::MAX)
        }
        StaticCondition::OpponentHasMore(what) => {
            let n = |p: crate::ids::PlayerId| -> i64 {
                match what {
                    crate::card::MoreWhat::Creatures => i64::try_from(
                        work.ids
                            .iter()
                            .filter(|id| {
                                work.on_battlefield(**id)
                                    && work.controller_of(**id) == p
                                    && work.chars[work.index[*id]].is_creature()
                            })
                            .count(),
                    )
                    .unwrap_or(i64::MAX),
                    crate::card::MoreWhat::Life => i64::from(state.player(p).life),
                    crate::card::MoreWhat::CardsInHand => {
                        i64::try_from(state.player(p).hand.len()).unwrap_or(i64::MAX)
                    }
                }
            };
            let mine = n(you);
            state.players_in_game().any(|p| p != you && n(p) > mine)
        }
        StaticCondition::OpponentMoreLands => {
            let lands = |p: crate::ids::PlayerId| {
                work.ids
                    .iter()
                    .filter(|id| {
                        work.on_battlefield(**id)
                            && work.controller_of(**id) == p
                            && chars[work.index[*id]].is_land()
                    })
                    .count()
            };
            let mine = lands(you);
            state.players_in_game().any(|p| p != you && lands(p) > mine)
        }
        _ => state.condition_history(cond, you, source),
    }
}

/// `GameState::listed` for the layer system: your attackers, or the
/// permanents that entered under your control, this turn matching the
/// filter, judged with the characteristics worked out so far if they are
/// still on the battlefield (asking `characteristics` here would compute
/// them again), or as they last existed.
fn listed(
    work: &Work<'_>,
    chars: &[Characteristics],
    kind: crate::card::TurnEvent,
    f: &crate::card::ObjFilter,
    you: crate::ids::PlayerId,
    source: ObjId,
) -> u32 {
    let state = work.state;
    let n = if kind == crate::card::TurnEvent::Attacked {
        state
            .attackers_this_turn
            .iter()
            .filter(|id| {
                let (controller, c) = match work.index.get(*id) {
                    Some(wi) if work.on_battlefield(**id) => (work.controller_of(**id), chars[*wi]),
                    _ => match state.lki.get(*id) {
                        Some(l) => (l.object.controller, l.characteristics),
                        None => return false,
                    },
                };
                controller == you && f.matches(**id, controller, &c, None, Some(you))
            })
            .count()
    } else {
        state
            .entered_this_turn
            .iter()
            .filter(|&&(controller, id)| {
                let c = match work.get(id) {
                    Some(c) => Some(*c),
                    None if state.objects.get(id).is_some() => None,
                    None => state.lki.get(&id).map(|l| l.characteristics),
                };
                controller == you
                    && c.is_some_and(|c| f.matches(id, controller, &c, Some(source), Some(you)))
            })
            .count()
    };
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Static abilities of battlefield permanents whose ability currently exists
/// become live effects (CR 611.3); already-live ones persist (CR 613.6).
fn gather_statics(work: &Work<'_>, live: &mut Vec<Live>, layer: Layer) {
    for id in &work.ids {
        let obj = &work.state.objects[*id];
        let Some(c) = work.get(*id) else {
            continue;
        };
        // A copy of the (inline, `Copy`) set, not a collected vector: this
        // runs for every object in every layer of every pass.
        let abilities = c.abilities;
        for ability in abilities.iter().copied() {
            let AbilityDef::Static(sa) = work.state.pool.ability(ability) else {
                continue;
            };
            if !sa.parts.iter().any(|p| p.layer() == layer) {
                continue;
            }
            if let Some(cond) = &sa.condition {
                // Power and toughness are final only after the last
                // layers: an earlier layer's condition on them waits for
                // a second pass.
                let late = layer < Layer::L7aCda && condition_reads_pt(cond);
                if late {
                    work.needs_settled.set(true);
                }
                work.use_settled.set(late);
                let holds = static_condition_holds(work, cond, *id);
                work.use_settled.set(false);
                if !holds {
                    continue;
                }
            }
            let origin = Origin::Static {
                source: *id,
                ability,
            };
            if live.iter().any(|l| l.origin == origin) {
                continue;
            }
            // CR 604.3a: a "power is equal to ..." the object has only by a
            // grant isn't a characteristic-defining ability: it sets base
            // power in layer 7b, timestamped as the later of the object and
            // the grant (CR 613.7a).
            let granted_cda = sa
                .parts
                .iter()
                .any(|p| matches!(p, Modification::CdaPt { .. }))
                && !work
                    .state
                    .base_characteristics(obj)
                    .abilities
                    .contains(&ability);
            // CR 613.2: characteristic-defining abilities apply before
            // other effects in their layer.
            let timestamp = if granted_cda {
                grant_timestamp(live, ability, *id, obj.timestamp)
            } else if sa.parts.iter().all(Modification::is_cda) {
                0
            } else {
                obj.timestamp
            };
            live.push(Live {
                origin,
                source: Some(*id),
                controller: Some(work.controller_of(*id)),
                timestamp,
                affected: None,
                // "Creatures of the chosen type": the source's choice
                // (CR 607.2a).
                // "Other creatures named ~ get +1/+1": the source's name.
                filter: Some(if sa.affected.enchanted_player {
                    // "Creatures enchanted player controls" (CR 303.4).
                    sa.affected
                        .with_enchanted_player(work.state.attached_player_of(*id))
                } else if sa.affected.chosen {
                    sa.affected.with_chosen(obj.chosen)
                } else if sa.affected.same.is_some() {
                    same_filled(work, &sa.affected, Some(*id)).into_owned()
                } else {
                    sa.affected.clone()
                }),
                // "the chosen color" is the source's (CR 607.2a); with none
                // chosen, that part does nothing.
                parts: live_parts(obj.chosen, &sa.parts, granted_cda),
                started: false,
                dead: false,
            });
        }
    }
}

/// A static ability's parts as they apply: "the chosen color" or type is
/// the source's `chosen`; with none chosen, that part does nothing. A
/// granted "power is equal to ..." applies in layer 7b (`granted_cda`).
fn live_parts(
    chosen: crate::state::Chosen,
    parts: &[Modification],
    granted_cda: bool,
) -> SmallVec<[Modification; 2]> {
    parts
        .iter()
        .filter_map(|p| match p {
            Modification::ChosenColor => chosen
                .color()
                .map(|c| Modification::SetColors(crate::mana::ColorSet::of([c]))),
            Modification::ProtectionFromChosen => chosen.color().map(|c| {
                Modification::AddKeywords(crate::card::KeywordSet::of([
                    crate::card::Keyword::protection_from(c),
                ]))
            }),
            // "Enchanted land is the chosen type."
            Modification::ChosenLandType => match chosen {
                crate::state::Chosen::Subtype(s) => s.basic_color().map(Modification::SetLandType),
                _ => None,
            },
            // "is the chosen type in addition to its other types".
            Modification::AddChosenType => match chosen {
                crate::state::Chosen::Subtype(s) => Some(Modification::AddSubtype(s)),
                _ => None,
            },
            Modification::CdaPt {
                count,
                sets,
                plus_toughness,
                ..
            } if granted_cda => Some(Modification::CdaPt {
                count: count.clone(),
                sets: *sets,
                plus_toughness: *plus_toughness,
                granted: true,
            }),
            p => Some(p.clone()),
        })
        .collect()
}

/// CR 613.7a: a granted static ability's effect is timestamped as the later
/// of its object (`own`) and the effect that granted it to `id`.
fn grant_timestamp(live: &[Live], ability: AbilityId, id: ObjId, own: Timestamp) -> Timestamp {
    live.iter()
        .filter(|l| {
            l.parts.contains(&Modification::AddAbility(ability))
                && l.affected.as_ref().is_some_and(|a| a.contains(&id))
        })
        .map(|l| l.timestamp)
        .fold(own, std::cmp::max)
}

/// CR 613.8: order `here` (already in timestamp order) so that an effect
/// applies after any effect it depends on. Cycles fall back to timestamps
/// (CR 613.8c).
/// Also the effects that depend on another (whose affected sets are found
/// again once those have applied).
fn dependency_order(
    work: &Work<'_>,
    live: &[Live],
    here: &[usize],
    layer: Layer,
) -> (Vec<usize>, Vec<usize>) {
    let n = here.len();
    if n < 2 {
        return (here.to_vec(), Vec::new());
    }
    // deps[a] = set of b such that here[a] depends on here[b].
    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];
    for a in 0..n {
        for b in 0..n {
            if a != b && depends(work, live, &live[here[a]], &live[here[b]], layer) {
                deps[a].push(b);
            }
        }
    }
    let waited: Vec<usize> = (0..n)
        .filter(|a| !deps[*a].is_empty())
        .map(|a| here[a])
        .collect();
    // Kahn's algorithm with timestamp (= input order) tiebreak.
    let mut done = vec![false; n];
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        let next = (0..n).find(|a| !done[*a] && deps[*a].iter().all(|b| done[*b]));
        match next {
            Some(a) => {
                done[a] = true;
                out.push(here[a]);
            }
            None => {
                // Cycle: take the remaining in timestamp order.
                for a in 0..n {
                    if !done[a] {
                        done[a] = true;
                        out.push(here[a]);
                    }
                }
            }
        }
    }
    (out, waited)
}

/// Whether `a` depends on `b` in `layer` (CR 613.8a): applying `b` would
/// change what `a` applies to or whether `a` exists.
fn depends(work: &Work<'_>, _live: &[Live], a: &Live, b: &Live, layer: Layer) -> bool {
    let b_targets = b.affected.clone().unwrap_or_default();
    let b_parts: Vec<&Modification> = b.parts.iter().filter(|p| p.layer() == layer).collect();
    if b_parts.is_empty() {
        return false;
    }
    // "has all activated abilities of creatures you control" waits for
    // what gives those creatures abilities or takes them away (CR 613.8a:
    // b changes what a does). Two such effects depend on each other and
    // fall back to timestamp order.
    if layer == Layer::L6Ability
        && a.parts.iter().any(|p| {
            matches!(p, Modification::AddActivatedOf(f)
                if matches!(**f, crate::card::CountExpr::Permanents(_)))
        })
        && b_parts.iter().any(|p| {
            matches!(
                p,
                Modification::AddAbility(_)
                    | Modification::AddActivatedOf(_)
                    | Modification::RemoveAllAbilities
            )
        })
    {
        return true;
    }
    // Existence: b removes the ability generating a from a's source.
    if let (Origin::Static { source, .. }, true) =
        (a.origin, b_targets.contains(&a.source.unwrap_or_default()))
    {
        let removes = b.parts.iter().any(|p| {
            matches!(
                p,
                Modification::RemoveAllAbilities | Modification::SetLandType(_)
            )
        });
        if removes && work.on_battlefield(source) {
            return true;
        }
    }
    // Applies-to: a's filter evaluates differently once b is applied. Only
    // parts that change something a filter can test need checking.
    let Some(filter) = &a.filter else {
        return false;
    };
    let relevant = match layer {
        Layer::L4Type => {
            filter.types != crate::card::TypeSet::default()
                || filter.not_types != crate::card::TypeSet::default()
                || !filter.not_subtypes.is_empty()
                || !filter.subtypes.is_empty()
        }
        Layer::L5Color => filter.colors.is_some(),
        Layer::L6Ability => filter.no_abilities,
        _ => false,
    };
    if !relevant {
        return false;
    }
    let before = work.matching(filter, a.source, a.controller);
    let mut scratch_chars = work.chars.clone();
    for t in &b_targets {
        if let Some(&wi) = work.index.get(t) {
            for part in &b_parts {
                apply_to(&mut scratch_chars[wi], part, work.state, work.ids[wi]);
            }
        }
    }
    let scratch = Work {
        state: work.state,
        ids: work.ids.clone(),
        index: work.index.clone(),
        chars: scratch_chars,
        lost_printed: work.lost_printed.clone(),
        entering: work.entering,
        settled: None,
        use_settled: std::cell::Cell::new(false),
        needs_settled: std::cell::Cell::new(false),
    };
    let after = scratch.matching(filter, a.source, a.controller);
    before != after
}

fn apply(work: &mut Work<'_>, wi: usize, part: &Modification) {
    if matches!(part, Modification::SetLandType(_)) {
        work.lost_printed[wi] = true;
    }
    let id = work.ids[wi];
    if let Modification::CdaPt {
        count,
        sets,
        plus_toughness,
        ..
    } = part
    {
        // CR 604.3: evaluated against the characteristics as they are
        // after layer 6 (this is layer 7a), from the source's controller.
        let n = per_count(work, Some(id), None, count);
        let c = &mut work.chars[wi];
        if sets.power() {
            c.power = pt16(n);
        }
        if sets.toughness() {
            c.toughness = pt16(n.saturating_add(*plus_toughness));
        }
        return;
    }
    apply_to(&mut work.chars[wi], part, work.state, id);
}

/// A "for each" count for a static ability of `source`.
#[allow(clippy::too_many_lines)] // one arm per count
fn per_count(
    work: &Work<'_>,
    source: Option<ObjId>,
    controller: Option<PlayerId>,
    count: &crate::card::CountExpr,
) -> i32 {
    use crate::card::CountExpr;
    let state = work.state;
    let Some(you) = controller.or_else(|| source.map(|s| work.controller_of(s))) else {
        return 0;
    };
    let n = match count {
        CountExpr::ExiledWith(f) => {
            source.map_or(0, |s| state.count_exiled_with(s, f.as_ref(), you))
        }
        CountExpr::CardsInExile { filter, graveyard } => {
            state.owned_in_exile(you, filter.as_ref(), *graveyard)
        }
        // "for each Aura and Equipment attached to it": judged from the
        // characteristics computed so far ("Aura or Equipment" needs no
        // state, and asking for it here would compute them again).
        CountExpr::Permanents(f) if f.attached_to.is_some() => {
            let host = match f.attached_to {
                Some(crate::card::AttachedTo::Source) => source,
                Some(crate::card::AttachedTo::SourceHost) => {
                    source.and_then(|s| state.objects.get(s)?.attached_to)
                }
                _ => None,
            };
            work.ids
                .iter()
                .filter(|id| {
                    let o = &state.objects[**id];
                    host.is_some()
                        && o.attached_to == host
                        && o.zone == Zone::Battlefield
                        && f.matches(
                            **id,
                            work.controller_of(**id),
                            &work.chars[work.index[*id]],
                            source,
                            Some(you),
                        )
                })
                .count()
        }
        CountExpr::Permanents(f) => work
            .matching(&same_filled(work, f, source), source, Some(you))
            .len(),
        CountExpr::Domain => {
            let mut seen = crate::mana::ColorSet::default();
            for id in &work.ids {
                if work.on_battlefield(*id) && work.controller_of(*id) == you {
                    let c = &work.chars[work.index[id]];
                    if c.is_land() {
                        for s in &c.subtypes {
                            if let Some(col) = s.basic_color() {
                                seen = seen.with(col);
                            }
                        }
                    }
                }
            }
            seen.iter().count()
        }
        // Vivid: "the number of colors among permanents you control"
        // (Squawkroaster's power).
        CountExpr::ColorsAmongPermanents => {
            let mut seen = crate::mana::ColorSet::default();
            for id in &work.ids {
                if work.on_battlefield(*id) && work.controller_of(*id) == you {
                    for col in work.chars[work.index[id]].colors.iter() {
                        seen = seen.with(col);
                    }
                }
            }
            seen.iter().count()
        }
        CountExpr::CardsInGraveyard(f)
        | CountExpr::CardsInAllGraveyards(f)
        | CountExpr::CardsInOpponentsGraveyards(f) => {
            let f = f.as_ref().map(|f| same_filled(work, f, source));
            let all = matches!(count, CountExpr::CardsInAllGraveyards(_));
            let theirs = matches!(count, CountExpr::CardsInOpponentsGraveyards(_));
            // Tokens in a graveyard (gone at the next state-based check,
            // CR 704.5d) are not cards.
            state
                .players
                .iter()
                .enumerate()
                .filter(|(i, _)| all || (usize::from(you.0) == *i) != theirs)
                .flat_map(|(_, p)| p.graveyard.iter())
                .filter(|id| state.objects[**id].kind != crate::state::ObjKind::Token)
                .filter(|id| {
                    f.as_ref().is_none_or(|f| {
                        let o = &state.objects[**id];
                        let c = state.base_characteristics(o);
                        f.matches(**id, o.owner, &c, None, Some(you))
                    })
                })
                .count()
        }
        // "for each scream counter on this Aura": a counter put on or
        // removed from a permanent invalidates the characteristics.
        CountExpr::SourceCounters(kind) => source
            .and_then(|s| state.objects.get(s))
            .map_or(0, |o| o.counters(*kind) as usize),
        CountExpr::HostGraveyard(f) => {
            // Unattached, or attached to what has left (an Aura waiting on
            // the state-based check, CR 704.5m): nothing.
            let Some(host) = source
                .and_then(|s| state.objects.get(s)?.attached_to)
                .filter(|h| state.objects.contains_key(*h))
            else {
                return 0;
            };
            let owner = work.controller_of(host);
            let f = f.as_ref().map(|f| same_filled(work, f, source));
            state
                .player(owner)
                .graveyard
                .iter()
                .filter(|id| state.objects[**id].kind != crate::state::ObjKind::Token)
                .filter(|id| {
                    f.as_ref().is_none_or(|f| {
                        let o = &state.objects[**id];
                        let c = state.base_characteristics(o);
                        f.matches(**id, o.owner, &c, None, Some(you))
                    })
                })
                .count()
        }
        // "the number of counters on this creature": every kind.
        CountExpr::SourceAllCounters => source
            .and_then(|s| state.objects.get(s))
            .map_or(0, |o| o.counters.iter().map(|(_, n)| *n as usize).sum()),
        CountExpr::Happened(c) => state.turn_count(c, you, source.unwrap_or_default()) as usize,
        CountExpr::OpponentsHandAtMost(n) => state
            .players_in_game()
            .filter(|p| *p != you && state.player(*p).hand.len() <= *n as usize)
            .count(),
        CountExpr::OpponentsMoreCards => {
            let mine = state.player(you).hand.len();
            state
                .players_in_game()
                .filter(|p| *p != you && state.player(*p).hand.len() > mine)
                .count()
        }
        CountExpr::TwoToX => {
            let x = source.map_or(0, |s| state.x_of(s));
            return 1i32 << x.min(30);
        }
        // From the mana costs as the layers have them so far, like
        // `GreatestManaValue`.
        CountExpr::Total(crate::card::Stat::ManaValue, f) => {
            let f = same_filled(work, f, source);
            work.matching(&f, source, Some(you))
                .iter()
                .map(|id| {
                    state
                        .pool
                        .get(work.chars[work.index[id]].def)
                        .mana_cost
                        .mana_value() as usize
                })
                .sum()
        }
        CountExpr::CardTypesInGraveyard => state
            .player(you)
            .graveyard
            .iter()
            .map(|id| &state.objects[*id])
            .filter(|o| o.kind != crate::state::ObjKind::Token)
            .fold(crate::card::TypeSet::default(), |acc, o| {
                acc.union(state.pool.get(o.def).types)
            })
            .len() as usize,
        CountExpr::CardTypesInAllGraveyards => state
            .players
            .iter()
            .flat_map(|p| p.graveyard.iter())
            .map(|id| &state.objects[*id])
            .filter(|o| o.kind != crate::state::ObjKind::Token)
            .fold(crate::card::TypeSet::default(), |acc, o| {
                acc.union(state.pool.get(o.def).types)
            })
            .len() as usize,
        CountExpr::Sum(a, b) => {
            return per_count(work, source, controller, a)
                .saturating_add(per_count(work, source, controller, b));
        }
        CountExpr::Plus(k, inner) => {
            return i32::try_from(*k)
                .unwrap_or(0)
                .saturating_add(per_count(work, source, controller, inner));
        }
        CountExpr::Difference(k, inner) => {
            return i32::try_from(*k)
                .unwrap_or(0)
                .saturating_sub(per_count(work, source, controller, inner))
                .max(0);
        }
        CountExpr::Divided { by, up, inner } => {
            let n = u32::try_from(per_count(work, source, controller, inner).max(0)).unwrap_or(0);
            return i32::try_from(CountExpr::divide(n, *by, *up)).unwrap_or(i32::MAX);
        }
        CountExpr::GreatestPowerInGraveyard(f) => {
            return state.greatest_power_in_graveyard(you, f);
        }
        CountExpr::GreatestManaValueInGraveyard(f) => {
            return i32::try_from(state.greatest_mana_value_in_graveyard(you, f.as_ref()))
                .unwrap_or(i32::MAX);
        }
        CountExpr::AtLeast(k, inner) => {
            return i32::try_from(*k)
                .unwrap_or(0)
                .max(per_count(work, source, controller, inner));
        }
        // From the mana costs as the layers have them so far, like
        // `GreatestManaValue`.
        CountExpr::GreatestManaValueOf(f) => {
            let f = same_filled(work, f, source);
            work.matching(&f, source, Some(you))
                .iter()
                .map(|id| {
                    state
                        .pool
                        .get(work.chars[work.index[id]].def)
                        .mana_cost
                        .mana_value() as usize
                })
                .max()
                .unwrap_or(0)
        }
        // "the greatest mana value among permanents you control" (Kraven,
        // Proud Predator), devotion (CR 700.5): from the mana costs of
        // the permanents as the layers have them so far (a copy effect,
        // layer 1, gives the copy's cost).
        CountExpr::GreatestManaValue => work
            .ids
            .iter()
            .filter(|id| {
                state.objects[**id].zone == Zone::Battlefield && work.controller_of(**id) == you
            })
            .map(|id| {
                state
                    .pool
                    .get(work.chars[work.index[id]].def)
                    .mana_cost
                    .mana_value() as usize
            })
            .max()
            .unwrap_or(0),
        CountExpr::Devotion(colors) => work
            .ids
            .iter()
            .filter(|id| {
                state.objects[**id].zone == Zone::Battlefield && work.controller_of(**id) == you
            })
            .map(|id| {
                state
                    .pool
                    .get(work.chars[work.index[id]].def)
                    .mana_cost
                    .devotion(*colors) as usize
            })
            .sum(),
        CountExpr::CardsInHand => state.player(you).hand.len(),
        CountExpr::Life => usize::try_from(state.player(you).life.max(0)).unwrap_or(0),
        CountExpr::Speed => usize::from(state.desig.speed(you)),
        CountExpr::Times(k, inner) => {
            return i32::try_from(*k)
                .unwrap_or(0)
                .saturating_mul(per_count(work, source, controller, inner));
        }
        // Resolution-only counts: nothing in a static ability (they would
        // read the characteristics being computed, or a resolving spell's
        // targets). The card language rejects them in a static ability
        // (`CountExpr::in_static`), so none reaches here from a card.
        CountExpr::SourcePower
        | CountExpr::TargetStat { .. }
        | CountExpr::ThisWay(..)
        | CountExpr::LifeLostThisWay
        | CountExpr::ChosenNumber
        | CountExpr::ThoseObjects
        | CountExpr::TriggerCounters(_)
        | CountExpr::Votes(_)
        | CountExpr::BlockingIt { .. }
        | CountExpr::TargetManaValue(_)
        | CountExpr::CardManaValue
        | CountExpr::GreatestPower(_)
        | CountExpr::DifferentlyNamed(_)
        | CountExpr::Devoured(_)
        | CountExpr::Party(_)
        | CountExpr::SourceToughness
        | CountExpr::OtherInstantSorceryCast { .. }
        | CountExpr::OpponentsAttacked
        | CountExpr::SpentColors
        | CountExpr::TimesKicked
        | CountExpr::EnergyPaid
        | CountExpr::SpentTreasure
        | CountExpr::SpentTotal
        | CountExpr::ThatSpellSpent
        | CountExpr::ThatSpellSpentColors
        | CountExpr::TriggerStat(_)
        | CountExpr::AttachedStat(_)
        | CountExpr::MovedStat { .. }
        | CountExpr::TargetHand(_)
        | CountExpr::GreatestToughness(_)
        | CountExpr::TargetLibrary(_)
        | CountExpr::ThatSpellColors
        | CountExpr::SourceDamage
        | CountExpr::TargetColors(_)
        | CountExpr::Their(_)
        | CountExpr::PlayersDidntDiscard { .. }
        | CountExpr::LibraryTop
        // Power and toughness are what the layers are computing.
        | CountExpr::Total(..)
        | CountExpr::DamageThisWay { .. } => 0,
    };
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// A static ability's filter with "named ~" filled in from its source as
/// the layers have it so far (a copy effect, layer 1, gives the copy's
/// name); other filters as they are.
fn same_filled<'f>(
    work: &Work<'_>,
    f: &'f ObjFilter,
    source: Option<ObjId>,
) -> std::borrow::Cow<'f, ObjFilter> {
    if f.same.is_none() {
        return std::borrow::Cow::Borrowed(f);
    }
    let c = source.and_then(|s| work.index.get(&s).map(|wi| work.chars[*wi]));
    let def = c.map(|c| c.def).filter(|d| {
        *d != work.state.pool.face_down(false) && *d != work.state.pool.face_down(true)
    });
    std::borrow::Cow::Owned(f.with_same(
        def,
        c.map(|c| c.colors).unwrap_or_default(),
        c.map(|c| c.power),
        c.map(|c| c.types).unwrap_or_default(),
    ))
}

/// The activated abilities `from` names for "has all activated abilities
/// of ..." (`Modification::AddActivatedOf`) from a static ability of
/// `source`: a permanent's as worked out so far in this pass (they may be
/// granted ones), a card's in another zone as printed. Basic land types'
/// mana abilities (CR 305.6) are among them though not listed; abilities
/// that work only from a hand or a graveyard are not.
fn activated_of(
    work: &Work<'_>,
    source: Option<ObjId>,
    controller: Option<PlayerId>,
    from: &crate::card::CountExpr,
) -> SmallVec<[AbilityId; 8]> {
    use crate::card::CountExpr;
    let state = work.state;
    let mut out: SmallVec<[AbilityId; 8]> = SmallVec::new();
    let Some(you) = controller.or_else(|| source.map(|s| work.controller_of(s))) else {
        return out;
    };
    let mut take = |c: &Characteristics| {
        if c.is_land() {
            for s in &c.subtypes {
                if let Some(color) = s.basic_color() {
                    let a = state.pool.basic_mana(color);
                    if !out.contains(&a) {
                        out.push(a);
                    }
                }
            }
        }
        for a in c.abilities.iter().copied() {
            let activated = match state.pool.ability(a) {
                AbilityDef::Activated(ab) => !ab.from_hand && !ab.from_graveyard,
                AbilityDef::TapForMana(_)
                | AbilityDef::SacForMana(_)
                | AbilityDef::RestrictedMana { .. } => true,
                _ => false,
            };
            if activated && !out.contains(&a) {
                out.push(a);
            }
        }
    };
    // A card in another zone: its printed characteristics; a token there
    // (gone at the next state-based check, CR 704.5d) is not a card.
    let card_ok = |id: &ObjId, f: Option<&ObjFilter>| {
        let o = &state.objects[*id];
        o.kind != crate::state::ObjKind::Token
            && f.is_none_or(|f| {
                let c = state.base_characteristics(o);
                f.matches(*id, o.owner, &c, None, Some(you))
            })
    };
    match from {
        CountExpr::Permanents(f) => {
            for id in work.matching(&same_filled(work, f, source), source, Some(you)) {
                if let Some(c) = work.get(id) {
                    take(c);
                }
            }
        }
        CountExpr::CardsInGraveyard(f) | CountExpr::CardsInAllGraveyards(f) => {
            let all = matches!(from, CountExpr::CardsInAllGraveyards(_));
            let f = f.as_ref().map(|f| same_filled(work, f, source));
            for (i, p) in state.players.iter().enumerate() {
                if !all && usize::from(you.0) != i {
                    continue;
                }
                for id in &p.graveyard {
                    if card_ok(id, f.as_deref()) {
                        take(&state.base_characteristics(&state.objects[*id]));
                    }
                }
            }
        }
        CountExpr::ExiledWith(f) => {
            if let Some(s) = source {
                for id in state.exiled_with(s) {
                    if card_ok(&id, f.as_ref()) {
                        take(&state.base_characteristics(&state.objects[id]));
                    }
                }
            }
        }
        // "this creature has all activated abilities of that card": the
        // top card of your library, as printed.
        CountExpr::LibraryTop => {
            if let Some(t) = state.player(you).library.last() {
                take(&state.base_characteristics(&state.objects[*t]));
            }
        }
        // The card language compiles no other source.
        _ => {}
    }
    out
}

/// CR 113.2c: an object given the same triggered ability twice has two
/// instances, and each triggers (two Snake Umbras on one creature draw two
/// cards). Other kinds gain nothing from a second copy, and an ability
/// that names its granter is told apart by granter where it triggers.
fn granted_twice(state: &GameState, a: crate::card::AbilityId) -> bool {
    matches!(state.pool.ability(a), crate::card::AbilityDef::Triggered(_))
        && !state.pool.ability_names_granter(a)
}

fn apply_to(c: &mut Characteristics, part: &Modification, state: &GameState, id: ObjId) {
    match part {
        Modification::AddTypes(t) => c.types = c.types.union(*t),
        Modification::BecomeTypes(t) => {
            use crate::card::{CardType, TypeSet};
            let land = c.types.contains(CardType::Land);
            c.types = *t;
            if land {
                c.types = c.types.union(TypeSet::of([CardType::Land]));
            }
            // CR 205.1a, 205.3d: an object that stops being a creature
            // (or kindred) loses its creature types ("It's an enchantment":
            // the Enduring cycle).
            if !t.contains(CardType::Creature) && !t.contains(CardType::Kindred) {
                c.subtypes.retain(|s| !s.is_creature_type());
            }
        }
        Modification::SetTypes(t) => c.types = *t,
        Modification::AddSupertype(s) => c.supertypes = c.supertypes.with(*s),
        Modification::AddLandType(color) => {
            let s = Subtype::basic(*color);
            if !c.subtypes.contains(&s) {
                c.subtypes.push(s);
            }
        }
        Modification::SetLandType(color) => {
            c.subtypes.retain(|s| s.basic_color().is_none());
            c.subtypes.push(Subtype::basic(*color));
        }
        // A permanent keeps at most six subtypes: a seventh given by an
        // effect is dropped (Mistform Sliver activated over and over).
        Modification::AddSubtype(s) => {
            if !c.subtypes.contains(s) && !c.subtypes.is_full() {
                c.subtypes.push(*s);
            }
        }
        // CR 205.1b: the new creature type replaces the old ones; a
        // changeling's every-type (CR 702.73a, applied earlier in layer 4)
        // goes too.
        Modification::SetCreatureType(s) => {
            c.subtypes.retain(|x| !x.is_creature_type());
            c.subtypes.push(*s);
            c.keywords = c.keywords.difference(crate::card::KeywordSet::of([
                crate::card::Keyword::Changeling,
            ]));
        }
        // "is every creature type": one entry stands for all of them.
        Modification::AllCreatureTypes => {
            c.subtypes.retain(|x| !x.is_creature_type());
            c.subtypes.push(Subtype::ALL_CREATURE_TYPES);
        }
        Modification::NoCreatureTypes => {
            c.subtypes.retain(|x| !x.is_creature_type());
            c.keywords = c.keywords.difference(crate::card::KeywordSet::of([
                crate::card::Keyword::Changeling,
            ]));
        }
        // Control is not a characteristic; `GameState::sync_control`
        // applies layer 2 to `GameObject::controller`. A P/T-defining
        // ability needs the working characteristics of other objects;
        // `apply` handles it, and a dependency probe never reaches layer
        // 7a (it has no filter-relevant parts). "The chosen color" is
        // resolved against the source as the static ability goes live
        // (`gather_statics`).
        Modification::Control
        | Modification::CdaPt { .. }
        | Modification::ModifyPtPer { .. }
        | Modification::AddActivatedOf(_)
        | Modification::ChosenColor
        | Modification::ProtectionFromChosen
        | Modification::ChosenLandType
        | Modification::AddChosenType => {}
        Modification::AddColors(cs) => c.colors = c.colors.union(*cs),
        Modification::SetColors(cs) | Modification::CdaColors(cs) => c.colors = *cs,
        Modification::AddKeywords(k) => c.keywords = c.keywords.union(*k),
        Modification::AddAbility(a) => {
            if (granted_twice(state, *a) || !c.abilities.contains(a)) && c.abilities.len() < 8 {
                c.abilities.push(*a);
            }
            // A granted protection from a quality (CR 702.16): the keyword
            // `protected_from` checks before looking for it.
            if matches!(
                state.pool.ability(*a),
                crate::card::AbilityDef::Protection(_)
            ) {
                c.keywords = c.keywords.with(crate::card::Keyword::ProtectionFromFilter);
            }
            if matches!(
                state.pool.ability(*a),
                crate::card::AbilityDef::HexproofFrom(_)
            ) {
                c.keywords = c.keywords.with(crate::card::Keyword::HexproofFromFilter);
            }
            if matches!(state.pool.ability(*a), crate::card::AbilityDef::Landwalk(_)) {
                c.keywords = c.keywords.with(crate::card::Keyword::LandwalkFilter);
            }
        }
        Modification::RemoveKeywords(k) => c.keywords = c.keywords.difference(*k),
        Modification::RemoveAllAbilities => {
            c.keywords = crate::card::KeywordSet::NONE;
            c.abilities.clear();
        }
        Modification::SetPt { power, toughness } => {
            let mv = i32::try_from(state.pool.get(c.def).mana_cost.mana_value()).unwrap_or(0);
            let v = |p: &PtValue, cur: i32| match p {
                PtValue::Fixed(n) => *n,
                PtValue::ManaValue => mv,
                PtValue::Unchanged => cur,
            };
            c.power = pt16(v(power, c.power()));
            c.toughness = pt16(v(toughness, c.toughness()));
        }
        Modification::ModifyPt { power, toughness } => {
            c.power = pt16(c.power() + power);
            c.toughness = pt16(c.toughness() + toughness);
        }
        Modification::SwitchPt => std::mem::swap(&mut c.power, &mut c.toughness),
    }
    let _ = id;
}

/// CR 613.4c: +1/+1 and -1/-1 counters apply in layer 7c; keyword
/// counters (CR 122.1b) in layer 6.
fn apply_counters(work: &mut Work<'_>, layer: Layer) {
    if layer == Layer::L6Ability {
        for (i, id) in work.ids.iter().enumerate() {
            let o = &work.state.objects[*id];
            if o.zone != Zone::Battlefield {
                continue;
            }
            for (k, n) in &o.counters {
                if *n > 0
                    && let Some(kw) = k.keyword()
                {
                    work.chars[i].keywords = work.chars[i].keywords.with(kw);
                }
            }
            // CR 702.62a: a creature cast by suspend has haste; CR
            // 702.109a: so does a dashed one; CR 702.136a: and one that
            // entered with riot's haste.
            if o.riot_haste()
                || matches!(
                    o.alt,
                    crate::state::AltCast::Suspend
                        | crate::state::AltCast::Dash
                        | crate::state::AltCast::Blitz
                )
            {
                work.chars[i].keywords = work.chars[i].keywords.with(crate::card::Keyword::Haste);
            }
            // CR 701.60c: a suspected creature has menace and "This
            // creature can't block."
            if o.suspected() {
                work.chars[i].keywords = work.chars[i]
                    .keywords
                    .with(crate::card::Keyword::Menace)
                    .with(crate::card::Keyword::CantBlock);
            }
        }
        return;
    }
    if layer != Layer::L7cModifyPt {
        return;
    }
    let mut hone: SmallVec<[(ObjId, u32); 2]> = SmallVec::new();
    for (i, id) in work.ids.iter().enumerate() {
        let o = &work.state.objects[*id];
        if o.zone != Zone::Battlefield {
            continue;
        }
        // CR 613.4c: every power/toughness counter, +1/+1 and -1/-1 and
        // the rarer kinds alike.
        let (mut dp, mut dt) = (0i32, 0i32);
        for (k, n) in &o.counters {
            if let Some((p, t)) = k.pt() {
                let n = i32::try_from(*n).unwrap_or(i32::MAX / 4);
                dp = dp.saturating_add(p.saturating_mul(n));
                dt = dt.saturating_add(t.saturating_mul(n));
            } else if *k == crate::card::CounterKind::Hone
                && let Some(to) = o.attached_to
            {
                hone.push((to, *n));
            }
        }
        if dp == 0 && dt == 0 {
            continue;
        }
        let c = &mut work.chars[i];
        c.power = pt16(c.power() + dp);
        c.toughness = pt16(c.toughness() + dt);
    }
    // Hone counters (Dwalin, Weaponmaster): "Each hone counter on an
    // Equipment grants +1/+0 to equipped creature."
    for (to, n) in hone {
        if let Some(&i) = work.index.get(&to)
            && work.state.objects[to].zone == Zone::Battlefield
        {
            let c = &mut work.chars[i];
            c.power = pt16(c.power() + i32::try_from(n).unwrap_or(i32::MAX / 4));
        }
    }
}

/// A filter's "token" / "nontoken" for a static ability's condition (CR
/// 111.1): whether the object is a token is its own state, not a
/// characteristic.
fn is_token_ok(state: &GameState, filter: &crate::card::ObjFilter, id: ObjId) -> bool {
    filter
        .status
        .token
        .is_none_or(|t| (state.objects[id].kind == crate::state::ObjKind::Token) == t)
}
