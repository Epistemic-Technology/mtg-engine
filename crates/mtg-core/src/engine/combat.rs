//! Combat (CR 506–511): attacker and blocker legality, declarations, and damage assignment.

use smallvec::SmallVec;

use super::MAX_DIVISION_OPTIONS;
use crate::card::{AbilityDef, BlockRule, Keyword};
use crate::event::Event;
use crate::ids::{ObjId, PlayerId};
use crate::state::{Attacker, Characteristics, DamageAssignment, Flow, GameState, Target, Zone};

impl GameState {
    // ----- combat -----------------------------------------------------------------------

    /// CR 508.1a: untapped creatures controlled since the turn began or with
    /// haste, without defender unless they can attack as though they didn't
    /// have it (CR 702.3b).
    pub(super) fn eligible_attackers(&self, p: PlayerId) -> impl Iterator<Item = ObjId> + '_ {
        self.creatures_of(p).filter(move |id| {
            let o = &self.objects[*id];
            let c = self.characteristics(*id).expect("creature exists");
            !o.tapped
                && (o.controlled_since_turn_start || c.has(Keyword::Haste))
                && (!c.has(Keyword::Defender) || c.has(Keyword::AttackDespiteDefender))
                && !c.has(Keyword::CantAttack)
                && self.unless_rules_allow(*id, &c, false)
        })
    }

    /// "Can't attack unless ..." / "can't block unless ..." (CR 508.1c,
    /// 509.1c): whether `id`'s conditions let it attack (or block).
    fn unless_rules_allow(
        &self,
        id: ObjId,
        c: &crate::state::Characteristics,
        block: bool,
    ) -> bool {
        let o = &self.objects[id];
        if !self.flags_of(o).has_attack_rule() {
            return true;
        }
        c.abilities.iter().all(|a| match self.pool.ability(*a) {
            AbilityDef::AttackUnless {
                condition,
                attack: at,
                block: bl,
            } => {
                let applies = if block { *bl } else { *at };
                !applies || self.condition_now(self.pool.condition(*condition), o.controller, id)
            }
            _ => true,
        })
    }

    /// "Can't attack unless at least two other creatures attack" (CR
    /// 508.1c, 509.1c): whether `id`'s declaration-wide conditions hold
    /// with the other creatures of `set` attacking (or blocking).
    fn set_rules_allow(&self, id: ObjId, set: &[ObjId], block: bool) -> bool {
        let Some(o) = self.objects.get(id) else {
            return true;
        };
        if !self.flags_of(o).has_attack_rule() {
            return true;
        }
        let Some(c) = self.characteristics(id) else {
            return true;
        };
        c.abilities.iter().all(|a| match self.pool.ability(*a) {
            AbilityDef::AttackUnless {
                condition,
                attack: at,
                block: bl,
            } if (if block { *bl } else { *at }) => match self.pool.condition(*condition) {
                crate::card::StaticCondition::OthersInCombat {
                    filter, at_least, ..
                } => {
                    let n = set
                        .iter()
                        .filter(|x| **x != id)
                        .filter(|x| {
                            let Some(xo) = self.objects.get(**x) else {
                                return false;
                            };
                            self.characteristics(**x).is_some_and(|xc| {
                                filter.matches(
                                    **x,
                                    xo.controller,
                                    &xc,
                                    Some(id),
                                    Some(o.controller),
                                )
                            }) && self.status_ok(filter, **x)
                        })
                        .count();
                    n >= *at_least as usize
                }
                _ => true,
            },
            _ => true,
        })
    }

    /// Whether every creature of an attack (or block) declaration's set
    /// may take part with the others (`set_rules_allow`).
    pub(super) fn set_legal(&self, set: &[ObjId], block: bool) -> bool {
        self.attack_rule_sources == 0 || set.iter().all(|id| self.set_rules_allow(*id, set, block))
    }

    /// The largest part of a declaration's set whose creatures' conditions
    /// on the others hold: those that break one stay out, until none does.
    pub(super) fn legal_part(&self, set: &[ObjId], block: bool) -> Vec<ObjId> {
        let mut out = set.to_vec();
        while let Some(i) = out
            .iter()
            .position(|id| !self.set_rules_allow(*id, &out, block))
        {
            out.remove(i);
        }
        out
    }

    /// CR 509.1a: untapped creatures of the defending player.
    pub(super) fn eligible_blockers(&self, p: PlayerId) -> impl Iterator<Item = ObjId> + '_ {
        self.creatures_of(p).filter(move |id| {
            !self.objects[*id].tapped
                && self.characteristics(*id).is_some_and(|c| {
                    !c.has(Keyword::CantBlock) && self.unless_rules_allow(*id, &c, true)
                })
        })
    }

    /// Evasion: flying (CR 702.9b).
    pub(super) fn can_block(&self, blocker: ObjId, attacker: ObjId) -> bool {
        let (Some(b), Some(a)) = (
            self.characteristics(blocker),
            self.characteristics(attacker),
        ) else {
            return false;
        };
        if a.has(Keyword::Unblockable) {
            return false;
        }
        // CR 702.16e: can't be blocked by creatures with the quality.
        if a.protected_from(&b, &self.pool) {
            return false;
        }
        // CR 509.1b: blocking restrictions of the attacker and the blocker.
        if !self.block_rules_allow(blocker, &b, attacker, &a) {
            return false;
        }
        if a.keywords.intersects(crate::card::KeywordSet::EVASION) || b.has(Keyword::Shadow) {
            let artifact = b.types.contains(crate::card::CardType::Artifact);
            // CR 702.28b: shadow blocks and is blocked only by shadow.
            if a.has(Keyword::Shadow) != b.has(Keyword::Shadow) {
                return false;
            }
            // CR 702.36b
            if a.has(Keyword::Fear) && !(artifact || b.colors.contains(crate::mana::Color::Black)) {
                return false;
            }
            // CR 702.13b
            if a.has(Keyword::Intimidate) && !(artifact || b.colors.intersects(a.colors)) {
                return false;
            }
            // CR 702.31b
            if a.has(Keyword::Horsemanship) && !b.has(Keyword::Horsemanship) {
                return false;
            }
            // CR 702.118b
            if a.has(Keyword::Skulk) && b.power > a.power {
                return false;
            }
        }
        // CR 702.14b: landwalk makes the attacker unblockable while the
        // defending player controls a land of that basic type.
        if a.keywords.intersects(crate::card::KeywordSet::LANDWALK) {
            let defender = self.objects[blocker].controller;
            for c in crate::mana::Color::ALL {
                if a.has(Keyword::landwalk_for(c)) && self.controls_basic_type(defender, c) {
                    return false;
                }
            }
        }
        // CR 702.9b: flying is blocked only by flying or reach.
        !a.has(Keyword::Flying) || b.has(Keyword::Flying) || b.has(Keyword::Reach)
    }

    /// The attacker's and the blocker's `BlockRule`s (other than "more than
    /// one", which `blockers_ok` checks).
    fn block_rules_allow(
        &self,
        blocker: ObjId,
        b: &Characteristics,
        attacker: ObjId,
        a: &Characteristics,
    ) -> bool {
        let bc = self.objects[blocker].controller;
        let ac = self.objects[attacker].controller;
        for id in &a.abilities {
            if let AbilityDef::BlockRule(r) = self.pool.ability(*id) {
                let ok = match r {
                    BlockRule::NotBy(f) => {
                        !(f.matches(blocker, bc, b, Some(attacker), Some(ac))
                            && self.status_ok(f, blocker))
                    }
                    BlockRule::OnlyBy(f) => {
                        f.matches(blocker, bc, b, Some(attacker), Some(ac))
                            && self.status_ok(f, blocker)
                    }
                    _ => true,
                };
                if !ok {
                    return false;
                }
            }
        }
        for id in &b.abilities {
            if let AbilityDef::BlockRule(r) = self.pool.ability(*id) {
                let ok = match r {
                    BlockRule::BlocksOnly(f) => {
                        f.matches(attacker, ac, a, Some(blocker), Some(bc))
                            && self.status_ok(f, attacker)
                    }
                    BlockRule::CantBlock(f) => {
                        !(f.matches(attacker, ac, a, Some(blocker), Some(bc))
                            && self.status_ok(f, attacker))
                    }
                    _ => true,
                };
                if !ok {
                    return false;
                }
            }
        }
        true
    }

    /// "can't be blocked by more than one creature".
    pub(super) fn at_most_one_blocker(&self, attacker: ObjId) -> bool {
        self.characteristics(attacker).is_some_and(|c| {
            c.abilities.iter().any(|id| {
                matches!(
                    self.pool.ability(*id),
                    AbilityDef::BlockRule(BlockRule::AtMostOne)
                )
            })
        })
    }

    /// How many attackers `blocker` may block (CR 509.1a): one, two with
    /// "can block an additional creature each combat", or any number.
    pub(super) fn block_capacity(&self, blocker: ObjId) -> usize {
        match self.characteristics(blocker) {
            Some(c) if c.has(Keyword::BlockAny) => usize::MAX,
            Some(c) if c.has(Keyword::BlockAdditional) => 2,
            _ => 1,
        }
    }

    /// The blocking requirements (CR 509.1c) on this declaration: creatures
    /// that block if able, attackers that must be blocked if able, and
    /// attackers all creatures able to block must block.
    pub(super) fn block_requirements(
        &self,
        attackers: &[ObjId],
        blockers: &[ObjId],
    ) -> BlockRequirements {
        let mut reqs = BlockRequirements::default();
        for b in blockers {
            if self
                .characteristics(*b)
                .is_some_and(|c| c.has(Keyword::MustBlock))
            {
                reqs.must_block.push(*b);
            }
        }
        for a in attackers {
            if let Some(c) = self.characteristics(*a) {
                if c.has(Keyword::MustBeBlocked) {
                    reqs.must_be_blocked.push(*a);
                }
                if c.has(Keyword::Lure) {
                    reqs.lure.push(*a);
                }
            }
        }
        // "Target creature blocks this creature this turn if able".
        for (b, a) in &self.block_demands {
            if blockers.contains(b) && attackers.contains(a) {
                reqs.pairs.push((*b, *a));
            }
        }
        reqs
    }

    /// A lone blocker that can't block alone (CR 506.5): the declaration
    /// breaks a restriction.
    pub(super) fn blocks_alone_illegally(&self, blocks: &[(ObjId, ObjId)]) -> bool {
        blocks.first().is_some_and(|(first, _)| {
            blocks.iter().all(|(b, _)| b == first)
                && self
                    .characteristics(*first)
                    .is_some_and(|c| c.has(Keyword::CantBlockAlone))
        })
    }

    /// Whether `p` controls a land with the basic land type of `c`.
    fn controls_basic_type(&self, p: PlayerId, c: crate::mana::Color) -> bool {
        self.permanents_of(p).any(|id| {
            self.characteristics(id).is_some_and(|ch| {
                ch.is_land() && ch.subtypes.iter().any(|s| s.basic_color() == Some(c))
            })
        })
    }

    pub(super) fn has_menace(&self, attacker: ObjId) -> bool {
        self.characteristics(attacker)
            .is_some_and(|c| c.has(Keyword::Menace))
    }

    /// Menace (CR 702.110b): an attacker is blocked by zero or at least two.
    pub(super) fn block_set_legal(&self, blocks: &[(ObjId, ObjId)]) -> bool {
        if self.attack_rule_sources > 0 {
            let mut blockers: SmallVec<[ObjId; 4]> = SmallVec::new();
            for (b, _) in blocks {
                if !blockers.contains(b) {
                    blockers.push(*b);
                }
            }
            if !self.set_legal(&blockers, true) {
                return false;
            }
        }
        let Some(combat) = &self.combat else {
            return true;
        };
        combat.attackers.iter().all(|a| {
            let n = blocks.iter().filter(|(_, x)| *x == a.id).count();
            (!self.has_menace(a.id) || n != 1) && (n <= 1 || !self.at_most_one_blocker(a.id))
        })
    }

    /// An attacker that can't be blocked by more than one creature has
    /// more (no later block can fix it).
    fn too_many_blockers(&self, blocks: &[(ObjId, ObjId)]) -> bool {
        blocks.iter().any(|(_, a)| {
            blocks.iter().filter(|(_, x)| x == a).count() > 1 && self.at_most_one_blocker(*a)
        })
    }

    /// Whether a partial incremental block declaration can still be
    /// completed legally with the remaining free blockers.
    pub(super) fn block_set_completable(
        &self,
        blocks: &[(ObjId, ObjId)],
        all_blockers: &[ObjId],
        can_block: &[Vec<ObjId>],
    ) -> bool {
        if self.too_many_blockers(blocks) {
            return false;
        }
        if self.block_set_legal(blocks) {
            return true;
        }
        let free: Vec<usize> = (0..all_blockers.len())
            .filter(|i| !blocks.iter().any(|(b, _)| *b == all_blockers[*i]))
            .collect();
        let attackers: Vec<ObjId> = self
            .combat
            .as_ref()
            .map(|c| c.attackers.iter().map(|a| a.id).collect())
            .unwrap_or_default();
        // Each menace attacker with exactly one blocker needs a distinct free
        // blocker able to block it. Small numbers; check greedily.
        let mut used = Vec::new();
        for a in attackers {
            let n = blocks.iter().filter(|(_, x)| *x == a).count();
            if self.has_menace(a) && n == 1 {
                let Some(i) = free
                    .iter()
                    .find(|i| !used.contains(*i) && can_block[**i].contains(&a))
                else {
                    return false;
                };
                used.push(*i);
            }
        }
        true
    }

    /// Attackers declared. If the defender controls a planeswalker, each
    /// attacker chooses what it attacks (CR 508.1a); otherwise they all
    /// attack the defending player.
    pub(super) fn finish_attackers(&mut self, mut set: Vec<ObjId>) {
        // CR 506.5: a creature that can't attack alone attacks only with
        // another; declared alone, it stays home.
        if let [only] = set.as_slice()
            && self
                .characteristics(*only)
                .is_some_and(|c| c.has(Keyword::CantAttackAlone))
        {
            set.clear();
        }
        // "Can't attack unless at least two other creatures attack": a
        // creature whose condition fails stays home.
        if !self.set_legal(&set, false) {
            set = self.legal_part(&set, false);
        }
        let defender = self.opponent(self.active_player);
        if !set.is_empty() && self.planeswalkers_of(defender).next().is_some() {
            self.flow = crate::state::Flow::AttackTargets {
                chosen: Vec::new(),
                remaining: set,
            };
            return;
        }
        let chosen = set
            .into_iter()
            .map(|id| (id, Target::Player(defender)))
            .collect();
        self.finish_attackers_with(chosen);
    }

    /// Planeswalkers `p` controls.
    pub(super) fn planeswalkers_of(&self, p: PlayerId) -> impl Iterator<Item = ObjId> + '_ {
        self.permanents_of(p).filter(move |id| {
            self.characteristics(*id)
                .is_some_and(|c| c.is_planeswalker())
        })
    }

    /// Where an attacker's combat damage goes: the player, or the
    /// planeswalker if it is still on the battlefield (CR 506.4c: an
    /// attacker whose planeswalker left deals no combat damage).
    fn attack_target_live(&self, a: &Attacker) -> Option<Target> {
        match a.attacking {
            Target::Player(_) => Some(a.attacking),
            Target::Object(id) => self
                .objects
                .get(id)
                .filter(|o| o.zone == Zone::Battlefield)
                .map(|_| a.attacking),
        }
    }

    pub(super) fn finish_attackers_with(&mut self, set: Vec<(ObjId, Target)>) {
        if !set.is_empty() {
            self.attacked_this_turn |= 1 << self.active_player.index().min(7);
        }
        let mut attackers = Vec::new();
        for (id, target) in set {
            // CR 508.1f: attacking taps the creature unless it has vigilance.
            if !self
                .characteristics(id)
                .is_some_and(|c| c.has(Keyword::Vigilance))
            {
                self.tap(id);
            }
            self.attackers_this_turn.push(id);
            attackers.push(Attacker {
                id,
                attacking: target,
                blockers: SmallVec::new(),
                blocked: false,
            });
        }
        // The attackers are declared together: triggers see all of them
        // ("attacks alone", CR 506.5).
        let events: SmallVec<[(ObjId, Target); 4]> =
            attackers.iter().map(|a| (a.id, a.attacking)).collect();
        if let Some(c) = self.combat.as_mut() {
            c.attackers = attackers;
        }
        for (attacker, target) in events {
            self.emit(Event::Attack { attacker, target });
        }
        // Attacking status feeds filters ("attacking creatures get +1/+0").
        self.combat_bump();
        self.give_priority(self.active_player);
    }

    /// A creature put onto the battlefield attacking (CR 506.3a): it was
    /// never declared as an attacker, so "whenever ... attacks" doesn't
    /// see it (CR 508.4); after blockers are declared it is unblocked.
    pub(super) fn enter_attacking(&mut self, id: ObjId, target: Target) {
        let Some(c) = self.combat.as_mut() else {
            return;
        };
        if !self.objects.contains_key(id) {
            return;
        }
        c.attackers.push(Attacker {
            id,
            attacking: target,
            blockers: SmallVec::new(),
            blocked: false,
        });
        self.combat_bump();
    }

    pub(super) fn finish_blocks(&mut self, mut blocks: Vec<(ObjId, ObjId)>) {
        // CR 506.5: "can't block alone".
        if self.blocks_alone_illegally(&blocks) {
            blocks.clear();
        }
        // "Can't block unless at least two other creatures block".
        let mut blockers: Vec<ObjId> = Vec::new();
        for (b, _) in &blocks {
            if !blockers.contains(b) {
                blockers.push(*b);
            }
        }
        if !self.set_legal(&blockers, true) {
            let ok = self.legal_part(&blockers, true);
            blocks.retain(|(b, _)| ok.contains(b));
        }
        let mut newly_blocked: SmallVec<[ObjId; 4]> = SmallVec::new();
        for (blocker, attacker) in &blocks {
            if let Some(a) = self.combat.as_mut().and_then(|c| c.attacker_mut(*attacker)) {
                if !a.blocked {
                    newly_blocked.push(*attacker);
                }
                a.blockers.push(*blocker);
                a.blocked = true;
            }
        }
        for b in &blockers {
            if blocks.iter().any(|(x, _)| x == b) && !self.blockers_this_turn.contains(b) {
                self.blockers_this_turn.push(*b);
            }
        }
        for (blocker, attacker) in blocks {
            self.emit(Event::Block { blocker, attacker });
        }
        for attacker in newly_blocked {
            self.emit(Event::Blocked { attacker });
        }
        // CR 509.1h: the attackers no creature blocks ("attacks and isn't
        // blocked").
        let unblocked: SmallVec<[ObjId; 4]> = self
            .combat
            .as_ref()
            .map(|c| {
                c.attackers
                    .iter()
                    .filter(|a| !a.blocked)
                    .map(|a| a.id)
                    .collect()
            })
            .unwrap_or_default();
        for attacker in unblocked {
            self.emit(Event::Unblocked { attacker });
        }
        self.combat_bump();
        self.give_priority(self.active_player);
    }

    pub(super) fn any_first_strike_in_combat(&self) -> bool {
        let Some(c) = &self.combat else {
            return false;
        };
        c.attackers
            .iter()
            .flat_map(|a| std::iter::once(a.id).chain(a.blockers.iter().copied()))
            .filter_map(|id| self.characteristics(id))
            .any(|ch| ch.has(Keyword::FirstStrike) || ch.has(Keyword::DoubleStrike))
    }

    /// Whether `id` assigns combat damage in the current damage step (CR 510.4).
    pub(super) fn deals_damage_this_step(&self, id: ObjId, first_strike_step: bool) -> bool {
        let Some(c) = self.characteristics(id) else {
            return false;
        };
        let fs = c.has(Keyword::FirstStrike);
        let ds = c.has(Keyword::DoubleStrike);
        let had_fs_step = self
            .combat
            .as_ref()
            .is_some_and(|c| c.had_first_strike_step);
        if first_strike_step {
            fs || ds
        } else if had_fs_step {
            ds || !fs
        } else {
            true
        }
    }

    /// CR 510.1: compute all automatic assignments, queue the ones needing a
    /// player's division, then deal damage.
    pub(super) fn start_damage_assignment(&mut self, first_strike_step: bool) {
        let combat = self.combat.clone().unwrap_or_default();
        let mut assigned = Vec::new();
        let mut queue = Vec::new();
        for a in &combat.attackers {
            if !self.objects.contains_key(a.id) || !self.is_creature_on_battlefield(a.id) {
                continue;
            }
            if !self.deals_damage_this_step(a.id, first_strike_step) {
                continue;
            }
            let c = self.characteristics(a.id).expect("attacker exists");
            let power = u32::try_from(c.power).unwrap_or(0);
            if power == 0 {
                continue;
            }
            let live_blockers: Vec<ObjId> = a
                .blockers
                .iter()
                .copied()
                .filter(|b| self.is_creature_on_battlefield(*b))
                .collect();
            if !a.blocked {
                if let Some(target) = self.attack_target_live(a) {
                    assigned.push(DamageAssignment {
                        source: a.id,
                        target,
                        amount: power,
                    });
                }
            } else if live_blockers.is_empty() {
                // CR 510.1c: no damage; trample assigns it all to the player (CR 702.19e).
                if c.has(Keyword::Trample)
                    && let Some(target) = self.attack_target_live(a)
                {
                    assigned.push(DamageAssignment {
                        source: a.id,
                        target,
                        amount: power,
                    });
                }
            } else if live_blockers.len() == 1 && !c.has(Keyword::Trample) {
                assigned.push(DamageAssignment {
                    source: a.id,
                    target: Target::Object(live_blockers[0]),
                    amount: power,
                });
            } else {
                queue.push(a.id);
            }
        }
        // Blockers (CR 510.1d): all to the one creature it blocks; a
        // blocker blocking several divides its damage among them.
        let mut seen: SmallVec<[ObjId; 8]> = SmallVec::new();
        for b in combat.attackers.iter().flat_map(|a| a.blockers.iter()) {
            if seen.contains(b) {
                continue;
            }
            seen.push(*b);
            if !self.is_creature_on_battlefield(*b)
                || !self.deals_damage_this_step(*b, first_strike_step)
            {
                continue;
            }
            let power = self
                .characteristics(*b)
                .map_or(0, |c| u32::try_from(c.power).unwrap_or(0));
            if power == 0 {
                continue;
            }
            let blocked = self.live_blocked_by(&combat, *b);
            match blocked.as_slice() {
                [] => {}
                [a] => assigned.push(DamageAssignment {
                    source: *b,
                    target: Target::Object(*a),
                    amount: power,
                }),
                _ => queue.push(*b),
            }
        }
        self.flow = Flow::AssignDamage { queue, assigned };
    }

    /// Lethal damage for a blocker from this source (CR 702.19b, 702.2c).
    pub(super) fn lethal_for(&self, source: ObjId, blocker: ObjId) -> u32 {
        if self
            .characteristics(source)
            .is_some_and(|c| c.has(Keyword::Deathtouch))
        {
            return 1;
        }
        let o = &self.objects[blocker];
        let t = self
            .characteristics(blocker)
            .map_or(0, |c| u32::try_from(c.toughness).unwrap_or(0));
        t.saturating_sub(o.damage).max(1)
    }

    /// The attackers still on the battlefield that `blocker` blocks.
    fn live_blocked_by(&self, combat: &crate::state::CombatState, blocker: ObjId) -> Vec<ObjId> {
        combat
            .attackers
            .iter()
            .filter(|a| a.blockers.contains(&blocker) && self.is_creature_on_battlefield(a.id))
            .map(|a| a.id)
            .collect()
    }

    /// All divisions of a blocker's combat damage among the creatures it
    /// blocks (CR 510.1d): any split, as its controller chooses.
    fn blocker_divisions(&self, blocker: ObjId) -> Vec<Vec<(Target, u32)>> {
        let combat = self.combat.as_ref().expect("in combat");
        let attackers = self.live_blocked_by(combat, blocker);
        let power = self
            .characteristics(blocker)
            .map_or(0, |c| u32::try_from(c.power).unwrap_or(0));
        let n = attackers.len();
        let mut out = Vec::new();
        let mut current = vec![0u32; n];
        compose(power, 0, &mut current, &mut |parts| {
            out.push(parts.to_vec());
            out.len() < MAX_DIVISION_OPTIONS
        });
        if out.len() >= MAX_DIVISION_OPTIONS {
            // Too many: lethal damage to each in turn, the rest to the last.
            out.clear();
            let mut canonical = vec![0u32; n];
            let mut left = power;
            for (i, a) in attackers.iter().enumerate() {
                let give = self.lethal_for(blocker, *a).min(left);
                canonical[i] = give;
                left -= give;
            }
            if n > 0 {
                canonical[n - 1] += left;
            }
            out.push(canonical);
        }
        out.into_iter()
            .map(|parts| {
                attackers
                    .iter()
                    .map(|a| Target::Object(*a))
                    .zip(parts)
                    .filter(|(_, amt)| *amt > 0)
                    .collect()
            })
            .collect()
    }

    /// All legal divisions of an attacker's damage (CR 510.1c, 702.19b),
    /// or of a blocker's that blocks several attackers (CR 510.1d).
    pub(super) fn damage_divisions(&self, attacker: ObjId) -> Vec<Vec<(Target, u32)>> {
        let Some(a) = self
            .combat
            .as_ref()
            .and_then(|c| c.attacker(attacker))
            .cloned()
        else {
            return self.blocker_divisions(attacker);
        };
        let c = self.characteristics(attacker).expect("attacker exists");
        let power = u32::try_from(c.power).unwrap_or(0);
        let trample = c.has(Keyword::Trample);
        let blockers: Vec<ObjId> = a
            .blockers
            .iter()
            .copied()
            .filter(|b| self.is_creature_on_battlefield(*b))
            .collect();
        let mut recipients: Vec<Target> = blockers.iter().map(|b| Target::Object(*b)).collect();
        let lethal: Vec<u32> = blockers
            .iter()
            .map(|b| self.lethal_for(attacker, *b))
            .collect();
        let through = trample.then(|| self.attack_target_live(&a)).flatten();
        if let Some(t) = through {
            recipients.push(t);
        }
        let n = recipients.len();
        // Enumerate compositions of `power` into n parts, filtering trample's
        // lethal rule: the player slot may be nonzero only if every blocker
        // has at least lethal.
        let mut out = Vec::new();
        let mut current = vec![0u32; n];
        compose(power, 0, &mut current, &mut |parts| {
            if trample {
                let to_player = parts[n - 1];
                if to_player > 0 && parts[..n - 1].iter().zip(&lethal).any(|(p, l)| p < l) {
                    return true;
                }
            }
            out.push(parts.to_vec());
            out.len() < MAX_DIVISION_OPTIONS
        });
        if out.len() >= MAX_DIVISION_OPTIONS {
            // Too many: offer the canonical divisions only.
            out.clear();
            let mut canonical = vec![0u32; n];
            let mut left = power;
            for (i, l) in lethal.iter().enumerate() {
                let give = (*l).min(left);
                canonical[i] = give;
                left -= give;
            }
            if n > 0 {
                // Remainder to the player (trample) or the last blocker.
                canonical[n - 1] += left;
            }
            out.push(canonical);
        }
        out.into_iter()
            .map(|parts| {
                recipients
                    .iter()
                    .copied()
                    .zip(parts)
                    .filter(|(_, amt)| *amt > 0)
                    .collect()
            })
            .collect()
    }

    /// CR 510.2: all combat damage is dealt simultaneously.
    pub(super) fn deal_combat_damage(&mut self, assigned: Vec<DamageAssignment>) {
        for DamageAssignment {
            source,
            target,
            amount,
        } in assigned
        {
            self.deal_damage(source, target, amount, true);
        }
        self.give_priority(self.active_player);
    }
}

/// All subsets of `items`, smallest first, each in the items' order.
pub(super) fn subsets(items: &[ObjId]) -> Vec<Vec<ObjId>> {
    let n = items.len();
    let mut out: Vec<Vec<ObjId>> = (0..(1usize << n))
        .map(|mask| {
            (0..n)
                .filter(|i| mask & (1 << i) != 0)
                .map(|i| items[i])
                .collect()
        })
        .collect();
    out.sort_by_key(Vec::len);
    out
}

/// All block assignments: each blocker picks none, one, or (with a
/// capacity above one, CR 509.1a) up to `caps[i]` of its allowed attackers.
pub(super) fn block_assignments(
    blockers: &[ObjId],
    can_block: &[Vec<ObjId>],
    caps: &[usize],
) -> Vec<Vec<(ObjId, ObjId)>> {
    let mut out = vec![Vec::new()];
    for ((b, allowed), cap) in blockers.iter().zip(can_block).zip(caps) {
        let choices: Vec<Vec<ObjId>> = if *cap <= 1 {
            std::iter::once(Vec::new())
                .chain(allowed.iter().map(|a| vec![*a]))
                .collect()
        } else {
            subsets(allowed)
                .into_iter()
                .filter(|s| s.len() <= *cap)
                .collect()
        };
        let mut next = Vec::with_capacity(out.len() * choices.len());
        for partial in &out {
            for choice in &choices {
                let mut p = partial.clone();
                p.extend(choice.iter().map(|a| (*b, *a)));
                next.push(p);
            }
        }
        out = next;
    }
    out
}

/// How many choices `block_assignments` gives a blocker with `n` allowed
/// attackers and capacity `cap`: the subsets of at most `cap` of them.
pub(super) fn block_choices(n: usize, cap: usize) -> usize {
    let mut total = 0usize;
    let mut binom = 1usize; // C(n, k)
    for k in 0..=n.min(cap) {
        total = total.saturating_add(binom);
        binom = binom.saturating_mul(n - k) / (k + 1);
    }
    total
}

/// The blocking requirements of one declaration (CR 509.1c).
#[derive(Default)]
pub(super) struct BlockRequirements {
    /// Creatures that block if able.
    must_block: Vec<ObjId>,
    /// Attackers that must be blocked if able.
    must_be_blocked: Vec<ObjId>,
    /// Attackers every creature able to block must block.
    lure: Vec<ObjId>,
    /// (blocker, attacker): this creature blocks that one if able.
    pairs: Vec<(ObjId, ObjId)>,
}

impl BlockRequirements {
    pub(super) fn is_empty(&self) -> bool {
        self.must_block.is_empty()
            && self.must_be_blocked.is_empty()
            && self.lure.is_empty()
            && self.pairs.is_empty()
    }

    /// How many requirements `blocks` obeys. Each blocker that blocks if
    /// able, each attacker that must be blocked, and each (blocker, lure
    /// attacker) pair is one requirement; a blocker unable to block can't
    /// obey its requirement, so counting only the obeyed ones is enough to
    /// compare declarations.
    pub(super) fn obeyed(&self, blocks: &[(ObjId, ObjId)]) -> usize {
        self.must_block
            .iter()
            .filter(|m| blocks.iter().any(|(b, _)| b == *m))
            .count()
            + self
                .must_be_blocked
                .iter()
                .filter(|m| blocks.iter().any(|(_, a)| a == *m))
                .count()
            + blocks.iter().filter(|(_, a)| self.lure.contains(a)).count()
            + self.pairs.iter().filter(|p| blocks.contains(p)).count()
    }
}

/// Visit every way of writing `total` as an ordered sum of `parts.len()`
/// non-negative parts. The visitor returns `false` to stop.
pub(super) fn compose(
    total: u32,
    index: usize,
    parts: &mut Vec<u32>,
    visit: &mut dyn FnMut(&[u32]) -> bool,
) -> bool {
    if parts.is_empty() {
        return true;
    }
    if index == parts.len() - 1 {
        parts[index] = total;
        return visit(parts);
    }
    for v in (0..=total).rev() {
        parts[index] = v;
        if !compose(total - v, index + 1, parts, visit) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;

    #[test]
    fn compositions_and_subsets() {
        let mut seen = Vec::new();
        let mut parts = vec![0; 3];
        compose(2, 0, &mut parts, &mut |p| {
            seen.push(p.to_vec());
            true
        });
        assert_eq!(seen.len(), 6); // C(2+3-1, 3-1)
        assert!(seen.contains(&vec![2, 0, 0]));
        assert!(seen.contains(&vec![0, 1, 1]));

        let mut sm: SlotMap<ObjId, ()> = SlotMap::with_key();
        let ids: Vec<ObjId> = (0..3).map(|_| sm.insert(())).collect();
        let subs = subsets(&ids);
        assert_eq!(subs.len(), 8);
        assert_eq!(subs[0].len(), 0);
        assert_eq!(subs[7].len(), 3);
        let blocks = block_assignments(&ids[..2], &[vec![ids[2]], vec![ids[2]]], &[1, 1]);
        assert_eq!(blocks.len(), 4);
        // A blocker that can block both of two attackers: none, either, both.
        let blocks = block_assignments(&ids[..1], &[ids[1..].to_vec()], &[2]);
        assert_eq!(blocks.len(), 4);
        assert_eq!(block_choices(2, 2), 4);
        assert_eq!(block_choices(2, 1), 3);
        assert_eq!(block_choices(3, usize::MAX), 8);
    }
}
