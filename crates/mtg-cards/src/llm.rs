//! Model-driven authoring (design.md §6.4): for a card the templates cannot
//! write, ask Claude for the card file and a few scenarios, then run the
//! same gate the templates face (parse, compile, render back to Oracle
//! text, run the scenarios) and feed any failure back for another attempt.
//! Nothing a model writes is accepted on trust: a card is written only when
//! it renders to its Oracle text exactly and its scenarios pass.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::io::BufRead;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use mtg_dsl::ast::{Card, Kind};
use mtg_dsl::render;
use serde::Deserialize;

use crate::DataDir;
use crate::scryfall::{self, Card as ScryCard};

/// The DSL reference the model is given.
const SYNTAX: &str = include_str!("../../mtg-dsl/SYNTAX.md");
const SCENARIO_SYNTAX: &str = include_str!("../../mtg-scenario/SYNTAX.md");

pub const DEFAULT_MODEL: &str = "claude-opus-5-5";

/// Instructions that do not change between cards (cached as the system
/// prompt).
fn system_prompt() -> String {
    format!(
        "You author Magic: The Gathering cards for a rules engine, in a small DSL, and rules tests for them, in a scenario DSL. \
Both languages are closed: a card file carries no English, and the engine renders it back to Oracle text, which must match the card's \
Oracle text exactly (reminder text in parentheses is ignored; keyword lines may be split). Your card file is accepted only if that \
round trip matches and your scenarios pass, so prefer the constructs the reference shows, write nothing the reference does not define, \
and never use `oracle_mismatch`.\n\n\
Reply format, nothing else:\n\
```card\n<the complete card block, header lines included exactly as given>\n```\n\
```scenario\n<two to four scenarios, each exercising one ability or interaction of the card, using only the card itself, basic lands, and the common cards listed>\n```\n\
If the card needs a rule or effect the DSL cannot express, reply with a single line `unsupported: <what is missing>` instead.\n\n\
# Card file syntax\n\n{SYNTAX}\n\n# Scenario file syntax\n\n{SCENARIO_SYNTAX}\n"
    )
}

/// Common cards the model may use in scenarios (all in the corpus).
const COMMON_CARDS: [&str; 14] = [
    "Plains",
    "Island",
    "Swamp",
    "Mountain",
    "Forest",
    "Grizzly Bears",
    "Hill Giant",
    "Serra Angel",
    "Wind Drake",
    "Lightning Bolt",
    "Shock",
    "Giant Growth",
    "Murder",
    "Divination",
];

/// Rulings for an oracle id, from the snapshot.
pub fn rulings(data: &DataDir, oracle_id: &str) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Ruling {
        oracle_id: String,
        comment: String,
    }
    let path = data.scryfall_bulk("rulings");
    let file = std::fs::File::open(&path).with_context(|| format!("opening {}", path.display()))?;
    let reader = std::io::BufReader::new(flate2::read::GzDecoder::new(file));
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if !line.contains(oracle_id) {
            continue;
        }
        if let Ok(r) = serde_json::from_str::<Ruling>(&line)
            && r.oracle_id == oracle_id
        {
            out.push(r.comment);
        }
    }
    Ok(out)
}

fn words(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| w.len() > 2)
        .map(str::to_ascii_lowercase)
        .collect()
}

/// The `k` corpus cards whose Oracle text shares the most words with the
/// target's, as DSL examples.
#[allow(clippy::cast_precision_loss)] // word counts
pub fn similar<'a>(cards: &'a [Card], oracle_text: &str, k: usize) -> Vec<&'a Card> {
    let target = words(oracle_text);
    let by_name: HashMap<&str, &Card> = cards.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut scored: Vec<(f64, &Card)> = cards
        .iter()
        .filter(|c| c.kind == Kind::Card && !c.abilities.is_empty())
        .map(|c| {
            let text = render::oracle_text(c, &by_name);
            let w = words(&text);
            let inter = w.intersection(&target).count() as f64;
            let union = w.union(&target).count().max(1) as f64;
            (inter / union, c)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(k).map(|(_, c)| c).collect()
}

/// The header lines of a card file from the Scryfall record, as the
/// template author writes them.
pub fn header(s: &ScryCard) -> String {
    let mut text = format!("card \"{}\" {{\n", s.name);
    if let Some(id) = &s.oracle_id {
        let _ = writeln!(text, "  oracle_id {id}");
    }
    if let Some(cost) = &s.mana_cost
        && !cost.is_empty()
    {
        let _ = writeln!(text, "  cost {cost}");
    }
    let _ = writeln!(text, "  type {}", s.type_line.as_deref().unwrap_or(""));
    if let (Some(p), Some(t)) = (&s.power, &s.toughness)
        && !p.contains('*')
        && !t.contains('*')
    {
        let _ = writeln!(text, "  pt {p}/{t}");
    }
    if let Some(l) = &s.loyalty {
        let _ = writeln!(text, "  loyalty {l}");
    }
    if let Some(ci) = &s.color_indicator {
        let names: Vec<&str> = ci
            .iter()
            .map(|c| match c.as_str() {
                "W" => "white",
                "U" => "blue",
                "B" => "black",
                "R" => "red",
                _ => "green",
            })
            .collect();
        let _ = writeln!(text, "  color {}", names.join(" "));
    }
    text
}

/// The per-card prompt.
pub fn user_prompt(s: &ScryCard, rulings: &[String], examples: &[&Card]) -> String {
    let mut p = String::new();
    let _ = writeln!(p, "# Card to author\n");
    let _ = writeln!(p, "Name: {}", s.name);
    let _ = writeln!(
        p,
        "Mana cost: {}",
        s.mana_cost.as_deref().unwrap_or("(none)")
    );
    let _ = writeln!(p, "Type line: {}", s.type_line.as_deref().unwrap_or(""));
    if let (Some(pw), Some(t)) = (&s.power, &s.toughness) {
        let _ = writeln!(p, "Power/toughness: {pw}/{t}");
    }
    let _ = writeln!(
        p,
        "\nOracle text:\n{}\n",
        s.oracle_text.as_deref().unwrap_or("")
    );
    if !rulings.is_empty() {
        let _ = writeln!(p, "Rulings:");
        for r in rulings {
            let _ = writeln!(p, "- {r}");
        }
        p.push('\n');
    }
    let _ = writeln!(
        p,
        "Header lines to keep exactly (add the ability lines after them and close the block):\n\n```card\n{}}}\n```\n",
        header(s)
    );
    let _ = writeln!(
        p,
        "# Example card files from the corpus (similar Oracle text)\n"
    );
    for e in examples {
        let _ = writeln!(p, "```card\n{}```\n", mtg_dsl::format::format(e));
    }
    let _ = writeln!(
        p,
        "# Cards available for scenarios\n\n{}, and the card itself.\n",
        COMMON_CARDS.join(", ")
    );
    p
}

/// One Messages API exchange, continued across attempts.
pub struct Conversation {
    pub model: String,
    pub system: String,
    pub messages: Vec<serde_json::Value>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    client: reqwest::blocking::Client,
    key: String,
}

impl Conversation {
    pub fn new(model: &str) -> Result<Self> {
        let key = std::env::var("ANTHROPIC_API_KEY").context(
            "ANTHROPIC_API_KEY is not set (use --prompt-only to print the prompt pack instead)",
        )?;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()?;
        Ok(Conversation {
            model: model.to_string(),
            system: system_prompt(),
            messages: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            client,
            key,
        })
    }

    /// Send `user` and return the assistant's text. Rate limits and server
    /// errors are retried a few times with backoff.
    pub fn ask(&mut self, user: &str) -> Result<String> {
        self.messages
            .push(serde_json::json!({"role": "user", "content": user}));
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 16000,
            "output_config": {"effort": "high"},
            "fallbacks": "default",
            "system": [{"type": "text", "text": self.system, "cache_control": {"type": "ephemeral"}}],
            "messages": self.messages,
        });
        let body = serde_json::to_string(&body)?;
        let mut text = String::new();
        for attempt in 0..5u32 {
            let resp = self
                .client
                .post("https://api.anthropic.com/v1/messages")
                .header("content-type", "application/json")
                .header("x-api-key", &self.key)
                .header("anthropic-version", "2026-10-21")
                .header("anthropic-beta", "server-side-fallback-2026-07-01")
                .body(body.clone())
                .send()
                .context("calling the Messages API")?;
            let status = resp.status();
            text = resp.text()?;
            if status.is_success() {
                break;
            }
            let retry = status.as_u16() == 429 || status.is_server_error();
            if !retry || attempt == 4 {
                bail!("Messages API returned {status}: {text}");
            }
            std::thread::sleep(Duration::from_secs(5 << attempt));
        }
        let v: serde_json::Value =
            serde_json::from_str(&text).context("parsing the API response")?;
        if let Some(u) = v.get("usage") {
            self.input_tokens += u["input_tokens"].as_u64().unwrap_or(0);
            self.output_tokens += u["output_tokens"].as_u64().unwrap_or(0);
            self.cache_read_tokens += u["cache_read_input_tokens"].as_u64().unwrap_or(0);
        }
        let stop = v["stop_reason"].as_str().unwrap_or("");
        if stop == "refusal" {
            bail!("the model declined the request ({:?})", v["stop_details"]);
        }
        let content = v["content"].as_array().cloned().unwrap_or_default();
        let reply: String = content
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if stop == "max_tokens" {
            bail!("the reply was cut off at max_tokens");
        }
        // Keep the assistant turn so the next attempt sees its own work.
        self.messages
            .push(serde_json::json!({"role": "assistant", "content": content}));
        Ok(reply)
    }
}

/// A fenced block of the given language in a reply.
pub fn fenced(reply: &str, lang: &str) -> Option<String> {
    let open = format!("```{lang}");
    let start = reply.find(&open)? + open.len();
    let rest = &reply[start..];
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    let end = rest.find("```")?;
    Some(rest[..end].to_string())
}

/// What the gate said about one attempt.
pub enum Verdict {
    /// The card file and scenario text to write.
    Accepted { card: String, scenarios: String },
    /// The model said the DSL cannot express the card.
    Unsupported(String),
    /// Problems to feed back.
    Rejected(String),
}

/// Check a reply: parse the card, compile it into a copy of the pool,
/// compare its rendering to the Oracle text, parse and run the scenarios.
pub fn check(reply: &str, scry: &ScryCard, corpus: &[Card]) -> Verdict {
    if let Some(rest) = reply.trim().strip_prefix("unsupported:") {
        return Verdict::Unsupported(rest.trim().to_string());
    }
    let Some(card_text) = fenced(reply, "card") else {
        return Verdict::Rejected("no ```card block in the reply".to_string());
    };
    let scen_text = fenced(reply, "scenario").unwrap_or_default();
    let mut problems = Vec::new();
    let cards = match mtg_dsl::parse(&card_text) {
        Ok(c) => c,
        Err(e) => return Verdict::Rejected(format!("card file does not parse: {e}")),
    };
    let [card] = cards.as_slice() else {
        return Verdict::Rejected("the card block must hold exactly one card".to_string());
    };
    if card.name != scry.name {
        problems.push(format!(
            "the card is named {:?}, expected {:?}",
            card.name, scry.name
        ));
    }
    if card.mismatch.is_some() {
        problems.push("oracle_mismatch is not allowed".to_string());
    }
    if let Some(d) = crate::gate::header_diff(card, scry) {
        problems.push(format!("the header differs from Scryfall: {d}"));
    }
    let by_name: HashMap<&str, &Card> = corpus.iter().map(|c| (c.name.as_str(), c)).collect();
    let rendered = render::normalize(&render::oracle_text(card, &by_name));
    let expected = render::normalize(scry.oracle_text.as_deref().unwrap_or(""));
    if rendered != expected {
        problems.push(format!(
            "the card renders to\n{rendered}\nbut the Oracle text is\n{expected}\nChange the statements so the rendering matches exactly."
        ));
    }
    // Compile into a copy of the pool with the card added.
    let mut pool = mtg_dsl::builtin_pool();
    if pool.lookup(&card.name).is_some() {
        problems.push(format!("{} is already in the corpus", card.name));
    } else if let Err(e) = mtg_dsl::load_into(&mut pool, &card_text) {
        problems.push(format!("the card does not compile: {e}"));
    }
    if !problems.is_empty() {
        return Verdict::Rejected(problems.join("\n\n"));
    }
    // Scenarios.
    if scen_text.trim().is_empty() {
        return Verdict::Rejected("no ```scenario block in the reply".to_string());
    }
    let scenarios = match mtg_scenario::parse(&scen_text) {
        Ok(s) => s,
        Err(e) => return Verdict::Rejected(format!("scenario file does not parse: {e}")),
    };
    if scenarios.len() < 2 {
        return Verdict::Rejected("write at least two scenarios".to_string());
    }
    let pool = Arc::new(pool);
    let mut failures = Vec::new();
    for sc in &scenarios {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mtg_scenario::run(&pool, sc)
        }));
        match outcome {
            Ok(r) => {
                if let Some(f) = r.failure {
                    failures.push(format!(
                        "scenario \"{}\" fails at line {}: {}",
                        r.name, f.line, f.message
                    ));
                }
            }
            Err(_) => failures.push(format!("scenario \"{}\": the engine panicked", sc.name)),
        }
    }
    if !failures.is_empty() {
        return Verdict::Rejected(failures.join("\n"));
    }
    Verdict::Accepted {
        card: card_text,
        scenarios: scen_text,
    }
}

/// The Scryfall records for `names`, in one pass over the snapshot. Names
/// not in the snapshot are left out.
pub fn records(data: &DataDir, names: &[String]) -> Result<HashMap<String, ScryCard>> {
    let wanted: HashSet<&str> = names.iter().map(String::as_str).collect();
    let path = data.scryfall_bulk("oracle_cards");
    let mut out: HashMap<String, ScryCard> = HashMap::new();
    for card in scryfall::read_cards(&path)? {
        let card = card?;
        if card.layout != "art_series"
            && card.layout != "token"
            && card.set_type != "token"
            && wanted.contains(card.name.as_str())
            && !out.contains_key(&card.name)
        {
            out.insert(card.name.clone(), card);
        }
    }
    Ok(out)
}

/// Rulings for a set of oracle ids, in one pass over the snapshot.
#[allow(clippy::implicit_hasher)]
pub fn rulings_for(data: &DataDir, ids: &HashSet<String>) -> Result<HashMap<String, Vec<String>>> {
    #[derive(Deserialize)]
    struct Ruling {
        oracle_id: String,
        comment: String,
    }
    let path = data.scryfall_bulk("rulings");
    let file = std::fs::File::open(&path).with_context(|| format!("opening {}", path.display()))?;
    let reader = std::io::BufReader::new(flate2::read::GzDecoder::new(file));
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for line in reader.lines() {
        let line = line?;
        if let Ok(r) = serde_json::from_str::<Ruling>(&line)
            && ids.contains(&r.oracle_id)
        {
            out.entry(r.oracle_id).or_default().push(r.comment);
        }
    }
    Ok(out)
}

/// The Scryfall record for a card name.
pub fn record(data: &DataDir, name: &str) -> Result<ScryCard> {
    let path = data.scryfall_bulk("oracle_cards");
    for card in scryfall::read_cards(&path)? {
        let card = card?;
        if card.name == name && card.layout != "art_series" {
            return Ok(card);
        }
    }
    bail!("{name} is not in the Scryfall snapshot")
}

/// Author one card with the model; returns the verdict of the last
/// attempt and the transcript's token usage.
pub fn author(
    data: &DataDir,
    corpus: &[Card],
    name: &str,
    model: &str,
    attempts: usize,
    log: &mut dyn FnMut(&str),
) -> Result<(Verdict, Option<Conversation>)> {
    let scry = record(data, name)?;
    let rulings = rulings(data, scry.oracle_id.as_deref().unwrap_or(""))?;
    let (verdict, conv) = author_record(&scry, &rulings, corpus, model, attempts, log)?;
    Ok((verdict, Some(conv)))
}

/// Author one card whose Scryfall record and rulings are in hand.
pub fn author_record(
    scry: &ScryCard,
    rulings: &[String],
    corpus: &[Card],
    model: &str,
    attempts: usize,
    log: &mut dyn FnMut(&str),
) -> Result<(Verdict, Conversation)> {
    let examples = similar(corpus, scry.oracle_text.as_deref().unwrap_or(""), 6);
    let user = user_prompt(scry, rulings, &examples);
    let mut conv = Conversation::new(model)?;
    let mut prompt = user;
    let mut last = Verdict::Rejected("no attempt".to_string());
    for attempt in 1..=attempts {
        log(&format!("attempt {attempt}: asking {model}"));
        let reply = conv.ask(&prompt)?;
        last = check(&reply, scry, corpus);
        match &last {
            Verdict::Accepted { .. } => {
                log("accepted");
                break;
            }
            Verdict::Unsupported(why) => {
                log(&format!("unsupported: {why}"));
                break;
            }
            Verdict::Rejected(problems) => {
                log(&format!("rejected:\n{problems}"));
                prompt = format!(
                    "The gate rejected that attempt:\n\n{problems}\n\nReply again in the same format with the corrected card file and scenarios (or `unsupported: ...` if the DSL cannot express the card)."
                );
            }
        }
    }
    Ok((last, conv))
}

/// Where a scenario file for a model-authored card goes.
pub fn scenario_path(repo: &Path, slug: &str) -> std::path::PathBuf {
    repo.join("scenarios/llm").join(format!("{slug}.scn"))
}
