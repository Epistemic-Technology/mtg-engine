//! `mtg arena`: record MTG Arena matches and turn them into engine traces.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use mtg_arena::archive::{self, ArchiveFile, Store};
use mtg_arena::cards::{CardDb, Names, NoNames};
use mtg_arena::recorder::{self, Event, Recorder, Watcher};
use mtg_arena::trace::MatchTrace;
use mtg_arena::{log, render, service, trace};
use mtg_core::card::m1_pool;

#[derive(Args)]
pub struct ArenaArgs {
    /// Directory of recorded matches (default: the per-user data directory).
    #[arg(long, global = true)]
    store: Option<PathBuf>,
    /// Arena's card database, for card names (default: found in the
    /// installed client).
    #[arg(long, global = true)]
    card_db: Option<PathBuf>,
    #[command(subcommand)]
    command: ArenaCommand,
}

#[derive(Subcommand)]
enum ArenaCommand {
    /// Follow Arena's log and record every match played. Runs until killed.
    Watch {
        /// Directory holding Player.log (default: Arena's log directory).
        #[arg(long)]
        log_dir: Option<PathBuf>,
        /// How often to check the log for new output.
        #[arg(long, default_value_t = 1000)]
        interval_ms: u64,
    },
    /// Record the matches in saved log files.
    Import {
        #[arg(required = true)]
        logs: Vec<PathBuf>,
    },
    /// List recorded matches.
    List,
    /// Print a play-by-play of a recorded match.
    Show {
        /// Which match: a number from `list` (negative counts from the
        /// newest), part of its name, or an archive path.
        #[arg(default_value = "-1", allow_hyphen_values = true)]
        which: String,
        /// Only this game of the match.
        #[arg(long)]
        game: Option<u32>,
    },
    /// Write a recorded match's trace as JSON: states, events and
    /// decisions, for replaying against the engine.
    Trace {
        #[arg(default_value = "-1", allow_hyphen_values = true)]
        which: String,
        /// Output file (default: standard output).
        #[arg(long, short)]
        output: Option<PathBuf>,
        #[arg(long)]
        pretty: bool,
    },
    /// List the cards in a recorded match and whether the engine has them.
    Cards {
        #[arg(default_value = "-1", allow_hyphen_values = true)]
        which: String,
    },
    /// Manage the background recorder (a launchd agent started at login).
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
}

#[derive(Subcommand)]
enum ServiceCommand {
    /// Install and start the recorder, using this executable.
    Install,
    /// Stop and remove the recorder.
    Uninstall,
    /// Report whether the recorder is running.
    Status,
}

pub fn run(args: ArenaArgs) -> Result<()> {
    let explicit_store = args.store.clone();
    let store = Store::new(match args.store {
        Some(p) => p,
        None => Store::default_root().context("no home directory; pass --store")?,
    });
    match args.command {
        ArenaCommand::Watch {
            log_dir,
            interval_ms,
        } => watch(store, log_dir, interval_ms),
        ArenaCommand::Import { logs } => {
            let mut rec = Recorder::new(store);
            for path in logs {
                let mut events = Vec::new();
                recorder::import(&mut rec, &path, &mut events)
                    .with_context(|| format!("reading {}", path.display()))?;
                if !events.iter().any(|e| matches!(e, Event::Finished { .. })) {
                    println!("{}: no new matches", path.display());
                }
                report(&events);
            }
            Ok(())
        }
        ArenaCommand::List => list(&store),
        ArenaCommand::Show { which, game } => {
            let names = names(args.card_db)?;
            let trace = load(&store, &which, names.as_ref())?;
            print!("{}", render::render(&trace, game));
            Ok(())
        }
        ArenaCommand::Trace {
            which,
            output,
            pretty,
        } => {
            let names = names(args.card_db)?;
            let trace = load(&store, &which, names.as_ref())?;
            let json = if pretty {
                serde_json::to_string_pretty(&trace)?
            } else {
                serde_json::to_string(&trace)?
            };
            match output {
                Some(path) => std::fs::write(&path, json + "\n")
                    .with_context(|| format!("writing {}", path.display()))?,
                None => writeln!(std::io::stdout().lock(), "{json}")?,
            }
            Ok(())
        }
        ArenaCommand::Cards { which } => {
            let names = names(args.card_db)?;
            cards(&load(&store, &which, names.as_ref())?);
            Ok(())
        }
        ArenaCommand::Service { command } => service_command(&command, &store, explicit_store),
    }
}

fn names(card_db: Option<PathBuf>) -> Result<Box<dyn Names>> {
    if let Some(path) = card_db.or_else(CardDb::locate) {
        return Ok(Box::new(CardDb::open(&path)?));
    }
    eprintln!("warning: Arena's card database not found; cards will be unnamed");
    Ok(Box::new(NoNames))
}

fn find(store: &Store, which: &str) -> Result<ArchiveFile> {
    match store.find(which)? {
        Some(a) => Ok(a),
        None => bail!(
            "no recorded match matches {which:?} in {} (try `mtg arena list`)",
            store.matches_dir().display()
        ),
    }
}

fn load(store: &Store, which: &str, names: &dyn Names) -> Result<MatchTrace> {
    let file = find(store, which)?;
    let records =
        archive::read(&file.path).with_context(|| format!("reading {}", file.path.display()))?;
    Ok(trace::build(&records, names))
}

fn report(events: &[Event]) {
    for e in events {
        match e {
            Event::Started { match_id } => println!("recording match {match_id}"),
            Event::AlreadyArchived { match_id } => println!("match {match_id} already recorded"),
            Event::Finished {
                match_id,
                path,
                reason,
                messages,
            } => println!(
                "match {match_id} {}: {messages} messages -> {}",
                format!("{reason:?}").to_lowercase(),
                path.display()
            ),
            Event::DetailedLogsDisabled => println!(
                "warning: Arena's detailed logs are off, so there is nothing to record. \
                 Enable Options > Account > Detailed Logs (Plugin Support) and restart Arena."
            ),
            Event::LogRotated => println!("Arena restarted; following the new log"),
        }
    }
    let _ = std::io::stdout().flush();
}

fn watch(store: Store, log_dir: Option<PathBuf>, interval_ms: u64) -> Result<()> {
    let log_dir = match log_dir {
        Some(d) => d,
        None => {
            log::default_log_dir().context("Arena's log directory is unknown; pass --log-dir")?
        }
    };
    println!(
        "watching {} -> {}",
        log_dir.join("Player.log").display(),
        store.matches_dir().display()
    );
    let mut events = Vec::new();
    let mut watcher = Watcher::start(store, &log_dir, &mut events)?;
    loop {
        // A read error (the log vanishing mid-read, a full disk) should not
        // kill a background service: report it and try again.
        if let Err(e) = watcher.tick(&mut events) {
            eprintln!("error: {e}");
        }
        report(&events);
        events.clear();
        std::thread::sleep(Duration::from_millis(interval_ms));
    }
}

fn list(store: &Store) -> Result<()> {
    let files = store.list()?;
    if files.is_empty() {
        println!("no recorded matches in {}", store.matches_dir().display());
        return Ok(());
    }
    for (i, file) in files.iter().enumerate() {
        let records = archive::read(&file.path)
            .with_context(|| format!("reading {}", file.path.display()))?;
        let t = trace::build(&records, &NoNames);
        let players: Vec<&str> = t.players.iter().map(|p| p.name.as_str()).collect();
        let outcome = match (t.end, t.winner) {
            (None, _) => "recording".to_string(),
            (Some(_), Some(w)) => format!("won by {}", t.player_name(w)),
            (Some(end), None) => format!("{end:?}").to_lowercase(),
        };
        println!(
            "{:>3}  {}  {}  {} game{}, {} states  {outcome}",
            i + 1,
            file.stem,
            players.join(" vs "),
            t.games.len(),
            if t.games.len() == 1 { "" } else { "s" },
            t.games.iter().map(|g| g.frames.len()).sum::<usize>(),
        );
    }
    Ok(())
}

fn cards(trace: &MatchTrace) {
    let pool = m1_pool();
    let all = trace.card_names();
    let (mut have, mut total) = (0, 0);
    for (name, token) in &all {
        if *token {
            continue;
        }
        total += 1;
        let known = pool.lookup(name).is_some();
        have += usize::from(known);
        println!("{} {name}", if known { "ok     " } else { "missing" });
    }
    let tokens: Vec<&str> = all
        .iter()
        .filter(|(_, t)| *t)
        .map(|(n, _)| n.as_str())
        .collect();
    if !tokens.is_empty() {
        println!("tokens: {}", tokens.join(", "));
    }
    println!("{have} of {total} cards are in the engine's pool");
}

fn service_command(
    command: &ServiceCommand,
    store: &Store,
    explicit_store: Option<PathBuf>,
) -> Result<()> {
    match command {
        ServiceCommand::Install => {
            let exe = std::env::current_exe().context("locating this executable")?;
            let mut args = vec!["arena".to_string(), "watch".to_string()];
            if let Some(dir) = explicit_store {
                let dir = std::path::absolute(&dir)?;
                args.extend(["--store".to_string(), dir.to_string_lossy().into_owned()]);
            }
            let log = store.root().join("recorder.log");
            let plist = service::install(&exe, &args, &log)?;
            println!("installed {}", plist.display());
            println!("running   {} {}", exe.display(), args.join(" "));
            println!("log       {}", log.display());
            if exe.components().any(|c| c.as_os_str() == "target") {
                println!(
                    "note: the service runs this build-directory executable; it breaks if \
                     that is cleaned or moved. `cargo install --path crates/mtg-cli`, then \
                     run `mtg arena service install` again to use the installed copy."
                );
            }
        }
        ServiceCommand::Uninstall => {
            if service::uninstall()? {
                println!("recorder service removed");
            } else {
                println!("recorder service was not installed");
            }
        }
        ServiceCommand::Status => match service::status()? {
            Some(s) => println!("{s}"),
            None => println!("recorder service is not loaded"),
        },
    }
    Ok(())
}
