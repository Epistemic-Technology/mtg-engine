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
const KEYWORDS: [&str; 39] = [
    "Banding",
    "Delve",
    "Exploit",
    "Cascade",
    "Rebound",
    "Convoke",
    "Improvise",
    "Flanking",
    "Shroud",
    "Fear",
    "Intimidate",
    "Shadow",
    "Horsemanship",
    "Skulk",
    "Wither",
    "Infect",
    "Exalted",
    "Persist",
    "Undying",
    "Deathtouch",
    "Plainswalk",
    "Islandwalk",
    "Swampwalk",
    "Mountainwalk",
    "Forestwalk",
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
const TEMPLATED_TAGS: [&str; 93] = [
    "Transform",
    "Role token",
    "Landcycling",
    "Basic landcycling",
    "Typecycling",
    "Plainscycling",
    "Islandcycling",
    "Swampcycling",
    "Mountaincycling",
    "Forestcycling",
    "Mana ability",
    "Protection",
    "Devoid",
    "Changeling",
    "Investigate",
    "Clue",
    "Treasure",
    "Food",
    "Kicker",
    "Flashback",
    "Landwalk",
    "Enchant",
    "Equip",
    "Regenerate",
    "Mill",
    "Scry",
    "Surveil",
    "Cycling",
    "Fight",
    "Affinity",
    "Bushido",
    "Soulshift",
    "Afflict",
    "Toxic",
    "Ward",
    "Echo",
    "Unearth",
    "Buyback",
    "Outlast",
    "Proliferate",
    "Explore",
    "Amass",
    // Scryfall tags some regenerating cards "Heal".
    "Heal",
    "Morph",
    "Megamorph",
    "Disguise",
    "Manifest",
    "Crew",
    "Evoke",
    "Madness",
    "Foretell",
    "Suspend",
    "Cascade",
    "Rebound",
    "Bloodthirst",
    "Adapt",
    "Connive",
    "Entwine",
    "Escape",
    "Discover",
    "Exploit",
    "Delve",
    "Modular",
    "Cumulative upkeep",
    "Exert",
    "Boast",
    "Exhaust",
    "Power-up",
    "Ninjutsu",
    "Scavenge",
    "Dash",
    "Bestow",
    "Overload",
    "Splice",
    "Embalm",
    "Eternalize",
    "Backup",
    "Offspring",
    "Bargain",
    "Conspire",
    "Casualty",
    "Teamwork",
    "Warp",
    "Ripple",
    "Saddle",
    "Fabricate",
    "Renown",
    "Evolve",
    "Unleash",
    "Support",
    "Bolster",
    "Populate",
    "Incubate",
];

/// Ability words (CR 207.2c): labels with no rules meaning, printed
/// "Landfall — <ability>". Scryfall tags them like keywords.
const ABILITY_WORDS: [&str; 41] = [
    "Landfall",
    "Threshold",
    "Metalcraft",
    "Delirium",
    "Raid",
    "Morbid",
    "Ferocious",
    "Hellbent",
    "Spell mastery",
    "Formidable",
    "Revolt",
    "Domain",
    "Battalion",
    "Constellation",
    "Heroic",
    "Alliance",
    "Coven",
    "Magecraft",
    "Fateful hour",
    "Kinship",
    "Imprint",
    "Chroma",
    "Radiance",
    "Inspired",
    "Enrage",
    "Addendum",
    "Adamant",
    "Corrupted",
    "Eerie",
    "Survival",
    "Valiant",
    "Celebration",
    "Descend 4",
    "Descend 8",
    "Bloodrush",
    "Lieutenant",
    "Strive",
    "Undergrowth",
    "Pack tactics",
    "Void",
    "Channel",
];

/// Keywords whose Oracle wording is a phrase the DSL takes as written.
const PHRASE_KEYWORDS: [&str; 7] = [
    "protection from white",
    "protection from blue",
    "protection from black",
    "protection from red",
    "protection from green",
    "devoid",
    "changeling",
];

/// Restrictions Oracle prints as sentences; the DSL keeps them with the
/// keywords.
const RESTRICTIONS: [&str; 11] = [
    "can't attack or block alone",
    "can't attack alone",
    "can't block alone",
    "can't block",
    "can't attack",
    "attacks each combat if able",
    "can't be blocked",
    "blocks each combat if able",
    "must be blocked if able",
    "can block an additional creature each combat",
    "can block any number of creatures",
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
    /// Token definitions the card creates that the corpus lacks, as
    /// (slug, file text), for `cards/tokens/`.
    pub tokens: Vec<(String, String)>,
}

/// "Create two 1/1 white Soldier creature tokens with flying": the count,
/// then the key the generated token is filed under (the description
/// without the count and "token").
const TOKEN_PHRASE: &str = r"^[Cc]reate (\S+) ((?:\d+/\d+) (?:colorless|(?:white|blue|black|red|green)(?: and (?:white|blue|black|red|green))?) (?:[A-Z][a-z]+ )+(?:artifact )?creature) tokens?((?: with [a-z ,]+)?)$";

/// The token block for a key from `TOKEN_PHRASE` ("1/1 white Soldier
/// creature"), or `None` if the key doesn't describe one.
fn token_block(key: &str) -> Option<String> {
    let re = Regex::new(
        r"^(\d+)/(\d+) (colorless|(?:white|blue|black|red|green)(?: and (?:white|blue|black|red|green))?) ((?:[A-Z][a-z]+ )+)(artifact )?creature(?: with ([a-z ,]+))?$",
    )
    .unwrap();
    let c = re.captures(key)?;
    let subtypes = cap(&c, 4).trim();
    let color = match cap(&c, 3) {
        "colorless" => String::new(),
        cs => format!(" {}", cs.replace(" and ", " ")),
    };
    let types = if cap(&c, 5).is_empty() {
        "Creature"
    } else {
        "Artifact Creature"
    };
    let keywords = if cap(&c, 6).is_empty() {
        String::new()
    } else {
        format!("  keywords {}\n", keyword_list(cap(&c, 6))?)
    };
    Some(format!(
        "token \"{key}\" {{\n  name \"{subtypes}\"\n  type {types} — {subtypes}\n  pt {}/{}\n  color{color}\n{keywords}}}\n",
        cap(&c, 1),
        cap(&c, 2)
    ))
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
        if card.set_type == "memorabilia" || card.set_type == "funny" || is_token_layout(&card) {
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

/// Layouts that are not cards a deck can hold.
fn is_token_layout(card: &ScryCard) -> bool {
    matches!(
        card.layout.as_str(),
        "token" | "double_faced_token" | "emblem" | "art_series" | "vanguard" | "scheme" | "planar"
    )
}

/// Cards legal in `format` (every layout) that are in the corpus, and how
/// many there are in all.
pub fn coverage(data: &DataDir, format: &str, corpus: &[Card]) -> Result<(usize, usize)> {
    let existing: HashSet<&str> = corpus.iter().map(|c| c.name.as_str()).collect();
    let path = data.scryfall_bulk("oracle_cards");
    let (mut have, mut total) = (0, 0);
    for card in scryfall::read_cards(&path)? {
        let card = card?;
        if !card.playable_in(format) || card.set_type == "memorabilia" || is_token_layout(&card) {
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
        .map(|l| {
            l.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .replace(" .", ".")
                .replace(" ,", ",")
        })
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
    // "protection from white and from blue"
    let s = s.replace(" and from ", ", protection from ");
    for part in s.split(", ").flat_map(|p| p.split(" and ")) {
        let p = part.trim();
        if KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(p))
            || PHRASE_KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(p))
        {
            out.push(p.to_ascii_lowercase());
        } else {
            return None;
        }
    }
    Some(out.join(", "))
}

/// "creatures you control", "basic land types among lands you control",
/// "creature cards in your graveyard" → the DSL's count expression; the
/// phrase may be singular ("for each creature you control").
fn count_expr(s: &str) -> Option<String> {
    let s = s.trim();
    match s {
        "basic land types among lands you control" | "basic land type among lands you control" => {
            return Some("domain".to_string());
        }
        "cards in your hand" | "card in your hand" => return Some("hand".to_string()),
        "cards in your graveyard" | "card in your graveyard" => {
            return Some("graveyard".to_string());
        }
        _ => {}
    }
    // "2 plus the number of cards named ~ in all graveyards".
    if let Some((n, rest)) = s.split_once(" plus the number of ")
        && !n.contains(' ')
    {
        number(n)?;
        return Some(format!("plus {n} {}", count_expr(rest)?));
    }
    // "cards named ~ in all graveyards", "card named ~ in each graveyard".
    for (suffix, place) in [
        (" in your graveyard", "graveyard"),
        (" in all graveyards", "all graveyards"),
        (" in each graveyard", "each graveyard"),
    ] {
        if let Some(f) = s.strip_suffix(suffix)
            && matches!(f, "cards named ~" | "card named ~")
        {
            return Some(format!("{place} named this"));
        }
    }
    if let Some(f) = s
        .strip_suffix(" cards in your graveyard")
        .or_else(|| s.strip_suffix(" card in your graveyard"))
    {
        return Some(format!("graveyard {}", singular(f)));
    }
    if let Some(f) = s.strip_suffix(" on the battlefield") {
        return Some(format!("{} on battlefield", singular(f)));
    }
    if s.ends_with(" you control") {
        return Some(singular(s));
    }
    // "permanent you control of that type".
    if let Some((base, of)) = chosen_suffix(s)
        && base.ends_with(" you control")
    {
        return Some(format!("{} {of}", singular(base)));
    }
    None
}

/// A noun phrase ending in a chosen color or type, split before it: the
/// rest and the DSL's qualifier ("of the chosen type" → `of chosen type`).
fn chosen_suffix(s: &str) -> Option<(&str, &'static str)> {
    [
        (" of the chosen color", "of chosen color"),
        (" of the chosen type", "of chosen type"),
        (" of that type", "of that type"),
        (" of that color", "of that color"),
        (" of the color of your choice", "of color of choice"),
        (
            " of the creature type of your choice",
            "of creature type of choice",
        ),
    ]
    .into_iter()
    .find_map(|(suffix, of)| s.strip_suffix(suffix).map(|b| (b, of)))
}

/// Recipient phrases the DSL accepts as written.
#[allow(clippy::too_many_lines)] // one check per phrase family
fn recipient(s: &str, targets: &mut usize) -> Option<String> {
    const PLAIN: [&str; 14] = [
        "enchanted creature",
        "equipped creature",
        "enchanted land",
        "enchanted permanent",
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
    let lowered;
    let s = if s.starts_with("Another target ")
        || s.starts_with("Up to ")
        || s.starts_with("One or two ")
        || s.starts_with("Any number of ")
        || s.starts_with("Target ")
        || s.starts_with("Enchanted ")
        || s.starts_with("Equipped ")
    {
        lowered = lowercase_first(s);
        lowered.as_str()
    } else {
        s
    };
    if PLAIN.contains(&s) {
        return Some(s.to_string());
    }
    if (s == "its controller" || s == "its owner") && *targets > 0 {
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
    // "X target creatures", "one or two target creatures", "any number of
    // target players": a variable group (CR 601.2c).
    if let Some((count, rest)) = many_count(s) {
        *targets += 1;
        return Some(format!("{count} {rest}"));
    }
    // "up to two target creatures": optional targets.
    if let Some(rest) = s.strip_prefix("up to ") {
        let (n, tail) = rest.split_once(' ')?;
        let k = ["one", "two", "three", "four", "five"]
            .iter()
            .position(|w| *w == n)?
            + 1;
        if tail.starts_with("target ") {
            *targets += k;
            return Some(s.to_string());
        }
    }
    // Any other "target <filter>" phrase: the DSL parser and the render
    // check decide whether it is one the engine knows.
    if s.starts_with("target ") || s.starts_with("another target ") {
        *targets += 1;
        return Some(s.to_string());
    }
    // "creatures of the creature type of your choice", "permanents you
    // control of that type": a group with a chosen color or type.
    if let Some((base, of)) = chosen_suffix(s) {
        let base = base
            .strip_prefix("all ")
            .or_else(|| base.strip_prefix("each "))
            .unwrap_or(base);
        return Some(format!("each {} {of}", singular(&lowercase_first(base))));
    }
    // Groups: "all artifacts", "each creature without flying", "white
    // creatures you control" (a plural subject). The DSL writes them
    // singular after `each`; the render check decides the rest.
    if let Some(rest) = s.strip_prefix("all ").or_else(|| s.strip_prefix("each ")) {
        return Some(format!("each {}", singular(rest)));
    }
    if s.split(' ').count() <= 5
        && s.split(' ').any(|w| {
            matches!(
                w,
                "creatures" | "artifacts" | "enchantments" | "lands" | "permanents"
            )
        })
    {
        return Some(format!("each {}", singular(&lowercase_first(s))));
    }
    if (s == "it" || s == "that permanent") && *targets > 0 {
        return Some(format!("target {targets}"));
    }
    None
}

/// The count of a variable target group in DSL form and the rest of the
/// phrase ("one or two target creatures" → `1 to 2`, "target creatures").
fn many_count(s: &str) -> Option<(&'static str, &str)> {
    const COUNTS: [(&str, &str); 4] = [
        ("X ", "X"),
        ("one or two ", "1 to 2"),
        ("one, two, or three ", "1 to 3"),
        ("any number of ", "any number of"),
    ];
    COUNTS.iter().find_map(|(words, dsl)| {
        let rest = s.strip_prefix(words)?;
        (rest.starts_with("target ") || rest == "targets").then_some((*dsl, rest))
    })
}

/// A plural noun phrase made singular for the DSL: "blue creatures you
/// control" → "blue creature you control", "Auras" → "Aura".
fn singular(s: &str) -> String {
    s.split(' ')
        .map(|w| match w {
            "creatures" | "artifacts" | "enchantments" | "lands" | "permanents" | "Auras"
            | "Slivers" | "Goblins" | "Walls" | "instants" | "sorceries" | "planeswalkers" => {
                if w == "sorceries" {
                    "sorcery".to_string()
                } else {
                    w.trim_end_matches('s').to_string()
                }
            }
            "Elves" => "Elf".to_string(),
            "Dwarves" => "Dwarf".to_string(),
            "Wolves" => "Wolf".to_string(),
            other
                if other.len() > 3
                    && other.ends_with('s')
                    && !other.ends_with("ss")
                    && !other.ends_with("us")
                    && other.chars().next().is_some_and(char::is_uppercase) =>
            {
                other[..other.len() - 1].to_string()
            }
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
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
            // CR 601.2d: "deals 3 damage divided as you choose among one,
            // two, or three targets".
            (
                r(
                    r"^(?:~|this|This creature|This artifact|This enchantment|This permanent) deals (\S+) damage divided as you choose among (.+)$",
                ),
                |c, t| {
                    let n = number(cap(c, 1))?;
                    let (count, rest) = many_count(cap(c, 2))?;
                    *t += 1;
                    Some(format!("deal {n} damage divided among {count} {rest}"))
                },
            ),
            (
                r(r"^Distribute (\S+) ([+-]1/[+-]1) counters among (.+)$"),
                |c, t| {
                    let n = number(cap(c, 1))?;
                    let (count, rest) = many_count(cap(c, 3))?;
                    *t += 1;
                    Some(format!(
                        "distribute {n} {} counters among {count} {rest}",
                        cap(c, 2)
                    ))
                },
            ),
            (
                r(
                    r"^(?:~|this|It|it|This creature|This artifact|This enchantment|This permanent|This land) deals (\S+) damage to (.+?)(?: and (.+))?$",
                ),
                |c, t| {
                    // "deals X damage to ..., where X is ...": below.
                    if cap(c, 2).contains(", where X is") {
                        return None;
                    }
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
            // "Choose a creature type." as the effect resolves, for "of
            // that type" and "becomes that type" after it.
            (
                r(
                    r"^Choose an? (color|creature type|basic land type)(?: other than ([A-Z]?[a-z]+))?$",
                ),
                |c, _| {
                    let what = cap(c, 1).strip_prefix("basic ").unwrap_or(cap(c, 1));
                    Some(if cap(c, 2).is_empty() {
                        format!("choose {what}")
                    } else {
                        format!("choose {what} except {}", cap(c, 2))
                    })
                },
            ),
            (
                r(
                    r"^(.+?) (becomes? that type|gains? protection from the chosen color) until end of turn$",
                ),
                |c, t| {
                    let who = recipient(&lowercase_first(&subject(cap(c, 1))), t)?;
                    Some(if cap(c, 2).ends_with("type") {
                        format!("{who} becomes that type")
                    } else {
                        format!("{who} gains protection from chosen color")
                    })
                },
            ),
            // Chosen as the effect resolves.
            (
                r(
                    r"^(.+?) (becomes?|gains? protection from) the (color|basic land type|creature type) of your choice until end of turn$",
                ),
                |c, t| {
                    let who = recipient(&subject(cap(c, 1)), t)?;
                    let what = match cap(c, 3) {
                        "color" => "color",
                        "basic land type" => "land type",
                        _ => "creature type",
                    };
                    let verb = if cap(c, 2).starts_with("gain") {
                        if what != "color" {
                            return None;
                        }
                        "gains protection from"
                    } else {
                        "becomes"
                    };
                    Some(format!("{who} {verb} {what} of choice"))
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
            (
                r(r"^(.+?) (?:each )?(gains?|loses?) (\S+) life$"),
                |c, t| {
                    let who = recipient(&subject(cap(c, 1)), t)?;
                    let verb = if cap(c, 2).starts_with("gain") {
                        "gains"
                    } else {
                        "loses"
                    };
                    Some(format!("{who} {verb} {} life", number(cap(c, 3))?))
                },
            ),
            (r(r"^Draw (\S+) cards?$"), |c, _| {
                Some(format!("draw {} cards", number(cap(c, 1))?))
            }),
            (
                r(
                    r"^[Cc]reate (\S+) (tapped )?(Treasure|Food|Clue|Blood|Map|Powerstone|Mutagen) tokens?$",
                ),
                |c, _| {
                    Some(format!(
                        "create {}{} \"{}\"",
                        number(cap(c, 1))?,
                        if cap(c, 2).is_empty() { "" } else { " tapped" },
                        cap(c, 3)
                    ))
                },
            ),
            (r(TOKEN_PHRASE), |c, _| {
                let key = format!("{}{}", cap(c, 2), cap(c, 3));
                token_block(&key)?;
                Some(format!("create {} \"{key}\"", number(cap(c, 1))?))
            }),
            // "Its controller creates a Treasure token", "Each player
            // creates a Treasure token".
            (
                r(
                    r"^(Its controller|Each player|Each opponent) creates (\S+) (Treasure|Food|Clue|Blood|Map|Powerstone) tokens?$",
                ),
                |c, t| {
                    let who = recipient(&lowercase_first(cap(c, 1)), t)?;
                    Some(format!(
                        "{who} creates {} \"{}\"",
                        number(cap(c, 2))?,
                        cap(c, 3)
                    ))
                },
            ),
            // "Create a Wicked Role token attached to up to one target
            // creature you control" (CR 111.10j-r, 303.7).
            (
                r(
                    r"^[Cc]reate an? (Cursed|Monster|Royal|Sorcerer|Virtuous|Wicked|Young Hero) Role token attached to (.+)$",
                ),
                |c, t| {
                    let to = match cap(c, 2) {
                        "up to one target creature you control" => {
                            *t += 1;
                            "up to 1 target creature you control".to_string()
                        }
                        "another target creature you control" => {
                            *t += 1;
                            "another target creature you control".to_string()
                        }
                        "up to one other target creature you control" => {
                            *t += 1;
                            "up to 1 another target creature you control".to_string()
                        }
                        other => recipient(other, t)?,
                    };
                    Some(format!("create 1 \"{} Role\" attached to {to}", cap(c, 1)))
                },
            ),
            (r(r"^Mill (\S+) cards?$"), |c, _| {
                Some(format!("mill {} cards", number(cap(c, 1))?))
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
            (r(r"^Investigate$"), |_, _| Some("investigate".to_string())),
            (
                r(
                    r"^(?:~|this|It|it|This creature) deals X damage to (.+?), where X is ((?:\S+ plus )?the number of .+)$",
                ),
                |c, t| {
                    let to = recipient(cap(c, 1), t)?;
                    let x = cap(c, 2);
                    Some(format!(
                        "deal count where {} damage to {to}",
                        count_expr(x.strip_prefix("the number of ").unwrap_or(x))?
                    ))
                },
            ),
            (
                r(r"^(?:~|this|It|it) deals damage to (.+?) equal to the number of (.+)$"),
                |c, t| {
                    let to = recipient(cap(c, 1), t)?;
                    Some(format!(
                        "deal count equal {} damage to {to}",
                        count_expr(cap(c, 2))?
                    ))
                },
            ),
            (
                r(r"^(.+?) gets? ([+-]\d+/[+-]\d+) until end of turn for each (.+)$"),
                |c, t| {
                    let who = recipient(&subject(cap(c, 1)), t)?;
                    Some(format!(
                        "{who} gets {} count each {}",
                        cap(c, 2),
                        count_expr(cap(c, 3))?
                    ))
                },
            ),
            (
                r(r"^(.+?) gets? \+X/\+X until end of turn, where X is the number of (.+)$"),
                |c, t| {
                    let who = recipient(&subject(cap(c, 1)), t)?;
                    Some(format!(
                        "{who} gets +1/+1 count where {}",
                        count_expr(cap(c, 2))?
                    ))
                },
            ),
            (
                r(r"^(.+?) gets? \+X/\+0 until end of turn, where X is the number of (.+)$"),
                |c, t| {
                    let who = recipient(&subject(cap(c, 1)), t)?;
                    Some(format!(
                        "{who} gets +1/+0 count where {}",
                        count_expr(cap(c, 2))?
                    ))
                },
            ),
            (
                r(r"^(.+?) gains? life equal to the number of (.+)$"),
                |c, t| {
                    let who = recipient(&subject(cap(c, 1)), t)?;
                    Some(format!(
                        "{who} gains count equal {} life",
                        count_expr(cap(c, 2))?
                    ))
                },
            ),
            (r(r"^Draw cards equal to the number of (.+)$"), |c, _| {
                Some(format!("draw count equal {} cards", count_expr(cap(c, 1))?))
            }),
            (r(r"^Draw a card for each (.+)$"), |c, _| {
                Some(format!("draw count each {} cards", count_expr(cap(c, 1))?))
            }),
            (
                r(r"^(.+?) sacrifices? (a|an|one|two|three|four|five) (.+?) of their choice$"),
                |c, t| {
                    let who = recipient(&subject(cap(c, 1)), t)?;
                    if who == "you" || who == "this" {
                        return None;
                    }
                    let n = number(cap(c, 2))?;
                    Some(format!("{who} sacrifices {n} {}", singular(cap(c, 3))))
                },
            ),
            (
                r(r"^Put (.+?) on (top|the bottom) of its owner's library$"),
                |c, t| {
                    let what = recipient(cap(c, 1), t)?;
                    let place = if cap(c, 2) == "top" { "top" } else { "bottom" };
                    Some(format!("put {what} on {place} of library"))
                },
            ),
            (
                r(r"^(.+?) loses ([a-z ,]+?)(?: until end of turn)?$"),
                |c, t| {
                    if cap(c, 2).ends_with(" life") || cap(c, 2).contains("abilities") {
                        return None;
                    }
                    let who = recipient(&subject(cap(c, 1)), t)?;
                    Some(format!("{who} loses {}", keyword_list(cap(c, 2))?))
                },
            ),
            (r(r"^(.+?) reveals (?:their|his or her) hand$"), |c, t| {
                let who = recipient(&subject(cap(c, 1)), t)?;
                Some(format!("{who} reveals hand"))
            }),
            (r(r"^(.+?) discards? (\S+) cards? at random$"), |c, t| {
                let who = recipient(&subject(cap(c, 1)), t)?;
                Some(format!(
                    "{who} discards {} cards at random",
                    number(cap(c, 2))?
                ))
            }),
            (
                r(r"^[Tt]ransform (this creature|this artifact|this permanent|it)$"),
                |_, _| Some("transform this".to_string()),
            ),
            (
                r(
                    r"^Search your library for (a|an|up to \S+) (basic land|land|Plains|Island|Swamp|Mountain|Forest|creature|artifact|enchantment|instant|sorcery)? ?cards?, (reveal (?:it|them), )?(?:put (it|them|that card) (into your hand|onto the battlefield tapped|onto the battlefield), then shuffle|then shuffle and put that card on top)$",
                ),
                |c, _| {
                    let n = cap(c, 1);
                    let (up_to, count) = match n.strip_prefix("up to ") {
                        Some(k) => ("up to ", number(k)?),
                        None => ("", "1".to_string()),
                    };
                    let mut t = format!("search {up_to}");
                    if count != "1" || !up_to.is_empty() {
                        let _ = write!(t, "{count} ");
                    }
                    if !cap(c, 2).is_empty() {
                        let _ = write!(t, "{} ", cap(c, 2));
                    }
                    t.push_str("card");
                    if !cap(c, 3).is_empty() {
                        t.push_str(" reveal");
                    }
                    t.push_str(match cap(c, 5) {
                        "into your hand" => " to hand",
                        "onto the battlefield tapped" => " to battlefield tapped",
                        "onto the battlefield" => " to battlefield",
                        _ => " to top",
                    });
                    // "then shuffle and put that card on top".
                    if cap(c, 4) == "that card" || cap(c, 5).is_empty() {
                        t.push_str(" that card");
                    }
                    Some(t)
                },
            ),
            (r(r"^Add ((?:\{[WUBRGC]\})+)$"), |c, _| {
                Some(format!("add {}", cap(c, 1)))
            }),
            (r(r"^Add (\{[WUBRGC]\}) or (\{[WUBRGC]\})$"), |c, _| {
                Some(format!("add {} or {}", cap(c, 1), cap(c, 2)))
            }),
            (
                r(r"^Add ((?:\{[WUBRGC]\}, )+)or (\{[WUBRGC]\})$"),
                |c, _| {
                    let syms: Vec<&str> = cap(c, 1)
                        .split(", ")
                        .filter(|s| !s.is_empty())
                        .chain([cap(c, 2)])
                        .collect();
                    Some(format!("add {}", syms.join(" or ")))
                },
            ),
            (r(r"^Add (\{[WUBRGC]\}) for each (.+)$"), |c, _| {
                Some(format!(
                    "add {} count each {}",
                    cap(c, 1),
                    count_expr(cap(c, 2))?
                ))
            }),
            (r(r"^Proliferate$"), |_, _| Some("proliferate".to_string())),
            (r(r"^[Dd]iscover (\d+)$"), |c, _| {
                Some(format!("discover {}", cap(c, 1)))
            }),
            (r(r"^Adapt (\d+)$"), |c, _| {
                Some(format!("adapt {}", cap(c, 1)))
            }),
            (r(r"^[Bb]olster (\d+)$"), |c, _| {
                Some(format!("bolster {}", cap(c, 1)))
            }),
            (r(r"^[Ss]upport (\d+)$"), |c, _| {
                Some(format!("support {}", cap(c, 1)))
            }),
            (r(r"^[Ii]ncubate (\d+)$"), |c, _| {
                Some(format!("incubate {}", cap(c, 1)))
            }),
            (r(r"^[Pp]opulate$"), |_, _| Some("populate".to_string())),
            (r(r"^(It|This creature|~) connives$"), |_, t| {
                (*t == 0).then(|| "this connives".to_string())
            }),
            (r(r"^(.+) connives$"), |c, t| {
                Some(format!(
                    "{} connives",
                    recipient(&lowercase_first(cap(c, 1)), t)?
                ))
            }),
            (
                r(r"^Return an? ([a-z ]+? you control) to its owner's hand$"),
                |c, _| Some(format!("return 1 {} chosen", cap(c, 1))),
            ),
            (r(r"^Attach (?:it|this Equipment) to (.+)$"), |c, t| {
                Some(format!("attach to {}", recipient(cap(c, 1), t)?))
            }),
            (
                r(
                    r"^Look at the top (\w+) cards of your library, then put them back in any order$",
                ),
                |c, _| Some(format!("look at top {}", number(cap(c, 1))?)),
            ),
            (r(r"^Manifest the top card of your library$"), |_, _| {
                Some("manifest".to_string())
            }),
            (
                r(r"^If (that creature|it) would die this turn, exile it instead$"),
                |_, t| (*t > 0).then(|| format!("this turn, replace target {t} dies: exile")),
            ),
            (
                r(r"^Draw (a|two|three) cards? at the beginning of the next turn's upkeep$"),
                |c, _| {
                    Some(format!(
                        "delayed this: draw {} cards at beginning of the next turn's upkeep",
                        number(cap(c, 1))?
                    ))
                },
            ),
            (
                r(r"^(.+) doesn't untap during its controller's next untap step$"),
                |c, t| {
                    let who = recipient(&lowercase_first(cap(c, 1)), t)?;
                    Some(format!("{who} doesn't untap next untap step"))
                },
            ),
            (
                r(r"^Switch (.+)'s power and toughness until end of turn$"),
                |c, t| Some(format!("switch pt of {}", recipient(cap(c, 1), t)?)),
            ),
            (
                r(r"^Put an? (.+) card from your hand onto the battlefield( tapped)?$"),
                |c, _| {
                    Some(format!(
                        "put {} card from hand onto battlefield{}",
                        singular(cap(c, 1)),
                        cap(c, 2)
                    ))
                },
            ),
            (r(r"^Amass (\w+) (\w+)$"), |c, _| {
                Some(format!("amass {} {}", cap(c, 1), number(cap(c, 2))?))
            }),
            (r(r"^(It|This creature|~) explores$"), |_, t| {
                (*t == 0).then(|| "this explores".to_string())
            }),
            (r(r"^(.+) gets? (a|\w+) poison counters?$"), |c, t| {
                Some(format!(
                    "{} gets {} poison counters",
                    recipient(&subject(cap(c, 1)), t)?,
                    number(cap(c, 2))?
                ))
            }),
            (r(r"^Pay ((?:\{[0-9WUBRGC]+\})+|\d+ life)$"), |c, _| {
                Some(format!("pay {}", cap(c, 1)))
            }),
            (r(r"^Pay ((?:\{E\})+)$"), |c, _| {
                Some(format!("pay {} energy", cap(c, 1).len() / 3))
            }),
            (
                r(r"^[Yy]ou gain (\d+) life and get ((?:\{E\})+)$"),
                |c, _| {
                    Some(format!(
                        "you gain {} life and you get {} energy",
                        cap(c, 1),
                        cap(c, 2).len() / 3
                    ))
                },
            ),
            (r(r"^(?:[Yy]ou )?[Gg]et ((?:\{E\})+)$"), |c, _| {
                Some(format!("you get {} energy", cap(c, 1).len() / 3))
            }),
            (r(r"^Add one mana of any color$"), |_, _| {
                Some("add any".to_string())
            }),
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
            (
                r(r"^(.+?) deals damage to itself equal to its power$"),
                |c, t| {
                    let a = recipient(&subject(cap(c, 1)), t)?;
                    Some(format!("{a} bites itself"))
                },
            ),
            (
                r(r"^(.+?) deals damage equal to its power to (.+)$"),
                |c, t| {
                    let a = recipient(&subject(cap(c, 1)), t)?;
                    let b = recipient(cap(c, 2), t)?;
                    Some(format!("{a} bites {b}"))
                },
            ),
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
            (
                r(
                    r"^Put (\S+) ([+-]1/[+-]1|stun|charge|oil|spore|depletion|quest|ki|time|tide|shield|age|fade|ice|level|lore|page|flying|first strike|trample|lifelink|deathtouch|reach|menace|vigilance|hexproof|indestructible) counters? on (.+)$",
                ),
                |c, t| {
                    Some(format!(
                        "put {} {} counters on {}",
                        number(cap(c, 1))?,
                        cap(c, 2),
                        recipient(cap(c, 3), t)?
                    ))
                },
            ),
            (
                r(
                    r"^Remove (\S+) ([+-]1/[+-]1|stun|charge|oil|spore|depletion|quest|ki|time|tide|shield|age|fade|ice|level|lore|page|flying|first strike|trample|lifelink|deathtouch|reach|menace|vigilance|hexproof|indestructible) counters? from (.+)$",
                ),
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
                        "players" => " to each player".to_string(),
                        "you and creatures you control" => {
                            " to you and creature you control".to_string()
                        }
                        _ => return None,
                    };
                    Some(format!("prevent all {kind}damage{to} this turn"))
                },
            ),
            (
                r(r"^Prevent all (combat )?damage that would be dealt by (target .+?) this turn$"),
                |c, t| {
                    let kind = if cap(c, 1).is_empty() { "" } else { "combat " };
                    Some(format!(
                        "prevent all {kind}damage from {} this turn",
                        recipient(cap(c, 2), t)?
                    ))
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
            // CR 615.7, 609.7a: Circle of Protection.
            (
                r(
                    r"^The next time an? (white |blue |black |red |green )?source of your choice would deal damage( to you)? this turn, prevent that damage$",
                ),
                |c, _| {
                    let to = if cap(c, 2).is_empty() { "" } else { " to you" };
                    Some(format!(
                        "this turn, replace damage from chosen {}source{to}: prevent next time",
                        cap(c, 1)
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
        // The first rule that matches and makes something; a rule that
        // matches but declines leaves the next one a chance.
        for (re, f) in &self.rules {
            if let Some(c) = re.captures(s) {
                let before = *targets;
                if let Some(out) = f(&c, targets) {
                    return Some(out);
                }
                *targets = before;
            }
        }
        None
    }

    /// The dig paragraphs ("Look at the top N cards of your library. ...
    /// Put the rest ..."), with whatever follows authored as usual.
    fn dig(&self, text: &str, targets: &mut usize) -> Result<Option<String>, Skip> {
        let text = capitalize(text);
        let rest_of = |s: &str| -> &'static str {
            if s.contains("graveyard") {
                "graveyard"
            } else {
                "bottom"
            }
        };
        let may = Regex::new(
            r"^(Look at|Reveal) the top (\w+) cards of your library\. You may (reveal|put) (an? .+?) card from among them (?:and put it )?into your hand\. Put the rest (on the bottom of your library(?: in any order)?|into your graveyard)\.?\s*(.*)$",
        )
        .unwrap();
        let one = Regex::new(
            r"^Look at the top (\w+) cards of your library\. Put one of them into your hand and the (?:rest|other) (on the bottom of your library(?: in any order)?|into your graveyard)\.?\s*(.*)$",
        )
        .unwrap();
        let all = Regex::new(
            r"^Reveal the top (\w+) cards of your library\. Put all (.+?) cards revealed this way into your hand and the rest (into your graveyard|on the bottom of your library in any order)\.?\s*(.*)$",
        )
        .unwrap();
        let (dsl, tail) = if let Some(c) = may.captures(&text) {
            let n = number(cap(&c, 2)).ok_or_else(|| Skip::Sentence(text.clone()))?;
            let reveal_top = if cap(&c, 1) == "Reveal" {
                " reveal"
            } else {
                ""
            };
            let revealing = if cap(&c, 3) == "reveal" {
                " revealing"
            } else {
                ""
            };
            let what = cap(&c, 4)
                .trim_start_matches("an ")
                .trim_start_matches("a ")
                .to_string();
            (
                format!(
                    "dig {n}{reveal_top} take {what} card may{revealing} rest {}",
                    rest_of(cap(&c, 5))
                ),
                cap(&c, 6).to_string(),
            )
        } else if let Some(c) = one.captures(&text) {
            let n = number(cap(&c, 1)).ok_or_else(|| Skip::Sentence(text.clone()))?;
            (
                format!("dig {n} take card rest {}", rest_of(cap(&c, 2))),
                cap(&c, 3).to_string(),
            )
        } else if let Some(c) = all.captures(&text) {
            let n = number(cap(&c, 1)).ok_or_else(|| Skip::Sentence(text.clone()))?;
            (
                format!(
                    "dig {n} reveal take all {} card rest {}",
                    singular(cap(&c, 2)),
                    rest_of(cap(&c, 3))
                ),
                cap(&c, 4).to_string(),
            )
        } else {
            return Ok(None);
        };
        if tail.trim().is_empty() {
            return Ok(Some(dsl));
        }
        let more = self.effects(tail.trim(), targets)?;
        Ok(Some(format!("{dsl}; {more}")))
    }

    /// Sentences of one paragraph to `;`-joined DSL effects. `targets`
    /// counts targets declared so far in the ability.
    #[allow(clippy::too_many_lines)] // the sentence forms that need more than one rule
    fn effects(&self, text: &str, targets: &mut usize) -> Result<String, Skip> {
        // "Target opponent reveals their hand. You choose a nonland card
        // from it. That player discards that card." is one effect.
        let duress = Regex::new(
            r"^(.+?) reveals their hand\. You choose (a|an) (.+? )?card from it\. That player discards that card\.?(.*)$",
        )
        .unwrap();
        if let Some(c) = duress.captures(text) {
            let who = recipient(&subject(cap(&c, 1)), targets)
                .ok_or_else(|| Skip::Sentence(text.to_string()))?;
            let what = cap(&c, 3).trim();
            let what = what.replace(", ", " ");
            let mut out = if what.is_empty() {
                format!("{who} reveals hand discarding card")
            } else {
                format!("{who} reveals hand discarding {what} card")
            };
            let rest = cap(&c, 4).trim();
            if !rest.is_empty() {
                out.push_str("; ");
                out.push_str(&self.effects(rest, targets)?);
            }
            return Ok(out);
        }
        let unless = Regex::new(
            r"^(.+) unless (you pay|they pay|its controller pays|that player pays) ((?:\{[0-9WUBRGC]+\})+|\d+ life)$",
        )
        .unwrap();
        // "Look at the top four cards of your library. You may reveal a
        // creature card from among them ..." and the other dig forms.
        if let Some(out) = self.dig(text, targets)? {
            return Ok(out);
        }
        // "Target creature gains \"...\" until end of turn."
        if let Some(c) = Regex::new(r#"^(.+?) gains? "(.+)" until end of turn\.?$"#)
            .unwrap()
            .captures(text)
        {
            let quoted = Builder {
                templates: self,
                corpus: &[],
                is_spell: false,
                statements: Vec::new(),
                keywords: Vec::new(),
                additional: None,
                spell_paragraphs: Vec::new(),
                spell_at: None,
                spell_targets: 0,
            }
            .quoted(cap(&c, 2));
            let who = recipient(&lowercase_first(cap(&c, 1)), targets);
            return match (quoted, who) {
                (Some(inner), Some(who)) => Ok(format!("{who} gains ability {{ {inner} }}")),
                _ => Err(Skip::Sentence(text.to_string())),
            };
        }
        let manifest_then = Regex::new(
            r"^Manifest the top card of your library, then put (a|two|three) \+1/\+1 counters? on it$",
        )
        .unwrap();
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
            let (prefix, body) = if let Some(rest) = sentence
                .strip_prefix("you may ")
                .or_else(|| sentence.strip_prefix("You may "))
            {
                ("you may ", rest.to_string())
            } else if let Some(rest) = sentence.strip_prefix("If you do, ") {
                ("if you do, ", rest.to_string())
            } else if let Some(rest) = sentence.strip_prefix("If this spell was kicked, ") {
                ("if kicked, ", rest.to_string())
            } else {
                ("", sentence.clone())
            };
            // "Manifest the top card of your library, then put two +1/+1
            // counters on it": "it" is the manifested creature.
            if let Some(c) = manifest_then.captures(&body) {
                let n = number(cap(&c, 1)).ok_or_else(|| Skip::Sentence(sentence.clone()))?;
                out.push(format!(
                    "{prefix}manifest then put {n} +1/+1 counters on that card"
                ));
                continue;
            }
            // "Draw a card, then discard a card": clauses joined by "then".
            let mut parts = Vec::new();
            let clauses: Vec<&str> = if body.contains("earch your library") {
                vec![body.as_str()]
            } else {
                body.split(", then ").collect()
            };
            for clause in clauses {
                let (head, tail) = match unless.captures(clause) {
                    Some(c) => (
                        c.get(1).map_or("", |m| m.as_str()).to_string(),
                        format!(" unless {} {}", cap(&c, 2), cap(&c, 3)),
                    ),
                    None => (clause.to_string(), String::new()),
                };
                // "create a token, then attach this Equipment to it": "it"
                // is the token.
                if parts
                    .last()
                    .is_some_and(|p: &String| p.starts_with("create "))
                    && matches!(
                        head.as_str(),
                        "attach this Equipment to it" | "attach it to it"
                    )
                {
                    parts.push("attach to that token".to_string());
                    continue;
                }
                let Some(e) = self.sentence(&capitalize(&head), targets) else {
                    return Err(Skip::Sentence(sentence.clone()));
                };
                parts.push(format!("{e}{tail}"));
            }
            let e = parts.join(" then ");
            out.push(format!("{prefix}{e}"));
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
    let remove = Regex::new(
        r"^Remove (an?|two|three|\w+) ([a-z+/1-]+(?: strike)?) counters? from this (?:artifact|creature|land|enchantment|permanent)$",
    )
    .unwrap();
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
                | "Sacrifice this land"
                | "Sacrifice this Aura"
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
        } else if let Some(c) = remove.captures(part) {
            let n = if matches!(cap(&c, 1), "a" | "an") {
                "1".to_string()
            } else {
                number(cap(&c, 1))?
            };
            format!("remove {n} {} counters from this", cap(&c, 2))
        } else if part == "Exile this card from your graveyard" {
            "exile this from graveyard".to_string()
        } else if let Some(rest) = part
            .strip_prefix("Return an ")
            .or_else(|| part.strip_prefix("Return a "))
            .and_then(|r| r.strip_suffix(" to its owner's hand"))
        {
            format!("return {rest}")
        } else if let Some(rest) = part
            .strip_prefix("Tap an ")
            .or_else(|| part.strip_prefix("Tap a "))
        {
            format!("tap {rest}")
        } else if let Some(what) = part
            .strip_prefix("Discard a ")
            .or_else(|| part.strip_prefix("Discard an "))
            .and_then(|r| r.strip_suffix(" card"))
            .filter(|w| !w.contains(' '))
        {
            // "Discard a creature card": one card matching a filter.
            format!("discard {what} card")
        } else {
            let rest = part.strip_prefix("Pay ")?;
            if !rest.is_empty() && rest.len() % 3 == 0 && rest.chars().all(|c| "{E}".contains(c)) {
                format!("pay {} energy", rest.len() / 3)
            } else {
                let n = rest.strip_suffix(" life")?;
                format!("pay {n} life")
            }
        };
        items.push(item);
    }
    Some(items.join(", "))
}

#[allow(clippy::too_many_lines)] // one arm per trigger phrase
fn trigger_text(head: &str) -> Option<String> {
    // A Vehicle or Equipment names itself so; the DSL's `this` prints the
    // noun.
    let head = head
        .replace("this Vehicle", "this creature")
        .replace("this Equipment", "this artifact");
    let head = head.as_str();
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
        "At the beginning of each upkeep" => "at beginning of every upkeep",
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
        "Whenever a land you control enters" => "whenever land you control enters",
        "Whenever you cast a noncreature spell" => "whenever you cast a noncreature spell",
        "Whenever you cast an instant or sorcery spell" => {
            "whenever you cast an instant or sorcery spell"
        }
        "Whenever you cast a spell" => "whenever you cast a spell",
        "Whenever a player casts a spell" => "whenever a player casts a spell",
        // Heroic (the ability word is a label the caller keeps).
        "Whenever you cast a spell that targets this creature" => {
            "whenever you cast a spell that targets this"
        }
        "Whenever this creature becomes the target of a spell or ability an opponent controls" => {
            "whenever this becomes targeted by opponent"
        }
        "Whenever this creature becomes the target of a spell or ability you control for the first time each turn" => {
            "whenever this becomes targeted by you first time each turn"
        }
        "When this creature becomes the target of a spell or ability" => {
            "when this becomes targeted"
        }
        "Whenever this creature becomes blocked" => "whenever this becomes blocked",
        "Whenever this creature blocks or becomes blocked" => {
            "whenever this blocks or becomes blocked"
        }
        "Whenever this creature becomes the target of a spell or ability" => {
            "whenever this becomes targeted"
        }
        "Whenever this creature becomes tapped" => "whenever this becomes tapped",
        "When this creature is turned face up" => "when this is turned face up",
        "Whenever a permanent is turned face up" => "whenever permanent is turned face up",
        "Whenever this creature or another permanent is turned face up" => {
            "whenever this or permanent is turned face up"
        }
        "When this creature enters or dies"
        | "When this artifact enters or is put into a graveyard from the battlefield" => {
            "when this enters or dies"
        }
        "Whenever this creature enters or dies" => "whenever this enters or dies",
        "Whenever this creature enters or attacks" => "whenever this enters or attacks",
        "When this creature enters or leaves the battlefield"
        | "When this artifact enters or leaves the battlefield" => {
            "when this enters or leaves the battlefield"
        }
        "When this creature enters or is turned face up" => "when this enters or is turned face up",
        "When this creature exploits a creature" => "when this exploits a creature",
        "Whenever this creature becomes untapped" => "whenever this becomes untapped",
        "Whenever this creature is dealt damage" => "whenever this is dealt damage",
        "Whenever this creature deals damage" => "whenever this deals damage",
        "Whenever this creature deals damage to a creature" => {
            "whenever this deals damage to creature"
        }
        "Whenever this creature deals damage to a player" => "whenever this deals damage to player",
        "Whenever this creature deals damage to an opponent" => {
            "whenever this deals damage to opponent"
        }
        "Whenever this creature deals combat damage to a creature" => {
            "whenever this deals combat damage to creature"
        }
        "When you cycle this card" => "when you cycle this",
        "Whenever you cycle a card"
        | "Whenever you cycle another card"
        | "Whenever you cycle or discard a card"
        | "Whenever you cycle or discard another card"
        | "Whenever you cast your first spell each turn"
        | "Whenever you cast your second spell each turn"
        | "Whenever you cast a spell other than your first spell each turn"
        | "Whenever you cast your first spell during each opponent's turn"
        | "Whenever you draw a card"
        | "Whenever a player draws a card"
        | "Whenever an opponent draws a card"
        | "Whenever you discard a card"
        | "Whenever a player discards a card"
        | "Whenever an opponent discards a card"
        | "Whenever one or more cards leave your graveyard"
        | "Whenever one or more creature cards leave your graveyard"
        | "Whenever you expend 4"
        | "Whenever you expend 8" => {
            return Some(lowercase_first(head));
        }
        "Whenever a land card is put into your graveyard from anywhere" => {
            "whenever land card is put into your graveyard from anywhere"
        }
        "Whenever a creature enters from your graveyard" => {
            "whenever creature enters from your graveyard"
        }
        _ => {
            // "When you control no Islands" (a state trigger, CR 603.8).
            let none = Regex::new(
                r"^When you control no (other creatures|Plains|Islands|Swamps|Mountains|Forests)$",
            )
            .unwrap();
            if let Some(c) = none.captures(head) {
                let what = match cap(&c, 1) {
                    "other creatures" => "other creature",
                    "Plains" => "Plains",
                    w => w.trim_end_matches('s'),
                };
                return Some(format!("when you control no {what}"));
            }
            // "Whenever you cast a spell with mana value 4 or greater".
            let mv =
                Regex::new(r"^Whenever you cast a spell with mana value (\d+) or (greater|less)$")
                    .unwrap();
            if let Some(c) = mv.captures(head) {
                return Some(format!(
                    "whenever you cast with mana value {} or {} spell",
                    cap(&c, 1),
                    cap(&c, 2)
                ));
            }
            // "Whenever this creature or another Ally you control enters".
            let this_or = Regex::new(
                r"^Whenever this creature or another (creature|Ally|artifact|enchantment) you control enters$",
            )
            .unwrap();
            if let Some(c) = this_or.captures(head) {
                return Some(format!(
                    "whenever this or {} you control enters",
                    cap(&c, 1)
                ));
            }
            // "Whenever this creature blocks a creature with flying",
            // "... becomes blocked by a creature".
            let pair = Regex::new(
                r"^Whenever this creature (blocks|becomes blocked by|blocks or becomes blocked by) (a creature|a creature with flying|a nonblack creature)$",
            )
            .unwrap();
            if let Some(c) = pair.captures(head) {
                return Some(format!("whenever this {} {}", cap(&c, 1), cap(&c, 2)));
            }
            // "Whenever you cast an artifact spell", "Whenever a player
            // casts a multicolored spell".
            let re = Regex::new(
                r"^Whenever (you cast|a player casts|an opponent casts) an? (.+) spell$",
            )
            .unwrap();
            let c = re.captures(head)?;
            let who = match cap(&c, 1) {
                "you cast" => "you cast",
                "a player casts" => "a player casts",
                _ => "an opponent casts",
            };
            let what = cap(&c, 2).replace(" and ", " or ");
            return Some(format!("whenever {who} {what} spell"));
        }
    };
    Some(t.to_string())
}

/// "seven or more cards are in your graveyard" → `7 cards in your
/// graveyard`, and the other conditions `static_cond` parses.
fn static_condition(s: &str) -> Option<String> {
    match s {
        "you attacked this turn" => return Some("attacked this turn".to_string()),
        "a creature died this turn" => return Some("creature died this turn".to_string()),
        "you have no cards in hand" => return Some("hand 0 or less".to_string()),
        "it's your turn" => return Some("your turn".to_string()),
        "it's attacking" | "this creature is attacking" => return Some("attacking".to_string()),
        "it's enchanted" | "this creature is enchanted" => return Some("enchanted".to_string()),
        "it's equipped" | "this creature is equipped" => return Some("equipped".to_string()),
        "this spell was cast using teamwork" => {
            return Some("teamwork".to_string());
        }
        "it's bargained" | "it was bargained" | "this spell was bargained" => {
            return Some("bargained".to_string());
        }
        _ => {}
    }
    if let Some(c) = Regex::new(r"^a player has (\d+) or less life$")
        .unwrap()
        .captures(s)
    {
        return Some(format!("player life {} or less", cap(&c, 1)));
    }
    if let Some(c) = Regex::new(r"^you have (\w+) or (fewer|more) cards in hand$")
        .unwrap()
        .captures(s)
    {
        let n = number(cap(&c, 1))?;
        let dir = if cap(&c, 2) == "fewer" {
            "less"
        } else {
            "more"
        };
        return Some(format!("hand {n} or {dir}"));
    }
    if let Some(rest) = s.strip_prefix("defending player controls ") {
        let one = rest
            .strip_prefix("an ")
            .or_else(|| rest.strip_prefix("a "))?;
        return Some(format!("defending player controls {}", singular(one)));
    }
    if let Some(rest) = s.strip_prefix("you control another ") {
        return Some(format!("you control other {}", singular(rest)));
    }
    if let Some(c) = Regex::new(r"^an opponent has (\d+) or less life$")
        .unwrap()
        .captures(s)
    {
        return Some(format!("opponent life {} or less", cap(&c, 1)));
    }
    if let Some(c) = Regex::new(r"^you have (\d+) or (more|less) life$")
        .unwrap()
        .captures(s)
    {
        return Some(format!("your life {} or {}", cap(&c, 1), cap(&c, 2)));
    }
    let more = Regex::new(r"^there are (\S+) or more cards in your graveyard$").unwrap();
    if let Some(c) = more.captures(s) {
        return Some(format!("{} cards in your graveyard", number(cap(&c, 1))?));
    }
    let types =
        Regex::new(r"^there are (\S+) or more card types among cards in your graveyard$").unwrap();
    if let Some(c) = types.captures(s) {
        return Some(format!(
            "{} card types in your graveyard",
            number(cap(&c, 1))?
        ));
    }
    let control = Regex::new(r"^you control (\S+) or more ([a-zA-Z ]+)$").unwrap();
    if let Some(c) = control.captures(s) {
        let f = control_filter(&singular(cap(&c, 2)))?;
        return Some(format!("you control {} {f}", number(cap(&c, 1))?));
    }
    let one = Regex::new(r"^you control an? ([a-zA-Z ]+)$").unwrap();
    if let Some(c) = one.captures(s) {
        return Some(format!("you control {}", control_filter(cap(&c, 1))?));
    }
    None
}

/// Filters a "you control" condition may name.
fn control_filter(s: &str) -> Option<String> {
    const OK: [&str; 13] = [
        "artifact",
        "creature",
        "enchantment",
        "land",
        "Plains",
        "Island",
        "Swamp",
        "Mountain",
        "Forest",
        "planeswalker",
        "artifact creature",
        "legendary creature",
        "Equipment",
    ];
    if OK.contains(&s) {
        return Some(s.to_string());
    }
    // "a Treefolk", "an Otter": one subtype.
    if !s.contains(' ') && s.chars().next().is_some_and(char::is_uppercase) {
        return Some(s.to_string());
    }
    // "a creature with flying".
    if let Some(k) = s.strip_prefix("creature with ")
        && KEYWORDS.iter().any(|kw| kw.eq_ignore_ascii_case(k))
    {
        return Some(format!("creature with {k}"));
    }
    None
}

/// The modifications of a static ability's predicate ("gets +1/+1 and
/// has flying").
fn static_mods(s: &str) -> Option<String> {
    let gets = Regex::new(r"^gets? ([+-]\d+/[+-]\d+)$").unwrap();
    let per = Regex::new(r"^gets? ([+-]\d+/[+-]\d+) for each (.+)$").unwrap();
    let mut mods = Vec::new();
    for part in s.split(" and ") {
        if let Some(c) = per.captures(part) {
            mods.push(format!(
                "{} count each {}",
                cap(&c, 1),
                count_expr(cap(&c, 2))?
            ));
        } else if let Some(c) = gets.captures(part) {
            mods.push(cap(&c, 1).to_string());
        } else if let Some(rest) = part
            .strip_prefix("has ")
            .or_else(|| part.strip_prefix("have "))
        {
            mods.push(format!("has {}", keyword_list(rest)?));
        } else if RESTRICTIONS.contains(&part) {
            mods.push(format!("has {part}"));
        } else {
            return None;
        }
    }
    (!mods.is_empty()).then(|| mods.join(", "))
}

/// "As long as <cond>, this creature gets +1/+1.", "This creature gets
/// +2/+2 as long as <cond>.", "During your turn, this creature has first
/// strike."
fn conditional_static(line: &str) -> Option<String> {
    let line = line.strip_suffix('.')?;
    let subject = |s: &str| -> Option<&'static str> {
        Some(match s {
            "this creature" | "This creature" | "this permanent" | "This permanent" => "this",
            "creatures you control" | "Creatures you control" => "creature you control",
            "other creatures you control" | "Other creatures you control" => {
                "other creature you control"
            }
            "enchanted creature" | "Enchanted creature" | "equipped creature"
            | "Equipped creature" => "enchanted",
            _ => return None,
        })
    };
    let split_subject = |s: &str| -> Option<(&'static str, String)> {
        for n in [4, 3, 2] {
            let words: Vec<&str> = s.splitn(n + 1, ' ').collect();
            if words.len() == n + 1
                && let Some(f) = subject(&words[..n].join(" "))
            {
                return Some((f, words[n].to_string()));
            }
        }
        None
    };
    if let Some(rest) = line.strip_prefix("During your turn, ") {
        let (f, pred) = split_subject(rest)?;
        return Some(format!(
            "static during your turn, {f}: {}",
            static_mods(&pred)?
        ));
    }
    if let Some(rest) = line.strip_prefix("As long as ") {
        let (cond, body) = rest.split_once(", ")?;
        let cond = static_condition(cond)?;
        let (f, pred) = split_subject(body)?;
        return Some(format!(
            "static as long as {cond}, {f}: {}",
            static_mods(&pred)?
        ));
    }
    let Some((body, cond)) = line.split_once(" as long as ") else {
        // A plain static with a "for each" count.
        if !line.contains(" for each ") {
            return None;
        }
        let (f, pred) = split_subject(line)?;
        return Some(format!("static {f}: {}", static_mods(&pred)?));
    };
    let cond = static_condition(cond)?;
    let (f, pred) = split_subject(body)?;
    Some(format!(
        "static {f}: {} as long as {cond}",
        static_mods(&pred)?
    ))
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
    /// How many statements come before the `spell` block (cycling after a
    /// spell's text prints after it).
    spell_at: Option<usize>,
    spell_targets: usize,
}

impl Builder<'_> {
    /// An ability quoted inside another's text, authored on its own: the
    /// one statement it makes.
    fn quoted(&self, text: &str) -> Option<String> {
        let mut inner = Builder {
            templates: self.templates,
            corpus: self.corpus,
            is_spell: false,
            statements: Vec::new(),
            keywords: Vec::new(),
            additional: None,
            spell_paragraphs: Vec::new(),
            spell_at: None,
            spell_targets: 0,
        };
        // A keyword alone ("has \"Flying\"") is not an ability statement.
        inner.statements.push(String::new());
        inner.line(text).ok()?;
        let made: Vec<String> = inner.statements.into_iter().skip(1).collect();
        match made.as_slice() {
            [one] if !one.contains('\n') => Some(one.trim().to_string()),
            _ => None,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn line(&mut self, line: &str) -> Result<(), Skip> {
        // "Landfall — <ability>": the label prefixes the one statement the
        // rest makes.
        if let Some((label, rest)) = line.split_once(" — ")
            && ABILITY_WORDS.contains(&label)
        {
            let before = self.statements.len();
            let paragraphs = self.spell_paragraphs.len();
            self.line(rest)?;
            if self.statements.len() != before + 1 || self.spell_paragraphs.len() != paragraphs {
                return Err(Skip::Sentence(line.to_string()));
            }
            let st = self.statements.pop().unwrap_or_default();
            self.statements.push(format!("label \"{label}\" {st}"));
            return Ok(());
        }
        if let Some(st) = conditional_static(line) {
            self.statements.push(st);
            return Ok(());
        }
        // "Enchanted creature has \"{T}: ...\"", "Creatures you control
        // have \"...\"", "Equipped creature gets +1/+1 and has \"...\"".
        if let Some(c) = Regex::new(
            r#"^(Enchanted creature|Equipped creature|Enchanted land|Enchanted permanent|Creatures you control|Other creatures you control|Lands you control|This creature) (?:(gets [+-]\d+/[+-]\d+) and )?(?:has|have) "(.+)"$"#,
        )
        .unwrap()
        .captures(line)
        {
            let filter = match cap(&c, 1) {
                "Enchanted creature" | "Equipped creature" | "Enchanted land" | "Enchanted permanent" => {
                    "enchanted"
                }
                "Creatures you control" => "creature you control",
                "Other creatures you control" => "other creature you control",
                "Lands you control" => "land you control",
                _ => "this",
            };
            let inner = self
                .quoted(cap(&c, 3))
                .ok_or_else(|| Skip::Sentence(line.to_string()))?;
            let pump = if cap(&c, 2).is_empty() {
                String::new()
            } else {
                format!("{}, ", cap(&c, 2).trim_start_matches("gets "))
            };
            self.statements
                .push(format!("static {filter}: {pump}has ability {{ {inner} }}"));
            return Ok(());
        }
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
        // "Flying, protection from enchantments": keywords and a quality.
        if line.contains(", ")
            && keyword_list(line).is_none()
            && line.chars().next().is_some_and(char::is_uppercase)
        {
            let parts: Vec<&str> = line.split(", ").collect();
            if parts.iter().all(|p| {
                keyword_list(p).is_some() || p.to_ascii_lowercase().starts_with("protection from ")
            }) {
                for p in parts {
                    let p = capitalize(p);
                    self.line(&p)?;
                }
                return Ok(());
            }
        }
        // "Protection from artifacts" (a quality other than a color).
        if let Some(q) = line.strip_prefix("Protection from ")
            && keyword_list(line).is_none()
            && !q.contains(" and ")
            && !q.contains(',')
        {
            let what = match q {
                "everything" => "everything".to_string(),
                "multicolored" | "monocolored" | "colorless" => q.to_string(),
                _ => singular(q),
            };
            self.statements.push(format!("protection from {what}"));
            return Ok(());
        }
        // Blocking restrictions with a filter.
        if let Some(rest) = line
            .strip_prefix("This creature ")
            .and_then(|r| r.strip_suffix('.'))
        {
            let rule = if rest == "can't be blocked by more than one creature" {
                Some("can't be blocked by more than one creature".to_string())
            } else if let Some(f) = rest.strip_prefix("can't be blocked except by ") {
                Some(format!("can't be blocked except by {}", singular(f)))
            } else if let Some(f) = rest.strip_prefix("can't be blocked by ") {
                Some(format!("can't be blocked by {}", singular(f)))
            } else if let Some(f) = rest.strip_prefix("can block only ") {
                Some(format!("can block only {}", singular(f)))
            } else if let Some(f) = rest.strip_prefix("can't block ") {
                (!RESTRICTIONS.contains(&rest)).then(|| format!("can't block {}", singular(f)))
            } else {
                None
            };
            if let Some(r) = rule {
                self.statements.push(r);
                return Ok(());
            }
        }
        // Lures (CR 509.1c): "All creatures able to block this creature do so."
        if line == "All creatures able to block this creature do so." {
            if self.statements.is_empty() {
                self.keywords.push("lure".to_string());
            } else {
                self.statements.push("keyword lure".to_string());
            }
            return Ok(());
        }
        if line == "All creatures able to block enchanted creature do so."
            || line == "All creatures able to block equipped creature do so."
        {
            self.statements
                .push("static enchanted: has lure".to_string());
            return Ok(());
        }
        // "This creature can't block." and friends are keywords to the DSL.
        if let Some(rest) = line
            .strip_prefix("This creature ")
            .or_else(|| line.strip_prefix("This permanent "))
            && let Some(rest) = rest.strip_suffix('.')
            && let Some(k) = RESTRICTIONS.iter().find(|r| **r == rest)
        {
            let k = if *k == "can't attack or block alone" {
                &"can't attack alone, can't block alone"
            } else {
                k
            };
            if self.statements.is_empty() {
                self.keywords.push((*k).to_string());
            } else {
                self.statements.push(format!("keyword {k}"));
            }
            return Ok(());
        }
        if let Some(c) = Regex::new(
            r"^(Basic land|Land|Plains|Island|Swamp|Mountain|Forest)cycling ((?:\{[0-9XWUBRGC]+\})+)$",
        )
        .unwrap()
        .captures(line)
        {
            self.statements.push(format!(
                "{}cycling {}",
                cap(&c, 1).to_lowercase(),
                cap(&c, 2)
            ));
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^Cycling ((?:\{[0-9XWUBRGC]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("cycling {}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^Kicker ((?:\{[0-9XWUBRGC]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("kicker {}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^Flashback ((?:\{[0-9XWUBRGC]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("flashback {}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^Flashback—((?:\{[0-9XWUBRGC]+\})+), Pay (\d+) life\.$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("flashback {}, pay {} life", cap(&c, 1), cap(&c, 2)));
            return Ok(());
        }
        if line == "{T}, Sacrifice this artifact: Add one mana of any color."
            || line == "{T}, Sacrifice this creature: Add one mana of any color."
        {
            self.statements.push("mana any, sacrifice this".to_string());
            return Ok(());
        }
        if line == "{T}: Add one mana of the chosen color." {
            self.statements.push("mana chosen".to_string());
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^\{T\}: Add \{([WUBRG])\} or one mana of the chosen color\.$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("mana {{{}}} or chosen", cap(&c, 1)));
            return Ok(());
        }
        // "As this land enters, choose a color." (CR 614.12), alone or after
        // "This land enters tapped." on the same line.
        if let Some(c) = Regex::new(
            r"^(This (?:land|artifact|creature|permanent) enters tapped\. )?As (?:it|this (?:land|artifact|creature|enchantment|permanent|Aura)) enters, choose a (color|creature type|basic land type)(?: other than (white|blue|black|red|green))?\.$",
        )
        .unwrap()
        .captures(line)
        {
            let who = if cap(&c, 1).is_empty() {
                "this"
            } else {
                self.statements.push("enters tapped".to_string());
                "it"
            };
            let what = cap(&c, 2).strip_prefix("basic ").unwrap_or(cap(&c, 2));
            let mut stmt = format!("as {who} enters choose {what}");
            if !cap(&c, 3).is_empty() {
                let _ = write!(stmt, " except {}", cap(&c, 3));
            }
            self.statements.push(stmt);
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
                other => {
                    cost_text(&capitalize(other)).ok_or_else(|| Skip::Sentence(line.to_string()))?
                }
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
                "tapped creature" => "tapped creature",
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
        // "{T}: Add {C}. Spend this mana only to cast an instant or
        // sorcery spell." and the other restricted mana (CR 106.6).
        if let Some(c) = Regex::new(
            r"^\{T\}: Add (\{[WUBRGC]\}|one mana of any color)\. (Spend this mana only to (?:cast an? ([A-Za-z ]+?) spell(?:( or to| or) activate an ability)?|activate an ability(?: or cast an? ([A-Za-z ]+?) spell)?)|This mana can't be spent to cast an? ([A-Za-z]+) spell)\.$",
        )
        .unwrap()
        .captures(line)
        {
            let mana = match cap(&c, 1) {
                "one mana of any color" => "any",
                m => m,
            };
            let rest = if !cap(&c, 6).is_empty() {
                format!("not to cast {} spell", cap(&c, 6))
            } else if !cap(&c, 3).is_empty() {
                let tail = match cap(&c, 4) {
                    " or to" => " or to activate",
                    " or" => " or activate",
                    _ => "",
                };
                format!("only to cast {} spell{tail}", cap(&c, 3))
            } else if !cap(&c, 5).is_empty() {
                format!("only to activate or cast {} spell", cap(&c, 5))
            } else {
                "only to activate".to_string()
            };
            self.statements.push(format!("mana {mana} {rest}"));
            return Ok(());
        }
        if line == "Spend only black mana on X." {
            self.statements
                .push("spend only black mana on x".to_string());
            return Ok(());
        }
        // "{T}: Add {R}, {G}, or {W}."
        if let Some(c) = Regex::new(r"^\{T\}: Add ((?:\{[WUBRGC]\}, )+)or (\{[WUBRGC]\})\.$")
            .unwrap()
            .captures(line)
        {
            let syms: Vec<&str> = cap(&c, 1)
                .split(", ")
                .filter(|s| !s.is_empty())
                .chain([cap(&c, 2)])
                .collect();
            self.statements.push(format!("mana {}", syms.join(" or ")));
            return Ok(());
        }
        if let Some(c) =
            Regex::new(r"^This (land|creature|artifact|permanent) enters tapped unless (.+)\.$")
                .unwrap()
                .captures(line)
            && let Some(cond) = static_condition(cap(&c, 2))
        {
            self.statements.push(format!("enters tapped unless {cond}"));
            return Ok(());
        }
        if let Some(c) =
            Regex::new(r"^This creature can't (attack or block|attack|block) unless (.+)\.$")
                .unwrap()
                .captures(line)
            && let Some(cond) = static_condition(cap(&c, 2))
        {
            self.statements
                .push(format!("can't {} unless {cond}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(c) = Regex::new(
            r"^(Enchanted creature|Enchanted permanent|Enchanted land|Equipped creature|Enchanted artifact) doesn't untap during its controller's untap step\.$",
        )
        .unwrap()
        .captures(line)
        {
            let _ = c;
            self.statements
                .push("static enchanted: has doesn't untap".to_string());
            return Ok(());
        }
        if let Some(c) = Regex::new(
            r"^(Enchanted creature|Equipped creature) (can't attack or block|can't block|can't attack|can't be blocked|attacks each combat if able|blocks each combat if able|must be blocked if able|can block an additional creature each combat|can block any number of creatures)\.$",
        )
        .unwrap()
        .captures(line)
        {
            let ks = match cap(&c, 2) {
                "can't attack or block" => "can't attack, can't block",
                other => other,
            };
            self.statements.push(format!("static enchanted: has {ks}"));
            return Ok(());
        }
        if line == "This creature doesn't untap during your untap step."
            || line == "This artifact doesn't untap during your untap step."
            || line == "This land doesn't untap during your untap step."
            || line == "This permanent doesn't untap during your untap step."
        {
            if self.statements.is_empty() {
                self.keywords.push("doesn't untap".to_string());
            } else {
                self.statements.push("keyword doesn't untap".to_string());
            }
            return Ok(());
        }
        if line == "You have no maximum hand size." {
            self.statements.push("hand size none".to_string());
            return Ok(());
        }
        if let Some(c) = Regex::new(
            r"^(Your|Each opponent's) maximum hand size is (increased|reduced) by (\w+)\.$",
        )
        .unwrap()
        .captures(line)
            && let Some(n) = number(cap(&c, 3))
        {
            let sign = if cap(&c, 2) == "increased" { "+" } else { "-" };
            let who = if cap(&c, 1) == "Your" {
                ""
            } else {
                " opponents"
            };
            self.statements.push(format!("hand size{who} {sign}{n}"));
            return Ok(());
        }
        if let Some(c) = Regex::new(
            r"^(Bushido|Soulshift|Afflict|Toxic|Crew|Bloodthirst|Modular|Backup|Saddle|Fabricate|Renown|Casualty|Teamwork|Ripple) (\d+)$",
        )
        .unwrap()
        .captures(line)
        {
            self.statements
                .push(format!("{} {}", cap(&c, 1).to_lowercase(), cap(&c, 2)));
            return Ok(());
        }
        if let Some(c) =
            Regex::new(r"^(Ward|Echo|Unearth|Buyback|Outlast|Morph|Megamorph|Disguise|Evoke|Madness|Foretell|Entwine|Ninjutsu|Scavenge|Dash|Warp|Embalm|Eternalize|Offspring|Bestow|Overload) ((?:\{[0-9WUBRGCX/]+\})+)$")
                .unwrap()
                .captures(line)
        {
            self.statements
                .push(format!("{} {}", cap(&c, 1).to_lowercase(), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.47: "Splice onto Arcane {1}{R}".
        if let Some(c) = Regex::new(r"^Splice onto Arcane ((?:\{[0-9WUBRGCX/]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("splice {}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(c) = Regex::new(
            r"^Escape\u{2014}((?:\{[0-9WUBRGCX/]+\})+), Exile (\w+) other cards from your graveyard\.$",
        )
        .unwrap()
        .captures(line)
        {
            let n = number(cap(&c, 2)).ok_or_else(|| Skip::Sentence(line.to_string()))?;
            self.statements
                .push(format!("escape {} exile {n}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(c) = Regex::new(
            r"^This (?:creature|permanent) escapes with (an?|\w+) \+1/\+1 counters? on it\.$",
        )
        .unwrap()
        .captures(line)
        {
            let n = if matches!(cap(&c, 1), "a" | "an") {
                "1".to_string()
            } else {
                number(cap(&c, 1)).ok_or_else(|| Skip::Sentence(line.to_string()))?
            };
            self.statements
                .push(format!("escapes with {n} +1/+1 counters"));
            return Ok(());
        }
        // "If this creature was kicked, it enters with two +1/+1 counters
        // on it [and with haste]." (CR 702.33c).
        if let Some(c) = Regex::new(
            r"^If this creature was kicked, it enters with (an?|\w+) \+1/\+1 counters? on it(?: and with ([a-z ]+))?\.$",
        )
        .unwrap()
        .captures(line)
        {
            let n = if matches!(cap(&c, 1), "a" | "an") {
                "1".to_string()
            } else {
                number(cap(&c, 1)).ok_or_else(|| Skip::Sentence(line.to_string()))?
            };
            let with = c
                .get(2)
                .map(|k| format!(" and {}", k.as_str()))
                .unwrap_or_default();
            self.statements
                .push(format!("kicked with {n} +1/+1 counters{with}"));
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^Suspend (\d+)\u{2014}((?:\{[0-9WUBRGCX/]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("suspend {} {}", cap(&c, 1), cap(&c, 2)));
            return Ok(());
        }
        if line == "Conspire" {
            self.statements.push("conspire".to_string());
            return Ok(());
        }
        if line == "Bargain" {
            self.statements.push("bargain".to_string());
            return Ok(());
        }
        if line == "Evolve" || line == "Unleash" {
            self.statements.push(line.to_lowercase());
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^Affinity for (\w+)$").unwrap().captures(line) {
            self.statements
                .push(format!("affinity for {}", singular(cap(&c, 1))));
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^This spell costs \{(\d+)\} (less|more) to cast if (.+?)\.?$")
            .unwrap()
            .captures(line)
            && !cap(&c, 3).starts_with("it targets")
            && let Some(cond) = static_condition(cap(&c, 3))
        {
            self.statements.push(format!(
                "this spell costs {{{}}} {} if {cond}",
                cap(&c, 1),
                cap(&c, 2)
            ));
            return Ok(());
        }
        if let Some(c) =
            Regex::new(r"^This spell costs \{(\d+)\} (less|more) to cast if it targets an? (.+)\.$")
                .unwrap()
                .captures(line)
        {
            self.statements.push(format!(
                "this spell costs {{{}}} {} if it targets {}",
                cap(&c, 1),
                cap(&c, 2),
                cap(&c, 3)
            ));
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^This spell costs \{(\d+)\} less to cast for each (.+)\.$")
            .unwrap()
            .captures(line)
            && let Some(expr) = count_expr(cap(&c, 2))
        {
            self.statements.push(format!(
                "this spell costs {{{}}} less count each {expr}",
                cap(&c, 1)
            ));
            return Ok(());
        }
        if let Some(c) = Regex::new(
            r"^(?:(.+) )?[Ss]pells( you cast| your opponents cast)? cost \{(\d+)\} (less|more) to cast\.$",
        )
        .unwrap()
        .captures(line)
        {
            let subject = cap(&c, 1).replace(" and ", " or ");
            let subject = lowercase_first(&subject);
            let subject = if subject.is_empty() {
                String::new()
            } else {
                format!(" {}", singular(&subject))
            };
            self.statements.push(format!(
                "spells{subject}{} cost {{{}}} {}",
                cap(&c, 2),
                cap(&c, 3),
                cap(&c, 4)
            ));
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
            r"^This (?:creature|permanent|artifact|land|enchantment|Vehicle) enters with (\S+) (\+1/\+1|-1/-1|stun|charge|oil|spore|depletion|quest|ki|time|tide|shield|age|fade|ice|level|lore|page) counters? on it\.$",
        )
        .unwrap()
        .captures(line)
        {
            let n = number(cap(&c, 1)).ok_or_else(|| Skip::Sentence(line.to_string()))?;
            self.statements
                .push(format!("enters with {n} {} counters", cap(&c, 2)));
            return Ok(());
        }
        // "Creatures you control of the chosen color get +1/+0.", "All
        // creatures of the chosen type get -1/-1."
        if let Some(c) = Regex::new(
            r"^(All creatures|Creatures|Creatures you control|Other creatures you control) of the chosen (color|type) (.+)\.$",
        )
        .unwrap()
        .captures(line)
        {
            let filter = match cap(&c, 1) {
                "All creatures" => "all creature",
                "Creatures" => "creature",
                "Creatures you control" => "creature you control",
                _ => "other creature you control",
            };
            let gets = Regex::new(r"^get ([+-]\d+/[+-]\d+)$").unwrap();
            let mut mods = Vec::new();
            for part in cap(&c, 3).split(" and ") {
                if let Some(c2) = gets.captures(part) {
                    mods.push(cap(&c2, 1).to_string());
                } else if let Some(rest) = part.strip_prefix("have ") {
                    let ks = keyword_list(rest).ok_or_else(|| Skip::Sentence(line.to_string()))?;
                    mods.push(format!("has {ks}"));
                } else {
                    return Err(Skip::Sentence(line.to_string()));
                }
            }
            self.statements.push(format!(
                "static {filter} of chosen {}: {}",
                cap(&c, 2),
                mods.join(", ")
            ));
            return Ok(());
        }
        if line == "Enchanted land is the chosen type." {
            self.statements
                .push("static enchanted: set land chosen".to_string());
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
        // "All Sliver creatures get +1/+1." / "Minotaur creatures get +1/+0."
        if let Some(c) = Regex::new(r"^(All )?([A-Z][a-z]+) creatures (.+)\.$")
            .unwrap()
            .captures(line)
        {
            let all = if cap(&c, 1).is_empty() { "" } else { "all " };
            let filter = format!("{all}{} creature", cap(&c, 2));
            let mut mods = Vec::new();
            let gets = Regex::new(r"^gets? ([+-]\d+/[+-]\d+)$").unwrap();
            let mut ok = true;
            for part in cap(&c, 3).split(" and ") {
                if let Some(c2) = gets.captures(part) {
                    mods.push(cap(&c2, 1).to_string());
                } else if let Some(rest) = part.strip_prefix("have ") {
                    match keyword_list(rest) {
                        Some(ks) => mods.push(format!("has {ks}")),
                        None => ok = false,
                    }
                } else {
                    ok = false;
                }
            }
            if ok && !mods.is_empty() {
                self.statements
                    .push(format!("static {filter}: {}", mods.join(", ")));
                return Ok(());
            }
        }
        if line == "You control enchanted creature." {
            self.statements
                .push("static enchanted: control".to_string());
            return Ok(());
        }
        // CR 702.142, 702.177, 702.193: "Boast — <cost>: <effects>" (the
        // keyword's rules are reminder text, already gone).
        let (word, line) = [
            ("Boast — ", "boast"),
            ("Exhaust — ", "exhaust"),
            ("Power-up — ", "power-up"),
        ]
        .iter()
        .find_map(|(prefix, word)| Some((*word, line.strip_prefix(prefix)?)))
        .unwrap_or(("activated", line));
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
            let rest = match rest.strip_suffix(" Activate only as a sorcery.") {
                Some(r) => {
                    cost.push_str(", sorcery");
                    r
                }
                None => rest,
            };
            let mut targets = 0;
            let effects = self.templates.effects(rest, &mut targets)?;
            self.statements.push(format!("{word} {cost}: {effects}"));
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^Cumulative upkeep ((?:\{[0-9WUBRGC]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("cumulative upkeep {}", cap(&c, 1)));
            return Ok(());
        }
        // CR 701.43: "You may exert this creature as it attacks. When you
        // do, ...".
        if let Some(rest) =
            line.strip_prefix("You may exert this creature as it attacks. When you do, ")
        {
            let mut targets = 0;
            let effects = self.templates.effects(rest, &mut targets)?;
            if targets > 0 {
                return Err(Skip::Sentence(line.to_string()));
            }
            self.statements.push(format!("exert: {effects}"));
            return Ok(());
        }
        // Triggered abilities: "<trigger>, [if <condition>, ]<effects>".
        if line.starts_with("When") || line.starts_with("At the beginning") {
            let Some((head, rest)) = line.split_once(", ") else {
                return Err(Skip::Sentence(line.to_string()));
            };
            // "Whenever this creature attacks while saddled" (CR 702.171b).
            let (head, saddled) = match head.strip_suffix(" while saddled") {
                Some(h) => (h, true),
                None => (head, false),
            };
            let Some(mut trigger) = trigger_text(head) else {
                return Err(Skip::Sentence(line.to_string()));
            };
            if saddled {
                trigger.push_str(" while saddled");
            }
            let (condition, rest) = if let Some(r) = rest
                .strip_prefix("if this creature is untapped, ")
                .or_else(|| rest.strip_prefix("if this artifact is untapped, "))
            {
                (" if this is untapped", r)
            } else if let Some(r) = rest.strip_prefix("if no creatures are on the battlefield, ") {
                (" if no creatures are on the battlefield", r)
            } else if let Some(r) = rest.strip_prefix("if it was kicked, ") {
                (" if kicked", r)
            } else if let Some(r) = rest.strip_prefix("if no spells were cast last turn, ") {
                (" if no spells were cast last turn", r)
            } else if let Some(r) =
                rest.strip_prefix("if a player cast two or more spells last turn, ")
            {
                (" if a player cast two or more spells last turn", r)
            } else {
                ("", rest)
            };
            // "if you control an artifact, ...", "if you attacked this
            // turn, ...": a static condition.
            let static_if = rest
                .strip_prefix("if ")
                .and_then(|r| r.split_once(", "))
                .and_then(|(cond, r)| Some((static_condition(cond)?, r)));
            let (condition, rest) = match (condition, static_if) {
                ("", Some((c, r))) => (format!(" if {c}"), r),
                (c, _) => (c.to_string(), rest),
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
            self.spell_at.get_or_insert(self.statements.len());
            self.spell_paragraphs.push(effects);
            return Ok(());
        }
        Err(Skip::Sentence(line.to_string()))
    }

    fn modal(&mut self, modes: &[String], head: &str) -> Result<(), Skip> {
        let mut lines = vec![format!("{head} {{")];
        for m in modes {
            lines.push(format!("  {}", self.mode(m)?));
        }
        lines.push("}".to_string());
        self.statements.push(lines.join("\n  "));
        let _ = self.corpus;
        Ok(())
    }

    /// One mode's line; a named mode ("Smash the Chest — Destroy target
    /// artifact.") keeps its name.
    fn mode(&self, m: &str) -> Result<String, Skip> {
        let m = m.trim();
        let (head, m) = match m.split_once(" — ") {
            Some((name, rest)) if name.len() < 50 && !name.contains(['{', '.', '"', ',']) => {
                (format!("mode \"{name}\""), rest)
            }
            _ => ("mode".to_string(), m),
        };
        let mut targets = 0;
        let effects = self.templates.effects(m, &mut targets)?;
        Ok(format!("{head}: {effects}"))
    }

    /// "When this creature enters, choose one —" with its modes.
    fn modal_trigger(&mut self, trigger: &str, modes: &[String]) -> Result<(), Skip> {
        let mut lines = vec![format!("triggered {trigger}: choose one {{")];
        for m in modes {
            lines.push(format!("  {}", self.mode(m)?));
        }
        lines.push("}".to_string());
        self.statements.push(lines.join("\n  "));
        Ok(())
    }
}

/// A multi-face card (CR 709, 712, 715): each face authored as a card of
/// its own, then joined into one block with `layout` and `face` lines.
fn author_faces(s: &ScryCard, corpus: &[Card]) -> Result<Authored, Skip> {
    let faces: Vec<ScryCard> = s.card_faces.iter().map(|f| s.face_record(f)).collect();
    let layout = match s.layout.as_str() {
        "adventure" if faces[1].type_line.as_deref().unwrap_or("").contains("Omen") => "omen",
        "adventure" => "adventure",
        "transform" => "transform",
        _ => {
            if faces
                .iter()
                .any(|f| f.type_line.as_deref().unwrap_or("").contains("Room"))
            {
                return Err(Skip::Layout("room".to_string()));
            }
            "split"
        }
    };
    let mut bodies = Vec::new();
    let mut tokens = Vec::new();
    for f in &faces {
        let mut rec = f.clone();
        rec.layout = "normal".to_string();
        let a = author_card(&rec, corpus)?;
        // The block's lines between its braces, without the oracle_id.
        let body: Vec<String> = a
            .text
            .lines()
            .skip(1)
            .filter(|l| !l.trim_start().starts_with("oracle_id ") && *l != "}")
            .map(str::to_string)
            .collect();
        bodies.push(body);
        tokens.extend(a.tokens);
    }
    let mut text = format!("card \"{}\" {{\n", s.name);
    if let Some(id) = &s.oracle_id {
        let _ = writeln!(text, "  oracle_id {id}");
    }
    let _ = writeln!(text, "  layout {layout}");
    let rest = if layout == "split" {
        &bodies[..]
    } else {
        for l in &bodies[0] {
            let _ = writeln!(text, "{l}");
        }
        &bodies[1..]
    };
    let face_names: Vec<&str> = if layout == "split" {
        faces.iter().map(|f| f.name.as_str()).collect()
    } else {
        faces[1..].iter().map(|f| f.name.as_str()).collect()
    };
    for (name, body) in face_names.iter().zip(rest) {
        let _ = writeln!(text, "  face \"{name}\" {{");
        for l in body {
            let _ = writeln!(text, "  {l}");
        }
        text.push_str("  }\n");
    }
    text.push_str("}\n");
    mtg_dsl::parse(&text).map_err(|e| Skip::Sentence(format!("parse: {e}")))?;
    Ok(Authored {
        name: s.name.clone(),
        slug: mtg_dsl::slug(&s.name),
        text,
        tokens,
    })
}

/// Build the card file for one Scryfall record, or say why not.
#[allow(clippy::too_many_lines)]
pub fn author_card(s: &ScryCard, corpus: &[Card]) -> Result<Authored, Skip> {
    if matches!(s.layout.as_str(), "adventure" | "transform" | "split") && s.card_faces.len() == 2 {
        return author_faces(s, corpus);
    }
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
        if !KEYWORDS.contains(&k.as_str())
            && !TEMPLATED_TAGS.contains(&k.as_str())
            && !ABILITY_WORDS.contains(&k.as_str())
        {
            return Err(Skip::Keyword(k.clone()));
        }
    }
    // Hybrid, Phyrexian, and snow symbols the engine parses are fine;
    // energy and the rarer hybrids ({W/U/P}, {C/W}) are not.
    let symbol = Regex::new(r"\{([^}]*)\}").unwrap();
    let odd = |text: &str| {
        symbol.captures_iter(text).any(|c| {
            let inner = &c[1];
            inner.contains('/') && mtg_core::mana::ManaCost::parse(&format!("{{{inner}}}")).is_err()
        })
    };
    if odd(s.mana_cost.as_deref().unwrap_or("")) || odd(s.oracle_text.as_deref().unwrap_or("")) {
        return Err(Skip::Keyword("energy or unusual hybrid symbol".to_string()));
    }
    let oracle = strip_reminder(s.oracle_text.as_deref().unwrap_or(""));
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
        spell_at: None,
        spell_targets: 0,
    };
    let lines: Vec<&str> = oracle.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let head = match line {
            "Choose one —" => Some("choose one"),
            "Choose one or both —" => Some("choose one or both"),
            "Choose two —" => Some("choose two"),
            _ => None,
        };
        if let Some(head) = head {
            let mut modes = Vec::new();
            i += 1;
            while i < lines.len() && lines[i].starts_with("• ") {
                modes.push(lines[i][3..].to_string());
                i += 1;
            }
            b.modal(&modes, head)?;
            continue;
        }
        // "When this creature enters, choose one —" and its modes.
        if let Some(head) = line.strip_suffix(", choose one —") {
            let Some(trigger) = trigger_text(head) else {
                return Err(Skip::Sentence(line.to_string()));
            };
            let mut modes = Vec::new();
            i += 1;
            while i < lines.len() && lines[i].starts_with("• ") {
                modes.push(lines[i][3..].to_string());
                i += 1;
            }
            b.modal_trigger(&trigger, &modes)?;
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
    if let Some(d) = &s.defense {
        let _ = writeln!(text, "  defense {d}");
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
    let split = b.spell_at.unwrap_or(b.statements.len());
    for st in &b.statements[..split] {
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
    for st in &b.statements[split..] {
        let _ = writeln!(text, "  {st}");
    }
    text.push_str("}\n");
    // Tokens it creates that the corpus lacks are generated from their key.
    let created = Regex::new(r#"create \S+ "([^"]+)""#).unwrap();
    let mut tokens = Vec::new();
    let mut token_cards = Vec::new();
    for c in created.captures_iter(&text) {
        let key = &c[1];
        if corpus.iter().any(|t| t.name == key) || tokens.iter().any(|(k, _)| k == key) {
            continue;
        }
        let block =
            token_block(key).ok_or_else(|| Skip::Sentence(format!("unknown token {key}")))?;
        token_cards.extend(
            mtg_dsl::parse(&block).map_err(|e| Skip::Sentence(format!("token parse: {e}")))?,
        );
        tokens.push((key.to_string(), block));
    }
    // Verify: parse, render, compare.
    let mut cards = mtg_dsl::parse(&text).map_err(|e| Skip::Sentence(format!("parse: {e}")))?;
    let card = cards.remove(0);
    if card.kind != Kind::Card {
        return Err(Skip::Layout("not a card".to_string()));
    }
    let by_name: std::collections::HashMap<&str, &Card> = corpus
        .iter()
        .chain(token_cards.iter())
        .map(|c| (c.name.as_str(), c))
        .collect();
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
        tokens: tokens
            .into_iter()
            .map(|(k, b)| (mtg_dsl::slug(&k), b))
            .collect(),
    })
}
