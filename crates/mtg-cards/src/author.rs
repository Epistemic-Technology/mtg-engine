//! Template authoring (design.md §6.4, the part that needs no model): build
//! card files straight from the Scryfall snapshot for every card whose
//! Oracle text the DSL's printer can reproduce from a fixed set of sentence
//! templates. Each candidate is parsed, rendered, and compared to its
//! Oracle text before it is accepted, so an accepted card passes the
//! round-trip gate by construction; anything with an unrecognized sentence,
//! keyword, or type is reported with the reason and left for hand or model
//! authoring.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

use anyhow::{Context, Result};
use mtg_dsl::ast::{Card, Kind};
use mtg_dsl::render::{self, Lookup};
use regex::Regex;

use crate::DataDir;
use crate::scryfall::{self, Card as ScryCard};

/// Keyword abilities the engine implements (CR 702), as Scryfall tags them.
const KEYWORDS: [&str; 15] = [
    "Deathtouch",
    "Defender",
    "Double strike",
    "First strike",
    "Flash",
    "Flying",
    "Haste",
    "Hexproof",
    "Indestructible",
    "Lifelink",
    "Menace",
    "Prowess",
    "Reach",
    "Trample",
    "Vigilance",
];

/// Scryfall keyword tags that templates below handle as abilities.
const TEMPLATED_TAGS: [&str; 8] = [
    "Enchant",
    "Equip",
    "Regenerate",
    "Mill",
    "Scry",
    "Surveil",
    "Cycling",
    "Fight",
];

/// Restrictions Oracle prints as sentences; the DSL keeps them with the
/// keywords.
const RESTRICTIONS: [&str; 4] = [
    "can't block",
    "can't attack",
    "attacks each combat if able",
    "can't be blocked",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    /// Multi-face, split, or otherwise unsupported layout.
    Layout(String),
    /// A type the engine does not model.
    Type(String),
    /// More subtypes than a definition holds.
    Subtypes,
    /// A keyword or ability word the engine does not implement.
    Keyword(String),
    /// A sentence no template recognizes.
    Sentence(String),
    /// The templates produced text that does not render back to Oracle.
    Mismatch { rendered: String, oracle: String },
    /// Already in the corpus.
    Exists,
}

impl std::fmt::Display for Skip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Skip::Layout(l) => write!(f, "layout {l}"),
            Skip::Type(t) => write!(f, "type {t}"),
            Skip::Subtypes => write!(f, "too many subtypes"),
            Skip::Keyword(k) => write!(f, "keyword {k}"),
            Skip::Sentence(s) => write!(f, "sentence: {s}"),
            Skip::Mismatch { rendered, oracle } => {
                write!(f, "mismatch: rendered {rendered:?}, oracle {oracle:?}")
            }
            Skip::Exists => write!(f, "exists"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Authored {
    pub name: String,
    pub slug: String,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Report {
    /// Cards in the format with a normal layout.
    pub total: usize,
    /// Of those, already in the corpus.
    pub existing: usize,
    pub authored: Vec<Authored>,
    pub skipped: Vec<(String, Skip)>,
}

impl Report {
    /// Skip reasons by frequency, with the sentence/keyword text collapsed
    /// to its template shape.
    pub fn reasons(&self) -> Vec<(String, usize)> {
        let mut m: BTreeMap<String, usize> = BTreeMap::new();
        for (_, s) in &self.skipped {
            let key = match s {
                Skip::Sentence(t) => format!("sentence: {}", shape(t)),
                Skip::Keyword(k) => format!("keyword: {k}"),
                Skip::Mismatch { .. } => "mismatch".to_string(),
                other => other.to_string(),
            };
            *m.entry(key).or_insert(0) += 1;
        }
        let mut v: Vec<(String, usize)> = m.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v
    }
}

/// A sentence with numbers and mana symbols abstracted, for counting.
fn shape(s: &str) -> String {
    let s = Regex::new(r"\{[^}]*\}").unwrap().replace_all(s, "{M}");
    let s = Regex::new(r"\b\d+\b").unwrap().replace_all(&s, "N");
    s.into_owned()
}

/// Author every card legal in `format` that the templates cover.
pub fn author_format(
    data: &DataDir,
    format: &str,
    corpus: &[Card],
    only: &[String],
) -> Result<Report> {
    let existing: HashSet<&str> = corpus.iter().map(|c| c.name.as_str()).collect();
    let path = data.scryfall_bulk("oracle_cards");
    let mut report = Report::default();
    for card in scryfall::read_cards(&path).context("reading the Scryfall snapshot")? {
        let card = card?;
        if !only.is_empty() && !only.contains(&card.name) {
            continue;
        }
        if only.is_empty() && !card.playable_in(format) {
            continue;
        }
        if card.layout != "normal" || card.set_type == "memorabilia" || card.set_type == "funny" {
            continue;
        }
        report.total += 1;
        if existing.contains(card.name.as_str()) {
            report.existing += 1;
            continue;
        }
        match author_card(&card, corpus) {
            Ok(a) => report.authored.push(a),
            Err(skip) => report.skipped.push((card.name.clone(), skip)),
        }
    }
    Ok(report)
}

/// Cards legal in `format` (normal layout) that are in the corpus, and how
/// many there are in all.
pub fn coverage(data: &DataDir, format: &str, corpus: &[Card]) -> Result<(usize, usize)> {
    let existing: HashSet<&str> = corpus.iter().map(|c| c.name.as_str()).collect();
    let path = data.scryfall_bulk("oracle_cards");
    let (mut have, mut total) = (0, 0);
    for card in scryfall::read_cards(&path)? {
        let card = card?;
        if !card.playable_in(format) || card.layout != "normal" || card.set_type == "memorabilia" {
            continue;
        }
        total += 1;
        if existing.contains(card.name.as_str()) {
            have += 1;
        }
    }
    Ok((have, total))
}

fn strip_reminder(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

const NUMBER_WORDS: [&str; 20] = [
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
];

/// "two" → "2", "a" → "1", "X" → "X", digits unchanged.
fn number(w: &str) -> Option<String> {
    if w == "X" {
        return Some("X".to_string());
    }
    if w == "a" || w == "an" {
        return Some("1".to_string());
    }
    if w.chars().all(|c| c.is_ascii_digit()) {
        return Some(w.to_string());
    }
    NUMBER_WORDS
        .iter()
        .position(|x| *x == w)
        .map(|i| (i + 1).to_string())
}

fn keyword_list(s: &str) -> Option<String> {
    let mut out = Vec::new();
    for part in s.split(", ").flat_map(|p| p.split(" and ")) {
        let p = part.trim();
        if KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(p)) {
            out.push(p.to_ascii_lowercase());
        } else {
            return None;
        }
    }
    Some(out.join(", "))
}

/// Recipient phrases the DSL accepts as written.
fn recipient(s: &str, targets: &mut usize) -> Option<String> {
    const PLAIN: [&str; 10] = [
        "you",
        "each opponent",
        "each player",
        "each creature",
        "each creature you control",
        "each permanent you control",
        "that creature",
        "that player",
        "all creatures",
        "this",
    ];
    const TARGETS: [&str; 18] = [
        "any target",
        "target creature",
        "target player",
        "target land",
        "target artifact",
        "target enchantment",
        "target permanent",
        "target tapped creature",
        "target creature you control",
        "target player or planeswalker",
        "target creature or planeswalker",
        "target artifact or enchantment",
        "target artifact, creature, or land",
        "target nonlegendary creature you control",
        "target creature card from your graveyard",
        "another target creature",
        "another target permanent",
        "two target creatures",
    ];
    if PLAIN.contains(&s) {
        return Some(s.to_string());
    }
    if matches!(
        s,
        "this creature"
            | "this permanent"
            | "this artifact"
            | "this enchantment"
            | "this land"
            | "this Aura"
    ) {
        return Some("this".to_string());
    }
    if s == "it" && *targets == 0 {
        return Some("this".to_string());
    }
    if (s == "that creature" || s == "that permanent") && *targets > 0 {
        return Some(format!("that target {targets}"));
    }
    if s == "creatures you control" {
        return Some("each creature you control".to_string());
    }
    if s == "permanents you control" {
        return Some("each permanent you control".to_string());
    }
    if TARGETS.contains(&s) {
        *targets += if s.starts_with("two ") { 2 } else { 1 };
        return Some(s.to_string());
    }
    // Any other "target <filter>" phrase: the DSL parser and the render
    // check decide whether it is one the engine knows.
    if s.starts_with("target ") || s.starts_with("another target ") {
        *targets += 1;
        return Some(s.to_string());
    }
    if (s == "it" || s == "that permanent") && *targets > 0 {
        return Some(format!("target {targets}"));
    }
    None
}

/// A sentence pattern and what it compiles to; the closure sees the
/// captures and the count of targets declared so far.
type Rule = (Regex, fn(&regex::Captures, &mut usize) -> Option<String>);

struct Templates {
    rules: Vec<Rule>,
}

fn cap<'a>(c: &'a regex::Captures, i: usize) -> &'a str {
    c.get(i).map_or("", |m| m.as_str())
}

impl Templates {
    #[allow(clippy::too_many_lines)]
    fn new() -> Self {
        let r = |s: &str| Regex::new(s).unwrap();
        let rules: Vec<Rule> = vec![
            (
                r(r"^(?:~|this|It|it) deals (\S+) damage to (.+?)(?: and (.+))?$"),
                |c, t| {
                    let n = number(cap(c, 1))?;
                    let mut to = recipient(cap(c, 2), t)?;
                    if !cap(c, 3).is_empty() {
                        let _ = write!(to, " and {}", recipient(cap(c, 3), t)?);
                    }
                    Some(format!("deal {n} damage to {to}"))
                },
            ),
            (
                r(
                    r"^(.+?) gets? ([+-]\d+/[+-]\d+)(?: and gains? ([a-z ,]+?))?(?: until end of turn)?$",
                ),
                |c, t| {
                    let who = recipient(&subject(cap(c, 1)), t)?;
                    let mut s = format!("{who} gets {}", cap(c, 2));
                    if !cap(c, 3).is_empty() {
                        let _ = write!(s, " and gains {}", keyword_list(cap(c, 3))?);
                    }
                    Some(s)
                },
            ),
            (
                r(r"^(.+?) gains? ([a-z ,]+?)(?: until end of turn)?$"),
                |c, t| {
                    if cap(c, 2).ends_with(" life") {
                        return None;
                    }
                    let who = recipient(&subject(cap(c, 1)), t)?;
                    Some(format!("{who} gains {}", keyword_list(cap(c, 2))?))
                },
            ),
            (r(r"^(.+?) (gains?|loses?) (\S+) life$"), |c, t| {
                let who = recipient(&subject(cap(c, 1)), t)?;
                let verb = if cap(c, 2).starts_with("gain") {
                    "gains"
                } else {
                    "loses"
                };
                Some(format!("{who} {verb} {} life", number(cap(c, 3))?))
            }),
            (r(r"^Draw (\S+) cards?$"), |c, _| {
                Some(format!("draw {} cards", number(cap(c, 1))?))
            }),
            (r(r"^Discard (\S+) cards?$"), |c, _| {
                Some(format!("discard {} cards", number(cap(c, 1))?))
            }),
            (r(r"^(.+?) discards? (\S+) cards?$"), |c, t| {
                let who = recipient(&subject(cap(c, 1)), t)?;
                Some(format!("{who} discards {} cards", number(cap(c, 2))?))
            }),
            (
                r(
                    r"^Counter (target .+?)(?: unless its controller pays ((?:\{[0-9XWUBRGC]+\})+))?$",
                ),
                |c, t| {
                    let to = recipient(cap(c, 1), t)?;
                    if cap(c, 2).is_empty() {
                        Some(format!("counter {to}"))
                    } else {
                        Some(format!(
                            "counter {to} unless its controller pays {}",
                            cap(c, 2)
                        ))
                    }
                },
            ),
            (r(r"^Scry (\d+)$"), |c, _| {
                Some(format!("scry {}", cap(c, 1)))
            }),
            (r(r"^Surveil (\d+)$"), |c, _| {
                Some(format!("surveil {}", cap(c, 1)))
            }),
            (r(r"^(.+?) fights (.+)$"), |c, t| {
                let a = recipient(&subject(cap(c, 1)), t)?;
                let b = recipient(cap(c, 2), t)?;
                Some(format!("{a} fights {b}"))
            }),
            (r(r"^Attach (?:it|~|this) to (.+)$"), |c, t| {
                Some(format!("attach to {}", recipient(cap(c, 1), t)?))
            }),
            (
                r(r"^(.+?) can't (block|attack|be blocked|attack or block) this turn$"),
                |c, t| {
                    let who = recipient(&subject(cap(c, 1)), t)?;
                    Some(format!("{who} can't {}", cap(c, 2)))
                },
            ),
            (r(r"^(.+?) (draws?|mills?) (\S+) cards?$"), |c, t| {
                let who = recipient(&subject(cap(c, 1)), t)?;
                let verb = if cap(c, 2).starts_with("draw") {
                    "draws"
                } else {
                    "mills"
                };
                Some(format!("{who} {verb} {} cards", number(cap(c, 3))?))
            }),
            (r(r"^Destroy (.+)$"), |c, t| {
                Some(format!("destroy {}", recipient(cap(c, 1), t)?))
            }),
            (r(r"^Exile (.+)$"), |c, t| {
                Some(format!("exile {}", recipient(cap(c, 1), t)?))
            }),
            (r(r"^Return (.+?) to (?:its owner's|your) hand$"), |c, t| {
                Some(format!("return {} to hand", recipient(cap(c, 1), t)?))
            }),
            (r(r"^(Tap|Untap) (.+)$"), |c, t| {
                Some(format!(
                    "{} {}",
                    cap(c, 1).to_ascii_lowercase(),
                    recipient(cap(c, 2), t)?
                ))
            }),
            (r(r"^Regenerate (.+)$"), |c, t| {
                Some(format!("regenerate {}", recipient(cap(c, 1), t)?))
            }),
            (r(r"^Put (\S+) ([+-]1/[+-]1) counters? on (.+)$"), |c, t| {
                Some(format!(
                    "put {} {} counters on {}",
                    number(cap(c, 1))?,
                    cap(c, 2),
                    recipient(cap(c, 3), t)?
                ))
            }),
            (
                r(r"^Remove (\S+) ([+-]1/[+-]1) counters? from (.+)$"),
                |c, t| {
                    Some(format!(
                        "remove {} {} counters from {}",
                        number(cap(c, 1))?,
                        cap(c, 2),
                        recipient(cap(c, 3), t)?
                    ))
                },
            ),
            (r(r"^Remove all counters from (.+)$"), |c, t| {
                Some(format!(
                    "remove all counters from {}",
                    recipient(cap(c, 1), t)?
                ))
            }),
            (r(r"^Sacrifice (.+)$"), |c, t| {
                Some(format!("sacrifice {}", recipient(cap(c, 1), t)?))
            }),
            (
                r(r"^Gain control of (.+?)(?: until end of turn)?$"),
                |c, t| Some(format!("gain control of {}", recipient(cap(c, 1), t)?)),
            ),
            (
                r(r"^Prevent all (combat )?damage that would be dealt(?: to (.+?))? this turn$"),
                |c, _| {
                    let kind = if cap(c, 1).is_empty() { "" } else { "combat " };
                    let to = match cap(c, 2) {
                        "" => String::new(),
                        "you" => " to you".to_string(),
                        "you and creatures you control" => {
                            " to you and creature you control".to_string()
                        }
                        _ => return None,
                    };
                    Some(format!("prevent all {kind}damage{to} this turn"))
                },
            ),
            (
                r(r"^Prevent the next (\d+) damage that would be dealt to (.+?) this turn$"),
                |c, t| {
                    Some(format!(
                        "prevent next {} damage to {} this turn",
                        cap(c, 1),
                        recipient(cap(c, 2), t)?
                    ))
                },
            ),
            (r(r"^Players can't gain life this turn$"), |_, _| {
                Some("players can't gain life this turn".to_string())
            }),
            (r(r"^Damage can't be prevented this turn$"), |_, _| {
                Some("damage can't be prevented this turn".to_string())
            }),
        ];
        Templates { rules }
    }

    fn sentence(&self, s: &str, targets: &mut usize) -> Option<String> {
        for (re, f) in &self.rules {
            if let Some(c) = re.captures(s) {
                return f(&c, targets);
            }
        }
        None
    }

    /// Sentences of one paragraph to `;`-joined DSL effects. `targets`
    /// counts targets declared so far in the ability.
    fn effects(&self, text: &str, targets: &mut usize) -> Result<String, Skip> {
        let mut out: Vec<String> = Vec::new();
        for sentence in split_sentences(text) {
            if sentence == "It can't be regenerated" || sentence == "They can't be regenerated" {
                match out.last_mut() {
                    Some(last) if last.starts_with("destroy ") => {
                        last.push_str(" without regeneration");
                        continue;
                    }
                    _ => return Err(Skip::Sentence(sentence)),
                }
            }
            let (may, body) = match sentence.strip_prefix("you may ") {
                Some(rest) => (true, rest.to_string()),
                None => (false, sentence.clone()),
            };
            // "Draw a card, then discard a card": clauses joined by "then".
            let mut parts = Vec::new();
            for clause in body.split(", then ") {
                let Some(e) = self.sentence(&capitalize(clause), targets) else {
                    return Err(Skip::Sentence(sentence.clone()));
                };
                parts.push(e);
            }
            let e = parts.join(" then ");
            out.push(if may { format!("you may {e}") } else { e });
        }
        if out.is_empty() {
            return Err(Skip::Sentence(text.to_string()));
        }
        Ok(out.join("; "))
    }
}

/// Subjects as the DSL names them ("This creature gets" → "this").
fn subject(s: &str) -> String {
    match s {
        "This creature" | "This permanent" | "this creature" | "This artifact"
        | "This enchantment" | "This land" | "~" => "this".to_string(),
        "It" => "it".to_string(),
        "You" => "you".to_string(),
        "Target creature" => "target creature".to_string(),
        "Target player" => "target player".to_string(),
        "Each player" => "each player".to_string(),
        "Each opponent" => "each opponent".to_string(),
        "Creatures you control" | "creatures you control" => {
            "each creature you control".to_string()
        }
        "Permanents you control" => "each permanent you control".to_string(),
        "Those creatures" => "those creatures".to_string(),
        "That creature" => "that creature".to_string(),
        "That player" => "that player".to_string(),
        other => other.to_string(),
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn lowercase_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Sentences of a paragraph without their final periods.
fn split_sentences(text: &str) -> Vec<String> {
    text.split(". ")
        .map(|s| s.trim().trim_end_matches('.').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// A cost prefix of an activated ability as the DSL writes it.
fn cost_text(s: &str) -> Option<String> {
    let symbols = Regex::new(r"^(\{[0-9XWUBRGCT]+\})+$").unwrap();
    let mut items = Vec::new();
    for part in s.split(", ") {
        let item = if symbols.is_match(part) {
            part.to_string()
        } else if matches!(
            part,
            "Sacrifice this creature"
                | "Sacrifice this artifact"
                | "Sacrifice this enchantment"
                | "Sacrifice this permanent"
        ) {
            "sacrifice this".to_string()
        } else if let Some(rest) = part
            .strip_prefix("Sacrifice a ")
            .or_else(|| part.strip_prefix("Sacrifice an "))
        {
            if rest.contains(' ') {
                return None;
            }
            format!("sacrifice {rest}")
        } else if part == "Discard a card" {
            "discard 1 card".to_string()
        } else if part == "Discard this card" {
            "discard this".to_string()
        } else {
            let n = part
                .strip_prefix("Pay ")
                .and_then(|r| r.strip_suffix(" life"))?;
            format!("pay {n} life")
        };
        items.push(item);
    }
    Some(items.join(", "))
}

fn trigger_text(head: &str) -> Option<String> {
    let t = match head {
        "When this creature enters"
        | "When this land enters"
        | "When this artifact enters"
        | "When this enchantment enters"
        | "When this permanent enters"
        | "When this Aura enters" => "when this enters",
        "Whenever another creature enters" => "whenever other creature enters",
        "Whenever a creature you control enters" => "whenever creature you control enters",
        "Whenever another creature you control enters" => {
            "whenever other creature you control enters"
        }
        "When this creature dies"
        | "When this Aura is put into a graveyard from the battlefield"
        | "When this enchantment is put into a graveyard from the battlefield"
        | "When this artifact is put into a graveyard from the battlefield"
        | "When this land is put into a graveyard from the battlefield" => "when this dies",
        "Whenever this creature or another creature dies" => "whenever creature dies",
        "Whenever another creature dies" => "whenever other creature dies",
        "Whenever this creature or another creature you control dies" => {
            "whenever creature you control dies"
        }
        "Whenever another creature you control dies" => "whenever other creature you control dies",
        "At the beginning of your upkeep" => "at beginning of your upkeep",
        "At the beginning of each player's upkeep" => "at beginning of each upkeep",
        "At the beginning of your end step" => "at beginning of your end step",
        "At the beginning of the end step" => "at beginning of each end step",
        "At the beginning of your draw step" => "at beginning of your draw step",
        "At the beginning of each player's draw step" => "at beginning of each draw step",
        "Whenever this creature attacks" => "whenever this attacks",
        "Whenever this creature blocks" => "whenever this blocks",
        "Whenever a creature you control attacks" => "whenever creature you control attacks",
        "Whenever this creature deals combat damage to a player" => {
            "whenever this deals combat damage to a player"
        }
        "Whenever you gain life" => "whenever you gain life",
        "Whenever you cast a noncreature spell" => "whenever you cast a noncreature spell",
        "Whenever you cast an instant or sorcery spell" => {
            "whenever you cast an instant or sorcery spell"
        }
        "Whenever you cast a spell" => "whenever you cast a spell",
        "When this creature becomes the target of a spell or ability" => {
            "when this becomes targeted"
        }
        _ => return None,
    };
    Some(t.to_string())
}

/// Lines a card's statements come from.
struct Builder<'a> {
    templates: &'a Templates,
    corpus: &'a [Card],
    is_spell: bool,
    statements: Vec<String>,
    keywords: Vec<String>,
    additional: Option<String>,
    /// Paragraphs of an instant's or sorcery's text, one `spell` block.
    spell_paragraphs: Vec<String>,
    spell_targets: usize,
}

impl Builder<'_> {
    #[allow(clippy::too_many_lines)]
    fn line(&mut self, line: &str) -> Result<(), Skip> {
        // Keyword lines: first, or in place among the abilities.
        if let Some(k) = keyword_list(line)
            && line.chars().next().is_some_and(char::is_uppercase)
        {
            if self.statements.is_empty() {
                self.keywords.push(k);
            } else {
                self.statements.push(format!("keyword {k}"));
            }
            return Ok(());
        }
        // "This creature can't block." and friends are keywords to the DSL.
        if let Some(rest) = line
            .strip_prefix("This creature ")
            .or_else(|| line.strip_prefix("This permanent "))
            && let Some(rest) = rest.strip_suffix('.')
            && let Some(k) = RESTRICTIONS.iter().find(|r| **r == rest)
        {
            if self.statements.is_empty() {
                self.keywords.push((*k).to_string());
            } else {
                self.statements.push(format!("keyword {k}"));
            }
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^Cycling ((?:\{[0-9XWUBRGC]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("cycling {}", cap(&c, 1)));
            return Ok(());
        }
        if line == "{T}: Add one mana of any color." {
            self.statements.push("mana any".to_string());
            return Ok(());
        }
        if let Some(rest) = line.strip_prefix("As an additional cost to cast this spell, ") {
            let rest = rest.trim_end_matches('.');
            let cost = match rest {
                "sacrifice a creature" => "sacrifice creature".to_string(),
                "discard a card" => "discard 1 card".to_string(),
                _ => return Err(Skip::Sentence(line.to_string())),
            };
            self.additional = Some(cost);
            return Ok(());
        }
        if let Some(what) = line.strip_prefix("Enchant ") {
            let spec = match what {
                "creature" => "creature",
                "land" => "land",
                "artifact" => "artifact",
                "enchantment" => "enchantment",
                "permanent" => "permanent",
                "creature you control" => "creature you control",
                "artifact or creature" => "artifact or creature",
                _ => return Err(Skip::Sentence(line.to_string())),
            };
            self.statements.push(format!("enchant {spec}"));
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^Equip \{(\d+)\}$").unwrap().captures(line) {
            self.statements.push(format!("equip {{{}}}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^\{T\}: Add ((?:\{[WUBRGC]\})(?: or \{[WUBRGC]\})*)\.$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("mana {}", cap(&c, 1)));
            return Ok(());
        }
        if line == "This land enters tapped."
            || line == "This creature enters tapped."
            || line == "This artifact enters tapped."
            || line == "This permanent enters tapped."
        {
            self.statements.push("enters tapped".to_string());
            return Ok(());
        }
        if let Some(c) = Regex::new(
            r"^This (?:creature|permanent|artifact) enters with (\S+) \+1/\+1 counters? on it\.$",
        )
        .unwrap()
        .captures(line)
        {
            let n = number(cap(&c, 1)).ok_or_else(|| Skip::Sentence(line.to_string()))?;
            self.statements
                .push(format!("enters with {n} +1/+1 counters"));
            return Ok(());
        }
        // Statics on the attached or on a class of permanents.
        if let Some(c) = Regex::new(
            r"^(Enchanted creature|Equipped creature|Creatures you control|Other creatures you control|All creatures|White creatures|Blue creatures|Black creatures|Red creatures|Green creatures|Enchanted land|Enchanted permanent) (.+)\.$",
        )
        .unwrap()
        .captures(line)
        {
            let filter = match cap(&c, 1) {
                "Enchanted creature" | "Equipped creature" => "enchanted".to_string(),
                "Enchanted land" | "Enchanted permanent" => return Err(Skip::Sentence(line.to_string())),
                "Creatures you control" => "creature you control".to_string(),
                "Other creatures you control" => "other creature you control".to_string(),
                "All creatures" => "creature".to_string(),
                colored => format!(
                    "{} creature",
                    colored.split(' ').next().unwrap_or("").to_ascii_lowercase()
                ),
            };
            let mut mods = Vec::new();
            let gets = Regex::new(r"^gets? ([+-]\d+/[+-]\d+)$").unwrap();
            for part in cap(&c, 2).split(" and ") {
                if let Some(c2) = gets.captures(part) {
                    mods.push(cap(&c2, 1).to_string());
                } else if let Some(rest) = part.strip_prefix("has ").or_else(|| part.strip_prefix("have ")) {
                    let ks = keyword_list(rest).ok_or_else(|| Skip::Sentence(line.to_string()))?;
                    mods.push(format!("has {ks}"));
                } else {
                    return Err(Skip::Sentence(line.to_string()));
                }
            }
            if !mods.is_empty() {
                self.statements.push(format!("static {filter}: {}", mods.join(", ")));
                return Ok(());
            }
        }
        if line == "You control enchanted creature." {
            self.statements
                .push("static enchanted: control".to_string());
            return Ok(());
        }
        // Activated abilities: "<cost>: <effects>[ Activate only once each turn.]".
        if let Some((cost, rest)) = line.split_once(": ")
            && let Some(mut cost) = cost_text(cost)
        {
            let rest = match rest.strip_suffix(" Activate only once each turn.") {
                Some(r) => {
                    cost.push_str(", once");
                    r
                }
                None => rest,
            };
            let mut targets = 0;
            let effects = self.templates.effects(rest, &mut targets)?;
            self.statements.push(format!("activated {cost}: {effects}"));
            return Ok(());
        }
        // Triggered abilities: "<trigger>, [if <condition>, ]<effects>".
        if line.starts_with("When") || line.starts_with("At the beginning") {
            let Some((head, rest)) = line.split_once(", ") else {
                return Err(Skip::Sentence(line.to_string()));
            };
            let Some(trigger) = trigger_text(head) else {
                return Err(Skip::Sentence(line.to_string()));
            };
            let (condition, rest) = if let Some(r) = rest
                .strip_prefix("if this creature is untapped, ")
                .or_else(|| rest.strip_prefix("if this artifact is untapped, "))
            {
                (" if this is untapped", r)
            } else if let Some(r) = rest.strip_prefix("if no creatures are on the battlefield, ") {
                (" if no creatures are on the battlefield", r)
            } else {
                ("", rest)
            };
            let mut targets = 0;
            let effects = self.templates.effects(rest, &mut targets)?;
            self.statements
                .push(format!("triggered {trigger}{condition}: {effects}"));
            return Ok(());
        }
        if self.is_spell {
            let mut targets = self.spell_targets;
            let effects = self.templates.effects(line, &mut targets)?;
            self.spell_targets = targets;
            self.spell_paragraphs.push(effects);
            return Ok(());
        }
        Err(Skip::Sentence(line.to_string()))
    }

    fn modal(&mut self, modes: &[String]) -> Result<(), Skip> {
        let mut lines = vec!["choose one {".to_string()];
        for m in modes {
            let mut targets = 0;
            let effects = self.templates.effects(m, &mut targets)?;
            lines.push(format!("  mode: {effects}"));
        }
        lines.push("}".to_string());
        self.statements.push(lines.join("\n  "));
        let _ = self.corpus;
        Ok(())
    }
}

/// Build the card file for one Scryfall record, or say why not.
#[allow(clippy::too_many_lines)]
pub fn author_card(s: &ScryCard, corpus: &[Card]) -> Result<Authored, Skip> {
    if s.name.contains(" // ") || s.layout != "normal" {
        return Err(Skip::Layout(s.layout.clone()));
    }
    let type_line = s.type_line.clone().unwrap_or_default();
    let (left, subtypes) = match type_line.split_once(" — ") {
        Some((l, r)) => (
            l.to_string(),
            r.split(' ').map(str::to_string).collect::<Vec<_>>(),
        ),
        None => (type_line.clone(), Vec::new()),
    };
    let mut is_spell = false;
    for w in left.split(' ') {
        match w {
            "Basic" | "Legendary" | "Snow" | "World" | "Artifact" | "Creature" | "Enchantment"
            | "Land" | "Planeswalker" | "Kindred" => {}
            "Instant" | "Sorcery" => is_spell = true,
            other => return Err(Skip::Type(other.to_string())),
        }
    }
    if subtypes.len() > 6 {
        return Err(Skip::Subtypes);
    }
    for k in &s.keywords {
        if !KEYWORDS.contains(&k.as_str()) && !TEMPLATED_TAGS.contains(&k.as_str()) {
            return Err(Skip::Keyword(k.clone()));
        }
    }
    let odd_symbol = Regex::new(r"\{[^}]*[/PSE][^}]*\}").unwrap();
    if odd_symbol.is_match(s.mana_cost.as_deref().unwrap_or(""))
        || odd_symbol.is_match(s.oracle_text.as_deref().unwrap_or(""))
    {
        return Err(Skip::Keyword(
            "hybrid, Phyrexian, snow, or energy symbol".to_string(),
        ));
    }
    let oracle = strip_reminder(s.oracle_text.as_deref().unwrap_or(""));
    if oracle.contains('"') {
        return Err(Skip::Sentence("quoted ability".to_string()));
    }
    let oracle = oracle.replace(&s.name, "~");
    let templates = Templates::new();
    let mut b = Builder {
        templates: &templates,
        corpus,
        is_spell,
        statements: Vec::new(),
        keywords: Vec::new(),
        additional: None,
        spell_paragraphs: Vec::new(),
        spell_targets: 0,
    };
    let lines: Vec<&str> = oracle.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line == "Choose one —" {
            let mut modes = Vec::new();
            i += 1;
            while i < lines.len() && lines[i].starts_with("• ") {
                modes.push(lines[i][3..].to_string());
                i += 1;
            }
            b.modal(&modes)?;
            continue;
        }
        b.line(line)?;
        i += 1;
    }
    // Assemble the file.
    let mut text = format!("card \"{}\" {{\n", s.name);
    if let Some(id) = &s.oracle_id {
        let _ = writeln!(text, "  oracle_id {id}");
    }
    if let Some(cost) = &s.mana_cost
        && !cost.is_empty()
    {
        let _ = writeln!(text, "  cost {cost}");
    }
    let _ = writeln!(text, "  type {type_line}");
    if let (Some(p), Some(t)) = (&s.power, &s.toughness) {
        if p.contains('*') || t.contains('*') {
            return Err(Skip::Sentence("variable power/toughness".to_string()));
        }
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
    if !b.keywords.is_empty() {
        let _ = writeln!(text, "  keywords {}", b.keywords.join(", "));
    }
    if let Some(a) = &b.additional {
        let _ = writeln!(text, "  additional cost {a}");
    }
    for st in &b.statements {
        let _ = writeln!(text, "  {st}");
    }
    match b.spell_paragraphs.as_slice() {
        [] => {}
        [one] => {
            let _ = writeln!(text, "  spell: {one}");
        }
        many => {
            text.push_str("  spell {\n");
            for p in many {
                let _ = writeln!(text, "    {p}");
            }
            text.push_str("  }\n");
        }
    }
    text.push_str("}\n");
    // Verify: parse, render, compare.
    let mut cards = mtg_dsl::parse(&text).map_err(|e| Skip::Sentence(format!("parse: {e}")))?;
    let card = cards.remove(0);
    if card.kind != Kind::Card {
        return Err(Skip::Layout("not a card".to_string()));
    }
    let by_name: std::collections::HashMap<&str, &Card> =
        corpus.iter().map(|c| (c.name.as_str(), c)).collect();
    let lookup: &dyn Lookup = &by_name;
    let rendered = render::normalize(&render::oracle_text(&card, lookup));
    let expected = render::normalize(s.oracle_text.as_deref().unwrap_or(""));
    if rendered != expected {
        return Err(Skip::Mismatch {
            rendered,
            oracle: expected,
        });
    }
    let _ = lowercase_first;
    Ok(Authored {
        name: s.name.clone(),
        slug: mtg_dsl::slug(&s.name),
        text,
    })
}
