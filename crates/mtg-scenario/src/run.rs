//! Drives the engine through a parsed scenario (design §7.1). Scripted
//! actions fail loudly when they are not among the legal options; unscripted
//! decisions pass or take the first option.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use mtg_core::card::CardPool;
use mtg_core::card::{AbilityDef, AbilityId, CardType, CounterKind, Keyword, Supertype};
use mtg_core::decision::{Choice, ChoiceId, Decision, DecisionKind, StepResult};
use mtg_core::engine::{Engine, PlayerSetup, Setup, SetupPermanent};
use mtg_core::event::Event;
use mtg_core::ids::{ObjId, PlayerId};
use mtg_core::state::{GameResult, GameState, ObjKind, Target, Zone};
use mtg_core::turn::Step;

use crate::ast::{
    Action, Assertion, ObjRef, Pick, Position, RunTarget, Scenario, ScriptedAction, Statement,
    TargetRef, expand_auto, player_name, step_name,
};

/// Safety valve against scenarios that never end.
pub const MAX_DECISIONS: u32 = 20_000;

/// Library given to a player whose scenario does not specify one, so that
/// draws succeed. Forty cards covers any plausible scenario length.
pub const DEFAULT_LIBRARY: (&str, usize) = ("Plains", 40);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// Line of the statement that failed, or of the scenario header.
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

#[derive(Clone, Debug)]
pub struct Report {
    pub name: String,
    pub line: usize,
    pub skipped: Option<String>,
    pub failure: Option<Failure>,
    /// Event log of the game, for diagnosis.
    pub log: Vec<Event>,
    pub decisions: u32,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.failure.is_none() && self.skipped.is_none()
    }
}

type Res<T> = Result<T, Failure>;

fn fail<T>(line: usize, message: impl Into<String>) -> Res<T> {
    Err(Failure {
        line,
        message: message.into(),
    })
}

/// Run one scenario against the pool.
pub fn run(pool: &Arc<CardPool>, sc: &Scenario) -> Report {
    if let Some(reason) = &sc.skip {
        return Report {
            name: sc.name.clone(),
            line: sc.line,
            skipped: Some(reason.clone()),
            failure: None,
            log: Vec::new(),
            decisions: 0,
        };
    }
    let mut runner = match Runner::new(pool, sc) {
        Ok(r) => r,
        Err(failure) => {
            return Report {
                name: sc.name.clone(),
                line: sc.line,
                skipped: None,
                failure: Some(failure),
                log: Vec::new(),
                decisions: 0,
            };
        }
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| runner.execute(sc)));
    let failure = match outcome {
        Ok(Ok(())) => None,
        Ok(Err(f)) => Some(f),
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_else(|| "non-string panic".to_string());
            Some(Failure {
                line: sc.line,
                message: format!("engine panicked: {message}"),
            })
        }
    };
    Report {
        name: sc.name.clone(),
        line: sc.line,
        skipped: None,
        failure,
        log: runner.engine.state().log.clone(),
        decisions: runner.decisions,
    }
}

#[allow(clippy::struct_excessive_bools)] // one flag per scripted cost choice
struct Runner {
    engine: Engine,
    actions: Vec<ScriptedAction>,
    fired: Vec<bool>,
    decisions: u32,
    /// Targets still to be chosen for the spell being cast.
    pending_targets: Option<Vec<TargetRef>>,
    /// The `trigger` line supplying `pending_targets` is `illegal`: its
    /// target must be refused.
    pending_illegal: bool,
    /// The scripted mode of the modal spell being cast (1-based).
    pending_mode: Option<usize>,
    /// The scripted X, sacrifice, and discard of the spell being cast.
    pending_x: Option<u32>,
    pending_sacrifice: Option<String>,
    pending_discard: Option<String>,
    /// Whether the spell being cast is scripted `kicked`.
    pending_kicked: bool,
    /// `conspired` on the cast being made (CR 702.78a).
    pending_conspired: bool,
    /// `other_cost` on the cast being made.
    pending_other_cost: bool,
    /// Cards still to splice onto the spell being cast (`splicing`).
    pending_splice: Vec<String>,
    /// `splicing` cards scripted `illegal`: they must not be offered.
    refused_splice: Vec<String>,
    /// The ability whose scripted targets have all been chosen.
    targets_done: Option<ObjId>,
    /// The scripted division (`dividing 2, 1`) of the spell being cast
    /// (CR 601.2d).
    pending_division: Vec<u32>,
    /// Attackers still to be added in an incremental declaration.
    attack_queue: Option<Vec<ObjId>>,
    /// Scripted `at` targets for attackers (CR 508.1a).
    attack_targets: Vec<(ObjId, TargetRef, bool)>,
    /// Blocks still to be added in an incremental declaration.
    block_queue: Option<Vec<(ObjId, ObjId)>>,
}

/// An engine at the scenario's starting position, its scripted actions
/// not taken (the benchmark's puzzle track plays them itself).
pub fn start_engine(pool: &Arc<CardPool>, sc: &Scenario) -> Result<Engine, Failure> {
    Runner::new(pool, sc).map(|r| r.engine)
}

impl Runner {
    fn new(pool: &Arc<CardPool>, sc: &Scenario) -> Res<Runner> {
        let (turn_number, active_player, step) =
            sc.setup.start.unwrap_or((1, PlayerId(0), Step::Untap));
        let mut players = vec![PlayerSetup::default(), PlayerSetup::default()];
        let mut library_given = [false; 2];
        for (i, p) in players.iter_mut().enumerate() {
            p.life = sc.setup.life[i].unwrap_or(20);
            p.poison = sc.setup.poison[i];
            p.energy = sc.setup.energy[i];
            p.speed = sc.setup.speed[i];
        }
        for z in &sc.setup.zones {
            let ps = &mut players[z.player.index()];
            for card in &z.cards {
                let Some(def) = pool.lookup(&card.name) else {
                    return fail(z.line, format!("\"{}\" is not in the card pool", card.name));
                };
                for _ in 0..card.count {
                    match z.zone {
                        Zone::Battlefield => {
                            let mut counters = Vec::new();
                            for (kind, n) in &card.counters {
                                let Some(k) = CounterKind::by_name(kind) else {
                                    return fail(z.line, format!("unknown counter kind `{kind}`"));
                                };
                                counters.push((k, *n));
                            }
                            ps.battlefield.push(SetupPermanent {
                                def,
                                tapped: card.tapped,
                                attached_to: card.attached_to.clone(),
                                counters,
                            });
                        }
                        Zone::Hand => ps.hand.push(def),
                        Zone::Graveyard => ps.graveyard.push(def),
                        Zone::Exile => ps.exile.push(def),
                        Zone::Library => ps.library.push(def),
                        Zone::Stack | Zone::Command => {
                            return fail(z.line, "cannot set up the stack or command zone");
                        }
                    }
                }
            }
            if z.zone == Zone::Library {
                library_given[z.player.index()] = true;
            }
        }
        for (i, p) in players.iter_mut().enumerate() {
            if library_given[i] {
                // Scenario files list the top card first; the engine wants
                // the top card last.
                p.library.reverse();
            } else {
                let def = pool
                    .lookup(DEFAULT_LIBRARY.0)
                    .expect("default library card is in the pool");
                p.library = vec![def; DEFAULT_LIBRARY.1];
            }
        }
        let setup = Setup {
            players,
            turn_number,
            active_player,
            step,
            seed: sc.setup.seed.unwrap_or(0),
            logging: true,
        };
        let actions: Vec<ScriptedAction> = sc
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Action(a) => Some(a.clone()),
                _ => None,
            })
            .collect();
        Ok(Runner {
            engine: Engine::from_setup(pool.clone(), &setup),
            fired: vec![false; actions.len()],
            actions,
            decisions: 0,
            pending_targets: None,
            pending_illegal: false,
            pending_mode: None,
            pending_x: None,
            pending_division: Vec::new(),
            targets_done: None,
            pending_sacrifice: None,
            pending_discard: None,
            pending_kicked: false,
            pending_conspired: false,
            pending_other_cost: false,
            pending_splice: Vec::new(),
            refused_splice: Vec::new(),
            attack_queue: None,
            attack_targets: Vec::new(),
            block_queue: None,
        })
    }

    fn state(&self) -> &GameState {
        self.engine.state()
    }

    fn execute(&mut self, sc: &Scenario) -> Res<()> {
        for st in &sc.statements {
            match st {
                Statement::Action(_) => {}
                Statement::RunTo { line, target } => self.run_until(*line, *target)?,
                Statement::Assert { line, assertion } => self.check(*line, assertion)?,
            }
        }
        if let Some(i) = self
            .fired
            .iter()
            .enumerate()
            .position(|(i, f)| !*f && !matches!(self.actions[i].action, Action::Wait))
        {
            let a = &self.actions[i];
            return fail(
                a.line,
                format!(
                    "scripted {} at turn {} was never reached (add a run_to past it, or the player never got that decision)",
                    a.action.kind_name(),
                    a.turn
                ),
            );
        }
        Ok(())
    }

    fn position(&self) -> Position {
        Position {
            turn: self.state().turn_number,
            step: self.state().step,
        }
    }

    fn run_until(&mut self, line: usize, target: RunTarget) -> Res<()> {
        loop {
            let decision = match self.engine.current() {
                StepResult::Over(_) => return Ok(()),
                StepResult::Decision(d) => d.clone(),
            };
            let pos = self.position();
            if let RunTarget::Position(t) = target
                && pos >= t
            {
                return Ok(());
            }
            self.check_overdue(pos)?;
            if self.decisions >= MAX_DECISIONS {
                return fail(
                    line,
                    format!("game did not reach the target within {MAX_DECISIONS} decisions"),
                );
            }
            let choice = self.decide(&decision, pos)?;
            self.engine.step(choice).map_err(|e| Failure {
                line,
                message: format!("engine rejected choice: {e}"),
            })?;
            self.decisions += 1;
        }
    }

    /// A scripted action whose point has passed without firing is an
    /// authoring error or a legality bug; either way, stop. An `illegal`
    /// action whose decision was never offered at all (for example a block
    /// when the player has no untapped creature) counts as satisfied.
    fn check_overdue(&mut self, pos: Position) -> Res<()> {
        for i in 0..self.actions.len() {
            if self.fired[i] {
                continue;
            }
            let a = &self.actions[i];
            let overdue = match a.step {
                Some(step) => Position { turn: a.turn, step } < pos,
                None => a.turn < pos.turn,
            };
            if overdue && a.illegal {
                self.fired[i] = true;
                continue;
            }
            if overdue {
                return fail(
                    a.line,
                    format!(
                        "scripted {} for {} at turn {}{} was never offered a decision there (now at turn {} {})",
                        a.action.kind_name(),
                        player_name(a.player),
                        a.turn,
                        a.step
                            .map_or(String::new(), |s| format!(" {}", step_name(s))),
                        pos.turn,
                        step_name(pos.step)
                    ),
                );
            }
        }
        Ok(())
    }

    /// Indices of unfired actions for `player` at `pos` whose action matches `pred`.
    fn matching(
        &self,
        player: PlayerId,
        pos: Position,
        any_step: bool,
        pred: impl Fn(&Action) -> bool,
    ) -> Vec<usize> {
        self.actions
            .iter()
            .enumerate()
            .filter(|(i, a)| {
                !self.fired[*i]
                    && a.player == player
                    && a.turn == pos.turn
                    && (if any_step {
                        a.step.is_none() || a.step == Some(pos.step)
                    } else {
                        a.step == Some(pos.step)
                    })
                    && pred(&a.action)
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn decide(&mut self, d: &Decision, pos: Position) -> Res<ChoiceId> {
        match d.kind {
            DecisionKind::Mulligan => Ok(index_of(d, &Choice::Keep).unwrap_or(0)),
            // Bottom nothing; keep everything on top; put nothing in the
            // graveyard; pay a counter's ransom.
            DecisionKind::Bottom
            | DecisionKind::Scry { .. }
            | DecisionKind::Surveil { .. }
            | DecisionKind::Edict
            | DecisionKind::SearchLibrary
            | DecisionKind::PutFromHand => Ok(0),
            // `decline "Owlin Shieldmage"`: don't pay its ward cost.
            DecisionKind::PayOrCounter { def } => Ok(self.you_may(d, pos, def)),
            DecisionKind::CastFromExile { card } => Ok(self.cast_from_exile(d, pos, card)),
            DecisionKind::PriorityAction => self.priority_action(d, pos),
            DecisionKind::ChooseTarget { spell, index } => {
                // Optional targets ("up to two") past the scripted ones are
                // declined.
                if let Some(done) = index_of(d, &Choice::Done)
                    && self.state().objects[spell].kind != ObjKind::Ability
                    && self
                        .pending_targets
                        .as_ref()
                        .is_none_or(|t| index >= t.len())
                {
                    self.pending_targets = None;
                    return Ok(done);
                }
                // A variable group of a triggered ability past its scripted
                // targets (CR 601.2c): no more.
                if let Some(done) = index_of(d, &Choice::Done)
                    && self.targets_done == Some(spell)
                {
                    return Ok(done);
                }
                if self.state().objects[spell].kind == ObjKind::Ability {
                    self.choose_ability_target(d, spell, index, pos)
                } else {
                    self.choose_target(d, index)
                }
            }
            DecisionKind::DeclareAttackers => self.declare_attackers(d, pos),
            DecisionKind::DeclareBlockers => self.declare_blockers(d, pos),
            DecisionKind::AssignCombatDamage { attacker } => self.assign_damage(d, pos, attacker),
            DecisionKind::Discard => self.discard(d, pos),
            DecisionKind::OrderTriggers => Ok(self.order_triggers(d, pos)),
            DecisionKind::ChooseReplacement => Ok(self.choose_replacement(d, pos)),
            DecisionKind::LegendRule => Ok(self.keep_legend(d, pos)),
            DecisionKind::ChooseMode { .. } => self.choose_mode(d),
            DecisionKind::ChooseX { .. } => self.choose_x(d),
            DecisionKind::Divide { .. } => self.choose_division(d),
            DecisionKind::ChooseSacrifice { .. } => self.choose_cost(d, "sacrifice"),
            DecisionKind::ChooseDiscardCost { .. } => self.choose_cost(d, "discard"),
            DecisionKind::ChooseAttackTarget { attacker } => self.choose_attack_target(d, attacker),
            // "On their choice of the top or bottom": `accept` is the top.
            DecisionKind::YouMay { def } | DecisionKind::TopOrBottom { def } => {
                Ok(self.you_may(d, pos, def))
            }
            DecisionKind::ChooseCopy => self.choose_copy(d, pos),
            DecisionKind::ChooseColor
            | DecisionKind::ChooseType
            | DecisionKind::ChooseCard
            | DecisionKind::ChooseOption { .. }
            // A copy's new targets (CR 707.10c): `choose "Hill Giant"`;
            // unscripted, it keeps the copied ones.
            | DecisionKind::CopyTarget { .. }
            // An attacker joins the band being formed (CR 702.22c):
            // `choose "Grizzly Bears"`; unscripted, no band.
            | DecisionKind::FormBand { .. } => self.choose_named(d, pos),
            // "Buyback—Sacrifice a land.": `kicked sacrificing "Forest"`.
            DecisionKind::Buyback { .. }
                if self.pending_kicked
                    && d.options.iter().any(|c| matches!(c, Choice::Sacrifice(_))) =>
            {
                self.pending_kicked = false;
                self.choose_cost(d, "sacrifice")
            }
            // `kicked` also pays buyback (an optional additional cost).
            DecisionKind::Kicker { .. }
            | DecisionKind::Buyback { .. }
            | DecisionKind::Offspring { .. } => Ok(self.choose_kicker(d)),
            DecisionKind::ChooseAdditionalCost { .. } => {
                let want = Choice::Mode(usize::from(std::mem::take(&mut self.pending_other_cost)));
                Ok(index_of(d, &want).unwrap_or(0))
            }
            DecisionKind::Bargain { .. } => self.choose_bargain(d),
            DecisionKind::Splice { .. } => self.choose_splice(d),
            DecisionKind::Conspire { .. } => {
                let want = if std::mem::take(&mut self.pending_conspired) {
                    Choice::Accept
                } else {
                    Choice::Decline
                };
                Ok(index_of(d, &want).unwrap_or(0))
            }
        }
    }

    /// Whether to kick the spell being cast: yes if scripted `kicked`,
    /// otherwise no.
    fn choose_kicker(&mut self, d: &Decision) -> ChoiceId {
        let want = if std::mem::take(&mut self.pending_kicked) {
            Choice::Accept
        } else {
            Choice::Decline
        };
        index_of(d, &want).unwrap_or(0)
    }

    /// What an attacker attacks: the scripted `at` target, else the player
    /// (the engine's first option).
    fn choose_attack_target(&mut self, d: &Decision, attacker: ObjId) -> Res<ChoiceId> {
        let Some(i) = self
            .attack_targets
            .iter()
            .position(|(a, ..)| *a == attacker)
        else {
            return Ok(0);
        };
        let (_, tr, illegal) = self.attack_targets.remove(i);
        let line = self.line_of_kind(self.position(), d.player, "attack");
        let t = self.resolve_target(line, &tr)?;
        // `illegal attack "X" at "Y"`: X attacks, but not Y.
        if illegal {
            if index_of(d, &Choice::AttackTarget(t)).is_some() {
                return fail(
                    line,
                    format!(
                        "{} attacking {} was legal but the scenario says it must not be",
                        self.state().name_of(attacker),
                        self.describe_target(t)
                    ),
                );
            }
            return Ok(0);
        }
        index_of(d, &Choice::AttackTarget(t)).ok_or_else(|| Failure {
            line,
            message: format!(
                "{} cannot attack {}; options: {}",
                self.state().name_of(attacker),
                self.describe_target(t),
                self.describe_options(d)
            ),
        })
    }

    /// The division of the spell being cast (CR 601.2d): the scripted
    /// `dividing N, M`, else the engine's first.
    fn choose_division(&mut self, d: &Decision) -> Res<ChoiceId> {
        let parts = std::mem::take(&mut self.pending_division);
        if parts.is_empty() {
            return Ok(0);
        }
        let line = self.current_cast_line();
        index_of(d, &Choice::Divide(parts.clone())).ok_or_else(|| Failure {
            line,
            message: format!(
                "dividing {parts:?} is not legal; options: {}",
                self.describe_options(d)
            ),
        })
    }

    /// X for the spell being cast: the scripted `x N`, else 0.
    fn choose_x(&mut self, d: &Decision) -> Res<ChoiceId> {
        let Some(x) = self.pending_x.take() else {
            return Ok(0);
        };
        let line = self.current_cast_line();
        index_of(d, &Choice::X(x)).ok_or_else(|| Failure {
            line,
            message: format!(
                "X = {x} cannot be paid for; available: {}",
                self.describe_options(d)
            ),
        })
    }

    /// The permanent to sacrifice or the card to discard as an additional
    /// cost of the spell being cast: the scripted one, else the engine's
    /// first option.
    fn choose_cost(&mut self, d: &Decision, what: &str) -> Res<ChoiceId> {
        let pending = if what == "sacrifice" {
            self.pending_sacrifice.take()
        } else {
            self.pending_discard.take()
        };
        let Some(name) = pending else {
            return Ok(0);
        };
        let line = self.current_cast_line();
        let found = d.options.iter().position(|c| match c {
            Choice::Sacrifice(id) => self.state().name_of(*id) == name,
            Choice::Discard { def, .. } => self.state().pool.get(*def).name == name,
            _ => false,
        });
        found.map(idx_u32).ok_or_else(|| Failure {
            line,
            message: format!(
                "cannot {what} \"{name}\"; available: {}",
                self.describe_options(d)
            ),
        })
    }

    /// The next scripted `splicing` card, else no (more) splicing.
    fn choose_splice(&mut self, d: &Decision) -> Res<ChoiceId> {
        if !self.refused_splice.is_empty() {
            let names = std::mem::take(&mut self.refused_splice);
            let st = self.state();
            for name in names {
                if d.options.iter().any(
                    |c| matches!(c, Choice::Discard { def, .. } if st.pool.get(*def).name == name),
                ) {
                    return fail(
                        self.current_cast_line(),
                        format!("\"{name}\" could be spliced but the scenario says it must not be"),
                    );
                }
            }
            return Ok(index_of(d, &Choice::Done).unwrap_or(0));
        }
        if self.pending_splice.is_empty() {
            return Ok(index_of(d, &Choice::Done).unwrap_or(0));
        }
        let name = self.pending_splice.remove(0);
        let st = self.state();
        d.options
            .iter()
            .position(
                |c| matches!(c, Choice::Discard { def, .. } if st.pool.get(*def).name == name),
            )
            .map(|i| ChoiceId::try_from(i).unwrap_or(0))
            .ok_or_else(|| Failure {
                line: self.current_cast_line(),
                message: format!(
                    "\"{name}\" cannot be spliced; options: {}",
                    self.describe_options(d)
                ),
            })
    }

    /// Whether to bargain the spell being cast (CR 702.166a): with the
    /// scripted `sacrificing` permanent; unscripted, not.
    fn choose_bargain(&mut self, d: &Decision) -> Res<ChoiceId> {
        if self.pending_sacrifice.is_some() {
            return self.choose_cost(d, "sacrifice");
        }
        Ok(index_of(d, &Choice::Decline).unwrap_or(0))
    }

    /// The mode of the modal spell being cast: the one scripted with
    /// `mode N`, else the engine's first option.
    fn choose_mode(&mut self, d: &Decision) -> Res<ChoiceId> {
        let Some(m) = self.pending_mode.take() else {
            return Ok(0);
        };
        let line = self.current_cast_line();
        index_of(d, &Choice::Mode(m - 1)).ok_or_else(|| Failure {
            line,
            message: format!(
                "mode {m} is not available (it may lack a target); available: {}",
                self.describe_options(d)
            ),
        })
    }

    /// A "you may" of `def`: the first unfired scripted `accept`/`decline`
    /// naming the card decides; otherwise yes.
    fn you_may(&mut self, d: &Decision, pos: Position, def: mtg_core::card::CardRef) -> ChoiceId {
        let card = self.state().pool.get(def).name.clone();
        let idxs = self.matching(
            d.player,
            pos,
            true,
            |a| matches!(a, Action::YouMay { name, .. } if *name == card),
        );
        if let Some(&i) = idxs.first() {
            self.fired[i] = true;
            let Action::YouMay { accept, .. } = &self.actions[i].action else {
                unreachable!()
            };
            let want = if *accept {
                Choice::Accept
            } else {
                Choice::Decline
            };
            return index_of(d, &want).unwrap_or(0);
        }
        0
    }

    /// A color or type: the first unfired scripted `choose` decides;
    /// otherwise the engine's first option.
    fn choose_named(&mut self, d: &Decision, pos: Position) -> Res<ChoiceId> {
        let idxs = self.matching(d.player, pos, true, |a| matches!(a, Action::Choose(_)));
        let Some(&i) = idxs.first() else {
            return Ok(0);
        };
        self.fired[i] = true;
        let a = self.actions[i].clone();
        let Action::Choose(want) = &a.action else {
            unreachable!()
        };
        let found = d.options.iter().position(|c| {
            self.describe_choice(c).eq_ignore_ascii_case(want)
                || self.choice_name(c).is_some_and(|n| n == want.as_str())
        });
        // `illegal ... choose X`: X must not be offered; the engine's first
        // option is taken instead.
        if a.illegal {
            return match found {
                None => Ok(0),
                Some(_) => Err(Failure {
                    line: a.line,
                    message: format!("\"{want}\" can be chosen here"),
                }),
            };
        }
        found.map(idx_u32).ok_or_else(|| Failure {
            line: a.line,
            message: format!(
                "\"{want}\" cannot be chosen here; options: {}",
                self.describe_options(d)
            ),
        })
    }

    /// What `choose` may name besides the option's description: the card
    /// an effect acts on ("Grizzly Bears"), or an alternative's number
    /// ("2" for `Mode(1)`).
    fn choice_name(&self, c: &Choice) -> Option<String> {
        let st = self.state();
        match c {
            Choice::Sacrifice(id)
            | Choice::AddAttacker(id)
            | Choice::Target(mtg_core::state::Target::Object(id)) => {
                Some(st.name_of(*id).to_string())
            }
            Choice::Discard { def, .. } => Some(st.pool.get(*def).name.clone()),
            Choice::Mode(i) => Some((i + 1).to_string()),
            _ => None,
        }
    }

    /// What to enter as a copy of: the first unfired scripted `copy`
    /// decides; otherwise the engine's first option.
    fn choose_copy(&mut self, d: &Decision, pos: Position) -> Res<ChoiceId> {
        let idxs = self.matching(d.player, pos, true, |a| matches!(a, Action::Copy(_)));
        let Some(&i) = idxs.first() else {
            return Ok(0);
        };
        self.fired[i] = true;
        let a = self.actions[i].clone();
        let Action::Copy(what) = &a.action else {
            unreachable!()
        };
        let Some(r) = what else {
            return index_of(d, &Choice::NoCopy)
                .ok_or_else(|| self.unexpected(d, "no NoCopy option"));
        };
        let st = self.state();
        let found = d.options.iter().position(|c| match c {
            Choice::CopyOf { obj, def } => {
                st.pool.get(*def).name == r.name
                    && r.player
                        .is_none_or(|p| st.obj(*obj).is_some_and(|o| o.controller == p))
            }
            _ => false,
        });
        found.map(idx_u32).ok_or_else(|| Failure {
            line: a.line,
            message: format!(
                "\"{}\" cannot be copied here; options: {}",
                r.name,
                self.describe_options(d)
            ),
        })
    }

    /// The legend rule asks which permanent to keep: the first unfired
    /// scripted `keep` naming them decides; otherwise the oldest (the
    /// engine's first option).
    fn keep_legend(&mut self, d: &Decision, pos: Position) -> ChoiceId {
        let idxs = self.matching(d.player, pos, true, |a| matches!(a, Action::Keep { .. }));
        for i in idxs {
            let Action::Keep { name, newest } = &self.actions[i].action else {
                unreachable!()
            };
            // The state shown is the one before the action (the question is
            // asked mid-event), so judge by the card, and by the order the
            // engine lists them in (oldest first).
            let found: Vec<usize> = d
                .options
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    matches!(c, Choice::KeepLegend { def, .. } if self.state().pool.get(*def).name == *name)
                })
                .map(|(k, _)| k)
                .collect();
            if found.is_empty() {
                continue;
            }
            self.fired[i] = true;
            let pick = if *newest {
                found[found.len() - 1]
            } else {
                found[0]
            };
            return idx_u32(pick);
        }
        0
    }

    /// Several replacement effects apply to one event: the first unfired
    /// scripted `apply` naming one of their sources goes first; otherwise
    /// the engine's first option.
    fn choose_replacement(&mut self, d: &Decision, pos: Position) -> ChoiceId {
        let idxs = self.matching(d.player, pos, true, |a| matches!(a, Action::Apply(_)));
        for i in idxs {
            let Action::Apply(source) = &self.actions[i].action else {
                unreachable!()
            };
            let found = d.options.iter().position(|c| match c {
                Choice::Replacement { def, .. } => self.state().pool.get(*def).name == *source,
                _ => false,
            });
            if let Some(idx) = found {
                self.fired[i] = true;
                return idx_u32(idx);
            }
        }
        0
    }

    /// Several of the player's triggers wait to go on the stack: the first
    /// unfired scripted `trigger` naming one of their sources goes next;
    /// otherwise the engine's first option.
    fn order_triggers(&mut self, d: &Decision, pos: Position) -> ChoiceId {
        let idxs = self.matching(d.player, pos, true, |a| matches!(a, Action::Trigger { .. }));
        for i in idxs {
            let a = self.actions[i].clone();
            let Action::Trigger { source, targets } = &a.action else {
                unreachable!()
            };
            let found = d.options.iter().position(|c| match c {
                Choice::Trigger(t) => self
                    .state()
                    .pending_triggers
                    .get(*t)
                    .is_some_and(|p| self.state().name_of_or_lki(p.ability.source) == source),
                _ => false,
            });
            if let Some(idx) = found {
                self.fired[i] = true;
                self.pending_targets = (!targets.is_empty()).then(|| targets.clone());
                return idx_u32(idx);
            }
        }
        0
    }

    /// Targets for a triggered ability on the stack: those scripted with its
    /// `trigger` line (fired now if the engine put the ability on the stack
    /// without an ordering decision), else the first legal option.
    fn choose_ability_target(
        &mut self,
        d: &Decision,
        ability: ObjId,
        index: usize,
        pos: Position,
    ) -> Res<ChoiceId> {
        if self.pending_targets.is_none() {
            let st = self.state();
            let source = st.objects[ability]
                .ability
                .map(|a| st.name_of_or_lki(a.source).to_string())
                .unwrap_or_default();
            let idxs = self.matching(
                d.player,
                pos,
                true,
                |a| matches!(a, Action::Trigger { source: s, .. } if *s == source),
            );
            if let Some(&i) = idxs.first() {
                self.fired[i] = true;
                let Action::Trigger { targets, .. } = &self.actions[i].action else {
                    unreachable!()
                };
                let specs = self.state().targets_of(ability);
                let (needed, optional, more) =
                    (specs.len(), optional_count(specs), unbounded(specs));
                let targets = expand_auto(targets, needed);
                if (targets.len() > needed && !more) || targets.len() + optional < needed {
                    return fail(
                        self.actions[i].line,
                        format!(
                            "\"{source}\"'s trigger takes {needed} target(s) but {} were scripted",
                            targets.len()
                        ),
                    );
                }
                self.pending_targets = Some(targets.clone());
                self.pending_illegal = self.actions[i].illegal;
            } else {
                return Ok(0);
            }
        }
        let line = self.current_ability_line();
        let targets = self.pending_targets.clone().unwrap_or_default();
        if index + 1 >= targets.len() {
            // Last target of this ability: the next ability starts afresh.
            self.pending_targets = None;
            self.targets_done = Some(ability);
        }
        let Some(tr) = targets.get(index) else {
            return fail(
                line,
                format!(
                    "trigger needs target #{} but only {} scripted",
                    index + 1,
                    targets.len()
                ),
            );
        };
        if matches!(tr, TargetRef::Auto) {
            if index + 1 == targets.len() {
                self.pending_targets = None;
            }
            return Ok(0);
        }
        let t = self.target_for(d, line, tr)?;
        if index + 1 == targets.len() {
            // All chosen: a trigger that fires while the costs are paid
            // (a sacrifice, CR 601.2h) must not inherit them.
            self.pending_targets = None;
        }
        // `illegal trigger ... targeting X`: X must be refused; the first
        // legal target is taken instead.
        if self.pending_illegal {
            self.pending_illegal = false;
            return match index_of(d, &Choice::Target(t)) {
                Some(_) => fail(
                    line,
                    format!(
                        "{} was a legal target but the scenario says it must not be",
                        self.describe_target(t)
                    ),
                ),
                None => Ok(0),
            };
        }
        index_of(d, &Choice::Target(t)).ok_or_else(|| Failure {
            line,
            message: format!(
                "{} is not a legal target; legal targets: {}",
                self.describe_target(t),
                self.describe_options(d)
            ),
        })
    }

    /// Line of the most recently fired `trigger`, for target errors.
    fn current_ability_line(&self) -> usize {
        self.actions
            .iter()
            .zip(&self.fired)
            .filter(|(a, f)| {
                **f && matches!(a.action, Action::Trigger { .. } | Action::Activate { .. })
            })
            .map(|(a, _)| a.line)
            .next_back()
            .unwrap_or(0)
    }

    #[allow(clippy::too_many_lines)] // one block per outcome of a scripted action
    fn priority_action(&mut self, d: &Decision, pos: Position) -> Res<ChoiceId> {
        self.pending_targets = None;
        let stack_names: Vec<String> = self
            .state()
            .stack
            .iter()
            .map(|id| self.state().name_of(*id).to_string())
            .collect();
        let special = self.matching(d.player, pos, true, |a| {
            matches!(
                a,
                Action::TurnFaceUp(_) | Action::Suspend(_) | Action::Foretell(_)
            )
        });
        for i in special {
            let (Action::TurnFaceUp(name) | Action::Suspend(name) | Action::Foretell(name)) =
                &self.actions[i].action
            else {
                unreachable!()
            };
            let action = &self.actions[i].action;
            let found = d.options.iter().position(|c| match (c, action) {
                (Choice::TurnFaceUp(id), Action::TurnFaceUp(_))
                | (Choice::Suspend(id), Action::Suspend(_))
                | (Choice::Foretell(id), Action::Foretell(_)) => {
                    self.state().pool.get(self.state().objects[*id].card).name == *name
                }
                _ => false,
            });
            // `illegal unmorph "X"`: not offered here, as required.
            if self.actions[i].illegal {
                if found.is_some() {
                    return fail(
                        self.actions[i].line,
                        format!(
                            "{} \"{name}\" was legal for {} but the scenario says it must not be",
                            self.actions[i].action.kind_name(),
                            player_name(d.player)
                        ),
                    );
                }
                self.fired[i] = true;
                continue;
            }
            if let Some(k) = found {
                self.fired[i] = true;
                return Ok(idx_u32(k));
            }
        }
        let candidates = self.matching(d.player, pos, false, |a| match a {
            Action::Play(_) | Action::Activate { .. } | Action::Wait => true,
            // A card waiting in exile (suspend, rebound) is cast by its
            // trigger, not at priority.
            Action::Cast {
                respond_to, name, ..
            } => {
                respond_to.as_ref().is_none_or(|n| stack_names.contains(n))
                    && !self.waiting_in_exile(d.player, name)
            }
            _ => false,
        });
        for i in candidates {
            let a = self.actions[i].clone();
            if matches!(a.action, Action::Wait) {
                if stack_names.is_empty() {
                    self.fired[i] = true;
                    continue;
                }
                return index_of(d, &Choice::Pass).ok_or_else(|| self.unexpected(d, "no pass"));
            }
            if let Action::Activate {
                source,
                ability,
                sacrifice,
                discard,
                targets,
            } = &a.action
            {
                match self.activation(
                    d,
                    &a,
                    source,
                    *ability,
                    sacrifice.as_deref(),
                    discard.as_deref(),
                    targets,
                )? {
                    Some(idx) => return Ok(idx),
                    None => continue,
                }
            }
            let (name, option, targets, mode, costs) = match &a.action {
                Action::Play(name) => {
                    let id = match self.playable_in_exile(d.player, name, true) {
                        Some(id) if self.in_hand(a.line, d.player, name).is_err() => id,
                        _ => self.in_hand(a.line, d.player, name)?,
                    };
                    (
                        name,
                        Choice::PlayLand(id),
                        None,
                        None,
                        (None, None, None, false),
                    )
                }
                Action::Cast {
                    name,
                    from_graveyard,
                    kicked,
                    targets,
                    mode,
                    x,
                    sacrifice,
                    discard,
                    face_down,
                    evoked,
                    dashed,
                    alternative,
                    bestowed,
                    overloaded,
                    ..
                } => {
                    let choice = if *face_down {
                        Choice::CastFaceDown(self.in_hand(a.line, d.player, name)?)
                    } else if *evoked {
                        Choice::CastEvoke(self.in_hand(a.line, d.player, name)?)
                    } else if *dashed {
                        Choice::CastDash(self.in_hand(a.line, d.player, name)?)
                    } else if *bestowed {
                        Choice::CastBestow(self.in_hand(a.line, d.player, name)?)
                    } else if *overloaded {
                        Choice::CastOverload(self.in_hand(a.line, d.player, name)?)
                    } else if *alternative {
                        Choice::CastAlt(self.in_hand(a.line, d.player, name)?)
                    } else if *from_graveyard {
                        Choice::Cast(self.in_graveyard(a.line, d.player, name)?)
                    } else if let Some(c) = self.face_in_hand(d.player, name) {
                        c
                    } else if let Some(id) = self.on_adventure(d.player, name) {
                        Choice::Cast(id)
                    } else {
                        match self.in_hand(a.line, d.player, name) {
                            Ok(id) => Choice::Cast(id),
                            Err(e) => {
                                Choice::Cast(self.playable_in_exile(d.player, name, true).ok_or(e)?)
                            }
                        }
                    };
                    (
                        name,
                        choice,
                        Some(targets.clone()),
                        *mode,
                        (*x, sacrifice.clone(), discard.clone(), *kicked),
                    )
                }
                _ => unreachable!("filtered to play/cast"),
            };
            let (Choice::PlayLand(id)
            | Choice::Cast(id)
            | Choice::CastFace { card: id, .. }
            | Choice::CastFaceDown(id)
            | Choice::CastEvoke(id)
            | Choice::CastDash(id)
            | Choice::CastAlt(id)
            | Choice::CastBestow(id)
            | Choice::CastOverload(id)) = option
            else {
                unreachable!()
            };
            let found = index_of(d, &option);
            // "illegal cast ... splicing X": the cast is made, and X must
            // not be offered for splicing onto it.
            let splice_refusal =
                a.illegal && matches!(&a.action, Action::Cast { splice, .. } if !splice.is_empty());
            let illegal = a.illegal && !splice_refusal;
            let refused = match (illegal, found, &targets) {
                (true, Some(idx), Some(ts)) if !ts.is_empty() => {
                    self.scripted_target_refused(idx, ts, costs.0, a.line)?
                }
                _ => false,
            };
            match (illegal, found) {
                // "illegal cast ... targeting X": castable, but not at X.
                (true, Some(_)) if refused => {
                    self.fired[i] = true;
                }
                (true, Some(_)) => {
                    return fail(
                        a.line,
                        format!(
                            "{} \"{name}\" was legal for {} but the scenario says it must not be",
                            a.action.kind_name(),
                            player_name(d.player)
                        ),
                    );
                }
                (true, None) => {
                    self.fired[i] = true;
                }
                (false, None) => {
                    // Sorcery-speed actions wait for an empty stack; they are
                    // retried at the player's next priority in this step.
                    let def = self.state().def_of(id).expect("card in hand");
                    let instant_speed =
                        def.is_instant() || def.keywords.contains(mtg_core::card::Keyword::Flash);
                    if !instant_speed && !self.state().stack.is_empty() {
                        break;
                    }
                    return fail(
                        a.line,
                        format!(
                            "{} \"{name}\" is not legal for {} at turn {} {}; legal options: {}",
                            a.action.kind_name(),
                            player_name(d.player),
                            pos.turn,
                            step_name(pos.step),
                            self.describe_options(d)
                        ),
                    );
                }
                (false, Some(idx)) => {
                    self.fired[i] = true;
                    if let Some(targets) = targets {
                        let ((mut needed, optional, variable), pending_mode) =
                            self.cast_needs(a.line, id, name, mode)?;
                        // A bestowed spell targets what it enchants; spliced
                        // cards' targets follow the spell's (CR 702.47a).
                        if let Action::Cast {
                            bestowed,
                            overloaded,
                            splice,
                            ..
                        } = &a.action
                        {
                            if *bestowed {
                                needed += 1;
                            }
                            if *overloaded {
                                needed = 0;
                            }
                            if !splice_refusal {
                                let pool = &self.state().pool;
                                needed += splice
                                    .iter()
                                    .filter_map(|n| pool.lookup(n))
                                    .map(|r| pool.get(r).cast_targets().len())
                                    .sum::<usize>();
                            }
                            if splice_refusal {
                                self.pending_splice.clear();
                                self.refused_splice.clone_from(splice);
                            } else {
                                self.refused_splice.clear();
                                self.pending_splice.clone_from(splice);
                            }
                        }
                        self.pending_division = match &a.action {
                            Action::Cast { divide, .. } => divide.clone(),
                            _ => Vec::new(),
                        };
                        self.pending_mode = pending_mode;
                        self.pending_other_cost = matches!(
                            &a.action,
                            Action::Cast {
                                other_cost: true,
                                ..
                            }
                        );
                        self.pending_conspired = matches!(
                            &a.action,
                            Action::Cast {
                                conspired: true,
                                ..
                            }
                        );
                        (
                            self.pending_x,
                            self.pending_sacrifice,
                            self.pending_discard,
                            self.pending_kicked,
                        ) = costs;
                        let targets = expand_auto(&targets, needed);
                        if (targets.len() > needed && !variable)
                            || targets.len() + optional < needed
                        {
                            return fail(
                                a.line,
                                format!(
                                    "\"{name}\" takes {needed} target(s) but {} were scripted",
                                    targets.len()
                                ),
                            );
                        }
                        // An untargeted spell leaves the script free for
                        // the triggers its cast causes.
                        if !targets.is_empty() {
                            self.pending_targets = Some(targets);
                        }
                    }
                    return Ok(idx);
                }
            }
        }
        index_of(d, &Choice::Pass).ok_or_else(|| self.unexpected(d, "no Pass option"))
    }

    /// How many targets a scripted cast of `id` takes, and the mode to
    /// choose for a modal spell (`mode N`, default the first).
    fn cast_needs(
        &self,
        line: usize,
        id: ObjId,
        name: &str,
        mode: Option<usize>,
    ) -> Res<((usize, usize, bool), Option<usize>)> {
        let st = self.state();
        let mut def = st.def_of(id).expect("card in hand");
        // `cast "Stomp"`: the named face of the card.
        if def.name != name
            && let Some(f) = def.faces.iter().find(|f| st.pool.get(**f).name == name)
        {
            def = st.pool.get(*f);
        }
        let Some(modes) = def.modes() else {
            if mode.is_some() {
                return fail(line, format!("\"{name}\" is not modal"));
            }
            let t = def.cast_targets();
            return Ok(((t.len(), optional_count(t), unbounded(t)), None));
        };
        let m = mode.unwrap_or(1);
        let Some(chosen) = modes.get(m - 1) else {
            return fail(
                line,
                format!("\"{name}\" has {} modes, not {m}", modes.len()),
            );
        };
        Ok((
            (
                chosen.targets.len(),
                optional_count(&chosen.targets),
                unbounded(&chosen.targets),
            ),
            Some(m),
        ))
    }

    /// A scripted `activate`: the option activating an ability of the named
    /// permanent (with the named sacrifice, if any). `Ok(None)` means the
    /// action did not fire this time (illegal as required, or waiting for
    /// an empty stack).
    #[allow(clippy::too_many_arguments)] // the parts of one `activate` line
    fn activation(
        &mut self,
        d: &Decision,
        a: &ScriptedAction,
        source: &str,
        nth: Option<usize>,
        sacrifice: Option<&str>,
        discard: Option<&str>,
        targets: &[TargetRef],
    ) -> Res<Option<ChoiceId>> {
        let found = {
            let st = self.state();
            let names_match = |want: Option<&str>, have: &Option<ObjId>| match (want, have) {
                (Some(want), Some(have)) => st.name_of(*have) == want,
                (None, _) => true,
                (Some(_), None) => false,
            };
            // `ability N`: the Nth activated ability the permanent has.
            let nth_match = |s: ObjId, ability: AbilityId| match nth {
                None => true,
                Some(n) => st.characteristics(s).is_some_and(|c| {
                    c.abilities
                        .iter()
                        .copied()
                        .filter(|a| matches!(st.pool.ability(*a), AbilityDef::Activated(_)))
                        .nth(n - 1)
                        == Some(ability)
                }),
            };
            d.options.iter().position(|c| match c {
                Choice::Activate {
                    source: s,
                    ability,
                    sacrifice: sac,
                    discard: dis,
                } => {
                    st.objects[*s].controller == d.player
                        && st.name_of(*s) == source
                        && nth_match(*s, *ability)
                        && names_match(sacrifice, sac)
                        && names_match(discard, dis)
                }
                _ => false,
            })
        };
        let stack_empty = self.state().stack.is_empty();
        let step = self.state().step;
        let i = self
            .actions
            .iter()
            .position(|x| x.line == a.line)
            .expect("action exists");
        match (a.illegal, found) {
            // "illegal ... targeting X": legal to activate, but not at X.
            (true, Some(idx))
                if self.scripted_target_refused(idx_u32(idx), targets, None, a.line)? =>
            {
                self.fired[i] = true;
                Ok(None)
            }
            (true, Some(_)) => fail(
                a.line,
                format!(
                    "activating \"{source}\" was legal for {} but the scenario says it must not be",
                    player_name(d.player)
                ),
            ),
            (true, None) => {
                self.fired[i] = true;
                Ok(None)
            }
            (false, None) => {
                // A sorcery-speed ability waits for an empty stack.
                if !stack_empty {
                    return Ok(None);
                }
                fail(
                    a.line,
                    format!(
                        "activating \"{source}\" is not legal for {} at turn {} {}; legal options: {}",
                        player_name(d.player),
                        a.turn,
                        step_name(step),
                        self.describe_options(d)
                    ),
                )
            }
            (false, Some(idx)) => {
                self.fired[i] = true;
                let Choice::Activate { ability, .. } = &d.options[idx] else {
                    unreachable!()
                };
                let specs = self.state().pool.ability(*ability).targets();
                let (needed, optional, more) =
                    (specs.len(), optional_count(specs), unbounded(specs));
                let targets = expand_auto(targets, needed);
                if (targets.len() > needed && !more) || targets.len() + optional < needed {
                    return fail(
                        a.line,
                        format!(
                            "\"{source}\"'s ability takes {needed} target(s) but {} were scripted",
                            targets.len()
                        ),
                    );
                }
                if needed > 0 {
                    self.pending_targets = Some(targets.clone());
                }
                Ok(Some(idx_u32(idx)))
            }
        }
    }

    fn choose_target(&mut self, d: &Decision, index: usize) -> Res<ChoiceId> {
        let line = self.current_cast_line();
        let Some(targets) = self.pending_targets.clone() else {
            return fail(
                line,
                "the engine asked for a target but no scripted cast is in progress",
            );
        };
        let Some(tr) = targets.get(index) else {
            return fail(
                line,
                format!(
                    "spell needs target #{} but only {} scripted",
                    index + 1,
                    targets.len()
                ),
            );
        };
        if matches!(tr, TargetRef::Auto) {
            if index + 1 == targets.len() {
                self.pending_targets = None;
            }
            return Ok(0);
        }
        let t = self.target_for(d, line, tr)?;
        if index + 1 == targets.len() {
            // All chosen: a trigger that fires while the costs are paid
            // (a sacrifice, CR 601.2h) must not inherit them.
            self.pending_targets = None;
        }
        index_of(d, &Choice::Target(t)).ok_or_else(|| Failure {
            line,
            message: format!(
                "{} is not a legal target; legal targets: {}",
                self.describe_target(t),
                self.describe_options(d)
            ),
        })
    }

    /// Line of the most recently fired cast, for target errors.
    /// "You may cast it": yes. A scripted `cast "X"` at this point (in any
    /// step) supplies the spell's targets.
    fn cast_from_exile(&mut self, d: &Decision, pos: Position, card: ObjId) -> ChoiceId {
        let name = self.state().name_of(card).to_string();
        let idxs = self.matching(
            d.player,
            pos,
            true,
            |a| matches!(a, Action::Cast { name: n, .. } if *n == name),
        );
        if let Some(&i) = idxs.first() {
            self.fired[i] = true;
            if let Action::Cast { targets, .. } = &self.actions[i].action
                && !targets.is_empty()
            {
                self.pending_targets = Some(targets.clone());
            }
        }
        index_of(d, &Choice::Accept).unwrap_or(0)
    }

    fn waiting_in_exile(&self, p: PlayerId, name: &str) -> bool {
        let st = self.state();
        st.player(p).exile.iter().any(|id| {
            matches!(
                st.objects[*id].alt,
                mtg_core::state::AltCast::Suspended
                    | mtg_core::state::AltCast::Rebounding
                    | mtg_core::state::AltCast::MadnessExiled
            ) && st.name_of(*id) == name
        })
    }

    fn current_cast_line(&self) -> usize {
        self.actions
            .iter()
            .zip(&self.fired)
            .filter(|(a, f)| **f && matches!(a.action, Action::Cast { .. }))
            .map(|(a, _)| a.line)
            .next_back()
            .unwrap_or(0)
    }

    fn declare_attackers(&mut self, d: &Decision, pos: Position) -> Res<ChoiceId> {
        if self.attack_queue.is_none() {
            let idxs = self.matching(d.player, pos, false, |a| matches!(a, Action::Attack(_)));
            let mut legal = Vec::new();
            let mut illegal = Vec::new();
            for i in idxs {
                self.fired[i] = true;
                let a = self.actions[i].clone();
                let Action::Attack(names) = &a.action else {
                    unreachable!()
                };
                for (name, at) in names {
                    let used: Vec<ObjId> = legal.iter().chain(&illegal).copied().collect();
                    let id = self.creature_of(a.line, d.player, name, &used)?;
                    if let Some(tr) = at {
                        self.attack_targets.push((id, tr.clone(), a.illegal));
                    }
                    if a.illegal && at.is_none() {
                        illegal.push(id);
                    } else {
                        legal.push(id);
                    }
                }
                for id in &illegal {
                    let offered = d.options.iter().any(|c| match c {
                        Choice::Attackers(set) => set.contains(id),
                        Choice::AddAttacker(x) => x == id,
                        _ => false,
                    });
                    if offered {
                        return fail(
                            a.line,
                            format!(
                                "attacking with \"{}\" was legal but the scenario says it must not be",
                                self.state().name_of(*id)
                            ),
                        );
                    }
                }
            }
            self.attack_queue = Some(legal);
        }
        let queue = self.attack_queue.clone().unwrap_or_default();
        let line = self.line_of_kind(pos, d.player, "attack");
        if d.options.iter().any(|c| matches!(c, Choice::Attackers(_))) {
            self.attack_queue = None;
            let idx = d.options.iter().position(|c| match c {
                Choice::Attackers(set) => same_set(set, &queue),
                _ => false,
            });
            // Unscripted, declare no attackers; when a creature must attack
            // (CR 508.1d) that is not offered, so take the first declaration.
            if idx.is_none() && queue.is_empty() {
                return Ok(0);
            }
            return idx.map(idx_u32).ok_or_else(|| Failure {
                line,
                message: format!(
                    "attacking with {} is not legal; legal declarations: {}",
                    self.describe_ids(&queue),
                    self.describe_options(d)
                ),
            });
        }
        // Incremental mode.
        let Some(next) = queue.first().copied() else {
            self.attack_queue = None;
            // No `Done` means a creature must attack: add the first offered.
            return Ok(index_of(d, &Choice::Done).unwrap_or(0));
        };
        self.attack_queue = Some(queue[1..].to_vec());
        index_of(d, &Choice::AddAttacker(next)).ok_or_else(|| Failure {
            line,
            message: format!(
                "attacking with \"{}\" is not legal",
                self.state().name_of(next)
            ),
        })
    }

    fn declare_blockers(&mut self, d: &Decision, pos: Position) -> Res<ChoiceId> {
        if self.block_queue.is_none() {
            let idxs = self.matching(d.player, pos, false, |a| matches!(a, Action::Block { .. }));
            let mut legal: Vec<(ObjId, ObjId)> = Vec::new();
            let mut illegal: Vec<(ObjId, ObjId)> = Vec::new();
            for i in idxs {
                self.fired[i] = true;
                let a = self.actions[i].clone();
                let Action::Block { attacker, blocker } = &a.action else {
                    unreachable!()
                };
                let used: Vec<ObjId> = legal.iter().chain(&illegal).map(|(b, _)| *b).collect();
                // A distinct creature per line while there is one; then the
                // same one again (a creature blocking several attackers).
                let b = self
                    .creature_of(a.line, d.player, blocker, &used)
                    .or_else(|_| self.creature_of(a.line, d.player, blocker, &[]))?;
                let at = self.attacker_named(a.line, attacker)?;
                if a.illegal {
                    illegal.push((b, at));
                } else {
                    legal.push((b, at));
                }
            }
            if !illegal.is_empty() {
                let full: Vec<(ObjId, ObjId)> = legal.iter().chain(&illegal).copied().collect();
                let offered = d.options.iter().any(|c| match c {
                    Choice::Blocks(set) => same_set(set, &full),
                    Choice::AddBlock { blocker, attacker } => {
                        illegal.contains(&(*blocker, *attacker))
                    }
                    _ => false,
                });
                if offered {
                    let line = self.line_of_kind(pos, d.player, "block");
                    return fail(
                        line,
                        format!(
                            "blocking {} was legal but the scenario says it must not be",
                            self.describe_blocks(&illegal)
                        ),
                    );
                }
            }
            self.block_queue = Some(legal);
        }
        let queue = self.block_queue.clone().unwrap_or_default();
        let line = self.line_of_kind(pos, d.player, "block");
        if d.options.iter().any(|c| matches!(c, Choice::Blocks(_))) {
            self.block_queue = None;
            let idx = d.options.iter().position(|c| match c {
                Choice::Blocks(set) => same_set(set, &queue),
                _ => false,
            });
            // Unscripted, declare no blockers; when a requirement forces a
            // block (CR 509.1c) that is not offered, so take the first
            // declaration.
            if idx.is_none() && queue.is_empty() {
                return Ok(0);
            }
            return idx.map(idx_u32).ok_or_else(|| Failure {
                line,
                message: format!(
                    "blocking {} is not legal; legal declarations: {}",
                    self.describe_blocks(&queue),
                    self.describe_options(d)
                ),
            });
        }
        let Some((blocker, attacker)) = queue.first().copied() else {
            self.block_queue = None;
            // No `Done` means a requirement forces another block (CR
            // 509.1c): add the first offered.
            return Ok(index_of(d, &Choice::Done).unwrap_or(0));
        };
        self.block_queue = Some(queue[1..].to_vec());
        index_of(d, &Choice::AddBlock { blocker, attacker }).ok_or_else(|| Failure {
            line,
            message: format!(
                "blocking {} is not legal",
                self.describe_blocks(&[(blocker, attacker)])
            ),
        })
    }

    fn assign_damage(&mut self, d: &Decision, pos: Position, attacker: ObjId) -> Res<ChoiceId> {
        let name = self.state().name_of(attacker).to_string();
        let idxs = self.matching(
            d.player,
            pos,
            true,
            |a| matches!(a, Action::Assign { attacker, .. } if *attacker == name),
        );
        let Some(&i) = idxs.first() else {
            return Ok(0);
        };
        self.fired[i] = true;
        let a = self.actions[i].clone();
        let Action::Assign { parts, .. } = &a.action else {
            unreachable!()
        };
        // The creatures it's dividing damage among: an attacker's blockers,
        // or the attackers a blocker blocks (CR 510.1d).
        let blockers: Vec<ObjId> = self
            .state()
            .combat
            .as_ref()
            .map(|c| match c.attacker(attacker) {
                Some(at) => at.blockers.to_vec(),
                None => c
                    .attackers
                    .iter()
                    .filter(|at| at.blockers.contains(&attacker))
                    .map(|at| at.id)
                    .collect(),
            })
            .unwrap_or_default();
        let mut division: Vec<(Target, u32)> = Vec::new();
        for (tr, amount) in parts {
            let t = match tr {
                TargetRef::Auto => return fail(a.line, "`auto` is not a damage recipient"),
                TargetRef::Player(p) => Target::Player(*p),
                TargetRef::Object(r) => {
                    let id = blockers
                        .iter()
                        .copied()
                        .find(|b| self.state().name_of(*b) == r.name)
                        .ok_or_else(|| Failure {
                            line: a.line,
                            message: format!("\"{}\" is not in combat with \"{name}\"", r.name),
                        })?;
                    Target::Object(id)
                }
            };
            division.push((t, *amount));
        }
        let mut want = division.clone();
        want.sort_by_key(|(t, n)| (format!("{t:?}"), *n));
        let found = d.options.iter().position(|c| match c {
            Choice::Division(div) => {
                let mut have = div.clone();
                have.sort_by_key(|(t, n)| (format!("{t:?}"), *n));
                have == want
            }
            _ => false,
        });
        match (a.illegal, found) {
            (true, Some(_)) => fail(
                a.line,
                format!(
                    "damage division {} was legal but the scenario says it must not be",
                    self.describe_division(&division)
                ),
            ),
            (true, None) => Ok(0),
            (false, Some(idx)) => Ok(idx_u32(idx)),
            (false, None) => fail(
                a.line,
                format!(
                    "damage division {} is not legal; legal divisions: {}",
                    self.describe_division(&division),
                    self.describe_options(d)
                ),
            ),
        }
    }

    fn discard(&mut self, d: &Decision, pos: Position) -> Res<ChoiceId> {
        let idxs = self.matching(d.player, pos, false, |a| matches!(a, Action::Discard(_)));
        let Some(&i) = idxs.first() else {
            return Ok(0);
        };
        self.fired[i] = true;
        let a = self.actions[i].clone();
        let Action::Discard(name) = &a.action else {
            unreachable!()
        };
        let id = self.in_hand(a.line, d.player, name)?;
        let found = d
            .options
            .iter()
            .position(|c| matches!(c, Choice::Discard { card, .. } if *card == id))
            .map(idx_u32);
        match (a.illegal, found) {
            (true, Some(_)) => fail(
                a.line,
                format!("discarding \"{name}\" was legal but the scenario says it must not be"),
            ),
            (true, None) => Ok(0),
            (false, Some(idx)) => Ok(idx),
            (false, None) => fail(a.line, format!("discarding \"{name}\" is not legal")),
        }
    }

    // ----- name resolution ---------------------------------------------------

    /// `cast "Stomp"`: a card in hand one of whose other faces has that
    /// name (an adventure, a split card's half).
    fn face_in_hand(&self, p: PlayerId, name: &str) -> Option<Choice> {
        let st = self.state();
        st.player(p).hand.iter().find_map(|id| {
            let def = st.pool.get(st.objects[*id].def);
            def.faces
                .iter()
                .position(|f| st.pool.get(*f).name == name)
                .map(|i| Choice::CastFace {
                    card: *id,
                    face: u8::try_from(i).unwrap_or(0),
                })
        })
    }

    /// A card of this name on an adventure in `p`'s exile (CR 715.3d).
    fn on_adventure(&self, p: PlayerId, name: &str) -> Option<ObjId> {
        let st = self.state();
        st.player(p)
            .exile
            .iter()
            .copied()
            .find(|id| {
                (st.objects[*id].cast.adventure()
                    || st.objects[*id].alt == mtg_core::state::AltCast::Foretold)
                    && st.name_of(*id) == name
            })
            .or_else(|| self.playable_in_exile(p, name, false))
    }

    /// An exiled card `p` may play (impulse draw); failing that, with
    /// `any`, any card in exile by that name (an `illegal` line checks it
    /// can't be played).
    fn playable_in_exile(&self, p: PlayerId, name: &str, any: bool) -> Option<ObjId> {
        let st = self.state();
        st.exile_casts
            .as_ref()
            .and_then(|e| {
                e.playable
                    .iter()
                    .find(|pe| {
                        pe.player == p
                            && st.objects.get(pe.card).is_some_and(|o| {
                                o.zone == Zone::Exile && st.name_of(pe.card) == name
                            })
                    })
                    .map(|pe| pe.card)
            })
            .or_else(|| {
                if !any {
                    return None;
                }
                st.objects
                    .iter()
                    .find(|(id, o)| o.zone == Zone::Exile && st.name_of(*id) == name)
                    .map(|(id, _)| id)
            })
    }

    fn in_hand(&self, line: usize, p: PlayerId, name: &str) -> Res<ObjId> {
        self.state()
            .player(p)
            .hand
            .iter()
            .copied()
            .find(|id| self.state().name_of(*id) == name)
            .ok_or_else(|| Failure {
                line,
                message: format!(
                    "\"{name}\" is not in {}'s hand (hand: {})",
                    player_name(p),
                    self.describe_ids(&self.state().player(p).hand)
                ),
            })
    }

    fn in_graveyard(&self, line: usize, p: PlayerId, name: &str) -> Res<ObjId> {
        self.state()
            .player(p)
            .graveyard
            .iter()
            .copied()
            .find(|id| self.state().name_of(*id) == name)
            .ok_or_else(|| Failure {
                line,
                message: format!(
                    "\"{name}\" is not in {}'s graveyard (graveyard: {})",
                    player_name(p),
                    self.describe_ids(&self.state().player(p).graveyard)
                ),
            })
    }

    /// A creature `p` controls named `name`, other than those in `used`.
    fn creature_of(&self, line: usize, p: PlayerId, name: &str, used: &[ObjId]) -> Res<ObjId> {
        let st = self.state();
        st.battlefield
            .iter()
            .copied()
            .filter(|id| st.objects[*id].controller == p && !used.contains(id))
            .find(|id| st.name_of(*id) == name)
            .ok_or_else(|| Failure {
                line,
                message: format!(
                    "{} controls no (further) \"{name}\" (battlefield: {})",
                    player_name(p),
                    self.describe_ids(&st.permanents_of(p).collect::<Vec<_>>())
                ),
            })
    }

    fn attacker_named(&self, line: usize, name: &str) -> Res<ObjId> {
        let st = self.state();
        st.combat
            .as_ref()
            .map(|c| c.attackers.iter().map(|a| a.id).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .find(|id| st.name_of(*id) == name)
            .ok_or_else(|| Failure {
                line,
                message: format!("\"{name}\" is not attacking"),
            })
    }

    /// Resolve an object reference against the battlefield. One match is
    /// required unless the reference names a controller, in which case the
    /// first match under that controller is used.
    /// A named object on the battlefield (or, failing that, a spell on the
    /// stack, or a card in a graveyard, for "target creature card in your
    /// graveyard").
    fn resolve_obj(&self, line: usize, r: &ObjRef) -> Res<ObjId> {
        let st = self.state();
        let mut matches: Vec<ObjId> = st
            .battlefield
            .iter()
            .copied()
            .filter(|id| r.player.is_none_or(|p| st.objects[*id].controller == p))
            .filter(|id| st.name_of(*id) == r.name)
            .collect();
        if matches.is_empty() {
            // A spell on the stack, for "target spell".
            matches = st
                .stack
                .iter()
                .copied()
                .filter(|id| st.objects[*id].kind != ObjKind::Ability)
                .filter(|id| r.player.is_none_or(|p| st.objects[*id].controller == p))
                .filter(|id| st.name_of(*id) == r.name)
                .collect();
        }
        if matches.is_empty() {
            matches = st
                .players
                .iter()
                .flat_map(|p| p.graveyard.iter().copied())
                .filter(|id| r.player.is_none_or(|p| st.objects[*id].owner == p))
                .filter(|id| st.name_of(*id) == r.name)
                .collect();
        }
        matches.sort_by_key(|id| st.objects[*id].timestamp);
        match (matches.len(), r.player, r.pick) {
            (0, None, _) => fail(line, format!("no \"{}\" on the battlefield", r.name)),
            (0, Some(p), _) => fail(
                line,
                format!("{} controls no \"{}\"", player_name(p), r.name),
            ),
            (_, _, Some(Pick::Newest)) => Ok(matches[matches.len() - 1]),
            (_, _, Some(Pick::Oldest)) | (1, _, None) | (_, Some(_), None) => Ok(matches[0]),
            (n, None, None) => fail(
                line,
                format!(
                    "{n} objects named \"{}\" on the battlefield; qualify with A or B, or oldest/newest",
                    r.name
                ),
            ),
        }
    }

    /// For an `illegal` line with targets: on a copy of the game, take the
    /// action and answer target choices as scripted; whether the engine
    /// refuses one of the scripted targets (other choices take the first
    /// option, X the scripted one). A scripted target the engine never
    /// asks for (a variable group with nothing legal to choose, CR
    /// 601.2c) counts as refused.
    fn scripted_target_refused(
        &self,
        idx: ChoiceId,
        targets: &[TargetRef],
        x: Option<u32>,
        line: usize,
    ) -> Res<bool> {
        let mut sim = self.engine.clone();
        if sim.step(idx).is_err() {
            return Ok(false);
        }
        let mut asked = 0;
        for _ in 0..64 {
            let StepResult::Decision(d) = sim.current().clone() else {
                return Ok(false);
            };
            let pick = match d.kind {
                DecisionKind::ChooseX { .. } => {
                    x.and_then(|x| index_of(&d, &Choice::X(x))).unwrap_or(0)
                }
                DecisionKind::ChooseTarget { index, .. } => {
                    let Some(tr) = targets.get(index) else {
                        return Ok(false);
                    };
                    asked = asked.max(index + 1);
                    if matches!(tr, TargetRef::Auto) {
                        0
                    } else {
                        let t = self.target_for(&d, line, tr)?;
                        match index_of(&d, &Choice::Target(t)) {
                            Some(k) => k,
                            None => return Ok(true),
                        }
                    }
                }
                DecisionKind::PriorityAction => return Ok(asked < targets.len()),
                _ => 0,
            };
            if sim.step(pick).is_err() {
                return Ok(false);
            }
        }
        Ok(false)
    }

    /// `resolve_target` for a target decision: when the named object is
    /// not among the options, an ability on the stack named after its
    /// source that is ("target activated ability").
    fn target_for(&self, d: &Decision, line: usize, tr: &TargetRef) -> Res<Target> {
        let t = self.resolve_target(line, tr)?;
        if index_of(d, &Choice::Target(t)).is_some() {
            return Ok(t);
        }
        let TargetRef::Object(r) = tr else {
            return Ok(t);
        };
        let st = self.state();
        Ok(st
            .stack
            .iter()
            .rev()
            .copied()
            .find(|id| {
                st.objects[*id].kind == ObjKind::Ability
                    && r.player.is_none_or(|p| st.objects[*id].controller == p)
                    && st.name_of(*id) == r.name
                    && index_of(d, &Choice::Target(Target::Object(*id))).is_some()
            })
            .map_or(t, Target::Object))
    }

    fn resolve_target(&self, line: usize, tr: &TargetRef) -> Res<Target> {
        Ok(match tr {
            TargetRef::Player(p) => Target::Player(*p),
            TargetRef::Object(r) => Target::Object(self.resolve_obj(line, r)?),
            TargetRef::Auto => return fail(line, "`auto` cannot name a specific target"),
        })
    }

    fn line_of_kind(&self, pos: Position, player: PlayerId, kind: &str) -> usize {
        self.actions
            .iter()
            .filter(|a| a.player == player && a.turn == pos.turn && a.action.kind_name() == kind)
            .map(|a| a.line)
            .next()
            .unwrap_or(0)
    }

    fn unexpected(&self, d: &Decision, what: &str) -> Failure {
        Failure {
            line: 0,
            message: format!(
                "{what} in {:?} decision: {}",
                d.kind,
                self.describe_options(d)
            ),
        }
    }

    // ----- assertions --------------------------------------------------------

    #[allow(clippy::too_many_lines)] // one arm per assertion kind
    fn check(&self, line: usize, a: &Assertion) -> Res<()> {
        let st = self.state();
        match a {
            Assertion::Life(p, want) => {
                let have = st.player(*p).life;
                expect(line, format!("life of {}", player_name(*p)), have, *want)
            }
            Assertion::Poison(p, want) => {
                let have = st.player(*p).poison;
                expect(line, format!("poison of {}", player_name(*p)), have, *want)
            }
            Assertion::Energy(p, want) => {
                let have = st.player(*p).energy;
                expect(line, format!("energy of {}", player_name(*p)), have, *want)
            }
            Assertion::Designation { key, player, want } => {
                let have = designation(st, key, *player);
                let what = match player {
                    Some(p) => format!("{key} of {}", player_name(*p)),
                    None => key.clone(),
                };
                if have == *want {
                    Ok(())
                } else {
                    fail(line, format!("{what} is {have:?}, expected {want:?}"))
                }
            }
            Assertion::ZoneContains {
                zone,
                player,
                name,
                count,
            } => {
                let have = self
                    .zone_names(*zone, *player)
                    .iter()
                    .filter(|n| *n == name)
                    .count();
                match count {
                    Some(want) => expect(
                        line,
                        format!(
                            "copies of \"{name}\" in {}'s {zone:?}",
                            player_name(*player)
                        ),
                        have,
                        *want,
                    ),
                    None if have >= 1 => Ok(()),
                    None => fail(
                        line,
                        format!(
                            "{}'s {zone:?} does not contain \"{name}\" (contains: {})",
                            player_name(*player),
                            join(&self.zone_names(*zone, *player))
                        ),
                    ),
                }
            }
            Assertion::ZoneLacks { zone, player, name } => {
                if self.zone_names(*zone, *player).iter().any(|n| n == name) {
                    fail(
                        line,
                        format!(
                            "{}'s {zone:?} contains \"{name}\" but should not",
                            player_name(*player)
                        ),
                    )
                } else {
                    Ok(())
                }
            }
            Assertion::ZoneCount {
                zone,
                player,
                count,
            } => expect(
                line,
                format!("cards in {}'s {zone:?}", player_name(*player)),
                self.zone_names(*zone, *player).len(),
                *count,
            ),
            Assertion::Power(r, want) => {
                let id = self.resolve_obj(line, r)?;
                let c = st.characteristics(id).expect("object exists");
                expect(line, format!("power of \"{}\"", r.name), c.power(), *want)
            }
            Assertion::Toughness(r, want) => {
                let id = self.resolve_obj(line, r)?;
                let c = st.characteristics(id).expect("object exists");
                expect(
                    line,
                    format!("toughness of \"{}\"", r.name),
                    c.toughness(),
                    *want,
                )
            }
            Assertion::Damage(r, want) => {
                let id = self.resolve_obj(line, r)?;
                expect(
                    line,
                    format!("damage on \"{}\"", r.name),
                    st.objects[id].damage,
                    *want,
                )
            }
            Assertion::Tapped(r, want) => {
                let id = self.resolve_obj(line, r)?;
                expect(
                    line,
                    format!("tapped state of \"{}\"", r.name),
                    st.objects[id].tapped,
                    *want,
                )
            }
            Assertion::Winner(p) => match st.result {
                Some(GameResult::Win { winner }) if winner == *p => Ok(()),
                other => fail(
                    line,
                    format!("expected {} to win; result is {other:?}", player_name(*p)),
                ),
            },
            Assertion::Draw => match st.result {
                Some(GameResult::Draw) => Ok(()),
                other => fail(line, format!("expected a draw; result is {other:?}")),
            },
            Assertion::GameOver(want) => {
                expect(line, "game over".to_string(), st.result.is_some(), *want)
            }
            Assertion::Turn(want) => expect(line, "turn".to_string(), st.turn_number, *want),
            Assertion::Step(want) => {
                if st.step == *want {
                    Ok(())
                } else {
                    fail(
                        line,
                        format!(
                            "step is {} but expected {}",
                            step_name(st.step),
                            step_name(*want)
                        ),
                    )
                }
            }
            Assertion::StackCount(want) => expect(
                line,
                "objects on the stack".to_string(),
                st.stack.len(),
                *want,
            ),
            Assertion::StackTop(want) => {
                let have = st
                    .stack
                    .last()
                    .map_or("nothing".to_string(), |id| st.name_of(*id).to_string());
                if have == *want {
                    Ok(())
                } else {
                    fail(
                        line,
                        format!("top of the stack is {have}, expected \"{want}\""),
                    )
                }
            }
            Assertion::HasKeyword(r, kw, want) => {
                let id = self.resolve_obj(line, r)?;
                let c = st.characteristics(id).expect("object exists");
                let Some(k) = keyword_by_name(kw) else {
                    return fail(line, format!("unknown keyword `{kw}`"));
                };
                expect(line, format!("\"{}\" has {kw}", r.name), c.has(k), *want)
            }
            Assertion::Is(r, what, want) => {
                let id = self.resolve_obj(line, r)?;
                let c = st.characteristics(id).expect("object exists");
                let lower = what.to_lowercase();
                let have = if let Some(t) = card_type_by_name(&lower) {
                    c.types.contains(t)
                } else if let Some(t) = supertype_by_name(&lower) {
                    c.supertypes.contains(t)
                } else if let Some(col) = color_by_name(&lower) {
                    c.colors.contains(col)
                } else if lower == "colorless" {
                    c.colors.is_colorless()
                } else if let Some(sub) = st
                    .pool
                    .subtype_lookup(what)
                    .or_else(|| st.pool.subtype_lookup(&capitalize(&lower)))
                {
                    c.has_subtype(sub)
                } else {
                    return fail(line, format!("unknown type, subtype, or color `{what}`"));
                };
                expect(line, format!("\"{}\" is {what}", r.name), have, *want)
            }
            Assertion::Counters(r, kind, want) => {
                let id = self.resolve_obj(line, r)?;
                let Some(k) = CounterKind::by_name(kind) else {
                    return fail(line, format!("unknown counter kind `{kind}`"));
                };
                expect(
                    line,
                    format!("{kind} counters on \"{}\"", r.name),
                    st.objects[id].counters(k),
                    *want,
                )
            }
            Assertion::Attached(aura, to) => {
                let a = self.resolve_obj(line, aura)?;
                let t = self.resolve_obj(line, to)?;
                if st.objects[a].attached_to == Some(t) {
                    return Ok(());
                }
                let have = st.objects[a]
                    .attached_to
                    .map_or("nothing".to_string(), |x| format!("\"{}\"", st.name_of(x)));
                fail(
                    line,
                    format!(
                        "\"{}\" is attached to {have}, expected \"{}\"",
                        aura.name, to.name
                    ),
                )
            }
            Assertion::NoAbilities(r) => {
                let id = self.resolve_obj(line, r)?;
                let c = st.characteristics(id).expect("object exists");
                if c.has_no_abilities() {
                    Ok(())
                } else {
                    fail(
                        line,
                        format!(
                            "\"{}\" has abilities: keywords {:?}, {} other",
                            r.name,
                            c.keywords,
                            c.abilities.len()
                        ),
                    )
                }
            }
        }
    }

    fn zone_names(&self, zone: Zone, p: PlayerId) -> Vec<String> {
        let st = self.state();
        let ids: Vec<ObjId> = match zone {
            Zone::Battlefield => st.permanents_of(p).collect(),
            Zone::Stack => st.stack.clone(),
            Zone::Command => Vec::new(),
            Zone::Library => st.player(p).library.iter().rev().copied().collect(),
            z => st.player(p).zone_mut_free(z).to_vec(),
        };
        ids.iter().map(|id| st.name_of(*id).to_string()).collect()
    }

    // ----- descriptions for messages -------------------------------------------

    fn describe_ids(&self, ids: &[ObjId]) -> String {
        if ids.is_empty() {
            return "nothing".to_string();
        }
        join(
            &ids.iter()
                .map(|id| format!("\"{}\"", self.state().name_of(*id)))
                .collect::<Vec<_>>(),
        )
    }

    fn describe_target(&self, t: Target) -> String {
        match t {
            Target::Player(p) => format!("player {}", player_name(p)),
            Target::Object(id) => format!("\"{}\"", self.state().name_of(id)),
        }
    }

    fn describe_blocks(&self, blocks: &[(ObjId, ObjId)]) -> String {
        if blocks.is_empty() {
            return "nothing".to_string();
        }
        join(
            &blocks
                .iter()
                .map(|(b, a)| {
                    format!(
                        "\"{}\" with \"{}\"",
                        self.state().name_of(*a),
                        self.state().name_of(*b)
                    )
                })
                .collect::<Vec<_>>(),
        )
    }

    fn describe_division(&self, div: &[(Target, u32)]) -> String {
        join(
            &div.iter()
                .map(|(t, n)| format!("{} {n}", self.describe_target(*t)))
                .collect::<Vec<_>>(),
        )
    }

    fn describe_choice(&self, c: &Choice) -> String {
        match c {
            Choice::Keep => "keep".into(),
            Choice::Mulligan => "mulligan".into(),
            Choice::Bottom(id) => format!("bottom {}", self.describe_ids(&[*id])),
            Choice::Pass => "pass".into(),
            Choice::PlayLand(id) => format!("play {}", self.describe_ids(&[*id])),
            Choice::Cast(id) => format!("cast {}", self.describe_ids(&[*id])),
            Choice::CastFace { card, face } => {
                format!("cast face {face} of {}", self.describe_ids(&[*card]))
            }
            Choice::CastFaceDown(id) => format!("cast {} face down", self.describe_ids(&[*id])),
            Choice::CastEvoke(id) => format!("cast {} evoked", self.describe_ids(&[*id])),
            Choice::CastDash(id) => format!("cast {} dashed", self.describe_ids(&[*id])),
            Choice::CastAlt(id) => format!("cast {} alternative", self.describe_ids(&[*id])),
            Choice::CastBestow(id) => format!("cast {} bestowed", self.describe_ids(&[*id])),
            Choice::CastOverload(id) => {
                format!("cast {} overloaded", self.describe_ids(&[*id]))
            }
            Choice::Suspend(id) => format!("suspend {}", self.describe_ids(&[*id])),
            Choice::Foretell(id) => format!("foretell {}", self.describe_ids(&[*id])),
            Choice::TurnFaceUp(id) => {
                let card = self.state().objects[*id].card;
                format!("turn {} face up", self.state().pool.get(card).name)
            }
            Choice::Target(t) => self.describe_target(*t),
            Choice::Attackers(set) => format!("attack with {}", self.describe_ids(set)),
            Choice::AddAttacker(id) => format!("add attacker {}", self.describe_ids(&[*id])),
            Choice::Blocks(set) => format!("block {}", self.describe_blocks(set)),
            Choice::AddBlock { blocker, attacker } => {
                format!(
                    "add block {}",
                    self.describe_blocks(&[(*blocker, *attacker)])
                )
            }
            Choice::Done => "done".into(),
            Choice::Color(c) => format!("{c:?}").to_lowercase(),
            Choice::Subtype(s) => self.state().pool.subtype_name(*s).to_string(),
            Choice::Division(div) => format!("assign {}", self.describe_division(div)),
            Choice::Divide(parts) => format!(
                "dividing {}",
                join(&parts.iter().map(u32::to_string).collect::<Vec<_>>())
            ),
            Choice::Discard { def, .. } => {
                format!("discard {}", self.state().pool.get(*def).name)
            }
            Choice::Trigger(i) => format!(
                "trigger of \"{}\"",
                self.state()
                    .pending_triggers
                    .get(*i)
                    .map_or("?", |t| self.state().name_of_or_lki(t.ability.source))
            ),
            Choice::Replacement { def, .. } => {
                format!("apply \"{}\"", self.state().pool.get(*def).name)
            }
            Choice::Activate {
                source,
                sacrifice,
                discard,
                ..
            } => {
                let mut out = format!("activate {}", self.describe_ids(&[*source]));
                if let Some(s) = sacrifice {
                    out = format!("{out} sacrificing {}", self.describe_ids(&[*s]));
                }
                if let Some(c) = discard {
                    out = format!("{out} discarding {}", self.describe_ids(&[*c]));
                }
                out
            }
            Choice::X(n) => format!("X = {n}"),
            Choice::ScryBottom(ids) => format!("bottom {}", self.describe_ids(ids)),
            Choice::SurveilGraveyard(ids) => format!("graveyard {}", self.describe_ids(ids)),
            Choice::Sacrifice(id) => format!("sacrifice {}", self.describe_ids(&[*id])),
            Choice::AttackTarget(t) => format!("attack {}", self.describe_target(*t)),
            Choice::KeepLegend { def, age, .. } => {
                format!("keep \"{}\" #{}", self.state().pool.get(*def).name, age + 1)
            }
            Choice::Mode(i) => format!("mode {}", i + 1),
            Choice::Accept => "accept".into(),
            Choice::Decline => "decline".into(),
            Choice::CopyOf { obj, def } => {
                let st = self.state();
                let who = st
                    .obj(*obj)
                    .map_or(String::new(), |o| format!("{} ", player_name(o.controller)));
                format!("copy {who}\"{}\"", st.pool.get(*def).name)
            }
            Choice::NoCopy => "copy nothing".into(),
        }
    }

    fn describe_options(&self, d: &Decision) -> String {
        let parts: Vec<String> = d
            .options
            .iter()
            .take(12)
            .map(|c| self.describe_choice(c))
            .collect();
        let more = d.options.len().saturating_sub(12);
        if more > 0 {
            format!("[{}; and {more} more]", parts.join("; "))
        } else {
            format!("[{}]", parts.join("; "))
        }
    }
}

trait ZoneFree {
    fn zone_mut_free(&self, z: Zone) -> &[ObjId];
}

impl ZoneFree for mtg_core::state::PlayerState {
    fn zone_mut_free(&self, z: Zone) -> &[ObjId] {
        match z {
            Zone::Hand => &self.hand,
            Zone::Graveyard => &self.graveyard,
            Zone::Exile => &self.exile,
            Zone::Library => &self.library,
            Zone::Battlefield | Zone::Stack | Zone::Command => &[],
        }
    }
}

fn index_of(d: &Decision, c: &Choice) -> Option<ChoiceId> {
    d.options.iter().position(|o| o == c).map(idx_u32)
}

fn idx_u32(i: usize) -> ChoiceId {
    u32::try_from(i).expect("option index fits in u32")
}

fn same_set<T: PartialEq>(a: &[T], b: &[T]) -> bool {
    a.len() == b.len() && a.iter().all(|x| b.contains(x)) && b.iter().all(|x| a.contains(x))
}

fn join(items: &[String]) -> String {
    if items.is_empty() {
        "nothing".to_string()
    } else {
        items.join(", ")
    }
}

fn keyword_by_name(name: &str) -> Option<Keyword> {
    let lower = name.to_lowercase().replace('_', "");
    Keyword::ALL
        .into_iter()
        .find(|k| format!("{k:?}").to_lowercase() == lower)
}

fn card_type_by_name(lower: &str) -> Option<CardType> {
    Some(match lower {
        "artifact" => CardType::Artifact,
        "battle" => CardType::Battle,
        "creature" => CardType::Creature,
        "enchantment" => CardType::Enchantment,
        "instant" => CardType::Instant,
        "kindred" => CardType::Kindred,
        "land" => CardType::Land,
        "planeswalker" => CardType::Planeswalker,
        "sorcery" => CardType::Sorcery,
        _ => return None,
    })
}

fn supertype_by_name(lower: &str) -> Option<Supertype> {
    Some(match lower {
        "basic" => Supertype::Basic,
        "legendary" => Supertype::Legendary,
        "snow" => Supertype::Snow,
        "world" => Supertype::World,
        _ => return None,
    })
}

fn color_by_name(lower: &str) -> Option<mtg_core::mana::Color> {
    use mtg_core::mana::Color;
    Some(match lower {
        "white" => Color::White,
        "blue" => Color::Blue,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        _ => return None,
    })
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// A designation's value as `Assertion::Designation` names it.
fn designation(st: &GameState, key: &str, p: Option<PlayerId>) -> String {
    let d = &st.desig;
    let who = |q: Option<PlayerId>| q.map_or("none", player_name).to_string();
    let p = p.unwrap_or(PlayerId(0));
    match key {
        "monarch" => who(d.monarch),
        "initiative" => who(d.initiative),
        "daynight" => match d.day_night {
            mtg_core::state::DayNight::Neither => "neither",
            mtg_core::state::DayNight::Day => "day",
            mtg_core::state::DayNight::Night => "night",
        }
        .to_string(),
        "speed" => d.speed(p).to_string(),
        "tempted" => d.tempted(p).to_string(),
        "ring_bearer" => st
            .ring_bearer(p)
            .map_or("none".to_string(), |b| st.name_of(b).to_string()),
        "blessing" => if d.blessed(p) { "yes" } else { "no" }.to_string(),
        "completed" => d.completed(p).to_string(),
        "dungeon" => d.dungeon(p).map_or("none".to_string(), |m| {
            let def = st.pool.get(st.objects[m.obj].def);
            format!("{} / {}", def.name, def.rooms[usize::from(m.room)].name)
        }),
        _ => "?".to_string(),
    }
}

#[allow(clippy::needless_pass_by_value)] // call sites pass temporaries
fn expect<T: PartialEq + std::fmt::Debug>(line: usize, what: String, have: T, want: T) -> Res<()> {
    if have == want {
        Ok(())
    } else {
        fail(line, format!("{what} is {have:?}, expected {want:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use mtg_dsl::builtin_pool;

    fn run_text(text: &str) -> Report {
        let pool = Arc::new(builtin_pool());
        let scenarios = parse(text).expect("parses");
        run(&pool, &scenarios[0])
    }

    #[test]
    fn illegal_scripted_action_fails_the_scenario() {
        let r = run_text(
            r#"scenario "x" {
  battlefield A: "Forest", "Forest", "Hill Giant"
  hand A: "Grizzly Bears"
  turn 1 A: cast "Grizzly Bears"
  turn 1 A: attack "Grizzly Bears"
  run_to turn 1 postcombat_main
}"#,
        );
        let f = r.failure.expect("summoning-sick attack must fail");
        assert_eq!(f.line, 5);
        assert!(f.message.contains("not legal"), "{}", f.message);
    }

    #[test]
    fn wrong_assertion_fails_with_its_line() {
        let r = run_text(
            r#"scenario "x" {
  battlefield A: "Grizzly Bears"
  turn 1 A: attack "Grizzly Bears"
  run_to turn 1 postcombat_main
  assert life B 20
}"#,
        );
        let f = r.failure.expect("wrong life must fail");
        assert_eq!(f.line, 5);
        assert!(f.message.contains("18"), "{}", f.message);
    }

    #[test]
    fn unreached_action_fails() {
        let r = run_text(
            r#"scenario "x" {
  battlefield A: "Grizzly Bears"
  turn 3 A: attack "Grizzly Bears"
  run_to turn 2 upkeep
}"#,
        );
        let f = r.failure.expect("unreached action must fail");
        assert_eq!(f.line, 3);
        assert!(f.message.contains("never reached"), "{}", f.message);
    }

    #[test]
    fn action_at_a_passed_point_fails() {
        let r = run_text(
            r#"scenario "x" {
  hand A: "Forest"
  turn 1 untap A: play "Forest"
  run_to turn 2 upkeep
}"#,
        );
        let f = r.failure.expect("nobody has priority in the untap step");
        assert_eq!(f.line, 3);
        assert!(f.message.contains("never offered"), "{}", f.message);
    }

    #[test]
    fn unknown_card_and_bad_target_are_reported() {
        let r = run_text("scenario \"x\" {\n  battlefield A: \"Black Lotus\"\n}");
        assert_eq!(r.failure.expect("unknown card").line, 2);
        let r = run_text(
            r#"scenario "x" {
  battlefield A: "Mountain"
  hand A: "Lightning Bolt"
  turn 1 A: cast "Lightning Bolt" targeting "Hill Giant"
  run_to turn 1 begin_combat
}"#,
        );
        let f = r.failure.expect("missing target");
        assert_eq!(f.line, 4);
        assert!(f.message.contains("no \"Hill Giant\""), "{}", f.message);
    }

    #[test]
    fn skip_is_reported() {
        let r = run_text("scenario \"x\" {\n  skip \"not yet\"\n}");
        assert_eq!(r.skipped.as_deref(), Some("not yet"));
        assert!(!r.passed());
    }
}

/// Targets that may be left unchosen ("up to two"; a variable group, CR
/// 601.2c, counts as one).
fn optional_count(specs: &[mtg_core::card::TargetSpec]) -> usize {
    specs
        .iter()
        .filter(|s| {
            matches!(
                s,
                mtg_core::card::TargetSpec::UpTo(_) | mtg_core::card::TargetSpec::Many(_)
            )
        })
        .count()
}

/// Whether more targets than specs may be chosen (a variable group).
fn unbounded(specs: &[mtg_core::card::TargetSpec]) -> bool {
    matches!(specs.last(), Some(mtg_core::card::TargetSpec::Many(_)))
}
