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
const KEYWORDS: &[&str] = &[
    "Cipher",
    "Banding",
    "Decayed",
    "Phasing",
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
const TEMPLATED_TAGS: &[&str] = &[
    "Hexproof from",
    "Job select",
    "Mayhem",
    "Mobilize",
    "Behold",
    "Empower Jace",
    "Station",
    "Start your engines!",
    "Craft",
    "Suspect",
    "Collect evidence",
    "Blight",
    "Manifest dread",
    "Soulbond",
    "Disturb",
    "Daybound",
    "Nightbound",
    "Transmute",
    "Sunburst",
    "Multikicker",
    "Graft",
    "Vanishing",
    "Transform",
    "To solve",
    "Solved",
    "Prepared",
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
    "Detain",
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
    "Encore",
    "Eternalize",
    "Backup",
    "Offspring",
    "Bargain",
    "Conspire",
    "Casualty",
    "Dredge",
    "Storm",
    "Awaken",
    "Assist",
    "Enlist",
    "Replicate",
    "Teamwork",
    "Warp",
    "Plot",
    "Sneak",
    "Harmonize",
    "Web-slinging",
    "Forage",
    "Impending",
    "Endure",
    "Spree",
    "Gift",
    "Freerunning",
    "Ripple",
    "Saddle",
    "Fabricate",
    "Renown",
    "Evolve",
    "Unleash",
    "Afterlife",
    "Extort",
    "Mentor",
    "Riot",
    "Living weapon",
    "Monstrosity",
    "Retrace",
    "Jump-start",
    "Surge",
    "Spectacle",
    "Ingest",
    "Reinforce",
    "Support",
    "Bolster",
    "Populate",
    "Incubate",
    "Earthbend",
    "Waterbend",
    "Firebending",
    "Double",
    "Ascend",
    "Venture into the dungeon",
    "Clash",
    "Learn",
    "Descend",
    "Umbra armor",
];

/// Ability words (CR 207.2c): labels with no rules meaning, printed
/// "Landfall — <ability>". Scryfall tags them like keywords.
const ABILITY_WORDS: &[&str] = &[
    "Repartee",
    "Vivid",
    // CR 702.179e: a condition, not a bare label; the DSL compiles it.
    "Max speed",
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
    "Converge",
    "Rally",
    "Infusion",
    "Renew",
    "Disappear",
    "Flurry",
    "Opus",
    "Cohort",
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
        // Vivid.
        "colors among permanents you control" | "color among permanents you control" => {
            return Some("colors among permanents".to_string());
        }
        // Converge (CR 601.2h).
        "colors of mana spent to cast it" | "color of mana spent to cast it" => {
            return Some("colors spent".to_string());
        }
        "colors of mana spent to cast this spell" | "color of mana spent to cast this spell" => {
            return Some("colors spent spell".to_string());
        }
        "colors of mana spent to cast this creature" => {
            return Some("colors spent creature".to_string());
        }
        "creatures in your party" | "creature in your party" => {
            return Some("party".to_string());
        }
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
        // "permanent cards" (CR 110.4a): `graveyard permanent card`.
        if f == "permanent" || f.ends_with(" permanent") {
            return Some(format!("graveyard {f} card"));
        }
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
            | "this Siege"
            | "~"
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
    // "up to one other target creature": `another` in the DSL.
    if let Some(rest) = s.strip_prefix("up to one other target ") {
        *targets += 1;
        return Some(format!("up to 1 another target {rest}"));
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
            // "up to two target creature cards": the DSL names one kind.
            return Some(s.replace(" cards ", " card ").replace(" cards", " card"));
        }
    }
    // Any other "target <filter>" phrase: the DSL parser and the render
    // check decide whether it is one the engine knows.
    if s.starts_with("target ") || s.starts_with("another target ") {
        *targets += 1;
        // "a nonland permanent card" (CR 110.4a): the DSL names the types
        // a permanent card isn't (`nonland permanent card` is a nonland
        // card).
        let nonperm = Regex::new(r"\b(non[a-z]+) permanent card").unwrap();
        return Some(
            nonperm
                .replace_all(s, "$1 noninstant nonsorcery permanent card")
                .into_owned(),
        );
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

/// A group subject Oracle introduces with "each" and a singular verb
/// ("each other Bird you control deals damage equal to its power"):
/// `each single`, which prints one at a time.
fn each_subject(oracle: &str, who: String) -> String {
    if !(oracle.starts_with("each ") || oracle.starts_with("Each ")) {
        return who;
    }
    match who.strip_prefix("each ") {
        Some(rest) if !rest.starts_with("single ") => format!("each single {rest}"),
        _ => who,
    }
}

/// A group object Oracle introduces with "all" where `each` prints
/// "each" ("exile all artifacts your opponents control until ..."):
/// `all`.
fn all_object(oracle: &str, who: String) -> String {
    match who.strip_prefix("each ") {
        Some(rest) if oracle.starts_with("all ") => format!("all {rest}"),
        _ => who,
    }
}

/// A group subject Oracle introduces with "all" ("all other creatures
/// get -2/-2"): `every`, which prints it.
fn all_subject(oracle: &str, who: String) -> String {
    match who.strip_prefix("each ") {
        Some(rest) if oracle.starts_with("all ") || oracle.starts_with("All ") => {
            format!("every {rest}")
        }
        _ => who,
    }
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
            | "Slivers" | "Goblins" | "Walls" | "instants" | "sorceries" | "planeswalkers"
            | "tokens" => {
                if w == "sorceries" {
                    "sorcery".to_string()
                } else {
                    w.trim_end_matches('s').to_string()
                }
            }
            "Elves" => "Elf".to_string(),
            "Dwarves" => "Dwarf".to_string(),
            "Wolves" => "Wolf".to_string(),
            // A planeswalker type spelled with "s" in the singular.
            "Bolas" => "Bolas".to_string(),
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
                    let who = all_subject(cap(c, 1), recipient(&subject(cap(c, 1)), t)?);
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
                    let who = all_subject(cap(c, 1), recipient(&subject(cap(c, 1)), t)?);
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
            // "each opponent loses 1 life and you gain 1 life".
            (
                r(r"^(.+?) (gains?|loses?) (\S+) life and (.+?) (gains?|loses?) (\S+) life$"),
                |c, t| {
                    let verb = |v: &str| {
                        if v.starts_with("gain") {
                            "gains"
                        } else {
                            "loses"
                        }
                    };
                    let a = recipient(&subject(cap(c, 1)), t)?;
                    let b = recipient(&lowercase_first(cap(c, 4)), t)?;
                    Some(format!(
                        "{a} {} {} life and {b} {} {} life",
                        verb(cap(c, 2)),
                        number(cap(c, 3))?,
                        verb(cap(c, 5)),
                        number(cap(c, 6))?
                    ))
                },
            ),
            (
                r(r"^[Yy]ou gain (\S+) life and draw (\S+) cards?$"),
                |c, _| {
                    Some(format!(
                        "you gain {} life and draw {} card",
                        number(cap(c, 1))?,
                        number(cap(c, 2))?
                    ))
                },
            ),
            (
                r(
                    r"^[Cc]reate (\S+) (tapped )?(Treasure|Food|Clue|Blood|Map|Powerstone|Mutagen|Lander) tokens?$",
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
            // The corpus's Eldrazi Spawn and Scion tokens (Writhing
            // Chrysalis).
            (
                r(
                    r#"^[Cc]reate (\S+) (?:0/1 colorless Eldrazi (Spawn)|1/1 colorless Eldrazi (Scion)) creature tokens? with "Sacrifice this token: Add \{C\}\."$"#,
                ),
                |c, _| {
                    let kind = if cap(c, 2).is_empty() {
                        cap(c, 3)
                    } else {
                        cap(c, 2)
                    };
                    Some(format!("create {} \"Eldrazi {kind}\"", number(cap(c, 1))?))
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
                    r"^(Its controller|Each player|Each opponent) creates (\S+) (Treasure|Food|Clue|Blood|Map|Powerstone|Lander) tokens?$",
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
            (r(r"^[Cc]ollect evidence (\d+)$"), |c, _| {
                Some(format!("collect evidence {}", cap(c, 1)))
            }),
            (r(r"^[Bb]light (\d+)$"), |c, _| {
                Some(format!("blight {}", cap(c, 1)))
            }),
            (r(r"^[Mm]anifest dread$"), |_, _| {
                Some("manifest dread".to_string())
            }),
            // CR 701.60a: "Suspect it.", "suspect up to one target creature".
            (r(r"^[Ss]uspect (.+)$"), |c, t| {
                Some(format!("suspect {}", recipient(cap(c, 1), t)?))
            }),
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
            // "target creature you control gets +X/+0 until end of turn,
            // where X is this creature's power".
            (
                r(
                    r"^(.+?) gets? \+X/\+(X|0) until end of turn, where X is (?:this creature|~)'s (power|toughness)$",
                ),
                |c, t| {
                    let who = recipient(&subject(cap(c, 1)), t)?;
                    Some(format!(
                        "{who} gets +1/+{} count where {}",
                        if cap(c, 2) == "X" { "1" } else { "0" },
                        cap(c, 3)
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
                r(r"^[Tt]ransform (this creature|this artifact|this permanent|it|~)$"),
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
            (r(r"^Monstrosity (\d+|X)$"), |c, _| {
                Some(format!("monstrosity {}", cap(c, 1)))
            }),
            // CR 701.66a: earthbend declares its own target land.
            (r(r"^[Ee]arthbend (\d+)$"), |c, _| {
                Some(format!("earthbend {}", cap(c, 1)))
            }),
            (
                r(r"^[Ee]arthbend X, where X is the number of (.+)$"),
                |c, _| Some(format!("earthbend count where {}", count_expr(cap(c, 1))?)),
            ),
            (
                r(r"^[Ee]arthbend X, where X is that creature's power$"),
                |_, _| Some("earthbend count where power of that creature".to_string()),
            ),
            // CR 701.10.
            (
                r(r"^[Dd]ouble (.+?)'s power( and toughness)? until end of turn$"),
                |c, t| {
                    let to = recipient(cap(c, 1), t)?;
                    let and = if cap(c, 2).is_empty() {
                        ""
                    } else {
                        " and toughness"
                    };
                    Some(format!("double {to} power{and}"))
                },
            ),
            (
                r(r"^[Dd]ouble the power( and toughness)? of (.+) until end of turn$"),
                |c, t| {
                    let to = recipient(cap(c, 2), t)?;
                    let and = if cap(c, 1).is_empty() {
                        ""
                    } else {
                        " and toughness"
                    };
                    Some(format!("double the power{and} of {to}"))
                },
            ),
            (
                r(r"^[Dd]ouble the number of ([+-]\d+/[+-]\d+|\w+) counters on (.+)$"),
                |c, t| {
                    let to = recipient(cap(c, 2), t)?;
                    Some(format!(
                        "double the number of {} counters on {to}",
                        cap(c, 1)
                    ))
                },
            ),
            (
                r(r"^[Dd]ouble the number of each kind of counter on (.+)$"),
                |c, t| {
                    let to = recipient(cap(c, 1), t)?;
                    Some(format!("double the number of each kind of counter on {to}"))
                },
            ),
            (r(r"^[Dd]ouble your life total$"), |_, _| {
                Some("double your life total".to_string())
            }),
            (r(r"^[Dd]ouble (.+?)'s life total$"), |c, t| {
                Some(format!("double {} life total", recipient(cap(c, 1), t)?))
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
            (
                r(r"^(?:[Ii]t|[Tt]his creature|~) becomes prepared$"),
                |_, _| Some("this becomes prepared".to_string()),
            ),
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
                r(
                    r"^If (that creature|that creature or planeswalker|it) would die this turn, exile it instead$",
                ),
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
            // CR 607.2a: what this permanent's behold cost exiled.
            (
                r(r"^[Rr]eturn the exiled card to its owner's hand$"),
                |_, _| Some("return exiled card to hand".to_string()),
            ),
            // "Empower Jace 2." (its reminder text already gone).
            (r(r"^[Ee]mpower Jace (\w+)$"), |c, _| {
                Some(format!("empower jace {}", number(cap(c, 1))?))
            }),
            (r(r"^Amass (\w+) (\w+)$"), |c, _| {
                Some(format!("amass {} {}", cap(c, 1), number(cap(c, 2))?))
            }),
            // CR 701.61a: "you may forage. If you do, ...".
            (r(r"^(?:[Yy]ou )?[Ff]orage$"), |_, _| {
                Some("forage".to_string())
            }),
            // CR 701.63a: "it endures 2", "Anafenza endures 2".
            (
                r(r"^(?:It|it|This creature|this creature|~) endures (\d+)$"),
                |c, t| (*t == 0).then(|| format!("this endures {}", cap(c, 1))),
            ),
            (r(r"^(It|This creature|~) explores$"), |_, t| {
                (*t == 0).then(|| "this explores".to_string())
            }),
            (
                r(r"^(It|it|This creature|~) explores, then it explores again$"),
                |_, t| (*t == 0).then(|| "this explores twice".to_string()),
            ),
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
            (r(r"^Add (two|three) mana of any one color$"), |c, _| {
                Some(format!("add any {}", number(cap(c, 1))?))
            }),
            (r(r"^Add two mana in any combination of colors$"), |_, _| {
                Some("add 2 mana any combination".to_string())
            }),
            // "you may gain 1 life" after `you may` (the DSL keeps `you`).
            (r(r"^Gain (\S+) life$"), |c, _| {
                Some(format!("you gain {} life", number(cap(c, 1))?))
            }),
            (r(r"^Scry (\d+)$"), |c, _| {
                Some(format!("scry {}", cap(c, 1)))
            }),
            // CR 707.10: "Copy target instant or sorcery spell", "copy that
            // spell", "copy it X times" ("You may choose new targets for
            // the copy" is the next sentence, joined in `effects`).
            (
                r(
                    r"^[Cc]opy (target .+ (?:spell|ability)(?: you control)?(?: from an? \w+ source)?|that spell|it|this spell)( twice| X times)?$",
                ),
                |c, t| {
                    let what = match cap(c, 1) {
                        "that spell" => "that spell".to_string(),
                        "it" => "it".to_string(),
                        "this spell" => "this".to_string(),
                        target => {
                            *t += 1;
                            target.to_string()
                        }
                    };
                    let times = match cap(c, 2) {
                        " twice" => " times 2",
                        " X times" => " times x",
                        _ => "",
                    };
                    Some(format!("copy {what}{times}"))
                },
            ),
            // CR 707.2, 111.4: "Create a token that's a copy of target
            // creature you control".
            (
                r(r"^[Cc]reate a token that's a copy of ([^,]+)$"),
                |c, t| {
                    let what = match cap(c, 1) {
                        // "a copy of this creature" names it in full.
                        w @ ("this creature" | "this artifact" | "this enchantment") => {
                            w.to_string()
                        }
                        w => recipient(w, t)?,
                    };
                    Some(format!("create token copy of {what}"))
                },
            ),
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
                    let a = each_subject(cap(c, 1), recipient(&subject(cap(c, 1)), t)?);
                    Some(format!("{a} bites itself"))
                },
            ),
            (
                r(r"^(.+?) deals damage equal to its power to (.+)$"),
                |c, t| {
                    let a = each_subject(cap(c, 1), recipient(&subject(cap(c, 1)), t)?);
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
                Some(format!(
                    "destroy {}",
                    each_subject(cap(c, 1), recipient(cap(c, 1), t)?)
                ))
            }),
            // CR 610.3: "exile target creature an opponent controls until
            // this creature leaves the battlefield" (Banishing Light).
            (
                r(
                    r"^Exile (.+) until (?:this (?:creature|enchantment|artifact|permanent|Aura|land)|~) leaves the battlefield$",
                ),
                |c, t| {
                    Some(format!(
                        "exile {} until this leaves",
                        all_object(cap(c, 1), recipient(cap(c, 1), t)?)
                    ))
                },
            ),
            (r(r"^Exile (.+)$"), |c, t| {
                Some(format!("exile {}", recipient(cap(c, 1), t)?))
            }),
            // A spell or legendary permanent returning itself (Molten Birth).
            (r(r"^Return ~ to its owner's hand$"), |_, _| {
                Some("return this to its owner's hand".to_string())
            }),
            (
                r(r"^Return (.+?) to (?:its owner's|your|their owners') hands?$"),
                |c, t| Some(format!("return {} to hand", recipient(cap(c, 1), t)?)),
            ),
            // "Return target creature card from your graveyard to the
            // battlefield" (Breath of Life).
            (
                r(r"^Return (.+? from your graveyard) to the battlefield( tapped)?$"),
                |c, t| {
                    Some(format!(
                        "return {} to battlefield{}",
                        recipient(cap(c, 1), t)?,
                        cap(c, 2)
                    ))
                },
            ),
            (r(r"^[Vv]enture into the dungeon$"), |_, _| {
                Some("venture".to_string())
            }),
            (r(r"^[Cc]lash with an opponent$"), |_, _| {
                Some("clash".to_string())
            }),
            (r(r"^Learn$"), |_, _| Some("learn".to_string())),
            // CR 500.7.
            (r(r"^[Tt]ake an extra turn after this one$"), |_, _| {
                Some("you take extra turn".to_string())
            }),
            (
                r(r"^(Target player|Target opponent) takes an extra turn after this one$"),
                |c, t| Some(format!("{} takes extra turn", recipient(cap(c, 1), t)?)),
            ),
            // CR 500.8.
            (
                r(
                    r"^After this (main )?phase, there is an additional combat phase( followed by an additional main phase)?$",
                ),
                |c, _| {
                    Some(format!(
                        "after this {}phase, additional combat phase{}",
                        cap(c, 1),
                        if cap(c, 2).is_empty() {
                            ""
                        } else {
                            " then main phase"
                        }
                    ))
                },
            ),
            // "Return this card from your graveyard to the battlefield
            // with a finality counter on it" (CR 122.1h).
            (
                r(
                    r"^[Rr]eturn this card from your graveyard to the battlefield( tapped)? with a finality counter on it$",
                ),
                |c, _| {
                    Some(format!(
                        "return this to battlefield{} with 1 finality counter",
                        cap(c, 1)
                    ))
                },
            ),
            (
                r(
                    r"^Return (.+? from your graveyard) to the battlefield( tapped)? with a finality counter on it$",
                ),
                |c, t| {
                    Some(format!(
                        "return {} to battlefield{} with 1 finality counter",
                        recipient(cap(c, 1), t)?,
                        cap(c, 2)
                    ))
                },
            ),
            (
                r(
                    r"^[Rr]eturn this card from your graveyard to (your hand|the battlefield|the battlefield tapped)$",
                ),
                |c, _| {
                    Some(match cap(c, 1) {
                        "your hand" => "return this to hand".to_string(),
                        "the battlefield" => "return this to battlefield".to_string(),
                        _ => "return this to battlefield tapped".to_string(),
                    })
                },
            ),
            (
                r(r"^(Target creature) blocks this turn if able$"),
                |c, t| Some(format!("{} blocks if able", recipient(cap(c, 1), t)?)),
            ),
            (
                r(r"^Discard all the cards in your hand, then draw that many cards$"),
                |_, _| {
                    Some("discard all cards in hand then draw count that many discarded this way cards".to_string())
                },
            ),
            (r(r"^[Pp]ut its counters on (.+)$"), |c, t| {
                Some(format!("put its counters on {}", recipient(cap(c, 1), t)?))
            }),
            // CR 119.3: "You gain life equal to the life lost this way."
            (
                r(r"^[Yy]ou gain life equal to the life lost this way$"),
                |_, _| Some("you gain count equal life lost this way life".to_string()),
            ),
            (
                r(r"^[Yy]ou gain life equal to (?:that creature's|its) (toughness|power)$"),
                |c, t| {
                    (*t > 0).then(|| {
                        format!("you gain count equal {} of that target {t} life", cap(c, 1))
                    })
                },
            ),
            (
                r(r"^[Yy]ou draw (\S+) cards? and you lose (\S+) life$"),
                |c, _| {
                    Some(format!(
                        "you draw {} card, and you lose {} life",
                        number(cap(c, 1))?,
                        number(cap(c, 2))?
                    ))
                },
            ),
            (r(r"^[Tt]hat player draws an additional card$"), |_, _| {
                Some("that player draws 1 card".to_string())
            }),
            (r(r"^[Yy]ou gain (\S+) life for each (.+)$"), |c, _| {
                (cap(c, 1) == "1" || cap(c, 1) == "one")
                    .then_some(())
                    .and_then(|()| count_expr(cap(c, 2)))
                    .map(|e| format!("you gain count each {e} life"))
            }),
            (
                r(r"^[Rr]eturn the exiled card to the battlefield under its owner's control$"),
                |_, _| Some("return exiled card to battlefield".to_string()),
            ),
            (
                r(r"^[Ll]ook at (target opponent|target player)'s hand$"),
                |c, t| Some(format!("look at hand of {}", recipient(cap(c, 1), t)?)),
            ),
            (r(r"^Discard your hand$"), |_, _| {
                Some("discard hand".to_string())
            }),
            // CR 705: "Flip a coin. If you win the flip, ...".
            (r(r"^[Ff]lip a coin$"), |_, _| Some("flip coin".to_string())),
            (
                r(
                    r"^(Target creature|Target creature an opponent controls) attacks this turn if able$",
                ),
                |c, t| Some(format!("{} attacks if able", recipient(cap(c, 1), t)?)),
            ),
            (
                r(r"^(.+?) blocks (?:this creature|~) this turn if able$"),
                |c, t| Some(format!("{} blocks this if able", recipient(cap(c, 1), t)?)),
            ),
            (r(r"^[Tt]ap or untap (.+)$"), |c, t| {
                Some(format!("tap or untap {}", recipient(cap(c, 1), t)?))
            }),
            // "Tap up to two target creatures. Those creatures don't untap
            // during their controller's next untap step." (Frost Breath).
            (
                r(r"^Those creatures don't untap during their controller's next untap step$"),
                |_, t| (*t > 1).then(|| format!("targets 1 to {t} doesn't untap next untap step")),
            ),
            (
                r(r"^If a creature dealt damage this way would die this turn, exile it instead$"),
                |_, _| {
                    Some(
                        "this turn, replace creature dealt damage this way dies: exile".to_string(),
                    )
                },
            ),
            (
                r(r"^(Target player|Each player) shuffles their graveyard into their library$"),
                |c, t| {
                    Some(format!(
                        "{} shuffles graveyard into library",
                        recipient(&lowercase_first(cap(c, 1)), t)?
                    ))
                },
            ),
            (
                r(
                    r"^Its controller may search their library for a basic land card, put it onto the battlefield tapped, then shuffle$",
                ),
                |_, t| {
                    (*t > 0).then(|| {
                        "its controller may search basic land card to battlefield tapped"
                            .to_string()
                    })
                },
            ),
            (r(r"^[Yy]ou become the monarch$"), |_, _| {
                Some("become monarch".to_string())
            }),
            (r(r"^[Yy]ou take the initiative$"), |_, _| {
                Some("take initiative".to_string())
            }),
            (r(r"^[Tt]he Ring tempts you$"), |_, _| {
                Some("the ring tempts you".to_string())
            }),
            (r(r"^Detain (.+)$"), |c, t| {
                Some(format!("detain {}", recipient(cap(c, 1), t)?))
            }),
            (
                r(r"^Untap all creatures (you control )?that attacked this turn$"),
                |c, _| {
                    Some(format!(
                        "untap all creature {}that attacked this turn",
                        cap(c, 1)
                    ))
                },
            ),
            (r(r"^(Tap|Untap) (.+)$"), |c, t| {
                Some(format!(
                    "{} {}",
                    cap(c, 1).to_ascii_lowercase(),
                    each_subject(cap(c, 2), recipient(cap(c, 2), t)?)
                ))
            }),
            (r(r"^Regenerate (.+)$"), |c, t| {
                Some(format!("regenerate {}", recipient(cap(c, 1), t)?))
            }),
            (
                r(
                    r"^Put (\S+) ([+-]\d/[+-]\d|stun|charge|oil|spore|depletion|quest|ki|time|tide|shield|age|fade|ice|level|lore|page|arrowhead|javelin|polyp|healing|mining|scream|credit|brick|currency|sleep|net|flood|glyph|loot|supply|finality|flying|first strike|trample|lifelink|deathtouch|reach|menace|vigilance|hexproof|indestructible) counters? on (.+)$",
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
                    r"^Remove (\S+) ([+-]\d/[+-]\d|stun|charge|oil|spore|depletion|quest|ki|time|tide|shield|age|fade|ice|level|lore|page|arrowhead|javelin|polyp|healing|mining|scream|credit|brick|currency|sleep|net|flood|glyph|loot|supply|finality|flying|first strike|trample|lifelink|deathtouch|reach|menace|vigilance|hexproof|indestructible) counters? from (.+)$",
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
            // "you may sacrifice another creature. If you do, ...": the
            // player chooses which (Undercity Scavenger).
            (
                r(r"^Sacrifice (a|an|another|two|three) ([a-z]+(?: or [a-z]+)?)$"),
                |c, _| {
                    let (n, other) = match cap(c, 1) {
                        "another" => ("1".to_string(), "another "),
                        w => (number(w)?, ""),
                    };
                    Some(format!(
                        "sacrifice {n} {other}{} chosen",
                        singular(cap(c, 2))
                    ))
                },
            ),
            (
                r(r"^Gain control of (.+?)( until end of turn)?$"),
                |c, t| {
                    // No duration: for good (CR 611.2a), `indefinitely`.
                    let forever = if cap(c, 2).is_empty() {
                        " indefinitely"
                    } else {
                        ""
                    };
                    Some(format!(
                        "gain control of {}{forever}",
                        recipient(cap(c, 1), t)?
                    ))
                },
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
            } else if s.contains("random") {
                "bottom random"
            } else {
                "bottom"
            }
        };
        let may = Regex::new(
            r"^(Look at|Reveal) the top (\w+) cards of your library\. You may (reveal|put) (an? .+?) card from among them (?:and put it )?into your hand\. Put the rest (on the bottom of your library(?: in any order| in a random order)?|into your graveyard)\.?\s*(.*)$",
        )
        .unwrap();
        let one = Regex::new(
            r"^Look at the top (\w+) cards of your library\. Put one of them into your hand and the (?:rest|other) (on the bottom of your library(?: in any order| in a random order)?|into your graveyard)\.?\s*(.*)$",
        )
        .unwrap();
        let all = Regex::new(
            r"^Reveal the top (\w+) cards of your library\. Put all (.+?) cards revealed this way into your hand and the rest (into your graveyard|on the bottom of your library in (?:any|a random) order)\.?\s*(.*)$",
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
            r"^(.+) unless (you pay|you waterbend|they pay|its controller pays|that player pays) ((?:\{[0-9WUBRGC]+\})+|\d+ life)$",
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
        let delayed = Regex::new(
            r"^(Sacrifice|Exile|Destroy|Return) (it|them|that card|that creature|that token|those tokens|this creature|~)( to (?:its owner's|their owners') hand| to the battlefield under (?:its|their) owner's control)? at (the beginning of the next end step|end of combat)$",
        )
        .unwrap();
        // A token whose quoted ability spans sentences: the corpus's
        // "Mercenary" token.
        let text = text.replace(
            "reate a 1/1 red Mercenary creature token with \"{T}: Target creature you control gets +1/+0 until end of turn. Activate only as a sorcery.\"",
            "\u{1}create 1 \"Mercenary\"",
        );
        let text = text.replace("C\u{1}", "\u{1}").replace("c\u{1}", "\u{1}");
        let text = text.as_str();
        let mut out: Vec<String> = Vec::new();
        let extra_land =
            Regex::new(r"^[Yy]ou may play (an|up to two) additional lands? this turn$").unwrap();
        for sentence in merge_impulse(split_sentences(text)) {
            // A sentence pair `merge_impulse` already wrote as DSL.
            if let Some(dsl) = sentence.strip_prefix('\u{1}') {
                out.push(dsl.to_string());
                continue;
            }
            // CR 707.10c: "You may choose new targets for the copy."
            if sentence == "You may choose new targets for the copy"
                || sentence == "You may choose new targets for the copies"
            {
                match out.last_mut() {
                    Some(last) if last.contains("copy ") => {
                        last.push_str(" new targets");
                        continue;
                    }
                    _ => return Err(Skip::Sentence(sentence)),
                }
            }
            if sentence == "It can't be regenerated" || sentence == "They can't be regenerated" {
                match out.last_mut() {
                    Some(last) if last.starts_with("destroy ") => {
                        last.push_str(" without regeneration");
                        continue;
                    }
                    _ => return Err(Skip::Sentence(sentence)),
                }
            }
            // "Counter target spell. If that spell is countered this way,
            // exile it instead ..." (Dissipate).
            let dest = match sentence.as_str() {
                "If that spell is countered this way, exile it instead of putting it into its owner's graveyard" => {
                    Some(" to exile")
                }
                "If that spell is countered this way, put it on top of its owner's library instead of into that player's graveyard" => {
                    Some(" to library top")
                }
                _ => None,
            };
            if let Some(dest) = dest {
                match out.last_mut() {
                    Some(last) if last.starts_with("counter ") => {
                        last.push_str(dest);
                        continue;
                    }
                    _ => return Err(Skip::Sentence(sentence)),
                }
            }
            // Eldrazi Spawn and Scion tokens (Call the Scions): the token
            // and its quoted mana ability are the corpus's token.
            if matches!(
                sentence.as_str(),
                "They have \"Sacrifice this token: Add {C}.\""
                    | "It has \"Sacrifice this token: Add {C}.\""
            ) {
                let last = out
                    .last_mut()
                    .ok_or_else(|| Skip::Sentence(sentence.clone()))?;
                let fixed = last
                    .replace(
                        "\"0/1 colorless Eldrazi Spawn creature\"",
                        "\"Eldrazi Spawn\"",
                    )
                    .replace(
                        "\"1/1 colorless Eldrazi Scion creature\"",
                        "\"Eldrazi Scion\"",
                    );
                if fixed == *last || !fixed.contains("create ") {
                    return Err(Skip::Sentence(sentence));
                }
                *last = format!("{fixed} it has");
                continue;
            }
            // "Exile target creature. Return that card to the battlefield
            // under its owner's control at the beginning of the next end
            // step." (Turn to Mist): a delayed trigger on what the earlier
            // sentence named.
            if let Some(c) = delayed.captures(&sentence) {
                let obj = cap(&c, 2);
                let made_token = out.last().is_some_and(|l| l.contains("create "));
                let who = if made_token && matches!(obj, "it" | "that token") {
                    "that token".to_string()
                } else if made_token && matches!(obj, "them" | "those tokens") {
                    "those tokens".to_string()
                } else if matches!(obj, "this creature" | "~") && cap(&c, 1) != "Return" {
                    // Sacrificed or destroyed where it is; a card returning
                    // from exile is a new object (CR 400.7).
                    "this".to_string()
                } else if *targets > 0
                    && matches!(obj, "it" | "that card" | "that creature")
                    && !out.iter().any(|l| l.contains("to battlefield"))
                {
                    // Not a card just returned to the battlefield: the
                    // target was the card in the graveyard.
                    format!("target {}", *targets)
                } else {
                    return Err(Skip::Sentence(sentence));
                };
                // `that card` prints so; `it` prints "it".
                let what = match obj {
                    "that token" => "it",
                    "~" => "this creature",
                    "them" => "those tokens",
                    o => o,
                };
                let dest = match cap(&c, 3) {
                    "" => "",
                    d if d.contains("hand") => " to hand",
                    _ => " to battlefield",
                };
                let when = if cap(&c, 4) == "end of combat" {
                    "end of combat"
                } else {
                    "beginning of the next end step"
                };
                out.push(format!(
                    "delayed {who}: {} {what}{dest} at {when}",
                    cap(&c, 1).to_ascii_lowercase()
                ));
                continue;
            }
            // CR 305.2: "You may play an additional land this turn" (the
            // "may" is the rule's, not an optional effect's).
            if let Some(c) = extra_land.captures(&sentence) {
                out.push(if cap(&c, 1) == "an" {
                    "play additional land this turn".to_string()
                } else {
                    "play 2 additional land this turn".to_string()
                });
                continue;
            }
            let if_cond;
            let (prefix, body) = if let Some(rest) = sentence
                .strip_prefix("you may ")
                .or_else(|| sentence.strip_prefix("You may "))
            {
                ("you may ", rest.to_string())
            } else if let Some(rest) = sentence.strip_prefix("If you do, ") {
                ("if you do, ", rest.to_string())
            } else if let Some(rest) = sentence
                .strip_prefix("If this spell was kicked, ")
                .or_else(|| sentence.strip_prefix("If the gift was promised, "))
            {
                ("if kicked, ", rest.to_string())
            } else if let Some(rest) = sentence.strip_prefix("If the gift wasn't promised, ") {
                ("if not kicked, ", rest.to_string())
            } else if let Some(rest) = sentence.strip_prefix("If you win, ") {
                ("if you win, ", rest.to_string())
            } else if let Some(rest) = sentence.strip_prefix("If you win the flip, ") {
                ("if you win the flip, ", rest.to_string())
            } else if let Some(rest) = sentence.strip_prefix("Otherwise, ") {
                ("otherwise, ", rest.to_string())
            } else if let Some(rest) = sentence.strip_prefix("When you do, ") {
                ("when you do, ", rest.to_string())
            } else if let Some((cond, rest)) = sentence
                .strip_prefix("If ")
                .and_then(|r| r.split_once(", "))
                .and_then(|(c, r)| Some((static_condition(c)?, r)))
            {
                if_cond = format!("if {cond}, ");
                (if_cond.as_str(), rest.to_string())
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
            let clauses: Vec<&str> = if body.contains("earch your library")
                || body.contains("earch their library")
                || body == "Discard all the cards in your hand, then draw that many cards"
                || body.ends_with("explores, then it explores again")
            {
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
            let mut e = parts.join(" then ");
            // After a first mention, `this` prints "it"; Oracle's "this
            // creature" there is `this creature`, named in full.
            if !out.is_empty()
                && let Some(noun) = ["creature", "artifact", "enchantment", "land", "permanent"]
                    .into_iter()
                    .find(|n| lowercase_first(&body).starts_with(&format!("this {n} ")))
                && let Some(rest) = e.strip_prefix("this ")
            {
                e = format!("this {noun} {rest}");
            }
            // "Create a 3/3 ... token with flying. It gains haste until
            // end of turn." (Artistic Process): "it" is the token.
            if body.starts_with("It ")
                && out.last().is_some_and(|l| l.contains("create 1 "))
                && let Some(rest) = e.strip_prefix("this ")
            {
                e = format!("that token {rest}");
            }
            out.push(format!("{prefix}{e}"));
        }
        if out.is_empty() {
            return Err(Skip::Sentence(text.to_string()));
        }
        // After a token is created with no target named, Oracle's "that
        // creature" is the token, but `that creature` would be a target or
        // the triggering object: not this card's meaning.
        if *targets == 0
            && let Some(k) = out.iter().position(|e| e.contains("create "))
            && out[k..].iter().any(|e| e.contains("that creature"))
        {
            return Err(Skip::Sentence(text.to_string()));
        }
        Ok(out.join("; "))
    }
}

/// Impulse draw (CR 305.2, 601.3): "Exile the top two cards of your
/// library. Until the end of your next turn, you may play those cards."
/// is one effect, `impulse 2 next turn first`; the pair becomes one
/// sentence holding that DSL after a `\u{1}` marker.
fn merge_impulse(sentences: Vec<String>) -> Vec<String> {
    let exile = Regex::new(
        r"^(If you do, |you may |You may )?[Ee]xile the top (card|\w+ cards) of your library$",
    )
    .unwrap();
    let mut out: Vec<String> = Vec::new();
    let mut it = sentences.into_iter().peekable();
    while let Some(s) = it.next() {
        let merged = exile.captures(&s).and_then(|c| {
            let n = match cap(&c, 2) {
                "card" => "1".to_string(),
                w => number(w.strip_suffix(" cards")?)?,
            };
            let next = it.peek()?;
            let when = match next
                .replace("that card", "those cards")
                .replace("them", "it")
                .as_str()
            {
                "Until the end of your next turn, you may play those cards" => "next turn first",
                "Until end of turn, you may play those cards" => "this turn first",
                "Until your next end step, you may play those cards" => "next end step first",
                "You may play those cards this turn" => "this turn",
                "You may play those cards until the end of your next turn" => "next turn",
                "You may play it this turn" => "this turn it",
                "You may play it until the end of your next turn" => "next turn it",
                "You may play it until your next end step" => "next end step it",
                _ => return None,
            };
            let prefix = match cap(&c, 1) {
                "If you do, " => "if you do, ",
                "" => "",
                _ => "you may ",
            };
            Some(format!("\u{1}{prefix}impulse {n} {when}"))
        });
        match merged {
            Some(m) => {
                it.next();
                out.push(m);
            }
            None => out.push(s),
        }
    }
    out
}

/// Subjects as the DSL names them ("This creature gets" → "this").
fn subject(s: &str) -> String {
    match s {
        "This creature" | "This permanent" | "this creature" | "This artifact"
        | "This enchantment" | "This land" | "This Siege" | "~" => "this".to_string(),
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
        // "Creatures your opponents control get -1/-1": a group the
        // recipient phrases read lowercase.
        other
            if matches!(
                other.split(' ').next(),
                Some(
                    "Creatures"
                        | "Artifacts"
                        | "Lands"
                        | "Permanents"
                        | "Other"
                        | "All"
                        | "Each"
                        | "Attacking"
                        | "Blocking"
                        | "Tapped"
                        | "Untapped"
                        | "Nontoken"
                        | "Another"
                        | "Those"
                        | "Its"
                )
            ) =>
        {
            lowercase_first(other)
        }
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
    let two = Regex::new(r"^[a-z]+ or [a-z]+$").unwrap();
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
            // "Sacrifice an artifact or creature": two types joined by `or`.
            if rest.contains(' ') && !two.is_match(rest) {
                return None;
            }
            format!("sacrifice {rest}")
        } else if let Some(mana) = part
            .strip_prefix("Waterbend ")
            .filter(|m| symbols.is_match(m) && !m.contains('T'))
        {
            // CR 701.67a.
            format!("waterbend {mana}")
        } else if part == "Discard a card" {
            "discard 1 card".to_string()
        } else if part == "Forage" {
            // CR 701.61a.
            "forage".to_string()
        } else if let Some(n) = part
            .strip_prefix("Collect evidence ")
            .filter(|n| n.chars().all(|c| c.is_ascii_digit()))
        {
            format!("collect evidence {n}")
        } else if let Some(n) = part
            .strip_prefix("Blight ")
            .filter(|n| n.chars().all(|c| c.is_ascii_digit()))
        {
            format!("blight {n}")
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
        .replace("this Spacecraft", "this creature")
        .replace("this Siege", "this creature")
        .replace("this Equipment", "this artifact")
        .replace("When ~ ", "When this creature ")
        .replace("Whenever ~ ", "Whenever this creature ");
    let head = head.as_str();
    // CR 716.2c: "When this Class becomes level 2".
    if let Some(n) = head.strip_prefix("When this enchantment becomes level ") {
        return Some(format!("when this becomes level {n}"));
    }
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
        // Repartee (the ability word is a label the caller keeps).
        "Whenever you cast an instant or sorcery spell that targets a creature" => {
            "whenever you cast instant or sorcery spell that targets creature"
        }
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
        // "this permanent": a card with impending (CR 702.176a).
        "Whenever this creature enters or attacks"
        | "Whenever this permanent enters or attacks" => "whenever this enters or attacks",
        "When this creature leaves the battlefield" => "when this leaves the battlefield",
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
        | "Whenever you draw your second card each turn"
        | "Whenever a player draws a card"
        | "Whenever an opponent draws a card"
        | "Whenever you discard a card"
        | "Whenever a player discards a card"
        | "Whenever an opponent discards a card"
        | "Whenever one or more cards leave your graveyard"
        | "Whenever one or more creature cards leave your graveyard"
        | "Whenever you expend 4"
        | "Whenever you expend 8"
        | "Whenever you collect evidence"
        | "Whenever you forage"
        | "Whenever you commit a crime"
        | "Whenever you commit a crime during your turn"
        | "Whenever a source you control deals noncombat damage to an opponent"
        | "Whenever an opponent is dealt noncombat damage"
        | "Whenever one or more opponents are dealt noncombat damage" => {
            return Some(lowercase_first(head));
        }
        "Whenever a creature dealt damage by this creature this turn dies" => {
            "whenever creature dealt damage by this this turn dies"
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
            if let Some(t) = trigger_general(head) {
                return Some(t);
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

/// Trigger phrases of a regular shape: the attached creature's events,
/// this permanent's other events, and a class of permanents entering,
/// dying, or attacking.
#[allow(clippy::too_many_lines)] // one arm per trigger phrase
fn trigger_general(head: &str) -> Option<String> {
    let fixed = match head {
        "When this creature leaves the battlefield"
        | "When this artifact leaves the battlefield"
        | "When this enchantment leaves the battlefield"
        | "When this permanent leaves the battlefield"
        | "When this land leaves the battlefield" => Some("when this leaves the battlefield"),
        "At the beginning of combat on your turn" => Some("at beginning of your combat"),
        "At the beginning of each end step" => Some("at beginning of every end step"),
        "At the beginning of each opponent's upkeep" => {
            Some("at beginning of each opponent's upkeep step")
        }
        "At the beginning of each opponent's end step" => {
            Some("at beginning of each opponent's end step")
        }
        "At the beginning of your first main phase"
        | "At the beginning of your precombat main phase" => {
            Some("at beginning of your first main")
        }
        "At the beginning of your second main phase"
        | "At the beginning of your postcombat main phase" => {
            Some("at beginning of your second main")
        }
        "When you cast this spell" => Some("when you cast this"),
        "Whenever an opponent casts a spell" => Some("whenever an opponent casts a spell"),
        "Whenever you cast or copy an instant or sorcery spell" => {
            Some("whenever you cast or copy instant or sorcery spell")
        }
        "Whenever this creature and at least two other creatures attack" => {
            Some("whenever this and at least 2 other creatures attack")
        }
        "Whenever this creature and at least one other creature attack" => {
            Some("whenever this and at least 1 other creatures attack")
        }
        "Whenever you attack with three or more creatures" => {
            Some("whenever you attack with 3 or more creatures")
        }
        "Whenever you attack with two or more creatures" => {
            Some("whenever you attack with 2 or more creatures")
        }
        "At the beginning of each combat" => Some("at beginning of each combat"),
        "Whenever this creature attacks or blocks" => Some("whenever this attacks or blocks"),
        "When this creature attacks or blocks" => Some("when this attacks or blocks"),
        "Whenever this creature attacks alone" => Some("whenever this attacks alone"),
        "Whenever this creature attacks and isn't blocked" => {
            Some("whenever this attacks and isn't blocked")
        }
        "Whenever you attack" => Some("whenever you attack"),
        "Whenever one or more creatures you control attack" => {
            Some("whenever one or more creatures you control attack")
        }
        "Whenever one or more creatures you control deal combat damage to a player" => {
            Some("whenever one or more creature you control deal combat damage to a player")
        }
        "Whenever you put one or more +1/+1 counters on a creature you control" => {
            Some("whenever you put one or more +1/+1 counters on creature you control")
        }
        "Whenever you put one or more +1/+1 counters on a creature" => {
            Some("whenever you put one or more +1/+1 counters on creature")
        }
        "Whenever you put one or more -1/-1 counters on a creature" => {
            Some("whenever you put one or more -1/-1 counters on creature")
        }
        "Whenever you put one or more +1/+1 counters on this creature" => {
            Some("whenever you put one or more +1/+1 counters on this")
        }
        "Whenever one or more +1/+1 counters are put on this creature" => {
            Some("whenever one or more +1/+1 counters put on this")
        }
        "Whenever you sacrifice another permanent" => {
            Some("whenever you sacrifice other permanent")
        }
        "When this creature becomes the target of a spell" => {
            Some("when this becomes targeted by spell")
        }
        _ => None,
    };
    if let Some(t) = fixed {
        return Some(t.to_string());
    }
    // "Whenever enchanted creature attacks", "When equipped creature
    // dies", "Whenever enchanted land becomes tapped".
    let attached = Regex::new(
        r"^(When|Whenever) (enchanted|equipped) (creature|land|permanent|artifact) (attacks|blocks|attacks or blocks|dies|becomes tapped|becomes untapped|is dealt damage|deals damage|deals combat damage to a player|becomes blocked|becomes the target of a spell or ability)$",
    )
    .unwrap();
    if let Some(c) = attached.captures(head) {
        let verb = match cap(&c, 4) {
            "becomes the target of a spell or ability" => "becomes targeted",
            v => v,
        };
        return Some(format!(
            "{} {} {} {verb}",
            cap(&c, 1).to_ascii_lowercase(),
            cap(&c, 2),
            cap(&c, 3)
        ));
    }
    // "Whenever another nontoken creature you control dies", "Whenever a
    // Goblin you control attacks", "Whenever an artifact you control
    // enters".
    let class = Regex::new(
        r"^(When|Whenever) (a|an|another) ((?:[a-z]+ )*?(?:[A-Z][a-z]+ )*?(?:creature|artifact|enchantment|land|permanent|planeswalker|token|[A-Z][a-z]+)(?: creature)?(?: token)?(?: you control| an opponent controls| your opponents control)?) (enters|dies|attacks|blocks|attacks alone|becomes blocked|deals combat damage to a player)$",
    )
    .unwrap();
    // "Whenever you sacrifice a Food", "Whenever you sacrifice another
    // creature".
    if let Some(c) = Regex::new(r"^Whenever you sacrifice (a|an|another) ([A-Za-z]+)$")
        .unwrap()
        .captures(head)
    {
        let other = if cap(&c, 1) == "another" {
            "other "
        } else {
            ""
        };
        return Some(format!("whenever you sacrifice {other}{}", cap(&c, 2)));
    }
    if let Some(c) = class.captures(head) {
        // "Whenever a creature dies" (not "this creature or another
        // creature"): the article is the older wording.
        let other = match (cap(&c, 2), cap(&c, 4)) {
            ("another", _) => "other ",
            (a, "dies") => {
                if a == "an" {
                    "an "
                } else {
                    "a "
                }
            }
            _ => "",
        };
        return Some(format!(
            "{} {other}{} {}",
            cap(&c, 1).to_ascii_lowercase(),
            cap(&c, 3),
            cap(&c, 4)
        ));
    }
    None
}

/// "seven or more cards are in your graveyard" → `7 cards in your
/// graveyard`, and the other conditions `static_cond` parses.
#[allow(clippy::too_many_lines)] // one arm per condition phrase
fn static_condition(s: &str) -> Option<String> {
    match s {
        "you attacked this turn" => return Some("attacked this turn".to_string()),
        "a creature died this turn" => return Some("creature died this turn".to_string()),
        "you have no cards in hand" => return Some("hand 0 or less".to_string()),
        "it's your turn" => return Some("your turn".to_string()),
        "it's attacking" | "this creature is attacking" => return Some("attacking".to_string()),
        "it's enchanted" | "this creature is enchanted" => return Some("enchanted".to_string()),
        "it's equipped" | "this creature is equipped" => return Some("equipped".to_string()),
        "this spell's sneak cost was paid" | "its sneak cost was paid" => {
            return Some("sneak paid".to_string());
        }
        "his sneak cost was paid" => return Some("his sneak paid".to_string()),
        "her sneak cost was paid" => return Some("her sneak paid".to_string()),
        "this spell was cast using teamwork" => {
            return Some("teamwork".to_string());
        }
        "evidence was collected" => return Some("evidence collected".to_string()),
        "this spell's additional cost was paid" => {
            return Some("additional cost paid".to_string());
        }
        "it's bargained" | "it was bargained" | "this spell was bargained" => {
            return Some("bargained".to_string());
        }
        // Designations (CR 702.131, 309.7, 725).
        "you have the city's blessing" => return Some("city's blessing".to_string()),
        "you've completed a dungeon" => return Some("completed dungeon".to_string()),
        "you're the monarch" => return Some("you are monarch".to_string()),
        "you gained life this turn" => return Some("gained life this turn".to_string()),
        "a card left your graveyard this turn" => {
            return Some("card left your graveyard this turn".to_string());
        }
        "an opponent lost life this turn" => {
            return Some("opponent lost life this turn".to_string());
        }
        "an opponent was dealt damage this turn" => {
            return Some("opponent damaged this turn".to_string());
        }
        "an opponent is poisoned" => return Some("opponent poisoned".to_string()),
        "you've cast an instant or sorcery spell this turn" => {
            return Some("cast instant or sorcery spell this turn".to_string());
        }
        "it's not your turn" => return Some("not your turn".to_string()),
        "you've surveilled this turn" => return Some("surveilled this turn".to_string()),
        "you descended this turn" => return Some("descended this turn".to_string()),
        "a permanent left the battlefield under your control this turn" => {
            return Some("permanent left this turn".to_string());
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
    let more =
        Regex::new(r"^there are (\S+) or more ((?:[a-z]+ )*)cards in your graveyard$").unwrap();
    // Not "card types among cards in your graveyard" (below).
    if let Some(c) = more.captures(s)
        && !cap(&c, 2).contains(" among ")
    {
        return Some(format!(
            "{} {}cards in your graveyard",
            number(cap(&c, 1))?,
            cap(&c, 2).replace("instant and/or sorcery", "instant or sorcery")
        ));
    }
    if let Some(c) = Regex::new(r"^creatures you control have total power (\d+) or greater$")
        .unwrap()
        .captures(s)
    {
        return Some(format!("total power {} or more", cap(&c, 1)));
    }
    if let Some(c) = Regex::new(r"^an opponent has (\w+) or more poison counters$")
        .unwrap()
        .captures(s)
    {
        return Some(format!("opponent poison {} or more", number(cap(&c, 1))?));
    }
    let types =
        Regex::new(r"^there are (\S+) or more card types among cards in your graveyard$").unwrap();
    if let Some(c) = types.captures(s) {
        return Some(format!(
            "{} card types in your graveyard",
            number(cap(&c, 1))?
        ));
    }
    // The fast lands: "you control two or fewer other lands".
    let fewer = Regex::new(r"^you control (\S+) or fewer ([a-zA-Z ]+)$").unwrap();
    if let Some(c) = fewer.captures(s) {
        let f = control_filter(&singular(cap(&c, 2)))?;
        return Some(format!("you control at most {} {f}", number(cap(&c, 1))?));
    }
    // The check lands: "you control a Mountain or a Plains".
    let either = Regex::new(r"^you control an? ([A-Z][a-z]+) or an? ([A-Z][a-z]+)$").unwrap();
    if let Some(c) = either.captures(s) {
        let (a, b) = (cap(&c, 1), cap(&c, 2));
        if ["Plains", "Island", "Swamp", "Mountain", "Forest"].contains(&a)
            && ["Plains", "Island", "Swamp", "Mountain", "Forest"].contains(&b)
        {
            return Some(format!("you control {a} or a {b}"));
        }
    }
    if s == "you control a Mount or Vehicle" {
        return Some("you control Mount or Vehicle".to_string());
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
    // "two or more basic lands", "two or more other lands", "three or more
    // other Mountains" (land entry conditions).
    if let Some(rest) = s
        .strip_prefix("basic ")
        .or_else(|| s.strip_prefix("other "))
        && OK.contains(&rest)
    {
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
        // CR 716.2a: "{R}{G}: Level 2".
        if let Some(c) = Regex::new(r"^((?:\{[0-9WUBRGC]+\})+): Level (\d)$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("level {} {}", cap(&c, 2), cap(&c, 1)));
            return Ok(());
        }
        // CR 719.2: "To solve — <condition>." and "Solved — <ability>".
        if let Some(rest) = line.strip_prefix("To solve \u{2014} ") {
            let cond = rest
                .strip_suffix('.')
                .and_then(|c| static_condition(&lowercase_first(c)))
                .ok_or_else(|| Skip::Sentence(line.to_string()))?;
            self.statements.push(format!("to solve {cond}"));
            return Ok(());
        }
        if let Some(rest) = line.strip_prefix("Solved \u{2014} ") {
            let before = self.statements.len();
            self.line(rest)?;
            if self.statements.len() != before + 1 {
                return Err(Skip::Sentence(line.to_string()));
            }
            let st = self.statements.pop().unwrap_or_default();
            self.statements.push(format!("solved {st}"));
            return Ok(());
        }
        // CR 714.2: "I, II — <effects>", a chapter perhaps named ("I — Wark
        // — <effects>").
        if let Some(c) = Regex::new(
            r"^((?:I|II|III|IV|V|VI|VII)(?:, (?:I|II|III|IV|V|VI|VII))*) \u{2014} (?:([A-Z][^.\u{2014}{}]{0,40}?) \u{2014} )?(.+)$",
        )
        .unwrap()
        .captures(line)
        {
            let roman = ["I", "II", "III", "IV", "V", "VI", "VII"];
            let ns: Vec<String> = cap(&c, 1)
                .split(", ")
                .filter_map(|r| roman.iter().position(|x| *x == r))
                .map(|i| (i + 1).to_string())
                .collect();
            let mut targets = 0;
            let effects = self.templates.effects(cap(&c, 3), &mut targets)?;
            let label = c
                .get(2)
                .map(|l| format!("label \"{}\" ", l.as_str()))
                .unwrap_or_default();
            self.statements.push(format!(
                "{label}triggered chapter {}: {effects}",
                ns.join(", ")
            ));
            return Ok(());
        }
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
        // CR 702.184a, 721.2: "Station", then its levels: "9+ | Flying,
        // first strike" or "2+ | <one ability>".
        // CR 702.181a: "Mobilize 2", "Mobilize X, where X is the number
        // of creature cards in your graveyard."
        if let Some(c) = Regex::new(r"^Mobilize (\d+)$").unwrap().captures(line) {
            self.statements.push(format!("mobilize {}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(rest) = line
            .strip_prefix("Mobilize X, where X is the number of ")
            .and_then(|r| r.strip_suffix('.'))
        {
            let n = count_expr(rest).ok_or_else(|| Skip::Sentence(line.to_string()))?;
            self.statements.push(format!("mobilize count where {n}"));
            return Ok(());
        }
        if line == "Station" {
            self.statements.push("station".to_string());
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^(\d+)\+ \| (.+)$").unwrap().captures(line) {
            let (n, rest) = (cap(&c, 1), cap(&c, 2));
            if let Some(k) = keyword_list(rest) {
                self.statements.push(format!("station {n}: {k}"));
                return Ok(());
            }
            let before = self.statements.len();
            self.line(rest)?;
            if self.statements.len() != before + 1 {
                return Err(Skip::Sentence(line.to_string()));
            }
            let st = self.statements.pop().unwrap_or_default();
            self.statements.push(format!("station {n}: {st}"));
            return Ok(());
        }
        // CR 702.179a.
        if line == "Start your engines!" {
            self.statements.push("start your engines".to_string());
            return Ok(());
        }
        if let Some(st) = conditional_static(line) {
            self.statements.push(st);
            return Ok(());
        }
        // CR 101.2: "This spell can't be countered." on any card, before
        // the spell's own paragraphs.
        if line == "This spell can't be countered." {
            self.statements
                .push("this spell can't be countered".to_string());
            return Ok(());
        }
        if line == "Creatures with power less than this creature's power can't block it." {
            self.statements
                .push("can't be blocked by creature with lesser power".to_string());
            return Ok(());
        }
        // "Other Elves you control get +1/+1.", "Creature tokens you
        // control get +1/+1.": a lord over a class of permanents.
        if let Some(c) = Regex::new(
            r"^(Other [A-Za-z ]+?|Creature tokens|Attacking creatures|Nontoken creatures|Tapped creatures|Untapped creatures|Artifact creatures|Legendary creatures) you control ((?:get|have) .+)\.$",
        )
        .unwrap()
        .captures(line)
            && let Some(mods) = static_mods(cap(&c, 2))
        {
            let class = singular(&lowercase_first(cap(&c, 1)));
            self.statements
                .push(format!("static {class} you control: {mods}"));
            return Ok(());
        }
        if line == "If you would gain life, you gain twice that much life instead." {
            self.statements
                .push("replace you gain life: double".to_string());
            return Ok(());
        }
        if line
            == "Enchanted creature can't attack or block, and its activated abilities can't be activated."
        {
            self.statements.push(
                "static enchanted: has can't attack, can't block, can't activate".to_string(),
            );
            return Ok(());
        }
        // "Each creature you control with a +1/+1 counter on it has
        // trample." (printed one at a time).
        if let Some(c) = Regex::new(
            r"^Each creature you control with an? \+1/\+1 counter on it ((?:has|gets) .+)\.$",
        )
        .unwrap()
        .captures(line)
            && let Some(mods) = static_mods(cap(&c, 1))
        {
            self.statements.push(format!(
                "static each creature you control with +1/+1 counter: {mods}"
            ));
            return Ok(());
        }
        // CR 702.89: "Umbra armor" on an Aura.
        if line == "Umbra armor" {
            self.statements
                .push("replace destroy enchanted: umbra armor".to_string());
            return Ok(());
        }
        // CR 401.2: information only.
        if line == "You may look at the top card of your library any time." {
            self.statements.push("may look top".to_string());
            return Ok(());
        }
        // CR 702.131: "Ascend" as an ability line.
        if line == "Ascend" {
            self.statements.push("ascend".to_string());
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
                keyword_list(p).is_some()
                    || p.to_ascii_lowercase().starts_with("protection from ")
                    || p.to_ascii_lowercase().starts_with("hexproof from ")
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
            .or_else(|| line.strip_prefix("~ "))
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
        // "As an additional cost to cast this spell, you may collect
        // evidence 6." (CR 601.2b), and "This spell costs {2} less to cast
        // if evidence was collected." after it.
        if let Some(c) = Regex::new(
            r"^As an additional cost to cast this spell, you may (collect evidence|blight) (\d+)\.(?: (This spell costs \{\d+\} less to cast if evidence was collected\.))?$",
        )
        .unwrap()
        .captures(line)
        {
            self.additional = Some(format!("may {} {}", cap(&c, 1), cap(&c, 2)));
            if !cap(&c, 3).is_empty() {
                return self.line(cap(&c, 3));
            }
            return Ok(());
        }
        if let Some(rest) = line.strip_prefix("As an additional cost to cast this spell, ") {
            let rest = rest.trim_end_matches('.');
            // CR 701.69a: "behold a Kithkin or pay {2}", "behold an Elf
            // and exile it", "you may behold a Dragon".
            if let Some(c) = Regex::new(
                r"^(you may )?behold (?:a|an) ([A-Z][a-z]+)( and exile it)?(?: or pay ((?:\{[0-9WUBRGC]+\})+))?$",
            )
            .unwrap()
            .captures(rest)
            {
                let may = if cap(&c, 1).is_empty() { "" } else { "may " };
                let exile = if cap(&c, 3).is_empty() { "" } else { "and exile " };
                let mut cost = format!("{may}behold {exile}{}", cap(&c, 2));
                if !cap(&c, 4).is_empty() {
                    let _ = write!(cost, "\n  additional cost or {}", cap(&c, 4));
                }
                self.additional = Some(cost);
                return Ok(());
            }
            let cost = match rest {
                "sacrifice a creature" => "sacrifice creature".to_string(),
                "exile a creature card from your graveyard" => {
                    "exile creature card from graveyard".to_string()
                }
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
                "creature or Vehicle" => "creature or Vehicle",
                "tapped creature" => "tapped creature",
                "nonland permanent" => "nonland permanent",
                _ => return Err(Skip::Sentence(line.to_string())),
            };
            self.statements.push(format!("enchant {spec}"));
            return Ok(());
        }
        if let Some(c) = Regex::new(r"^Equip \{(\d+)\}$").unwrap().captures(line) {
            self.statements.push(format!("equip {{{}}}", cap(&c, 1)));
            return Ok(());
        }
        // A flavor word before equip ("Perseus's Bow — Equip {6}", job
        // select Equipment).
        if let Some(c) = Regex::new(r#"^([A-Z][^"—]*) — Equip \{(\d+)\}$"#)
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("label \"{}\" equip {{{}}}", cap(&c, 1), cap(&c, 2)));
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
        if let Some(c) = Regex::new(
            r"^(?:This (?:land|creature|artifact|permanent)|~) enters tapped unless (.+)\.$",
        )
        .unwrap()
        .captures(line)
            && let Some(cond) = static_condition(cap(&c, 1))
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
        // Player-level statics and the Leylines (CR 305.2, 702.11c,
        // 702.18c, 103.6a).
        let player_static = match line {
            "You have hexproof." => Some("you have hexproof"),
            "You have shroud." => Some("you have shroud"),
            "Your opponents can't cast spells during your turn." => {
                Some("opponents can't cast spells during your turn")
            }
            "Players can't gain life." => Some("players can't gain life"),
            "Damage can't be prevented." => Some("damage can't be prevented"),
            "You may play an additional land on each of your turns." => Some("additional land"),
            "You may play two additional lands on each of your turns." => Some("additional land 2"),
            "Each player may play an additional land on each of their turns." => {
                Some("additional land each player")
            }
            "If this card is in your opening hand, you may begin the game with it on the battlefield." => {
                Some("opening hand")
            }
            _ => None,
        };
        if let Some(s) = player_static {
            self.statements.push(s.to_string());
            return Ok(());
        }
        if let Some(c) = Regex::new(
            r"^As long as (.+), you may play an additional land on each of your turns\.$",
        )
        .unwrap()
        .captures(line)
            && let Some(cond) = static_condition(cap(&c, 1))
        {
            self.statements
                .push(format!("additional land as long as {cond}"));
            return Ok(());
        }
        // "Hexproof from black", "Hexproof from monocolored", "Hexproof
        // from instants" (CR 702.11d).
        if let Some(q) = line.strip_prefix("Hexproof from ")
            && !q.contains(" and ")
            && !q.contains(',')
        {
            let what = match q {
                "white" | "blue" | "black" | "red" | "green" | "multicolored" | "monocolored"
                | "colorless" => q.to_string(),
                _ => singular(q),
            };
            self.statements.push(format!("hexproof from {what}"));
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
            r"^(Bushido|Soulshift|Afflict|Toxic|Crew|Bloodthirst|Modular|Backup|Saddle|Fabricate|Renown|Afterlife|Casualty|Teamwork|Ripple|Dredge|Firebending|Graft|Vanishing) (\d+)$",
        )
        .unwrap()
        .captures(line)
        {
            self.statements
                .push(format!("{} {}", cap(&c, 1).to_lowercase(), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.189a: "Firebending X, where X is the number of ...".
        if let Some(c) = Regex::new(r"^Firebending X, where X is the number of (.+)\.$")
            .unwrap()
            .captures(line)
        {
            let x = count_expr(cap(&c, 1)).ok_or_else(|| Skip::Sentence(line.to_string()))?;
            self.statements.push(format!("firebending count where {x}"));
            return Ok(());
        }
        if line == "Firebending X, where X is this creature's power." {
            self.statements
                .push("firebending count where power".to_string());
            return Ok(());
        }
        // A disturb card's back face (CR 702.146).
        if line == "If ~ would be put into a graveyard from anywhere, exile it instead." {
            self.statements
                .push("exiled instead of graveyard".to_string());
            return Ok(());
        }
        // CR 702.145.
        if line == "Daybound" || line == "Nightbound" {
            self.statements.push(line.to_lowercase());
            return Ok(());
        }
        // CR 702.95a.
        if line == "Soulbond" {
            self.statements.push("soulbond".to_string());
            return Ok(());
        }
        // Soulbond (CR 702.95b): "As long as this creature is paired with
        // another creature, both creatures have flying.", "... each of
        // those creatures gets +1/+1.", authored as an Aura's static
        // ability and given the paired subject.
        if let Some(c) = Regex::new(
            r"^As long as (this creature|~) is paired with another creature, (?:both creatures have|each of those creatures (has|gets)) (.+)$",
        )
        .unwrap()
        .captures(line)
        {
            let verb = c.get(2).map_or("has", |m| m.as_str());
            let inner = format!("Enchanted creature {verb} {}", cap(&c, 3));
            let made = self
                .quoted(&inner)
                .ok_or_else(|| Skip::Sentence(line.to_string()))?;
            let rest = made
                .strip_prefix("static enchanted: ")
                .ok_or_else(|| Skip::Sentence(line.to_string()))?;
            let named = if cap(&c, 1) == "~" { " named" } else { "" };
            self.statements.push(format!("static paired{named}: {rest}"));
            return Ok(());
        }
        // CR 702.44a.
        if line == "Sunburst" {
            self.statements.push("sunburst".to_string());
            return Ok(());
        }
        // CR 702.63b: vanishing without a number.
        if line == "Vanishing" {
            self.statements.push("vanishing".to_string());
            return Ok(());
        }
        if let Some(c) =
            Regex::new(r"^(Ward|Echo|Unearth|Buyback|Outlast|Morph|Megamorph|Disguise|Evoke|Madness|Mayhem|Foretell|Entwine|Ninjutsu|Scavenge|Dash|Warp|Embalm|Eternalize|Offspring|Bestow|Overload|Surge|Spectacle|Replicate|Encore|Plot|Sneak|Harmonize|Freerunning|Multikicker|Transmute|Disturb) ((?:\{[0-9WUBRGCX/]+\})+)$")
                .unwrap()
                .captures(line)
        {
            self.statements
                .push(format!("{} {}", cap(&c, 1).to_lowercase(), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.176: "Impending 4—{1}{U}{U}".
        if let Some(c) = Regex::new(r"^Impending (\d+)—((?:\{[0-9WUBRGCX/]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("impending {} {}", cap(&c, 1), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.188: "Web-slinging {W}".
        if let Some(c) = Regex::new(r"^Web-slinging ((?:\{[0-9WUBRGCX/]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("web slinging {}", cap(&c, 1)));
            return Ok(());
        }
        // CR 702.174: "Gift a card", "Gift a tapped Fish".
        if let Some(c) = Regex::new(r"^Gift (a card|a Food|a tapped Fish|a Treasure)$")
            .unwrap()
            .captures(line)
        {
            let kind = match cap(&c, 1) {
                "a card" => "card",
                "a Food" => "food",
                "a tapped Fish" => "tapped fish",
                _ => "treasure",
            };
            self.statements.push(format!("gift {kind}"));
            return Ok(());
        }
        // CR 702.167: "Craft with artifact {2}{R}" (one material).
        if let Some(c) =
            Regex::new(r"^Craft with (artifact|creature|[A-Z][a-z]+) ((?:\{[0-9WUBRGC]+\})+)$")
                .unwrap()
                .captures(line)
        {
            self.statements
                .push(format!("craft with {} {}", cap(&c, 1), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.21: "Ward—Pay 2 life."
        if let Some(c) = Regex::new(r"^Ward\u{2014}Pay (\d+) life\.?$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("ward pay {} life", cap(&c, 1)));
            return Ok(());
        }
        if line == "Ward\u{2014}Discard a card." {
            self.statements.push("ward discard 1 card".to_string());
            return Ok(());
        }
        // CR 702.113: "Awaken 3—{5}{B}".
        if let Some(c) = Regex::new(r"^Awaken (\d+)\u{2014}((?:\{[0-9WUBRGCX/]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("awaken {} {}", cap(&c, 1), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.77: "Reinforce 1—{1}{R}".
        if let Some(c) = Regex::new(r"^Reinforce (\d+)\u{2014}((?:\{[0-9WUBRGCX/]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("reinforce {} {}", cap(&c, 1), cap(&c, 2)));
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
        if line == "Storm" || line == "Assist" || line == "Enlist" {
            self.statements.push(line.to_lowercase());
            return Ok(());
        }
        if line == "Jump-start" {
            self.statements.push("jump start".to_string());
            return Ok(());
        }
        if line == "Living weapon" {
            self.statements.push("living weapon".to_string());
            return Ok(());
        }
        if line == "Job select" {
            self.statements.push("job select".to_string());
            return Ok(());
        }
        if line == "Bargain" {
            self.statements.push("bargain".to_string());
            return Ok(());
        }
        if matches!(
            line,
            "Evolve" | "Unleash" | "Extort" | "Mentor" | "Riot" | "Retrace" | "Ingest"
        ) {
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
        if let Some(c) = Regex::new(
            r"^As this land enters, you may pay (\d+) life\. If you don't, it enters tapped\.$",
        )
        .unwrap()
        .captures(line)
        {
            self.statements
                .push(format!("enters tapped unless pay {} life", cap(&c, 1)));
            return Ok(());
        }
        if line == "This creature enters prepared." || line == "~ enters prepared." {
            self.statements.push("enters prepared".to_string());
            return Ok(());
        }
        // CR 604.3: "~'s power and toughness are each equal to the number
        // of creatures you control."
        if let Some(c) = Regex::new(
            r"^~'s (power and toughness are each|power is|toughness is) equal to the number of (.+)\.$",
        )
        .unwrap()
        .captures(line)
            && let Some(expr) = count_expr(cap(&c, 2))
        {
            let which = match cap(&c, 1) {
                "power is" => "power",
                "toughness is" => "toughness",
                _ => "pt",
            };
            // Permanents are `count <filter>`; other counts as written.
            let count = if expr.ends_with(" you control") || expr.ends_with(" on battlefield") {
                "count "
            } else {
                ""
            };
            self.statements
                .push(format!("static this: cda {which} {count}{expr}"));
            return Ok(());
        }
        if line == "This land enters tapped."
            || line == "~ enters tapped."
            || line == "This creature enters tapped."
            || line == "This artifact enters tapped."
            || line == "This permanent enters tapped."
        {
            self.statements.push("enters tapped".to_string());
            return Ok(());
        }
        if let Some(c) = Regex::new(
            r"^(?:This (?:creature|permanent|artifact|land|enchantment|Vehicle)|~) enters with (\S+) (\+1/\+1|-1/-1|stun|charge|oil|spore|depletion|quest|ki|time|tide|shield|age|fade|ice|level|lore|page|divinity) counters? on it(?: if (.+))?\.$",
        )
        .unwrap()
        .captures(line)
        {
            let n = number(cap(&c, 1)).ok_or_else(|| Skip::Sentence(line.to_string()))?;
            if cap(&c, 3).is_empty() {
                self.statements
                    .push(format!("enters with {n} {} counters", cap(&c, 2)));
            } else {
                // Raid: "... if you attacked this turn." (any static
                // condition, judged as it enters).
                let cond = static_condition(cap(&c, 3))
                    .or_else(|| (cap(&c, 3) == "you cast it from your hand").then(|| "cast from hand".to_string()))
                    .ok_or_else(|| Skip::Sentence(line.to_string()))?;
                let s = if n == "1" { "" } else { "s" };
                self.statements
                    .push(format!("enters with {n} {} counter{s} if {cond}", cap(&c, 2)));
            }
            return Ok(());
        }
        // Multikicker (CR 702.33d): "This creature enters with a +1/+1
        // counter on it for each time it was kicked."
        if let Some(c) = Regex::new(
            r"^This (?:creature|artifact|enchantment) enters with an? (\+1/\+1|charge) counter on it for each time it was kicked\.$",
        )
        .unwrap()
        .captures(line)
        {
            self.statements.push(format!(
                "enters with count each times kicked {} counters",
                cap(&c, 1)
            ));
            return Ok(());
        }
        // "This creature enters with a +1/+1 counter on it for each color
        // of mana spent to cast it." (converge), "... two +1/+1 counters
        // ... for each ...".
        if let Some(c) = Regex::new(
            r"^This (?:creature|permanent|artifact|enchantment|Vehicle) enters with (\S+) (\+1/\+1|-1/-1|[a-z]+) counters? on it for each (.+)\.$",
        )
        .unwrap()
        .captures(line)
        {
            let n = number(cap(&c, 1)).ok_or_else(|| Skip::Sentence(line.to_string()))?;
            let what = count_expr(cap(&c, 3)).ok_or_else(|| Skip::Sentence(line.to_string()))?;
            let per = if n == "1" {
                String::new()
            } else {
                format!("{n} ")
            };
            self.statements
                .push(format!("enters with count each {per}{what} {} counters", cap(&c, 2)));
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
            r"^(Enchanted creature|Equipped creature|Creatures you control|Other creatures you control|Creatures your opponents control|All creatures|White creatures|Blue creatures|Black creatures|Red creatures|Green creatures|Enchanted land|Enchanted permanent) (.+)\.$",
        )
        .unwrap()
        .captures(line)
        {
            let filter = match cap(&c, 1) {
                "Enchanted creature" | "Equipped creature" => "enchanted".to_string(),
                "Enchanted land" | "Enchanted permanent" => return Err(Skip::Sentence(line.to_string())),
                "Creatures you control" => "creature you control".to_string(),
                "Other creatures you control" => "other creature you control".to_string(),
                "Creatures your opponents control" => "creature your opponents control".to_string(),
                "All creatures" => "creature".to_string(),
                colored => format!(
                    "{} creature",
                    colored.split(' ').next().unwrap_or("").to_ascii_lowercase()
                ),
            };
            let mut mods = Vec::new();
            let gets = Regex::new(r"^gets? ([+-]\d+/[+-]\d+)$").unwrap();
            // "gets +2/+2, has reach, and is a Bard in addition to its
            // other types" (job select Equipment): a list of three.
            let body = cap(&c, 2);
            let parts: Vec<&str> = if body.ends_with(" in addition to its other types") {
                Regex::new(r",? and |, ").unwrap().split(body).collect()
            } else {
                body.split(" and ").collect()
            };
            let subtype = Regex::new(r"^is an? ([A-Z][a-z]+) in addition to its other types$").unwrap();
            for part in parts {
                if let Some(c2) = gets.captures(part) {
                    mods.push(cap(&c2, 1).to_string());
                } else if let Some(c2) = subtype.captures(part) {
                    mods.push(format!("add subtype {}", cap(&c2, 1)));
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
        // Loyalty abilities (CR 606): "+1: ...", "−2: ...", "0: ...".
        if let Some(c) = Regex::new(r"^([+\u{2212}]?)(\d+|X): (.+)$")
            .unwrap()
            .captures(line)
            && word == "activated"
        {
            let sign = match cap(&c, 1) {
                "+" => "+",
                "" => "",
                _ => "-",
            };
            let mut targets = 0;
            let effects = self.templates.effects(cap(&c, 3), &mut targets)?;
            self.statements
                .push(format!("loyalty {sign}{}: {effects}", cap(&c, 2)));
            return Ok(());
        }
        // Activated abilities: "<cost>: <effects>[ Activate only once each turn.]".
        if let Some((cost, rest)) = line.split_once(": ")
            && let Some(mut cost) = cost_text(cost)
        {
            // Where the cost ends and its restrictions begin.
            let base_len = cost.len();
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
            // Activation restrictions (CR 602.5b): "Activate only during
            // your turn.", "Activate only if you gained life this turn."
            let rest = if let Some((r, when)) = rest.rsplit_once(" Activate only ")
                && let Some(when) = when.strip_suffix('.')
            {
                for part in when.split(" and only ") {
                    let restriction = match part {
                        "during your turn" => "during your turn".to_string(),
                        "during your upkeep" => "during your upkeep".to_string(),
                        "during combat" => "during combat".to_string(),
                        "during an opponent's upkeep" => "during opponent upkeep".to_string(),
                        "before attackers are declared" => "before attackers".to_string(),
                        "once" => "once ever".to_string(),
                        "as a sorcery" => "sorcery".to_string(),
                        "once each turn" => "once".to_string(),
                        w => {
                            // "if this land entered this turn or if you
                            // control a basic land".
                            let one = |c: &str| match c {
                                "this land entered this turn" => {
                                    Some("this is that entered this turn".to_string())
                                }
                                "you control a basic land" => {
                                    Some("you control basic land".to_string())
                                }
                                c => static_condition(c),
                            };
                            let cond =
                                w.strip_prefix("if ")
                                    .and_then(|c| match c.split_once(" or if ") {
                                        Some((a, b)) => {
                                            Some(format!("{} or if {}", one(a)?, one(b)?))
                                        }
                                        None => one(c),
                                    });
                            cond.map(|c| format!("if {c}"))
                                .ok_or_else(|| Skip::Sentence(line.to_string()))?
                        }
                    };
                    let _ = write!(cost, ", {restriction}");
                }
                r
            } else {
                rest
            };
            // "{2}{B}: Return this card from your graveyard to your hand."
            // is activated from the graveyard.
            let from_graveyard = match rest.trim_end_matches('.') {
                "Return this card from your graveyard to your hand" => Some("return this to hand"),
                "Return this card from your graveyard to the battlefield" => {
                    Some("return this to battlefield")
                }
                "Return this card from your graveyard to the battlefield tapped" => {
                    Some("return this to battlefield tapped")
                }
                _ => None,
            };
            if let Some(effect) = from_graveyard {
                // The restrictions after `from graveyard`: a cost's list
                // of alternatives ("sacrifice artifact or creature")
                // would take a restriction after it as one more.
                let (base, restrictions) = cost.split_at(base_len);
                self.statements.push(format!(
                    "{word} {base}, from graveyard{restrictions}: {effect}"
                ));
                return Ok(());
            }
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
            // "This ability triggers only once each turn.", "Do this only
            // once each turn."
            let (rest, limit) = [
                (
                    " This ability triggers only once each turn.",
                    "once each turn ",
                ),
                (
                    " This ability triggers only twice each turn.",
                    "twice each turn ",
                ),
                (" Do this only once each turn.", "do once each turn "),
            ]
            .iter()
            .find_map(|(s, l)| Some((rest.strip_suffix(s)?, *l)))
            .unwrap_or((rest, ""));
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
            } else if let Some(r) = rest.strip_prefix("if this creature isn't prepared, ") {
                (" if this isn't prepared", r)
            } else if let Some(r) = rest.strip_prefix("if no creatures are on the battlefield, ") {
                (" if no creatures are on the battlefield", r)
            } else if let Some(r) = rest
                .strip_prefix("if it was kicked, ")
                .or_else(|| rest.strip_prefix("if the gift was promised, "))
            {
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
            // "Whenever you cast a multicolored spell, you may return this
            // card from your graveyard to your hand." triggers from the
            // graveyard (a dies trigger looks back instead).
            let zone = if rest.contains("this card from your graveyard")
                && !trigger.starts_with("when this dies")
                && !trigger.starts_with("whenever this dies")
            {
                "from graveyard "
            } else {
                ""
            };
            self.statements.push(format!(
                "triggered {zone}{limit}{trigger}{condition}: {effects}"
            ));
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

    /// "Spree" with its modes and their costs (CR 702.172).
    fn spree(&mut self, modes: &[(String, String)]) -> Result<(), Skip> {
        let mut lines = vec!["spree {".to_string()];
        for (cost, m) in modes {
            let mut targets = 0;
            let effects = self.templates.effects(m, &mut targets)?;
            lines.push(format!("  mode {cost}: {effects}"));
        }
        lines.push("}".to_string());
        self.statements.push(lines.join("\n  "));
        Ok(())
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
        "prepare" => "prepare",
        "modal_dfc" => "modal_dfc",
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

/// How much of two texts agrees: the common prefix and suffix lengths.
fn similarity(a: &str, b: &str) -> usize {
    let pre = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    let suf = a
        .bytes()
        .rev()
        .zip(b.bytes().rev())
        .take_while(|(x, y)| x == y)
        .count();
    pre + suf.min(a.len().min(b.len()) - pre)
}

/// Single-word rewrites of a card file that name the same object or group
/// another way, so they change only the wording the renderer prints:
/// `each`/`all` (a filter's wording flags), `this`/`this creature` (the
/// source either way), and `this`/`it` where `it` is the source too (a
/// spell or an activated ability) or the triggering object of a trigger
/// on something other than this permanent, which is what Oracle's "it"
/// names there.
#[allow(clippy::too_many_lines)]
fn wording_variants(text: &str) -> Vec<String> {
    const VERBS: [&str; 10] = [
        "untap",
        "tap",
        "destroy",
        "exile",
        "sacrifice",
        "regenerate",
        "from",
        "then",
        "and",
        "return",
    ];
    const HEADS: [&str; 10] = [
        "card ",
        "oracle_id ",
        "cost ",
        "type ",
        "pt ",
        "loyalty ",
        "defense ",
        "color ",
        "keywords ",
        "layout ",
    ];
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    // Inside a block (`spell {`, `choose one {`), whether its opener lets
    // `it` name the source.
    let mut block_it_ok = false;
    for (i, line) in lines.iter().enumerate() {
        let body = line.trim_start();
        // `label "Channel" activated ...`: the ability after its label.
        let ability = match body.strip_prefix("label \"") {
            Some(rest) => rest.split_once("\" ").map_or(body, |(_, b)| b),
            None => body,
        };
        let opener_it_ok = [
            "activated ",
            "spell",
            "loyalty ",
            "boast ",
            "exhaust ",
            "power-up ",
            "choose ",
        ]
        .iter()
        .any(|p| ability.starts_with(p))
            || (ability.starts_with("triggered when")
                && !ability.starts_with("triggered when this")
                && !ability.starts_with("triggered whenever this"));
        let indent = line.len() - body.len();
        if body.ends_with('{') {
            block_it_ok = opener_it_ok && !body.starts_with("triggered ");
        }
        if body == "}"
            || (HEADS.iter().any(|h| body.starts_with(h))
                && !body.starts_with("loyalty +")
                && !body.starts_with("loyalty -")
                && !body.starts_with("loyalty 0:"))
        {
            continue;
        }
        if body.contains("delayed") || body.starts_with("face ") {
            continue;
        }
        let it_ok = if indent > 2 {
            block_it_ok
        } else {
            opener_it_ok
        };
        // Only the effects: after the first ": " of a statement line.
        let start = if indent > 2 && !body.starts_with("mode") {
            0
        } else {
            match line.find(": ") {
                Some(k) => line[..k].split(' ').count(),
                None => continue,
            }
        };
        let words: Vec<&str> = line.split(' ').collect();
        for (j, w) in words.iter().enumerate().skip(start) {
            let prev = if j > 0 { words[j - 1] } else { "" };
            let next = words.get(j + 1).copied().unwrap_or("");
            let after_clause = prev.ends_with(':') || prev.ends_with(';') || VERBS.contains(&prev);
            let mut alts: Vec<String> = Vec::new();
            match *w {
                "each" if after_clause && next != "counters" => alts.push("all".to_string()),
                "all" if after_clause && !matches!(next, "counters" | "graveyards") => {
                    alts.push("each".to_string());
                }
                "this" if !matches!(next, "turn" | "spell" | "creature" | "way") => {
                    alts.push("this creature".to_string());
                    // Oracle's "it" names the source only before anything
                    // else has been named: after a token, a target, or a
                    // moved card, "it" is that object, which `this` and
                    // `it` here are not.
                    // "put a +1/+1 counter on it": `put` names nothing
                    // before "it".
                    let before = words[start..j]
                        .iter()
                        .filter(|w| !(**w == "put" && prev == "on"))
                        .copied()
                        .collect::<Vec<_>>()
                        .join(" ");
                    let named = [
                        "create", "target", "that", "return", "exile", "search", "reveal", "dig",
                        "put", "attach", "copy", "mill", "discard",
                    ]
                    .iter()
                    .any(|w| before.split(' ').any(|b| b == *w));
                    if it_ok && !named {
                        alts.push("it".to_string());
                    }
                }
                "this" if next == "creature" => alts.push(String::new()),
                // "{1}{W}, Discard this card: It deals 4 damage ...": the
                // source, named "it" where `it` is the source.
                "deal" if it_ok && prev.ends_with(':') => alts.push("it deals".to_string()),
                _ => {}
            }
            for alt in alts {
                let mut ws: Vec<String> = words.iter().map(|s| (*s).to_string()).collect();
                if alt.is_empty() {
                    // `this creature` → `this`.
                    ws.remove(j + 1);
                } else {
                    ws[j] = alt;
                }
                let mut ls: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
                ls[i] = ws.join(" ");
                out.push(ls.join("\n") + "\n");
            }
        }
    }
    out
}

/// Build the card file for one Scryfall record, or say why not.
#[allow(clippy::too_many_lines)]
/// A legendary character's Oracle text with "he"/"she" and their other
/// forms read as "it"/"its", and which pronoun it used (`he`, `she`).
fn neuter_pronouns(oracle: &str) -> (String, Option<&'static str>) {
    let he = Regex::new(r"\b(?:[Hh]e|[Hh]im|[Hh]is)\b").unwrap();
    let she = Regex::new(r"\b(?:[Ss]he|[Hh]er)\b").unwrap();
    let pronoun = match (he.is_match(oracle), she.is_match(oracle)) {
        (true, false) => "he",
        (false, true) => "she",
        _ => return (oracle.to_string(), None),
    };
    let word = Regex::new(r"\b([Hh]e|[Hh]im|[Hh]is|[Ss]he|[Hh]er)\b( ?)(\w*)").unwrap();
    let out = word.replace_all(oracle, |c: &regex::Captures| {
        let w = &c[1];
        let next = &c[3];
        // "her owner's", "her power": possessive; "on her", "her to the
        // battlefield": the object.
        let possessive = w.eq_ignore_ascii_case("his")
            || (w.eq_ignore_ascii_case("her")
                && !c[2].is_empty()
                && next.chars().next().is_some_and(char::is_lowercase)
                && !matches!(
                    next,
                    "to" | "into"
                        | "onto"
                        | "and"
                        | "or"
                        | "on"
                        | "from"
                        | "with"
                        | "until"
                        | "this"
                        | "that"
                        | "a"
                        | "an"
                        | "the"
                        | "as"
                        | "if"
                        | "unless"
                        | "at"
                        | "under"
                        | "instead"
                ));
        let it = if possessive { "its" } else { "it" };
        let it = if w.starts_with(char::is_uppercase) {
            if possessive { "Its" } else { "It" }
        } else {
            it
        };
        format!("{it}{}{next}", &c[2])
    });
    (out.into_owned(), Some(pronoun))
}

#[allow(clippy::too_many_lines)] // one step per part of the card
pub fn author_card(s: &ScryCard, corpus: &[Card]) -> Result<Authored, Skip> {
    if matches!(
        s.layout.as_str(),
        "adventure" | "transform" | "split" | "prepare" | "modal_dfc"
    ) && s.card_faces.len() == 2
    {
        return author_faces(s, corpus);
    }
    if s.name.contains(" // ") || !matches!(s.layout.as_str(), "normal" | "saga" | "class" | "case")
    {
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
            | "Land" | "Planeswalker" | "Kindred" | "Battle" => {}
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
            // A flavor word before equip ("Hagneia — Equip {3}") is tagged
            // as a keyword; the line is templated as a labeled equip.
            && !s
                .oracle_text
                .as_deref()
                .is_some_and(|t| t.contains(&format!("{k} \u{2014} Equip {{")))
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
    let mut oracle = oracle.replace(&s.name, "~");
    // A Saga, Class, or Case names itself so; the DSL's `this` prints the
    // noun.
    for noun in ["Saga", "Class", "Case"] {
        oracle = oracle
            .replace(&format!("this {noun}"), "this enchantment")
            .replace(&format!("This {noun}"), "This enchantment");
    }
    // A legendary card names itself by the part before a comma ("Thrun"
    // for "Thrun, the Last Troll"; CR 201.5), as the renderer prints it.
    if type_line.contains("Legendary")
        && let Some((short, _)) = s.name.split_once(", ")
    {
        let word = Regex::new(&format!(r"\b{}\b", regex::escape(short))).unwrap();
        oracle = word.replace_all(&oracle, "~").into_owned();
    }
    // "Ajani" for "Ajani Resolute", "Smaug" for "Smaug the Magnificent":
    // a legendary card without a comma in its name may call itself by its
    // first word (not "Ajani's Pridemate", a token's name).
    let mut short = None;
    if type_line.contains("Legendary")
        && !s.name.contains(", ")
        && let Some((first, _)) = s.name.split_once(' ')
        && first != "The"
        && first.chars().all(char::is_alphabetic)
    {
        let word = Regex::new(&format!(r"\b{}\b('s )?", regex::escape(first))).unwrap();
        let mut found = false;
        oracle = word
            .replace_all(&oracle, |c: &regex::Captures| {
                if c.get(1).is_some() {
                    c[0].to_string()
                } else {
                    found = true;
                    "~".to_string()
                }
            })
            .into_owned();
        if found {
            short = Some(first.to_string());
        }
    }
    // A character's text calls it "he" or "she"; the templates read "it"
    // and the card gets a `pronoun` line.
    let pronoun = if type_line.contains("Legendary") {
        let (oracle_it, pronoun) = neuter_pronouns(&oracle);
        oracle = oracle_it;
        pronoun
    } else {
        None
    };
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
        // CR 702.172: "Spree" and its "+ {1} — ..." modes.
        if line == "Spree" {
            let mut modes = Vec::new();
            i += 1;
            while i < lines.len() && lines[i].starts_with("+ ") {
                let Some((cost, rest)) = lines[i][2..].split_once(" — ") else {
                    return Err(Skip::Sentence(lines[i].to_string()));
                };
                modes.push((cost.to_string(), rest.to_string()));
                i += 1;
            }
            b.spree(&modes)?;
            continue;
        }
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
            // "Landfall — Whenever a land you control enters, choose one —".
            let (label, head) = match head.split_once(" — ") {
                Some((word, h)) if ABILITY_WORDS.contains(&word) => (Some(word), h),
                _ => (None, head),
            };
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
            if let Some(l) = label
                && let Some(st) = b.statements.last_mut()
            {
                *st = format!("label \"{l}\" {st}");
            }
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
        // A characteristic-defining ability sets a `*` (CR 604.3): `pt
        // */1` beside `cda power ...`, no line for `*/*`.
        let cda = b.statements.iter().any(|st| st.contains(": cda "));
        if (p.contains('*') || t.contains('*')) && !cda {
            return Err(Skip::Sentence("variable power/toughness".to_string()));
        }
        if p != "*" || t != "*" {
            let _ = writeln!(text, "  pt {p}/{t}");
        }
    }
    if let Some(l) = &s.loyalty {
        let _ = writeln!(text, "  loyalty {l}");
    }
    if let Some(d) = &s.defense {
        let _ = writeln!(text, "  defense {d}");
    }
    if let Some(n) = &short {
        let _ = writeln!(text, "  short_name {n:?}");
    }
    if let Some(p) = pronoun {
        let _ = writeln!(text, "  pronoun {p}");
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
    let by_name: std::collections::HashMap<&str, &Card> = corpus
        .iter()
        .chain(token_cards.iter())
        .map(|c| (c.name.as_str(), c))
        .collect();
    let lookup: &dyn Lookup = &by_name;
    let expected = render::normalize(s.oracle_text.as_deref().unwrap_or(""));
    let render_of = |text: &str| -> Result<String, Skip> {
        let mut cards = mtg_dsl::parse(text).map_err(|e| Skip::Sentence(format!("parse: {e}")))?;
        let card = cards.remove(0);
        if card.kind != Kind::Card {
            return Err(Skip::Layout("not a card".to_string()));
        }
        Ok(render::normalize(&render::oracle_text(&card, lookup)))
    };
    let mut rendered = render_of(&text)?;
    // A near miss may be wording only: try the DSL's synonyms for the
    // same object (`each`/`all`, `this`/`this creature`/`it`), keeping
    // whichever brings the text closer, until it matches.
    let mut rounds = 0;
    while rendered != expected && rounds < 4 {
        rounds += 1;
        let score = |r: &str| similarity(r, &expected);
        let best = wording_variants(&text)
            .into_iter()
            .filter_map(|v| render_of(&v).ok().map(|r| (score(&r), v, r)))
            .max_by_key(|(sc, _, _)| *sc);
        match best {
            Some((sc, variant, again)) if sc > score(&rendered) => {
                text = variant;
                rendered = again;
            }
            _ => break,
        }
    }
    if rendered != expected {
        // `MTG_AUTHOR_SHOW=1`: print the near miss's card file.
        if std::env::var_os("MTG_AUTHOR_SHOW").is_some() {
            eprintln!("{text}");
        }
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
