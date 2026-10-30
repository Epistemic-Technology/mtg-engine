//! Human and debugging CLI. See `design.md` §9.3.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mtg_core::card::m1_pool;
use mtg_core::record::GameRecord;

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
    },
    /// Re-simulate a game record (JSON) and print its event log.
    Replay { record: PathBuf },
    /// List built-in decks.
    Decks,
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
        Command::Decks => {
            for n in mtg_sim::decks::NAMES {
                println!("{n}");
            }
        }
        Command::Sim {
            deck1,
            deck2,
            games,
            seed,
            logging,
        } => {
            let pool = Arc::new(m1_pool());
            let Some(d1) = mtg_sim::decks::by_name(&pool, &deck1) else {
                bail!("unknown deck {deck1}");
            };
            let Some(d2) = mtg_sim::decks::by_name(&pool, &deck2) else {
                bail!("unknown deck {deck2}");
            };
            let stats = mtg_sim::random_batch(&pool, &[d1, d2], games, seed, logging);
            println!("{deck1} vs {deck2}: {} games, seeds {seed}..", stats.games);
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

fn rayon_threads() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}
