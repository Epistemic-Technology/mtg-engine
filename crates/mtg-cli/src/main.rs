//! Human and debugging CLI. See `design.md` §9.3.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mtg_core::card::m1_pool;
use mtg_core::record::GameRecord;

mod arena;
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
    /// Record MTG Arena matches and translate them for engine testing.
    Arena(arena::ArenaArgs),
    /// Measure state clone cost on a mid-game state.
    Bench {
        #[arg(long, default_value_t = 200)]
        warmup: u32,
        #[arg(long, default_value_t = 100_000)]
        iterations: u32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Version => println!("mtg-engine {}", mtg_core::ENGINE_VERSION),
        Command::Arena(args) => arena::run(args)?,
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
        Command::Sim {
            deck1,
            deck2,
            games,
            seed,
            logging,
            agent1,
            agent2,
        } => {
            let pool = Arc::new(m1_pool());
            let Some(d1) = mtg_sim::decks::by_name(&pool, &deck1) else {
                bail!("unknown deck {deck1}");
            };
            let Some(d2) = mtg_sim::decks::by_name(&pool, &deck2) else {
                bail!("unknown deck {deck2}");
            };
            for a in [&agent1, &agent2] {
                if mtg_agents::by_name(a, 0).is_none() {
                    bail!("unknown agent {a} (random, greedy, first)");
                }
            }
            let stats = mtg_sim::batch(&pool, &[d1, d2], games, seed, logging, [&agent1, &agent2]);
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
            let pool = Arc::new(m1_pool());
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
            let pool = Arc::new(m1_pool());
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
    let pool = Arc::new(m1_pool());
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
