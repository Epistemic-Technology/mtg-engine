//! Human and debugging CLI. See `design.md` §9.3.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mtg_core::record::GameRecord;
use mtg_dsl::builtin_pool;

mod play;

#[derive(Parser)]
#[command(name = "mtg", about = "MTG engine command line")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the engine version.
    Version,
    /// Play random-agent games between two built-in decks and report.
    Sim {
        #[arg(long, default_value = "mono-red")]
        deck1: String,
        #[arg(long, default_value = "mono-green")]
        deck2: String,
        #[arg(long, default_value_t = 1000)]
        games: usize,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Keep event logs (slower; needed only for debugging).
        #[arg(long)]
        logging: bool,
        /// Agent for player 1: random, greedy, or first.
        #[arg(long, default_value = "random")]
        agent1: String,
        /// Agent for player 2.
        #[arg(long, default_value = "random")]
        agent2: String,
        /// Write each game's record (JSON) into this directory, for
        /// `mtg-review`.
        #[arg(long)]
        record_dir: Option<PathBuf>,
    },
    /// Play a game in the terminal against a baseline agent.
    Play {
        /// Your deck.
        #[arg(long, default_value = "mono-red")]
        deck: String,
        /// The opponent's deck.
        #[arg(long, default_value = "mono-green")]
        opponent: String,
        /// Opponent agent: greedy, random, or first.
        #[arg(long, default_value = "greedy")]
        agent: String,
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },
    /// Re-simulate a game record (JSON) and print its event log.
    Replay { record: PathBuf },
    /// Print a deck list: a built-in deck, or `random` / `random:<seed>`.
    Deck {
        name: String,
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },
    /// List built-in decks.
    Decks,
    /// Run scenario files (or directories of them). Defaults to the
    /// repository corpus.
    Scenario {
        paths: Vec<PathBuf>,
        /// Print the event log of each failing scenario.
        #[arg(long, short)]
        verbose: bool,
    },
    /// Measure state clone cost on a mid-game state.
    Bench {
        #[arg(long, default_value_t = 200)]
        warmup: u32,
        #[arg(long, default_value_t = 100_000)]
        iterations: u32,
    },
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Version => println!("mtg-engine {}", mtg_core::ENGINE_VERSION),
        Command::Decks => {
            for n in mtg_sim::decks::NAMES {
                println!("{n}");
            }
        }
        Command::Play {
            deck,
            opponent,
            agent,
            seed,
        } => play::run(&deck, &opponent, &agent, seed)?,
        Command::Deck { name, seed } => {
            let pool = builtin_pool();
            let Some(deck) = mtg_sim::decks::by_name_seeded(&pool, &name, seed) else {
                bail!("unknown deck {name}");
            };
            let mut counts: std::collections::BTreeMap<&str, usize> =
                std::collections::BTreeMap::new();
            for r in &deck {
                *counts.entry(&pool.get(*r).name).or_insert(0) += 1;
            }
            for (n, c) in &counts {
                println!("{c:>2} {n}");
            }
        }
        Command::Sim {
            deck1,
            deck2,
            games,
            seed,
            logging,
            agent1,
            agent2,
            record_dir,
        } => {
            let pool = Arc::new(builtin_pool());
            let Some(d1) = mtg_sim::decks::by_name_seeded(&pool, &deck1, seed) else {
                bail!("unknown deck {deck1}");
            };
            let Some(d2) = mtg_sim::decks::by_name_seeded(&pool, &deck2, seed + 1) else {
                bail!("unknown deck {deck2}");
            };
            for a in [&agent1, &agent2] {
                if mtg_agents::by_name(a, 0).is_none() {
                    bail!("unknown agent {a} (random, greedy, first)");
                }
            }
            if let Some(dir) = &record_dir {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("creating {}", dir.display()))?;
            }
            let (a1, a2, dk1, dk2) = (agent1.clone(), agent2.clone(), deck1.clone(), deck2.clone());
            let on_game = |s: &mtg_sim::GameSummary| {
                let Some(dir) = &record_dir else {
                    return;
                };
                let mut rec = s.record.clone();
                rec.meta.extend([
                    ("agent_a".to_string(), a1.clone()),
                    ("agent_b".to_string(), a2.clone()),
                    ("deck_a".to_string(), dk1.clone()),
                    ("deck_b".to_string(), dk2.clone()),
                    ("source".to_string(), "mtg sim".to_string()),
                ]);
                if let Ok(text) = serde_json::to_string(&rec) {
                    let _ = std::fs::write(dir.join(format!("sim-{}.json", s.seed)), text);
                }
            };
            let stats = mtg_sim::batch_with(
                &pool,
                &[d1, d2],
                games,
                seed,
                logging,
                [&agent1, &agent2],
                &on_game,
            );
            println!(
                "{deck1} ({agent1}) vs {deck2} ({agent2}): {} games, seeds {seed}..",
                stats.games
            );
            println!(
                "wins: P1 {}  P2 {}  draws {}  unfinished {}  panics {}",
                stats.wins[0],
                stats.wins[1],
                stats.draws,
                stats.unfinished,
                stats.panics.len()
            );
            println!(
                "mean turns {:.1}, mean decisions {:.0}",
                stats.mean_turns(),
                stats.mean_decisions()
            );
            println!(
                "throughput: {:.0} games/s wall ({} threads), {:.0} games/s per core",
                stats.games_per_second_wall(),
                rayon_threads(),
                stats.games_per_second_cpu()
            );
            for (s, m) in stats.panics.iter().take(10) {
                println!("panic (seed {s}): {m}");
            }
            if !stats.panics.is_empty() {
                std::process::exit(1);
            }
        }
        Command::Scenario { paths, verbose } => run_scenarios(paths, verbose),
        Command::Bench { warmup, iterations } => {
            let pool = Arc::new(builtin_pool());
            let decks = [
                mtg_sim::decks::by_name(&pool, "mono-red").unwrap(),
                mtg_sim::decks::by_name(&pool, "mono-green").unwrap(),
            ];
            let per = mtg_sim::clone_cost(&pool, &decks, 1, warmup, iterations);
            println!("GameState::clone after {warmup} decisions: {per:?} per clone");
        }
        Command::Replay { record } => {
            let text = std::fs::read_to_string(&record)
                .with_context(|| format!("reading {}", record.display()))?;
            let rec: GameRecord = serde_json::from_str(&text).context("parsing record")?;
            let pool = Arc::new(builtin_pool());
            let engine = rec.replay(pool, true)?;
            for (i, e) in engine.state().log.iter().enumerate() {
                println!("{i:5} {e:?}");
            }
            println!("result: {:?}", engine.state().result);
        }
    }
    Ok(())
}

fn run_scenarios(paths: Vec<PathBuf>, verbose: bool) {
    let pool = Arc::new(builtin_pool());
    let paths = if paths.is_empty() {
        vec![mtg_scenario::corpus_dir()]
    } else {
        paths
    };
    let mut files = Vec::new();
    for p in paths {
        if p.is_dir() {
            files.extend(mtg_scenario::find_files(&p));
        } else {
            files.push(p);
        }
    }
    let (mut passed, mut failed, mut skipped) = (0, 0, 0);
    for path in &files {
        let reports = match mtg_scenario::run_file(&pool, path) {
            Ok(r) => r,
            Err(e) => {
                println!("ERROR {e}");
                failed += 1;
                continue;
            }
        };
        for r in reports {
            let at = format!("{}:{}", path.display(), r.line);
            if let Some(reason) = &r.skipped {
                skipped += 1;
                println!("skip  {at} \"{}\" ({reason})", r.name);
            } else if let Some(f) = &r.failure {
                failed += 1;
                println!(
                    "FAIL  {at} \"{}\"\n      line {}: {}",
                    r.name, f.line, f.message
                );
                if verbose {
                    for (i, e) in r.log.iter().enumerate() {
                        println!("      {i:4} {e:?}");
                    }
                }
            } else {
                passed += 1;
                println!("ok    {at} \"{}\" ({} decisions)", r.name, r.decisions);
            }
        }
    }
    println!("{passed} passed, {failed} failed, {skipped} skipped");
    if failed > 0 {
        std::process::exit(1);
    }
}

fn rayon_threads() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}
