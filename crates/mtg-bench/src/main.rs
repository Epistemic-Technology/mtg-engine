//! `mtg-bench`: the benchmark harness (design §9.5). Rated games put a
//! language model against the built-in agent ladder in mirrored pairs (the
//! same seed and decks twice, the controllers swapped); the ladder track
//! plays the agents against each other to anchor the ratings; the puzzle
//! track gives a model a scenario position it must win this turn. Every
//! game appends one JSON line to a results file and can write its
//! `GameRecord` (with the model's reasoning and spend per decision) for
//! `mtg-review`. `report` fits Elo ratings, greedy anchored at 1500.

mod elo;
mod llm;
mod play;

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mtg_core::card::CardPool;
use mtg_core::engine::{Engine, GameConfig};
use mtg_core::ids::PlayerId;
use play::{ModelSeat, Seat};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(about = "Benchmark harness: models against the agent ladder and puzzles")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Built-in agents against each other in mirrored pairs (no model;
    /// free), to anchor the ladder.
    Ladder {
        #[arg(long, value_delimiter = ',', default_value = "random,greedy,mcts")]
        agents: Vec<String>,
        #[arg(long, value_delimiter = ',', default_value = DEFAULT_MATCHUPS)]
        matchups: Vec<String>,
        /// Mirrored pairs per agent pair and matchup.
        #[arg(long, default_value_t = 10)]
        pairs: u64,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },
    /// A model against built-in agents in mirrored pairs.
    Rated {
        #[command(flatten)]
        model: ModelArgs,
        #[arg(long, value_delimiter = ',', default_value = "greedy")]
        opponents: Vec<String>,
        #[arg(long, value_delimiter = ',', default_value = DEFAULT_MATCHUPS)]
        matchups: Vec<String>,
        #[arg(long, default_value_t = 1)]
        pairs: u64,
        #[arg(long)]
        out: PathBuf,
        /// Write each game's record (with the model's notes) here.
        #[arg(long)]
        records: Option<PathBuf>,
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },
    /// Win-this-turn puzzles from scenario files: the model (or an agent)
    /// plays seat A from each scenario's starting position.
    Puzzles {
        #[command(flatten)]
        model: ModelArgs,
        /// Play an agent instead of a model (to check the track).
        #[arg(long)]
        agent: Option<String>,
        #[arg(long, default_value = "scenarios/puzzles")]
        dir: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        records: Option<PathBuf>,
    },
    /// Ratings and per-player statistics from results files.
    Report {
        files: Vec<PathBuf>,
        /// USD per million input and output tokens, to show cost.
        #[arg(long)]
        price_in: Option<f64>,
        #[arg(long)]
        price_out: Option<f64>,
    },
}

#[derive(clap::Args)]
struct ModelArgs {
    /// Messages API model id.
    #[arg(long, default_value = "claude-haiku-5-5")]
    model: String,
    /// `output_config.effort` (low, medium, high).
    #[arg(long)]
    effort: Option<String>,
    #[arg(long, default_value_t = 4000)]
    max_tokens: u32,
    /// Model decisions allowed per game; the greedy agent plays the seat
    /// after that (recorded as fallbacks).
    #[arg(long, default_value_t = 300)]
    max_model_decisions: u32,
}

/// Deck pairs played by default: each mono-color deck against another.
const DEFAULT_MATCHUPS: &str =
    "mono-red:mono-green,mono-white:mono-black,mono-blue:mono-red,mono-green:mono-white";

const MAX_DECISIONS: u32 = 5000;

/// One game's line in a results file.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct Line {
    track: String,
    /// Seat 1's and seat 2's player.
    p1: String,
    p2: String,
    deck1: String,
    deck2: String,
    seed: u64,
    /// Seat 1's score: 1, 0.5 (draw or unfinished), or 0.
    score1: f64,
    decisions: u32,
    turns: u32,
    /// The model's name, when a model played a seat, and its spend.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    model: String,
    model_decisions: u32,
    invalid: u32,
    fallbacks: u32,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    latency_ms: u64,
    /// Puzzle track: the puzzle's name and whether it was solved.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    puzzle: String,
    engine_version: String,
    corpus_cards: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let pool = Arc::new(mtg_dsl::builtin_pool());
    match cli.command {
        Command::Ladder {
            agents,
            matchups,
            pairs,
            out,
            seed,
        } => ladder(&pool, &agents, &matchups, pairs, &out, seed),
        Command::Rated {
            model,
            opponents,
            matchups,
            pairs,
            out,
            records,
            seed,
        } => rated(
            &pool,
            &model,
            &opponents,
            &matchups,
            pairs,
            &out,
            records.as_deref(),
            seed,
        ),
        Command::Puzzles {
            model,
            agent,
            dir,
            out,
            records,
        } => puzzles(
            &pool,
            &model,
            agent.as_deref(),
            &dir,
            &out,
            records.as_deref(),
        ),
        Command::Report {
            files,
            price_in,
            price_out,
        } => report(&files, price_in, price_out),
    }
}

fn matchup(pool: &CardPool, m: &str) -> Result<(String, String, GameConfig)> {
    let (a, b) = m.split_once(':').context("a matchup is deck1:deck2")?;
    let deck =
        |n: &str| mtg_sim::decks::by_name(pool, n).with_context(|| format!("unknown deck {n}"));
    Ok((
        a.to_string(),
        b.to_string(),
        GameConfig {
            decks: vec![deck(a)?, deck(b)?],
            starting_life: 20,
            seed: 0,
            logging: true,
        },
    ))
}

fn agent(name: &str, seed: u64) -> Result<Seat> {
    Ok(Seat::Agent(
        mtg_agents::by_name(name, seed).with_context(|| format!("unknown agent {name}"))?,
    ))
}

fn append(out: &Path, line: &Line) -> Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(out)
        .with_context(|| format!("opening {}", out.display()))?;
    writeln!(f, "{}", serde_json::to_string(line)?)?;
    Ok(())
}

fn base_line(track: &str, pool: &CardPool) -> Line {
    Line {
        track: track.to_string(),
        engine_version: mtg_core::ENGINE_VERSION.to_string(),
        corpus_cards: pool.len(),
        ..Line::default()
    }
}

fn ladder(
    pool: &Arc<CardPool>,
    agents: &[String],
    matchups: &[String],
    pairs: u64,
    out: &Path,
    seed: u64,
) -> Result<()> {
    let mut jobs = Vec::new();
    for (i, a) in agents.iter().enumerate() {
        for b in &agents[i + 1..] {
            for m in matchups {
                for k in 0..pairs {
                    let s = seed * 1_000_003 + k;
                    jobs.push((a.clone(), b.clone(), m.clone(), s, false));
                    jobs.push((a.clone(), b.clone(), m.clone(), s, true));
                }
            }
        }
    }
    let lines: Mutex<Vec<Line>> = Mutex::new(Vec::new());
    jobs.par_iter()
        .try_for_each(|(a, b, m, s, swap)| -> Result<()> {
            let (d1, d2, mut config) = matchup(pool, m)?;
            config.seed = *s;
            config.logging = false;
            let (p1, p2) = if *swap { (b, a) } else { (a, b) };
            let mut seats = [agent(p1, *s)?, agent(p2, s + 1)?];
            let engine = Engine::new(pool.clone(), &config);
            let g = play::play(pool, engine, &config, &mut seats, MAX_DECISIONS, &|_| false)?;
            let mut line = base_line("ladder", pool);
            line.p1.clone_from(p1);
            line.p2.clone_from(p2);
            line.deck1 = d1;
            line.deck2 = d2;
            line.seed = *s;
            line.score1 = play::score(g.result, PlayerId(0));
            line.decisions = g.decisions;
            line.turns = g.record.turns;
            lines.lock().expect("not poisoned").push(line);
            Ok(())
        })?;
    for l in lines.into_inner().expect("not poisoned") {
        append(out, &l)?;
    }
    println!("{} ladder games appended to {}", jobs.len(), out.display());
    Ok(())
}

fn model_name(m: &ModelArgs) -> String {
    match &m.effort {
        Some(e) => format!("{}@{e}", m.model),
        None => m.model.clone(),
    }
}

fn save_record(dir: Option<&Path>, name: &str, rec: &mtg_core::record::GameRecord) -> Result<()> {
    if let Some(dir) = dir {
        std::fs::create_dir_all(dir)?;
        std::fs::write(
            dir.join(format!("{name}.json")),
            serde_json::to_string(rec)?,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // the parts of one `rated` command
fn rated(
    pool: &Arc<CardPool>,
    m: &ModelArgs,
    opponents: &[String],
    matchups: &[String],
    pairs: u64,
    out: &Path,
    records: Option<&Path>,
    seed: u64,
) -> Result<()> {
    let model = Arc::new(llm::Model::new(&m.model, m.effort.clone(), m.max_tokens)?);
    let name = model_name(m);
    for opp in opponents {
        for mu in matchups {
            for k in 0..pairs {
                let s = seed * 1_000_003 + k;
                for model_seat in [0usize, 1] {
                    let (d1, d2, mut config) = matchup(pool, mu)?;
                    config.seed = s;
                    let ms = ModelSeat::new(model.clone(), m.max_model_decisions);
                    let mut seats = if model_seat == 0 {
                        [Seat::Model(Box::new(ms)), agent(opp, s)?]
                    } else {
                        [agent(opp, s)?, Seat::Model(Box::new(ms))]
                    };
                    let engine = Engine::new(pool.clone(), &config);
                    let mut g =
                        play::play(pool, engine, &config, &mut seats, MAX_DECISIONS, &|_| false)?;
                    let mut line = base_line("rated", pool);
                    let (p1, p2) = if model_seat == 0 {
                        (name.clone(), opp.clone())
                    } else {
                        (opp.clone(), name.clone())
                    };
                    g.record.meta.extend([
                        ("p1".to_string(), p1.clone()),
                        ("p2".to_string(), p2.clone()),
                        ("source".to_string(), "mtg-bench rated".to_string()),
                    ]);
                    fill_model(&mut line, &seats[model_seat], &g.record.notes);
                    line.model.clone_from(&name);
                    line.p1 = p1;
                    line.p2 = p2;
                    line.deck1 = d1;
                    line.deck2 = d2;
                    line.seed = s;
                    line.score1 = play::score(g.result, PlayerId(0));
                    line.decisions = g.decisions;
                    line.turns = g.record.turns;
                    save_record(
                        records,
                        &format!(
                            "rated-{}-{opp}-{mu}-{s}-{model_seat}",
                            name.replace(['/', ':'], "_")
                        )
                        .replace(':', "_"),
                        &g.record,
                    )?;
                    append(out, &line)?;
                    println!(
                        "{} vs {} ({mu}, seed {s}): P1 scored {}; {} model decisions, {} invalid, {} in / {} out tokens",
                        line.p1,
                        line.p2,
                        line.score1,
                        line.model_decisions,
                        line.invalid,
                        line.input_tokens,
                        line.output_tokens
                    );
                }
            }
        }
    }
    Ok(())
}

fn fill_model(line: &mut Line, seat: &Seat, notes: &[mtg_core::record::DecisionNote]) {
    if let Seat::Model(m) = seat {
        line.model_decisions = m.decisions;
        line.invalid = m.invalid;
        line.fallbacks = m.fallbacks;
    }
    for n in notes {
        line.input_tokens += n.input_tokens;
        line.output_tokens += n.output_tokens;
        line.cache_read_tokens += n.cache_read_tokens;
        line.latency_ms += n.latency_ms;
    }
}

fn puzzles(
    pool: &Arc<CardPool>,
    m: &ModelArgs,
    agent_name: Option<&str>,
    dir: &Path,
    out: &Path,
    records: Option<&Path>,
) -> Result<()> {
    let model = if agent_name.is_none() {
        Some(Arc::new(llm::Model::new(
            &m.model,
            m.effort.clone(),
            m.max_tokens,
        )?))
    } else {
        None
    };
    let name = agent_name.map_or_else(|| model_name(m), str::to_string);
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "scn"))
        .collect();
    files.sort();
    let (mut solved, mut total) = (0, 0);
    for f in files {
        let text = std::fs::read_to_string(&f)?;
        let scenarios = mtg_scenario::parse(&text).map_err(|e| anyhow::anyhow!("{e}"))?;
        for sc in scenarios {
            let engine = mtg_scenario::start_engine(pool, &sc)
                .map_err(|e| anyhow::anyhow!("{}: {e:?}", sc.name))?;
            let turn = engine.state().turn_number;
            let a = match (&model, agent_name) {
                (Some(model), _) => Seat::Model(Box::new(ModelSeat::new(model.clone(), 100))),
                (None, Some(n)) => agent(n, 1)?,
                (None, None) => bail!("a model or an agent"),
            };
            let mut seats = [a, agent("greedy", 2)?];
            let config = GameConfig {
                decks: vec![Vec::new(), Vec::new()],
                starting_life: 20,
                seed: 0,
                logging: true,
            };
            let mut g = play::play(pool, engine, &config, &mut seats, 500, &|e| {
                e.state().turn_number > turn
            })?;
            let won = play::score(g.result, PlayerId(0)) > 0.75;
            total += 1;
            if won {
                solved += 1;
            }
            g.record.meta.push(("puzzle".to_string(), sc.name.clone()));
            let mut line = base_line("puzzle", pool);
            fill_model(&mut line, &seats[0], &g.record.notes);
            if model.is_some() {
                line.model.clone_from(&name);
            }
            line.p1.clone_from(&name);
            line.p2 = "greedy".to_string();
            line.puzzle.clone_from(&sc.name);
            line.score1 = if won { 1.0 } else { 0.0 };
            line.decisions = g.decisions;
            save_record(records, &format!("puzzle-{total}"), &g.record)?;
            append(out, &line)?;
            println!("{}: {}", sc.name, if won { "solved" } else { "not solved" });
        }
    }
    println!("{name}: {solved} of {total} puzzles solved");
    Ok(())
}

#[derive(Default)]
struct Stats {
    games: u32,
    score: f64,
    decisions: u64,
    invalid: u64,
    input: u64,
    output: u64,
    cache: u64,
    latency: u64,
    puzzles: u32,
    solved: u32,
}

#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
fn report(files: &[PathBuf], price_in: Option<f64>, price_out: Option<f64>) -> Result<()> {
    let mut lines = Vec::new();
    for f in files {
        for l in std::fs::read_to_string(f)?.lines() {
            if !l.trim().is_empty() {
                lines.push(serde_json::from_str::<Line>(l)?);
            }
        }
    }
    let games: Vec<elo::Game<'_>> = lines
        .iter()
        .filter(|l| l.track != "puzzle")
        .map(|l| elo::Game {
            a: &l.p1,
            b: &l.p2,
            score: l.score1,
        })
        .collect();
    let ratings = elo::fit(&games, "greedy", 1500.0);
    let mut stats: BTreeMap<&str, Stats> = BTreeMap::new();
    for l in &lines {
        if l.track == "puzzle" {
            let s = stats.entry(&l.p1).or_default();
            s.puzzles += 1;
            if l.score1 > 0.75 {
                s.solved += 1;
            }
            s.decisions += u64::from(l.model_decisions);
            s.invalid += u64::from(l.invalid);
            continue;
        }
        for (name, sc) in [(&l.p1, l.score1), (&l.p2, 1.0 - l.score1)] {
            let s = stats.entry(name).or_default();
            s.games += 1;
            s.score += sc;
        }
        if !l.model.is_empty() {
            let s = stats.entry(&l.model).or_default();
            s.decisions += u64::from(l.model_decisions);
            s.invalid += u64::from(l.invalid);
            s.input += l.input_tokens + l.cache_read_tokens;
            s.output += l.output_tokens;
            s.cache += l.cache_read_tokens;
            s.latency += l.latency_ms;
        }
    }
    println!(
        "{:<28} {:>6} {:>7} {:>14} {:>9} {:>11} {:>10} {:>9} {:>8}",
        "player",
        "games",
        "score",
        "Elo (95%)",
        "invalid",
        "out tok/g",
        "s/decision",
        "cost/g",
        "puzzles"
    );
    for (name, s) in &stats {
        let (r, half) = ratings.get(*name).copied().unwrap_or((f64::NAN, 0.0));
        let per_game = |x: u64| {
            if s.games > 0 {
                x as f64 / f64::from(s.games)
            } else {
                0.0
            }
        };
        let cost = match (price_in, price_out) {
            (Some(pi), Some(po)) if s.games > 0 => format!(
                "${:.3}",
                (per_game(s.input) * pi + per_game(s.output) * po) / 1e6
            ),
            _ => "-".to_string(),
        };
        println!(
            "{:<28} {:>6} {:>6.1}% {:>8.0} ±{:<4.0} {:>8.1}% {:>11.0} {:>10.2} {:>9} {:>8}",
            name,
            s.games,
            if s.games > 0 {
                100.0 * s.score / f64::from(s.games)
            } else {
                0.0
            },
            r,
            half,
            if s.decisions > 0 {
                100.0 * s.invalid as f64 / s.decisions as f64
            } else {
                0.0
            },
            per_game(s.output),
            if s.decisions > 0 {
                s.latency as f64 / s.decisions as f64 / 1000.0
            } else {
                0.0
            },
            cost,
            if s.puzzles > 0 {
                format!("{}/{}", s.solved, s.puzzles)
            } else {
                "-".to_string()
            },
        );
    }
    Ok(())
}
