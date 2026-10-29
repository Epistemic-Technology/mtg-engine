use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use mtg_cards::{DataDir, ingest, stats};

#[derive(Parser)]
#[command(name = "mtg-cards", about = "Card data ingest and statistics")]
struct Cli {
    /// Data directory holding sources.lock and fetched files.
    #[arg(long, default_value = "data", global = true)]
    data_dir: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fetch Scryfall bulk files and MTGJSON enumerations.
    Ingest {
        /// Fetch exactly the snapshot named in sources.lock and verify hashes,
        /// instead of fetching the latest and rewriting the lockfile.
        #[arg(long)]
        locked: bool,
    },
    /// Verify that the files named in sources.lock are present with the right
    /// hashes. No network.
    Verify,
    /// Report pool sizes from the local snapshot.
    Stats {
        /// Also query live Scryfall totals and fail if any format differs by
        /// more than 1%.
        #[arg(long)]
        check: bool,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let data = DataDir::new(cli.data_dir);
    match cli.command {
        Command::Ingest { locked } => {
            let lock = if locked {
                ingest::ingest_locked(&data)?
            } else {
                ingest::ingest_latest(&data)?
            };
            for (t, e) in &lock.scryfall {
                println!(
                    "scryfall {t}: updated_at {} ({} bytes)",
                    e.updated_at, e.size
                );
            }
            println!("mtgjson {}", lock.mtgjson.version);
            Ok(ExitCode::SUCCESS)
        }
        Command::Verify => {
            let lock = ingest::verify(&data)?;
            println!(
                "ok: {} scryfall files, {} mtgjson files (fetched {})",
                lock.scryfall.len(),
                lock.mtgjson.files.len(),
                lock.fetched_at
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::Stats { check } => {
            let lock = ingest::verify(&data)?;
            let s = stats::compute(&data)?;
            println!(
                "snapshot: oracle_cards updated_at {}",
                lock.scryfall
                    .get("oracle_cards")
                    .map_or("?", |e| e.updated_at.as_str())
            );
            println!("objects: {}", s.objects);
            println!("cards:   {}", s.cards);
            println!();
            println!("{:<12} {:>8}", "format", "cards");
            for (f, n) in &s.by_format {
                println!("{f:<12} {n:>8}");
            }
            println!();
            println!("{:<20} {:>8}", "layout", "cards");
            for (l, n) in &s.by_layout {
                println!("{l:<20} {n:>8}");
            }
            if !check {
                return Ok(ExitCode::SUCCESS);
            }
            println!();
            println!(
                "checking against live Scryfall search totals (tolerance {:.0}%)",
                stats::TOLERANCE * 100.0
            );
            let live = stats::live_format_totals()?;
            let rows = stats::compare(&s, &live);
            println!(
                "{:<12} {:>8} {:>8} {:>7}  ok",
                "format", "local", "live", "diff"
            );
            let mut all_ok = true;
            for r in &rows {
                all_ok &= r.ok;
                println!(
                    "{:<12} {:>8} {:>8} {:>6.2}%  {}",
                    r.format,
                    r.local,
                    r.live,
                    r.rel_diff * 100.0,
                    if r.ok { "yes" } else { "NO" }
                );
            }
            Ok(if all_ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
    }
}
