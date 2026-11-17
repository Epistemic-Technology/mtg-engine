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
    AbilityDef, AbilityId, CdaCount, CounterKind, Layer, Modification, ObjFilter, PtValue, Subtype,
};
use crate::ids::{IdMap, ObjId, PlayerId, Timestamp};
use crate::state::{Affected, Characteristics, Derived, GameState, Zone};

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
        if filter.enchanted {
            return source
                .and_then(|s| self.state.objects.get(s))
                .and_then(|o| o.attached_to)
                .filter(|t| self.on_battlefield(*t))
                .into_iter()
                .collect();
        }
        self.ids
            .iter()
            .copied()
            .filter(|id| {
                let affectable = self.state.objects[*id].zone == Zone::Battlefield
                    || self.entering.is_some_and(|(e, _)| e == *id);
                let c = &self.chars[self.index[id]];
                affectable && filter.matches(*id, self.controller_of(*id), c, source, controller)
            })
            .collect()
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

fn compute(state: &GameState, entering: Option<(ObjId, PlayerId)>) -> Derived {
    // Fast path: nothing can modify anything.
    let plain = |id: &ObjId| {
        let o = &state.objects[*id];
        o.counters.is_empty() && o.except.is_none() && !state.pool.get(o.def).has_static()
    };
    let simple = state.effects.is_empty()
        && state.emblems.is_empty()
        && state.battlefield.iter().all(plain)
        && entering.is_none_or(|(e, _)| plain(&e));
    if simple {
        return Derived::Printed;
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
    let mut work = Work {
        state,
        ids,
        index,
        chars: std::mem::take(&mut chars),
        lost_printed,
        entering,
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
        gather_statics(&work, &mut live, layer);
        // Effects with a part in this layer.
        let mut here: Vec<usize> = live
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.dead && l.parts.iter().any(|p| p.layer() == layer))
            .map(|(i, _)| i)
            .collect();
        if here.is_empty() {
            apply_counters(&mut work, layer);
            continue;
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
        let order = dependency_order(&work, &live, &here, layer);
        for i in order {
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
                for t in &targets {
                    if let Some(&wi) = work.index.get(t) {
                        apply(&mut work, wi, part);
                    }
                }
            }
        }
        apply_counters(&mut work, layer);
    }

    Derived::Battlefield(work.ids.into_iter().zip(work.chars).collect())
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

/// Static abilities of battlefield permanents whose ability currently exists
/// become live effects (CR 611.3); already-live ones persist (CR 613.6).
fn gather_statics(work: &Work<'_>, live: &mut Vec<Live>, layer: Layer) {
    for id in &work.ids {
        let obj = &work.state.objects[*id];
        let Some(c) = work.get(*id) else {
            continue;
        };
        for ability in c.abilities.iter().copied().collect::<Vec<_>>() {
            let AbilityDef::Static(sa) = work.state.pool.ability(ability) else {
                continue;
            };
            if !sa.parts.iter().any(|p| p.layer() == layer) {
                continue;
            }
            let origin = Origin::Static {
                source: *id,
                ability,
            };
            if live.iter().any(|l| l.origin == origin) {
                continue;
            }
            // CR 613.2: characteristic-defining abilities apply before
            // other effects in their layer.
            let timestamp = if sa.parts.iter().all(Modification::is_cda) {
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
                filter: Some(sa.affected.clone()),
                parts: sa.parts.iter().cloned().collect(),
                started: false,
                dead: false,
            });
        }
    }
}

/// CR 613.8: order `here` (already in timestamp order) so that an effect
/// applies after any effect it depends on. Cycles fall back to timestamps
/// (CR 613.8c).
fn dependency_order(work: &Work<'_>, live: &[Live], here: &[usize], layer: Layer) -> Vec<usize> {
    let n = here.len();
    if n < 2 {
        return here.to_vec();
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
    out
}

/// Whether `a` depends on `b` in `layer` (CR 613.8a): applying `b` would
/// change what `a` applies to or whether `a` exists.
fn depends(work: &Work<'_>, _live: &[Live], a: &Live, b: &Live, layer: Layer) -> bool {
    let b_targets = b.affected.clone().unwrap_or_default();
    let b_parts: Vec<&Modification> = b.parts.iter().filter(|p| p.layer() == layer).collect();
    if b_parts.is_empty() {
        return false;
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
        plus_toughness,
    } = part
    {
        // CR 604.3: evaluated against the characteristics as they are
        // after layer 6 (this is layer 7a).
        let n = cda_count(work, id, count);
        let c = &mut work.chars[wi];
        c.power = n;
        c.toughness = n + plus_toughness;
        return;
    }
    apply_to(&mut work.chars[wi], part, work.state, id);
}

/// What a power/toughness-defining ability of `source` counts right now.
fn cda_count(work: &Work<'_>, source: ObjId, count: &CdaCount) -> i32 {
    let state = work.state;
    // Tokens in a graveyard (gone at the next state-based check, CR
    // 704.5d) are not cards.
    let cards = || {
        state
            .players
            .iter()
            .flat_map(|p| p.graveyard.iter())
            .map(|id| &state.objects[*id])
            .filter(|o| o.kind != crate::state::ObjKind::Token)
    };
    let n = match count {
        CdaCount::CardTypesInGraveyards => cards()
            .fold(crate::card::TypeSet::default(), |acc, o| {
                acc.union(state.pool.get(o.def).types)
            })
            .len(),
        CdaCount::CreatureCardsInGraveyards => u32::try_from(
            cards()
                .filter(|o| state.pool.get(o.def).is_creature())
                .count(),
        )
        .unwrap_or(u32::MAX),
        CdaCount::Permanents(filter) => {
            let controller = work.controller_of(source);
            u32::try_from(work.matching(filter, Some(source), Some(controller)).len())
                .unwrap_or(u32::MAX)
        }
        CdaCount::ControllerLife => {
            u32::try_from(state.player(work.controller_of(source)).life).unwrap_or(0)
        }
    };
    i32::try_from(n).unwrap_or(i32::MAX)
}

fn apply_to(c: &mut Characteristics, part: &Modification, state: &GameState, id: ObjId) {
    match part {
        Modification::AddTypes(t) => c.types = c.types.union(*t),
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
        Modification::AddSubtype(s) => {
            if !c.subtypes.contains(s) {
                c.subtypes.push(*s);
            }
        }
        // Control is not a characteristic; `GameState::sync_control`
        // applies layer 2 to `GameObject::controller`. A P/T-defining
        // ability needs the working characteristics of other objects;
        // `apply` handles it, and a dependency probe never reaches layer
        // 7a (it has no filter-relevant parts).
        Modification::Control | Modification::CdaPt { .. } => {}
        Modification::AddColors(cs) => c.colors = c.colors.union(*cs),
        Modification::SetColors(cs) | Modification::CdaColors(cs) => c.colors = *cs,
        Modification::AddKeywords(k) => c.keywords = c.keywords.union(*k),
        Modification::RemoveAllAbilities => {
            c.keywords = crate::card::KeywordSet::NONE;
            c.abilities.clear();
        }
        Modification::SetPt { power, toughness } => {
            let mv = i32::try_from(state.pool.get(c.def).mana_cost.mana_value()).unwrap_or(0);
            let v = |p: &PtValue| match p {
                PtValue::Fixed(n) => *n,
                PtValue::ManaValue => mv,
            };
            c.power = v(power);
            c.toughness = v(toughness);
        }
        Modification::ModifyPt { power, toughness } => {
            c.power += power;
            c.toughness += toughness;
        }
        Modification::SwitchPt => std::mem::swap(&mut c.power, &mut c.toughness),
    }
    let _ = id;
}

/// CR 613.4c: +1/+1 and -1/-1 counters apply in layer 7c.
fn apply_counters(work: &mut Work<'_>, layer: Layer) {
    if layer != Layer::L7cModifyPt {
        return;
    }
    for (i, id) in work.ids.iter().enumerate() {
        let o = &work.state.objects[*id];
        if o.zone != Zone::Battlefield {
            continue;
        }
        let plus = i32::try_from(o.counters(CounterKind::PlusOnePlusOne)).unwrap_or(0);
        let minus = i32::try_from(o.counters(CounterKind::MinusOneMinusOne)).unwrap_or(0);
        work.chars[i].power += plus - minus;
        work.chars[i].toughness += plus - minus;
    }
}
