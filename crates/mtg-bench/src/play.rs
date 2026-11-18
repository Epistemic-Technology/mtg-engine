//! Playing one benchmark game: each seat is a built-in agent or a model.

use std::fmt::Write as _;
use std::sync::Arc;

use anyhow::Result;
use mtg_agents::{Agent, GreedyAgent};
use mtg_core::card::CardPool;
use mtg_core::decision::{Choice, ChoiceId, Decision, DecisionKind, StepResult};
use mtg_core::engine::{Engine, GameConfig};
use mtg_core::ids::PlayerId;
use mtg_core::record::{DecisionNote, GameRecord};
use mtg_core::state::GameResult;
use regex::Regex;

use crate::llm::Model;

/// The fixed auto-pass policy (design §9.5), the same for every model:
/// a decision with one option is taken without asking, and opening hands
/// are kept. Everything else goes to the model.
pub fn automatic(d: &Decision) -> Option<ChoiceId> {
    if d.options.len() == 1 {
        return Some(0);
    }
    if d.kind == DecisionKind::Mulligan {
        return d
            .options
            .iter()
            .position(|c| *c == Choice::Keep)
            .and_then(|i| u32::try_from(i).ok());
    }
    None
}

pub const SYSTEM: &str = "You are playing a game of Magic: The Gathering through a rules engine. \
Each message shows the game from your seat (objects carry #n labels), the events since your last \
decision, and the numbered legal options for the decision in front of you. Think briefly, then end \
your reply with a final line `ANSWER: <number>` naming exactly one of the options. Only the listed \
options are legal; the engine handles everything else (mana payment, untapping, drawing).";

/// A model in a seat, with its running totals.
pub struct ModelSeat {
    pub model: Arc<Model>,
    /// Replies that named no legal option, in all.
    pub invalid: u32,
    pub fallbacks: u32,
    /// Retries allowed after an illegal reply before the fallback agent
    /// chooses.
    pub retries: u32,
    /// Model decisions allowed in one game; after that the fallback agent
    /// plays the seat (a cost cap, recorded).
    pub max_decisions: u32,
    pub decisions: u32,
    log_cursor: usize,
    fallback: GreedyAgent,
    answer: Regex,
}

impl ModelSeat {
    pub fn new(model: Arc<Model>, max_decisions: u32) -> Self {
        ModelSeat {
            model,
            invalid: 0,
            fallbacks: 0,
            retries: 2,
            max_decisions,
            decisions: 0,
            log_cursor: 0,
            fallback: GreedyAgent::default(),
            answer: Regex::new(r"(?i)ANSWER:\s*\**\s*(\d+)").unwrap(),
        }
    }

    fn prompt(&mut self, engine: &Engine, d: &Decision) -> String {
        let state = engine.state();
        let me = d.player;
        let mut s = format!("You are P{}.\n\n{}\n", me.0 + 1, state.describe(me));
        let events: Vec<String> = state.log[self.log_cursor.min(state.log.len())..]
            .iter()
            .filter_map(|e| mtg_core::event_text::describe(state, e, me))
            .collect();
        self.log_cursor = state.log.len();
        if !events.is_empty() {
            s.push_str("\nSince your last decision:\n");
            for e in events.iter().rev().take(40).rev() {
                let _ = writeln!(s, "- {e}");
            }
        }
        let _ = writeln!(
            s,
            "\nDecision: {:?} (turn {}, {:?}). Options:",
            d.kind, state.turn_number, state.step
        );
        for (i, c) in d.options.iter().enumerate() {
            let _ = writeln!(s, "  {}. {}", i + 1, c.describe(state));
        }
        s
    }

    /// Ask the model; return its choice and the note for the record.
    pub fn decide(&mut self, engine: &Engine, d: &Decision) -> Result<(ChoiceId, DecisionNote)> {
        let mut note = DecisionNote {
            player: d.player.0,
            ..DecisionNote::default()
        };
        if self.decisions >= self.max_decisions {
            note.fallback = true;
            self.fallbacks += 1;
            return Ok((self.fallback.choose_in(engine, d), note));
        }
        self.decisions += 1;
        let mut turns = vec![self.prompt(engine, d)];
        for attempt in 0..=self.retries {
            let (reply, u) = self.model.ask(SYSTEM, &turns)?;
            note.input_tokens += u.input_tokens;
            note.output_tokens += u.output_tokens;
            note.cache_read_tokens += u.cache_read_tokens;
            note.latency_ms += u.latency_ms;
            let n = self
                .answer
                .captures_iter(&reply)
                .last()
                .and_then(|c| c[1].parse::<usize>().ok());
            if let Some(n) = n
                && n >= 1
                && n <= d.options.len()
            {
                note.reasoning = reply;
                return Ok((u32::try_from(n - 1).unwrap_or(0), note));
            }
            note.invalid += 1;
            self.invalid += 1;
            if attempt < self.retries {
                turns.push(reply);
                turns.push(format!(
                    "That does not name a legal option. Reply with `ANSWER: <number>` for one of options 1 to {}.",
                    d.options.len()
                ));
            } else {
                note.reasoning = reply;
            }
        }
        note.fallback = true;
        self.fallbacks += 1;
        Ok((self.fallback.choose_in(engine, d), note))
    }
}

/// Who plays a seat.
pub enum Seat {
    Agent(Box<dyn Agent + Send>),
    Model(Box<ModelSeat>),
}

/// A finished benchmark game.
pub struct Played {
    pub result: Option<GameResult>,
    pub record: GameRecord,
    pub decisions: u32,
}

/// Play to the end (or `max_decisions`), with `stop` able to end the game
/// early as a non-result (puzzles stop when the turn ends).
pub fn play(
    pool: &Arc<CardPool>,
    mut engine: Engine,
    config: &GameConfig,
    seats: &mut [Seat; 2],
    max_decisions: u32,
    stop: &dyn Fn(&Engine) -> bool,
) -> Result<Played> {
    let mut notes = Vec::new();
    let mut decisions = 0u32;
    let result = loop {
        if stop(&engine) {
            break None;
        }
        let d = match engine.current() {
            StepResult::Over(r) => break Some(*r),
            StepResult::Decision(d) => d.clone(),
        };
        if decisions >= max_decisions {
            break None;
        }
        decisions += 1;
        let choice = if let Some(c) = automatic(&d) {
            c
        } else {
            match &mut seats[d.player.index()] {
                Seat::Agent(a) => a.choose_in(&engine, &d),
                Seat::Model(m) => {
                    let (c, mut note) = m.decide(&engine, &d)?;
                    note.action = engine.history().len();
                    notes.push(note);
                    c
                }
            }
        };
        engine.step(choice).map_err(|e| anyhow::anyhow!("{e}"))?;
    };
    let mut record = GameRecord::from_engine(&engine, pool, config);
    record.notes = notes;
    Ok(Played {
        result,
        record,
        decisions,
    })
}

/// Score of a result for `p`: 1, 0, or 0.5 (a draw or unfinished game).
pub fn score(r: Option<GameResult>, p: PlayerId) -> f64 {
    match r {
        Some(GameResult::Win { winner }) if winner == p => 1.0,
        Some(GameResult::Win { .. }) => 0.0,
        _ => 0.5,
    }
}
