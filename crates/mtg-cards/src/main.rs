use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};
use mtg_cards::{DataDir, author, gate, ingest, llm, stats, xmage};

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
    /// Run the round-trip gate: the DSL corpus's rendered text against the
    /// Oracle text in the local snapshot.
    Gate {
        /// Show the rendered and Oracle text of every card that differs.
        #[arg(long)]
        diff: bool,
        /// Only these cards (by name).
        names: Vec<String>,
    },
    /// Author card files from the snapshot for every card in a format whose
    /// Oracle text the templates cover; each is verified against its Oracle
    /// text before it is written.
    Author {
        /// Format whose legal cards to author (Scryfall name: pauper, ...).
        #[arg(long, default_value = "pauper")]
        format: String,
        /// Write the files into cards/ (otherwise report only).
        #[arg(long)]
        write: bool,
        /// Show every skipped card with its reason.
        #[arg(long)]
        verbose: bool,
        /// Only these cards (by name), from any format.
        names: Vec<String>,
    },
    /// Author cards with a model (design.md §6.4): prompt, gate, feed back,
    /// retry; write the card and its scenarios on acceptance.
    ModelAuthor {
        /// Model id for the Messages API.
        #[arg(long, default_value = llm::DEFAULT_MODEL)]
        model: String,
        /// Attempts per card before giving up.
        #[arg(long, default_value_t = 3)]
        attempts: usize,
        /// Write accepted cards into cards/ and their scenarios into
        /// scenarios/llm/.
        #[arg(long)]
        write: bool,
        /// Print the prompt pack for each card and make no API call.
        #[arg(long)]
        prompt_only: bool,
        /// A file of card names, one per line (`#` comments), authored
        /// after `names`. Cards already in the corpus are skipped.
        #[arg(long)]
        list: Option<PathBuf>,
        /// Cards authored concurrently.
        #[arg(long, default_value_t = 4)]
        jobs: usize,
        /// Author at most this many cards.
        #[arg(long)]
        limit: Option<usize>,
        /// Append one tab-separated line per card (name, verdict, detail,
        /// tokens in, tokens out) to this file.
        #[arg(long)]
        report: Option<PathBuf>,
        /// Card names.
        names: Vec<String>,
    },
    /// Convert `XMage`'s card tests (data/xmage, see README) into scenarios.
    /// Each converted test is run; those that fail are written with a
    /// `skip` line naming the failure, for adjudication.
    ConvertXmage {
        /// Write the scenario files under this directory.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Show every skipped test with its reason.
        #[arg(long)]
        verbose: bool,
    },
    /// How much of a format the corpus covers, and (with --scenarios) which
    /// corpus cards no scenario exercises (gate 2 of design.md §6.3).
    Coverage {
        #[arg(long, default_value = "pauper")]
        format: String,
        /// Count cards named in scenarios/ and list the hand-written ones
        /// that are not.
        #[arg(long)]
        scenarios: bool,
    },
    /// Report pool sizes from the local snapshot.
    Stats {
        /// Also query live Scryfall totals and fail if any format differs by
        /// more than 1%.
        #[arg(long)]
        check: bool,
    },
}

/// A scenario text with a `skip` line carrying the failure, for
/// adjudication.
fn skipped(text: &str, message: &str) -> String {
    let reason: String = message
        .replace(['"', '\n'], "'")
        .chars()
        .take(160)
        .collect();
    text.replacen('\n', &format!("\n  skip \"unadjudicated: {reason}\"\n"), 1)
}

struct ModelAuthorArgs {
    model: String,
    attempts: usize,
    write: bool,
    prompt_only: bool,
    list: Option<PathBuf>,
    jobs: usize,
    limit: Option<usize>,
    report: Option<PathBuf>,
    names: Vec<String>,
}

/// What one worker reports back for a card.
struct Outcome {
    name: String,
    verdict: Result<llm::Verdict>,
    log: Vec<String>,
    /// Input, output, and cache-read tokens.
    tokens: (u64, u64, u64),
}

/// `model-author`: author cards with the model, `jobs` at a time, writing
/// each accepted card as it arrives.
#[allow(clippy::too_many_lines)]
fn model_author(data: &DataDir, args: &ModelAuthorArgs) -> Result<ExitCode> {
    use std::collections::{HashSet, VecDeque};
    use std::sync::{Arc, Mutex, mpsc};

    let corpus = mtg_dsl::builtin_cards()?;
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut names = args.names.clone();
    if let Some(path) = &args.list {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        names.extend(
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_string),
        );
    }
    let existing: HashSet<&str> = corpus.iter().map(|c| c.name.as_str()).collect();
    let mut seen: HashSet<String> = HashSet::new();
    names.retain(|n| seen.insert(n.clone()) && !existing.contains(n.as_str()));
    if let Some(n) = args.limit {
        names.truncate(n);
    }
    let records = llm::records(data, &names)?;
    for n in &names {
        if !records.contains_key(n) {
            eprintln!("{n}: not in the Scryfall snapshot");
        }
    }
    let ids: HashSet<String> = records
        .values()
        .filter_map(|r| r.oracle_id.clone())
        .collect();
    let rulings = llm::rulings_for(data, &ids)?;
    let rulings_of = |scry: &mtg_cards::scryfall::Card| -> Vec<String> {
        rulings
            .get(scry.oracle_id.as_deref().unwrap_or(""))
            .cloned()
            .unwrap_or_default()
    };
    if args.prompt_only {
        for name in &names {
            let Some(scry) = records.get(name) else {
                continue;
            };
            let examples = llm::similar(&corpus, scry.oracle_text.as_deref().unwrap_or(""), 6);
            println!("{}", llm::user_prompt(scry, &rulings_of(scry), &examples));
        }
        return Ok(ExitCode::SUCCESS);
    }
    let queue: Arc<Mutex<VecDeque<mtg_cards::scryfall::Card>>> = Arc::new(Mutex::new(
        names
            .iter()
            .filter_map(|n| records.get(n).cloned())
            .collect(),
    ));
    let total = queue.lock().expect("queue").len();
    let (tx, rx) = mpsc::channel::<Outcome>();
    let (mut accepted, mut unsupported, mut rejected, mut errors) =
        (0usize, 0usize, 0usize, 0usize);
    let (mut input, mut output, mut cached) = (0u64, 0u64, 0u64);
    let mut report_file = match &args.report {
        Some(path) => Some(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("opening {}", path.display()))?,
        ),
        None => None,
    };
    let (model, attempts, write) = (&args.model, args.attempts, args.write);
    let result: Result<()> = std::thread::scope(|s| {
        for _ in 0..args.jobs.max(1) {
            let queue = Arc::clone(&queue);
            let tx = tx.clone();
            let corpus = &corpus;
            let rulings_of = &rulings_of;
            s.spawn(move || {
                loop {
                    let next = queue.lock().expect("queue").pop_front();
                    let Some(scry) = next else {
                        break;
                    };
                    let rulings = rulings_of(&scry);
                    let mut log = Vec::new();
                    let result =
                        llm::author_record(&scry, &rulings, corpus, model, attempts, &mut |m| {
                            log.push(m.to_string());
                        });
                    let (verdict, tokens) = match result {
                        Ok((v, c)) => (
                            Ok(v),
                            (c.input_tokens, c.output_tokens, c.cache_read_tokens),
                        ),
                        Err(e) => (Err(e), (0, 0, 0)),
                    };
                    if tx
                        .send(Outcome {
                            name: scry.name.clone(),
                            verdict,
                            log,
                            tokens,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let mut done = 0usize;
        for o in rx {
            done += 1;
            println!("== {} ({done}/{total})", o.name);
            for m in &o.log {
                println!("   {m}");
            }
            input += o.tokens.0;
            output += o.tokens.1;
            cached += o.tokens.2;
            let (kind, detail) = match &o.verdict {
                Ok(llm::Verdict::Accepted { card, scenarios }) => {
                    accepted += 1;
                    if write {
                        let slug = mtg_dsl::slug(&o.name);
                        let dir = repo.join("cards").join(mtg_dsl::shard(&slug));
                        std::fs::create_dir_all(&dir)?;
                        std::fs::write(dir.join(format!("{slug}.card")), card)?;
                        let sp = llm::scenario_path(&repo, &slug);
                        std::fs::create_dir_all(sp.parent().expect("scenario dir"))?;
                        let header = format!(
                            "# {}: card and scenarios authored by {model} through `mtg-cards model-author`,\n# accepted by the round-trip gate and these scenarios.\n\n",
                            o.name
                        );
                        std::fs::write(&sp, format!("{header}{scenarios}"))?;
                        println!(
                            "   wrote cards/{}/{slug}.card and scenarios/llm/{slug}.scn",
                            mtg_dsl::shard(&slug)
                        );
                    } else {
                        println!("{card}\n{scenarios}");
                    }
                    ("accepted", String::new())
                }
                Ok(llm::Verdict::Unsupported(why)) => {
                    unsupported += 1;
                    ("unsupported", why.clone())
                }
                Ok(llm::Verdict::Rejected(problems)) => {
                    rejected += 1;
                    (
                        "rejected",
                        problems.lines().next().unwrap_or("").to_string(),
                    )
                }
                Err(e) => {
                    errors += 1;
                    println!("   error: {e:#}");
                    ("error", format!("{e:#}"))
                }
            };
            if let Some(f) = report_file.as_mut() {
                use std::io::Write as _;
                let detail: String = detail
                    .replace(['\t', '\n'], " ")
                    .chars()
                    .take(300)
                    .collect();
                writeln!(
                    f,
                    "{}\t{kind}\t{detail}\t{}\t{}",
                    o.name, o.tokens.0, o.tokens.1
                )?;
            }
        }
        Ok(())
    });
    result?;
    println!();
    println!(
        "{accepted} accepted, {unsupported} unsupported, {rejected} rejected, {errors} errors; tokens: {input} in ({cached} from cache), {output} out"
    );
    Ok(ExitCode::SUCCESS)
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

#[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
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
        Command::Author {
            format,
            write,
            verbose,
            names,
        } => {
            let corpus = mtg_dsl::builtin_cards()?;
            let report = author::author_format(&data, &format, &corpus, &names)?;
            if verbose {
                for (name, skip) in &report.skipped {
                    println!("skip     {name}: {skip}");
                }
            }
            for a in &report.authored {
                println!("authored {}", a.name);
            }
            println!();
            println!(
                "{}: {} cards; {} already in the corpus; {} authored; {} skipped",
                format,
                report.total,
                report.existing,
                report.authored.len(),
                report.skipped.len()
            );
            println!();
            println!("top skip reasons:");
            for (reason, n) in report.reasons().iter().take(40) {
                println!("{n:>6}  {reason}");
            }
            if write {
                let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cards");
                let mut written = 0;
                for a in &report.authored {
                    let shard = dir.join(mtg_dsl::shard(&a.slug));
                    std::fs::create_dir_all(&shard)?;
                    let path = shard.join(format!("{}.card", a.slug));
                    if path.exists() {
                        eprintln!("not overwriting {}", path.display());
                        continue;
                    }
                    std::fs::write(&path, &a.text)?;
                    written += 1;
                }
                println!("wrote {written} files to {}", dir.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::ModelAuthor {
            model,
            attempts,
            write,
            prompt_only,
            list,
            jobs,
            limit,
            report,
            names,
        } => model_author(
            &data,
            &ModelAuthorArgs {
                model,
                attempts,
                write,
                prompt_only,
                list,
                jobs,
                limit,
                report,
                names,
            },
        ),
        Command::ConvertXmage { out, verbose } => {
            let root = data
                .root()
                .join("xmage/Mage.Tests/src/test/java/org/mage/test");
            anyhow::ensure!(
                root.is_dir(),
                "no XMage checkout at {} (see README: data/sources.lock names the commit)",
                root.display()
            );
            let cards = mtg_dsl::builtin_cards()?;
            let corpus = xmage::Corpus::new(&cards);
            let report = xmage::convert_dir(&root, &corpus)?;
            if verbose {
                for (class, method, reason) in &report.skipped {
                    println!("skip     {class}.{method}: {reason}");
                }
            }
            // Run each converted scenario; failures become `skip` lines.
            let pool = std::sync::Arc::new(mtg_dsl::builtin_pool());
            let mut passing = 0usize;
            let mut failing: Vec<(String, String)> = Vec::new();
            let mut by_file: std::collections::BTreeMap<String, Vec<String>> =
                std::collections::BTreeMap::new();
            for c in &report.converted {
                let parsed = match mtg_scenario::parse(&c.text) {
                    Ok(p) => p,
                    Err(e) => {
                        failing.push((format!("{}.{}", c.class, c.method), format!("parse: {e}")));
                        by_file
                            .entry(c.source.clone())
                            .or_default()
                            .push(skipped(&c.text, &format!("parse: {e}")));
                        continue;
                    }
                };
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    parsed
                        .iter()
                        .map(|sc| mtg_scenario::run(&pool, sc))
                        .collect::<Vec<_>>()
                }));
                let text = if let Ok(reports) = outcome {
                    match reports.iter().find_map(|r| r.failure.as_ref()) {
                        None => {
                            passing += 1;
                            c.text.clone()
                        }
                        Some(f) => {
                            failing.push((format!("{}.{}", c.class, c.method), f.message.clone()));
                            skipped(&c.text, &f.message)
                        }
                    }
                } else {
                    failing.push((
                        format!("{}.{}", c.class, c.method),
                        "engine panic".to_string(),
                    ));
                    skipped(&c.text, "engine panic")
                };
                by_file.entry(c.source.clone()).or_default().push(text);
            }
            if verbose {
                for (name, msg) in &failing {
                    println!("FAIL     {name}: {msg}");
                }
            }
            println!();
            println!(
                "{} files, {} tests: {} converted ({passing} pass, {} fail), {} skipped",
                report.files,
                report.tests,
                report.converted.len(),
                failing.len(),
                report.skipped.len()
            );
            println!();
            println!("top skip reasons:");
            for (reason, n) in report.reasons().iter().take(25) {
                println!("{n:>6}  {reason}");
            }
            println!();
            println!("cards most often missing from the corpus:");
            let mut missing: Vec<(&String, &usize)> = report.missing_cards.iter().collect();
            missing.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            for (name, n) in missing.iter().take(40) {
                println!("{n:>6}  {name}");
            }
            let mut fail_reasons: std::collections::BTreeMap<String, usize> =
                std::collections::BTreeMap::new();
            for (_, m) in &failing {
                let key: String = m.chars().take(60).collect();
                *fail_reasons.entry(key).or_insert(0) += 1;
            }
            let mut fr: Vec<(String, usize)> = fail_reasons.into_iter().collect();
            fr.sort_by_key(|x| std::cmp::Reverse(x.1));
            println!();
            println!("most common failures of converted tests:");
            for (m, n) in fr.iter().take(25) {
                println!("{n:>6}  {m}");
            }
            if let Some(out) = out {
                let lock = ingest::verify(&data).ok();
                let commit = lock
                    .and_then(|l| l.xmage.map(|x| x.commit))
                    .unwrap_or_else(|| "unknown".to_string());
                let mut written = 0;
                for (source, scenarios) in &by_file {
                    let path = xmage::output_path(&out, source);
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let mut text = format!(
                        "# Converted from XMage's {source} (commit {commit}) by `mtg-cards convert-xmage`.
# XMage is MIT-licensed, copyright its contributors; see README.
# Do not edit: rerun the converter instead.

"
                    );
                    text.push_str(&scenarios.join(
                        "
",
                    ));
                    std::fs::write(&path, text)?;
                    written += 1;
                }
                println!();
                println!("wrote {written} files under {}", out.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Coverage { format, scenarios } => {
            let corpus = mtg_dsl::builtin_cards()?;
            let (have, total) = author::coverage(&data, &format, &corpus)?;
            println!(
                "{format}: {have} of {total} cards in the corpus ({:.1}%)",
                100.0 * have as f64 / total as f64
            );
            if scenarios {
                let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
                let mut named = std::collections::HashSet::new();
                let mut stack = vec![dir];
                while let Some(d) = stack.pop() {
                    for e in std::fs::read_dir(&d)?.flatten() {
                        let p = e.path();
                        if p.is_dir() {
                            stack.push(p);
                        } else if p.extension().is_some_and(|x| x == "scn") {
                            let text = std::fs::read_to_string(&p)?;
                            for piece in text.split('"').skip(1).step_by(2) {
                                named.insert(piece.to_string());
                            }
                        }
                    }
                }
                let cards: Vec<&mtg_dsl::ast::Card> = corpus
                    .iter()
                    .filter(|c| c.kind == mtg_dsl::ast::Kind::Card)
                    .collect();
                let covered = cards.iter().filter(|c| named.contains(&c.name)).count();
                println!(
                    "scenarios name {covered} of {} corpus cards; {} have abilities but no scenario",
                    cards.len(),
                    cards
                        .iter()
                        .filter(|c| !named.contains(&c.name) && !c.abilities.is_empty())
                        .count()
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Gate { diff, names } => {
            let cards = mtg_dsl::builtin_cards()?;
            let cards: Vec<_> = if names.is_empty() {
                cards
            } else {
                cards
                    .into_iter()
                    .filter(|c| names.iter().any(|n| n == &c.name))
                    .collect()
            };
            let rows = gate::run(&data, &cards)?;
            let mut counts = std::collections::BTreeMap::new();
            for r in &rows {
                let key = match &r.status {
                    gate::Status::Match => "match",
                    gate::Status::Allowed(_) => "allowed",
                    gate::Status::Mismatch => "MISMATCH",
                    gate::Status::Skipped => "skipped",
                    gate::Status::NotInSnapshot => "NOT IN SNAPSHOT",
                    gate::Status::HeaderMismatch(_) => "HEADER",
                };
                *counts.entry(key).or_insert(0usize) += 1;
                match &r.status {
                    gate::Status::Match | gate::Status::Skipped => {}
                    gate::Status::Allowed(reason) => println!("allowed  {}: {reason}", r.name),
                    gate::Status::NotInSnapshot => println!("missing  {}", r.name),
                    gate::Status::HeaderMismatch(d) => println!("header   {}: {d}", r.name),
                    gate::Status::Mismatch => {
                        println!("MISMATCH {}", r.name);
                        if diff {
                            println!("  rendered:\n    {}", r.rendered.replace('\n', "\n    "));
                            println!(
                                "  oracle:\n    {}",
                                r.oracle.as_deref().unwrap_or("").replace('\n', "\n    ")
                            );
                        }
                    }
                }
            }
            println!();
            for (k, n) in &counts {
                println!("{k:<16}{n:>6}");
            }
            let bad = counts.get("MISMATCH").copied().unwrap_or(0)
                + counts.get("HEADER").copied().unwrap_or(0)
                + counts.get("NOT IN SNAPSHOT").copied().unwrap_or(0);
            Ok(if bad == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
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
