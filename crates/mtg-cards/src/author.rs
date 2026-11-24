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

/// A compiled regex, built once per pattern: the templates name hundreds
/// of patterns, most built per card or per line, and compiling them was
/// most of the author's time (a Modern run took over ten minutes).
fn cached_regex(pattern: &str) -> Result<Regex, regex::Error> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(r) = cache.lock().unwrap().get(pattern) {
        return Ok(r.clone());
    }
    let r = Regex::new(pattern)?;
    cache.lock().unwrap().insert(pattern.to_string(), r.clone());
    Ok(r)
}

/// Keyword abilities the engine implements (CR 702), as Scryfall tags them.
const KEYWORDS: &[&str] = &[
    "Split second",
    "Cipher",
    "Paradigm",
    "Aftermath",
    "Fuse",
    "Meld",
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
    "Read ahead",
    "Read Ahead",
    "Champion",
    "Cleave",
    "Haunt",
    "Reconfigure",
    "Devour",
    "Amplify",
    "Tribute",
    "Emerge",
    "Miracle",
    "Prowl",
    "Escalate",
    "Recover",
    "Blitz",
    "Prototype",
    "Level Up",
    "Provoke",
    "Fading",
    "Annihilator",
    "Rampage",
    "Battle Cry",
    "For Mirrodin!",
    "Training",
    "Melee",
    "Dethrone",
    "Myriad",
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
    "Compleated",
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
    "Increment",
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
    "Recruit",
    "Airbend",
    "Waterbend",
    "Firebending",
    "Double",
    "Ascend",
    "Storied",
    "Venture into the dungeon",
    "Clash",
    "Learn",
    "Descend",
    "Umbra armor",
];

/// Ability words (CR 207.2c): labels with no rules meaning, printed
/// "Landfall — <ability>". Scryfall tags them like keywords.
const ABILITY_WORDS: &[&str] = &[
    // CR 702.57: a keyword printed as a label ("Forecast — {2}{W}, Reveal
    // this card from your hand: ..."); the DSL's `label "Forecast"`.
    "Forecast",
    // Scryfall tags jump-start "Jump" too; "Jump — During your turn, ..."
    // (Final Fantasy) is an ability word.
    "Jump",
    "Repartee",
    "Parley",
    "Grandeur",
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
    "Fathomless descent",
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
    let re = cached_regex(
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

/// "Create Mowu, a legendary 3/3 green Dog creature token" (CR 111.4: a
/// named token): the key is the description from the name on ("Mowu, a
/// legendary 3/3 green Dog creature", keywords included).
const LEGENDARY_TOKEN_PHRASE: &str = r"^([Cc]reate|[Yy]ou may create) ((?:[A-Z][^,]*?)(?:, [A-Za-z ]+?)?, a legendary \d+/\d+ (?:colorless|(?:white|blue|black|red|green)(?: and (?:white|blue|black|red|green))?) (?:[A-Z][a-z]+ )+(?:artifact )?creature) token((?: with [a-z ,]+)?)$";

/// The token block for a key from `LEGENDARY_TOKEN_PHRASE`: a legendary
/// creature token named by its key's first part.
fn legendary_token_block(key: &str) -> Option<String> {
    let re = cached_regex(
        r"^(.+?), a legendary (\d+)/(\d+) (colorless|(?:white|blue|black|red|green)(?: and (?:white|blue|black|red|green))?) ((?:[A-Z][a-z]+ )+)(artifact )?creature(?: with ([a-z ,]+))?$",
    )
    .unwrap();
    let c = re.captures(key)?;
    let name = cap(&c, 1);
    let subtypes = cap(&c, 5).trim();
    let color = match cap(&c, 4) {
        "colorless" => String::new(),
        cs => format!(" {}", cs.replace(" and ", " ")),
    };
    let types = if cap(&c, 6).is_empty() {
        "Creature"
    } else {
        "Artifact Creature"
    };
    let keywords = if cap(&c, 7).is_empty() {
        String::new()
    } else {
        format!("  keywords {}\n", keyword_list(cap(&c, 7))?)
    };
    Some(format!(
        "token \"{key}\" {{\n  name \"{name}\"\n  type Legendary {types} — {subtypes}\n  pt {}/{}\n  color{color}\n{keywords}}}\n",
        cap(&c, 2),
        cap(&c, 3)
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
    let s = cached_regex(r"\{[^}]*\}").unwrap().replace_all(s, "{M}");
    let s = cached_regex(r"\b\d+\b").unwrap().replace_all(&s, "N");
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
    let mut pool = None;
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
        match author_card(&card, corpus).and_then(|a| {
            compiles(&mut pool, corpus, &a)?;
            Ok(a)
        }) {
            Ok(a) => report.authored.push(a),
            Err(skip) => report.skipped.push((card.name.clone(), skip)),
        }
    }
    Ok(report)
}

/// The authored card compiles against the corpus (built on first use),
/// with the tokens it brings: a draft that renders right but doesn't
/// compile (Ravager's Mace) is skipped.
fn compiles(
    pool: &mut Option<mtg_core::card::CardPool>,
    corpus: &[Card],
    a: &Authored,
) -> Result<(), Skip> {
    let pool = match pool {
        Some(p) => p,
        None => pool.insert(
            mtg_dsl::compile::compile_all(corpus)
                .map_err(|e| Skip::Sentence(format!("compile: the corpus: {e}")))?,
        ),
    };
    let mut text = String::new();
    let head = cached_regex(r#"^\w+ "([^"]+)""#).unwrap();
    for (_, block) in &a.tokens {
        let name = head.captures(block).map(|c| c[1].to_string());
        if name.is_some_and(|n| pool.lookup(&n).is_none()) {
            text.push_str(block);
            text.push('\n');
        }
    }
    if pool.lookup(&a.name).is_some() {
        return Ok(());
    }
    text.push_str(&a.text);
    mtg_dsl::load_into(pool, &text).map_err(|e| Skip::Sentence(format!("compile: {e}")))?;
    Ok(())
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

/// The named counter kinds as a regex alternation ("stun|charge|...|first
/// strike"), from `CounterKind::NAMED`, longest first so "first strike"
/// wins over a prefix.
fn counter_names() -> &'static str {
    static ALT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ALT.get_or_init(|| {
        let mut names: Vec<&str> = mtg_core::card::CounterKind::NAMED
            .iter()
            .map(|(n, _)| *n)
            .collect();
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));
        names.join("|")
    })
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
/// "X, where X is ..." for a dig's count: `count where <expr>`.
fn x_count(s: &str) -> Option<String> {
    if let Some(rest) = s.strip_prefix("the number of ") {
        return count_expr(rest).map(|e| format!("count where {e}"));
    }
    match s {
        "this creature's power" | "~'s power" | "its power" => {
            Some("count where power".to_string())
        }
        "your life total" => Some("count where life".to_string()),
        _ => None,
    }
}

/// The amount after "where X is" in DSL count form: the fixed amounts of
/// `bound_amount` and the sacrificed object's characteristic ("the
/// sacrificed artifact's mana value": `mana value of the sacrificed
/// artifact`).
fn x_amount(a: &str) -> Option<String> {
    if let Some(c) = cached_regex(
        r"^the sacrificed (artifact|creature|permanent)'s (mana value|power|toughness)$",
    )
    .expect("a sacrificed pattern")
    .captures(a)
    {
        return Some(format!("{} of the sacrificed {}", &c[2], &c[1]));
    }
    bound_amount(a)
}

/// What a dig takes: "a creature card with mana value 4 or less", "up to
/// two artifact cards", "any number of creature and/or land cards", "a
/// creature card and/or a land card" as `take`'s count and filter
/// (`take 2 artifact card`, `take creature with mana value 4 or less
/// card`).
fn dig_what(s: &str) -> Option<(String, String)> {
    let plural = |r: &str| -> Option<String> {
        let (a, b) = r.split_once(" cards")?;
        let a = a.replace(" and/or ", " or ");
        Some(format!("{a}{b} card").trim().to_string())
    };
    if let Some(r) = s.strip_prefix("any number of ") {
        return Some(("any ".to_string(), plural(r)?));
    }
    if let Some(r) = s.strip_prefix("up to ") {
        let (k, r) = r.split_once(' ')?;
        let k = number(k)?;
        return Some((format!("{k} "), plural(r)?));
    }
    let r = s.strip_prefix("an ").or_else(|| s.strip_prefix("a "))?;
    // "a creature card and/or a land card".
    if let Some((x, y)) = r.split_once(" card and/or ") {
        let y = y.strip_prefix("an ").or_else(|| y.strip_prefix("a "))?;
        let y = y.strip_suffix(" card")?;
        if x.contains(',') || y.contains(',') {
            return None;
        }
        return Some((String::new(), format!("{x} card and {y} card")));
    }
    if r == "card" {
        return Some((String::new(), "card".to_string()));
    }
    let (a, b) = r.split_once(" card")?;
    if a.contains(',') {
        return None;
    }
    Some((String::new(), format!("{a}{b} card").trim().to_string()))
}

/// One card's destination in a dig split: "into your hand", "on top of
/// your library", ...
fn split_dest(s: &str) -> Option<&'static str> {
    match s {
        "into your hand" => Some("hand"),
        "into your graveyard" | "into that player's graveyard" => Some("graveyard"),
        "on top of your library" => Some("top"),
        "on the bottom of your library" => Some("bottom"),
        _ => None,
    }
}

/// The dig splits (`split top N ...`): "Look at the top three cards of
/// your library. Put one of those cards into your hand, one on top of your
/// library, and one on the bottom of your library.", "Look at the top two
/// cards of target opponent's library, then exile one of them.", "Reveal
/// the top three cards of your library. An opponent chooses one of them.
/// Put that card into your graveyard and the rest into your hand.", and
/// the optional reveal of a looked-at card ("You may reveal that card. If
/// an instant or sorcery card is revealed this way, ..."). Returns the DSL
/// and the sentences after it.
fn dig_split(text: &str, targets: &mut usize) -> Option<(String, String)> {
    let dest = r"(into your hand|into your graveyard|on top of your library|on the bottom of your library)";
    if let Some(c) = cached_regex(&format!(
        r"^Look at the top (\w+) cards of your library\. Put one of those cards {dest}, one {dest}, and one {dest}\.?\s*(.*)$"
    ))
    .unwrap()
    .captures(text)
    {
        let n = number(cap(&c, 1))?;
        let ds = [cap(&c, 2), cap(&c, 3), cap(&c, 4)]
            .iter()
            .map(|d| split_dest(d))
            .collect::<Option<Vec<_>>>()?;
        return Some((
            format!("split top {n} into {}", ds.join(", ")),
            cap(&c, 5).to_string(),
        ));
    }
    if let Some(c) = cached_regex(
        r"^Look at the top (\w+) cards of target (opponent|player)'s library, then exile one of them\.?\s*(.*)$",
    )
    .unwrap()
    .captures(text)
    {
        let n = number(cap(&c, 1))?;
        *targets += 1;
        return Some((
            format!("split top {n} of target {} into exile then", cap(&c, 2)),
            cap(&c, 3).to_string(),
        ));
    }
    if let Some(c) = cached_regex(
        r"^Look at the top (\w+) cards of target (opponent|player)'s library\. Put one back and the rest into that player's graveyard\.?\s*(.*)$",
    )
    .unwrap()
    .captures(text)
    {
        let n = number(cap(&c, 1))?;
        *targets += 1;
        return Some((
            format!(
                "split top {n} of target {} into top rest graveyard",
                cap(&c, 2)
            ),
            cap(&c, 3).to_string(),
        ));
    }
    if let Some(c) = cached_regex(
        r"^Look at the top (\w+) cards of your library\. Put one of them into your graveyard\.?\s*(.*)$",
    )
    .unwrap()
    .captures(text)
    {
        let n = number(cap(&c, 1))?;
        return Some((
            format!("split top {n} into graveyard"),
            cap(&c, 2).to_string(),
        ));
    }
    if let Some(c) = cached_regex(
        r"^Reveal the top (\w+) cards of your library\. An opponent chooses one of them\. Put that card (into your hand|into your graveyard) and the rest (into your hand|into your graveyard)\.?\s*(.*)$",
    )
    .unwrap()
    .captures(text)
    {
        let n = number(cap(&c, 1))?;
        return Some((
            format!(
                "split top {n} reveal opponent into {} rest {}",
                split_dest(cap(&c, 2))?,
                split_dest(cap(&c, 3))?
            ),
            cap(&c, 4).to_string(),
        ));
    }
    None
}

/// Where a dig puts the rest: `bottom`, `bottom random`, `graveyard`.
fn dig_rest(s: &str) -> Option<&'static str> {
    match s {
        "on the bottom of your library in a random order" => Some("bottom random"),
        "on the bottom of your library in any order" | "on the bottom of your library" => {
            Some("bottom")
        }
        "into your graveyard" => Some("graveyard"),
        _ => None,
    }
}

/// The dig paragraphs the fixed templates miss: "Look at the top X cards
/// of your library, where X is ...", "You may reveal a creature card with
/// mana value 4 or less from among them ...", "You may reveal up to two
/// artifact cards from among them and put them into your hand", "You may
/// put any number of ... onto the battlefield", "Then put the rest ...".
/// Returns the dig and the sentences after it.
#[allow(clippy::too_many_lines)] // one arm per dig wording
fn dig_general(text: &str) -> Option<(String, String)> {
    // "Reveal cards from the top of your library until you reveal a
    // creature card. Put that card into your hand and the rest on the
    // bottom of your library in a random order."
    if let Some(c) = cached_regex(
        r"^Reveal cards from the top of your library until you reveal an? (.+?) card\. Put that card (into your hand|onto the battlefield(?: tapped)?) and the rest on the bottom of your library in (a random|any) order\.?\s*(.*)$",
    )
    .unwrap()
    .captures(text)
    {
        let filter = cap(&c, 1);
        if filter.contains(',') {
            return None;
        }
        let to = match cap(&c, 2) {
            "into your hand" => "into hand",
            "onto the battlefield" => "onto battlefield",
            _ => "onto battlefield tapped",
        };
        let random = if cap(&c, 3) == "a random" { " random" } else { "" };
        // "the number of cards revealed this way" counts the card put
        // somewhere too (Madcap Experiment).
        return Some((
            format!(
                "reveal until {filter} card; put that card {to} and the rest on bottom of library{random}"
            ),
            cap(&c, 4).to_string(),
        ));
    }
    let head = cached_regex(
        r"^(Look at|Reveal) the top (\w+) cards of your library(?:, where X is ([^.]+?))?\. (.+)$",
    )
    .unwrap();
    let c = head.captures(text)?;
    let n = match (cap(&c, 2), c.get(3)) {
        ("X", Some(x)) => x_count(x.as_str())?,
        (_, Some(_)) => return None,
        (w, None) => number(w)?,
    };
    let reveal_top = if cap(&c, 1) == "Reveal" {
        " reveal"
    } else {
        ""
    };
    let body = cap(&c, 4);
    // A filter's "mana value X" after "where X is ..." is the dig's X
    // (Hatchery Spider), as the engine reads it.
    let reveal = cached_regex(
        r"^You may reveal (.+?) from among them and put (it|that card|them) into your hand\. (Then put|Put) the rest (.+?)\.\s*(.*)$",
    )
    .unwrap();
    let put_hand = cached_regex(
        r"^You may put (.+?) from among them into your hand\. Put the rest (.+?)\.\s*(.*)$",
    )
    .unwrap();
    let onto = cached_regex(
        r"^You may put (.+?) from among them onto the battlefield( tapped)?\. Put the rest (.+?)\.\s*(.*)$",
    )
    .unwrap();
    let one = cached_regex(
        r"^Put one of (them|those cards) into your hand and the (?:rest|other) (.+?)\.\s*(.*)$",
    )
    .unwrap();
    let several = cached_regex(
        r"^Put (up to )?(two|three) of (them|those cards) into your hand and the rest (.+?)\.\s*(.*)$",
    )
    .unwrap();
    if let Some(c) = reveal.captures(body) {
        let (take, what) = dig_what(cap(&c, 1))?;
        let several = !take.is_empty() || what.contains(" and ");
        // `revealing that card` prints "that card", or "them" for several.
        let that = match (cap(&c, 2), several) {
            ("it", false) => "",
            ("them", true) | ("that card", false) => " that card",
            _ => return None,
        };
        let reword = if cap(&c, 3) == "Then put" {
            " reword"
        } else {
            ""
        };
        let rest = dig_rest(cap(&c, 4))?;
        return Some((
            format!(
                "dig {n}{reveal_top} take {take}{what} may revealing{that} rest {rest}{reword}"
            ),
            cap(&c, 5).to_string(),
        ));
    }
    if let Some(c) = put_hand.captures(body) {
        let (take, what) = dig_what(cap(&c, 1))?;
        if !take.is_empty() && take != "any " {
            return None;
        }
        let rest = dig_rest(cap(&c, 2))?;
        return Some((
            format!("dig {n}{reveal_top} take {take}{what} may rest {rest}"),
            cap(&c, 3).to_string(),
        ));
    }
    if let Some(c) = onto.captures(body) {
        let (take, what) = dig_what(cap(&c, 1))?;
        let tapped = if c.get(2).is_some() { " tapped" } else { "" };
        let reword = if take.is_empty() || take == "any " {
            ""
        } else {
            " reword"
        };
        let rest = dig_rest(cap(&c, 3))?;
        return Some((
            format!(
                "dig {n}{reveal_top} take {take}{what} may onto battlefield{tapped} rest {rest}{reword}"
            ),
            cap(&c, 4).to_string(),
        ));
    }
    // "Put a land card from among them into your hand and the rest ...".
    if let Some(c) =
        cached_regex(r"^Put (an? .+?) from among them into your hand and the rest (.+?)\.\s*(.*)$")
            .unwrap()
            .captures(body)
    {
        let (take, what) = dig_what(cap(&c, 1))?;
        if !take.is_empty() {
            return None;
        }
        let rest = dig_rest(cap(&c, 2))?;
        return Some((
            format!("dig {n}{reveal_top} take {what} rest {rest}"),
            cap(&c, 3).to_string(),
        ));
    }
    if let Some(c) = one.captures(body) {
        let reword = if cap(&c, 1) == "those cards" {
            " reword"
        } else {
            ""
        };
        let rest = dig_rest(cap(&c, 2))?;
        return Some((
            format!("dig {n}{reveal_top} take card rest {rest}{reword}"),
            cap(&c, 3).to_string(),
        ));
    }
    if let Some(c) = several.captures(body) {
        let k = number(cap(&c, 2))?;
        let up_to = c.get(1).is_some();
        let those = cap(&c, 3) == "those cards";
        // `reword` prints "two of them"; "up to" is only "of them".
        let (may, reword) = match (up_to, those) {
            (true, false) => (" may", ""),
            (true, true) => return None,
            (false, true) => ("", ""),
            (false, false) => ("", " reword"),
        };
        let rest = dig_rest(cap(&c, 4))?;
        return Some((
            format!("dig {n}{reveal_top} take {k} card{may} rest {rest}{reword}"),
            cap(&c, 5).to_string(),
        ));
    }
    None
}

/// "cast artifact spells or activate abilities of artifacts" (after
/// "Spend this mana only to ") as a DSL restriction: `only to cast
/// artifact spells or activate of artifact bare`.
fn spend_rule(rule: &str) -> Option<String> {
    // "abilities of artifact sources", "abilities of Elementals".
    let of = |g: &str| -> Option<String> {
        if let Some(n) = g
            .strip_suffix(" sources")
            .or_else(|| g.strip_suffix(" source"))
        {
            let n = n
                .strip_prefix("an ")
                .or_else(|| n.strip_prefix("a "))
                .unwrap_or(n);
            return Some(format!(" of {n}"));
        }
        let n = g
            .strip_prefix("an ")
            .or_else(|| g.strip_prefix("a "))
            .unwrap_or(g);
        let one = match n {
            "Myr" | "Eldrazi" | "colorless Eldrazi" => n.to_string(),
            _ if n.ends_with("Elves") => n.replace("Elves", "Elf"),
            _ if g == n => n.strip_suffix('s')?.to_string(),
            _ => n.to_string(),
        };
        Some(format!(" of {one} bare"))
    };
    let activate = |a: &str| -> Option<String> {
        match a {
            "an ability" => Some(String::new()),
            "abilities" => Some(" abilities".to_string()),
            _ => {
                if let Some(g) = a.strip_prefix("abilities of ") {
                    Some(format!(" abilities{}", of(g)?))
                } else {
                    Some(of(a.strip_prefix("an ability of ")?)?)
                }
            }
        }
    };
    if let Some(a) = rule.strip_prefix("activate ") {
        return Some(format!("only to activate{}", activate(a)?));
    }
    let rest = rule.strip_prefix("cast ")?;
    let (spells, tail, join) = if let Some((s, t)) = rest.split_once(" or activate ") {
        (s, Some(t), "or")
    } else if let Some((s, t)) = rest.split_once(" and activate ") {
        (s, Some(t), "and")
    } else {
        (rest, None, "")
    };
    let spells = if let Some(one) = spells.strip_suffix(" spell") {
        let one = one.strip_prefix("an ").or_else(|| one.strip_prefix("a "))?;
        format!("{one} spell")
    } else {
        let many = spells.strip_suffix(" spells")?;
        if many.contains(" and/or ") {
            format!("{} spells joined andor", many.replace(" and/or ", " or "))
        } else if many.contains(" and ") {
            format!("{} spells joined and", many.replace(" and ", " or "))
        } else {
            format!("{many} spells")
        }
    };
    if !spells.chars().all(|c| c.is_ascii_alphabetic() || c == ' ') {
        return None;
    }
    let tail = match tail {
        None => String::new(),
        Some(t) => {
            let a = activate(t)?;
            // Plural and singular wordings don't mix.
            let a = a.strip_prefix(" abilities").unwrap_or(&a).to_string();
            format!(" {join} activate{a}")
        }
    };
    Some(format!("only to cast {spells}{tail}"))
}

fn number(w: &str) -> Option<String> {
    if w == "X" {
        return Some("X".to_string());
    }
    // "twice X", made one word by `sentence_rules`.
    if w == "twiceX" {
        return Some("2 times X".to_string());
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
#[allow(clippy::too_many_lines)] // one arm per phrase
fn count_expr(s: &str) -> Option<String> {
    let s = s.trim();
    // "half the number of artifacts you control, rounded down".
    if let Some(rest) = s.strip_prefix("half the number of ")
        && let Some((what, how)) = rest.rsplit_once(", rounded ")
        && matches!(how, "down" | "up")
    {
        return count_expr(what).map(|e| format!("half {how} {e}"));
    }
    // "+2/+0 for every seven cards in your graveyard": `every 7 ...`.
    if let Some(rest) = s.strip_prefix("every ")
        && let Some((n, what)) = rest.split_once(' ')
        && let Some(n) = number(n)
        && n.chars().all(|c| c.is_ascii_digit())
    {
        return count_expr(what).map(|e| format!("every {n} {e}"));
    }
    // Devour (CR 702.82c): "for each creature it devoured", "the number
    // of Goblins it devoured".
    if let Some(f) = s.strip_suffix(" it devoured")
        && !f.contains(' ')
    {
        return Some(format!("{} it devoured", singular(f)));
    }
    match s {
        "basic land types among lands you control" | "basic land type among lands you control" => {
            return Some("domain".to_string());
        }
        "cards in your hand" | "card in your hand" => return Some("hand".to_string()),
        // Parley.
        "nonland card revealed this way" => {
            return Some("nonland card revealed this way".to_string());
        }
        // "Reveal any number of green cards in your hand."
        "cards revealed this way" | "card revealed this way" => {
            return Some("revealed this way".to_string());
        }
        "cards you've discarded this turn" | "card you've discarded this turn" => {
            return Some("discarded this turn".to_string());
        }
        "creatures that entered the battlefield under your control this turn"
        | "creature that entered the battlefield under your control this turn" => {
            return Some("creatures entered this turn".to_string());
        }
        "creatures that left the battlefield under your control this turn"
        | "creature that left the battlefield under your control this turn" => {
            return Some("creatures left this turn".to_string());
        }
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
        "card types among cards in your graveyard" | "card type among cards in your graveyard" => {
            return Some("card types in your graveyard".to_string());
        }
        "the amount of life you gained this turn" | "amount of life you gained this turn" => {
            return Some("life you gained this turn".to_string());
        }
        "permanents you've sacrificed this turn" | "permanent you've sacrificed this turn" => {
            return Some("permanents sacrificed this turn".to_string());
        }
        "creatures that attacked this turn" | "creature that attacked this turn" => {
            return Some("creature attacked this turn".to_string());
        }
        "opponents who lost life this turn" | "opponent who lost life this turn" => {
            return Some("opponents lost life this turn".to_string());
        }
        "creatures that died this turn" | "creature that died this turn" => {
            return Some("creature died this turn".to_string());
        }
        "creatures that died under your control this turn"
        | "creature that died under your control this turn" => {
            return Some("creature you control died this turn".to_string());
        }
        "counters on this creature" | "counters on ~" => {
            return Some("counters on this".to_string());
        }
        _ => {}
    }
    // "cards you own in exile [and in your graveyard]", "cards exiled
    // with this enchantment" (CR 607.2a).
    if matches!(s, "cards you own in exile" | "card you own in exile") {
        return Some("exile".to_string());
    }
    if let Some(f) = s
        .strip_suffix(" cards you own in exile and in your graveyard")
        .or_else(|| s.strip_suffix(" card you own in exile and in your graveyard"))
        .map(|f| f.strip_prefix("the total number of ").unwrap_or(f))
        && !f.contains(',')
    {
        return Some(format!(
            "exile and graveyard {} card",
            f.replace(" and ", " or ")
        ));
    }
    if let Some(noun) = s
        .strip_prefix("cards exiled with ")
        .or_else(|| s.strip_prefix("card exiled with "))
        && (noun == "~" || (noun.starts_with("this ") && !noun[5..].contains(' ')))
    {
        return Some("exiled with this".to_string());
    }
    // "the total power of creatures you control".
    for (prefix, stat) in [
        ("the total power of ", "power"),
        ("the total toughness of ", "toughness"),
        ("the total mana value of ", "mana value"),
    ] {
        if let Some(f) = s.strip_prefix(prefix)
            && f.ends_with(" you control")
        {
            return Some(format!("total {stat} {}", singular(f)));
        }
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
        // "instant and sorcery cards in your graveyard": cards of either
        // type, `graveyard instant or sorcery`.
        if let Some((a, b)) = f.split_once(" and ")
            && !a.contains(' ')
            && !b.contains(' ')
        {
            return Some(format!("graveyard {a} or {b}"));
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
    const TARGETS: [&str; 20] = [
        "any target",
        "target spell or creature",
        "target spell or permanent",
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
            | "this Equipment"
            | "this Vehicle"
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
    // "up to three target creature cards with total mana value 3 or less
    // from your graveyard": the DSL puts the group's bound last.
    if let Some(c) = cached_regex(r"^(.+?) with total mana value (\w+) or less( from .+)?$")
        .unwrap()
        .captures(s)
        && (cap(&c, 1).starts_with("up to ") || cap(&c, 1).starts_with("any number of "))
    {
        let n = number(cap(&c, 2))?;
        let head = format!("{}{}", cap(&c, 1), cap(&c, 3));
        let r = recipient(&head, targets)?;
        return Some(format!("{r} with total mana value {n} or less"));
    }
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
    // "six target creatures", "two other target legendary creatures": a
    // fixed number of targets (CR 115.1), `6 target creature`.
    if let Some((n, tail)) = s.split_once(' ')
        && let Some(k) = ["two", "three", "four", "five", "six", "seven"]
            .iter()
            .position(|w| *w == n)
        && (tail.starts_with("target ") || tail.starts_with("other target "))
    {
        *targets += k + 2;
        let tail = tail.replacen("other target ", "another target ", 1);
        return Some(format!("{} {}", k + 2, singular(&tail)));
    }
    // Any other "target <filter>" phrase: the DSL parser and the render
    // check decide whether it is one the engine knows.
    if s.starts_with("target ") || s.starts_with("another target ") {
        *targets += 1;
        // "a nonland permanent card" (CR 110.4a): the DSL names the types
        // a permanent card isn't (`nonland permanent card` is a nonland
        // card).
        let nonperm = cached_regex(r"\b(non[a-z]+) permanent card").unwrap();
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

/// Brackets the lines of a copy effect's exceptions inside a templated
/// effect (`token_copy`); `author_card` moves them to the card's `except`
/// block (one per card).
const EXCEPT_MARK: char = '\u{1f}';
/// Separates those lines.
const EXCEPT_LINE: char = '\u{1e}';

/// "Create a [tapped [and attacking]] token that's a copy of <what>[,
/// except <exceptions>]", "create two tokens that are copies of ...".
fn token_copy(n: &str, entry: &str, what: &str, except: &str, t: &mut usize) -> Option<String> {
    let what = match what {
        // "a copy of this creature" names it in full.
        w @ ("this creature" | "this artifact" | "this enchantment") => w.to_string(),
        w => recipient(w, t)?,
    };
    let entry = match entry.trim() {
        "" => String::new(),
        e => format!("{e} "),
    };
    let mut s = if n == "1" {
        format!("create {entry}token copy of {what}")
    } else {
        format!(
            "create {} {entry}token copies of {what}",
            number(n)?.to_lowercase()
        )
    };
    if !except.is_empty() {
        let lines = copy_exceptions(except)?;
        s.push_str(" with exceptions");
        s.push(EXCEPT_MARK);
        s.push_str(&lines.join(&EXCEPT_LINE.to_string()));
        s.push(EXCEPT_MARK);
    }
    Some(s)
}

/// Quoted abilities a copy exception grants, in the DSL.
const EXCEPT_ABILITIES: [(&str, &str); 2] = [
    (
        "\"At the beginning of the end step, sacrifice this token.\"",
        "triggered at the beginning of the end step: sacrifice this",
    ),
    (
        "\"At the beginning of the end step, sacrifice this creature.\"",
        "triggered at the beginning of the end step: sacrifice this",
    ),
];

/// A copy effect's exceptions (CR 707.9b) as `except` block lines: "it
/// isn't legendary", "the token isn't legendary", "it's legendary", "it's
/// an artifact in addition to its other types", "it's a 1/1 Soldier
/// creature in addition to its other types", "it's a 1/1 green Frog",
/// "it has haste and \"...\"", joined by "and" or commas.
#[allow(clippy::too_many_lines)] // one arm per exception
fn copy_exceptions(x: &str) -> Option<Vec<String>> {
    let pt_word = Regex::new(r"^\d+/\d+$").unwrap();
    let (head, has) = if let Some(h) = x.strip_prefix("it has ") {
        ("", h)
    } else if let Some(i) = x.find(" it has ") {
        let head = x[..i]
            .strip_suffix(" and")
            .or_else(|| x[..i].strip_suffix(", and"))
            .or_else(|| x[..i].strip_suffix(','))?;
        (head, &x[i + " it has ".len()..])
    } else {
        (x, "")
    };
    let mut lines = Vec::new();
    if !head.is_empty() {
        let head = head.replace(", and ", ", ").replace(" and ", ", ");
        for clause in head.split(", ") {
            match clause {
                "it isn't legendary" => lines.push("not legendary".to_string()),
                "the token isn't legendary" => {
                    lines.insert(0, "the token".to_string());
                    lines.push("not legendary".to_string());
                }
                "it's legendary" => lines.push("legendary".to_string()),
                c => {
                    let body = c.strip_prefix("it's ")?;
                    let (body, exactly, colors_too) =
                        if let Some(b) = body.strip_suffix(" in addition to its other types") {
                            (b, false, false)
                        } else if let Some(b) =
                            body.strip_suffix(" in addition to its other colors and types")
                        {
                            (b, false, true)
                        } else {
                            (body, true, false)
                        };
                    let body = body
                        .strip_prefix("a ")
                        .or_else(|| body.strip_prefix("an "))
                        .unwrap_or(body);
                    let (mut pt, mut colors, mut types, mut subtypes) =
                        (None, Vec::new(), Vec::new(), Vec::new());
                    for w in body.split(' ') {
                        if pt_word.is_match(w) && pt.is_none() {
                            pt = Some(w);
                        } else if ["white", "blue", "black", "red", "green"].contains(&w) {
                            colors.push(w);
                        } else if card_type_word(w) {
                            types.push(capitalize(w));
                        } else if w.chars().next().is_some_and(char::is_uppercase) {
                            subtypes.push(w);
                        } else {
                            return None;
                        }
                    }
                    if colors.is_empty() == colors_too && !exactly {
                        return None;
                    }
                    if exactly {
                        // "it's 1/1", or "it's a 1/1 green Frog".
                        if subtypes.is_empty() {
                            if pt.is_none() || !colors.is_empty() || !types.is_empty() {
                                return None;
                            }
                        } else if !types.is_empty() {
                            return None;
                        } else {
                            lines.push("exactly".to_string());
                        }
                    } else if types.is_empty() && subtypes.is_empty() {
                        return None;
                    }
                    if let Some(p) = pt {
                        lines.push(format!("pt {p}"));
                    }
                    if !colors.is_empty() {
                        lines.push(format!("colors {}", colors.join(" ")));
                    }
                    if !types.is_empty() {
                        lines.push(format!("types {}", types.join(" ")));
                    }
                    if !subtypes.is_empty() {
                        lines.push(format!("subtypes {}", subtypes.join(" ")));
                    }
                }
            }
        }
    }
    if !has.is_empty() {
        let mut keywords = Vec::new();
        let mut abilities = Vec::new();
        let mut rest = has;
        while !rest.is_empty() {
            if let Some((quoted, dsl)) = EXCEPT_ABILITIES.iter().find(|(q, _)| rest.starts_with(q))
            {
                abilities.push((*dsl).to_string());
                rest = &rest[quoted.len()..];
            } else {
                let end = rest
                    .find(" and \"")
                    .or_else(|| rest.find(", \""))
                    .unwrap_or(rest.len());
                keywords.push(keyword_list(&rest[..end])?);
                rest = &rest[end..];
            }
            rest = rest
                .strip_prefix(", and ")
                .or_else(|| rest.strip_prefix(" and "))
                .or_else(|| rest.strip_prefix(", "))
                .unwrap_or(rest);
            if rest.starts_with('"') && !EXCEPT_ABILITIES.iter().any(|(q, _)| rest.starts_with(q)) {
                return None;
            }
        }
        if !keywords.is_empty() {
            lines.push(format!("keywords {}", keywords.join(", ")));
        }
        lines.extend(abilities);
    }
    (!lines.is_empty()).then_some(lines)
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
    const COUNTS: [(&str, &str); 5] = [
        ("up to X ", "up to X"),
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
            "Werewolves" => "Werewolf".to_string(),
            "Foxes" => "Fox".to_string(),
            "Sphinxes" => "Sphinx".to_string(),
            "Heroes" => "Hero".to_string(),
            "Mice" => "Mouse".to_string(),
            "Oxen" => "Ox".to_string(),
            "Homunculi" => "Homunculus".to_string(),
            // A planeswalker type spelled with "s" in the singular.
            "Bolas" | "Plains" => w.to_string(),
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
    /// The triggered ability being authored is about its source ("When
    /// this creature attacks, sacrifice it at end of combat"): a bare
    /// "it" with no target is the source.
    about_this: std::cell::Cell<bool>,
}

/// A sentence's duration other than "until end of turn" (CR 611.2b): the
/// sentence without it, whether it came first, and the DSL's words for
/// it.
fn lasting_split(s: &str) -> Option<(String, bool, &'static str)> {
    let s = s.trim_end_matches('.');
    if let Some(rest) = s.strip_prefix("Until your next turn, ") {
        return Some((capitalize(rest), true, "until your next turn"));
    }
    // CR 500.5a: "until end of combat", "can't block this combat",
    // "blocks it this combat if able".
    if let Some(rest) = s.strip_prefix("Until end of combat, ") {
        return Some((capitalize(rest), true, "until end of combat"));
    }
    if let Some(rest) = s.strip_suffix(" until end of combat") {
        return Some((rest.to_string(), false, "until end of combat"));
    }
    let combat = cached_regex(
        r"^(.+ (?:can't attack or block|can't attack|can't block|can't be blocked|blocks (?:it|that creature|this creature|~)|attacks|blocks)) this combat( if able)?$",
    )
    .ok()?;
    if let Some(c) = combat.captures(s)
        && !s.contains("this turn")
        && !cap(&c, 1).contains("this combat")
    {
        let inner = format!("{} this turn{}", cap(&c, 1), cap(&c, 2));
        return Some((inner, false, "until end of combat"));
    }
    let this = r"(?:~|this (?:creature|artifact|enchantment|Saga|Vehicle|Equipment|permanent|land|planeswalker))";
    let re = cached_regex(&format!(
        r"^(.+?) (until your next turn|for as long as {this} remains on the battlefield|for as long as you control {this}|for as long as it remains tapped)$"
    ))
    .ok()?;
    let c = re.captures(s)?;
    let d = cap(&c, 2);
    let lasting = if d == "until your next turn" {
        "until your next turn"
    } else if d.ends_with("remains on the battlefield") {
        "for as long as this remains"
    } else if d.ends_with("remains tapped") {
        "for as long as it remains tapped"
    } else {
        "for as long as you control this"
    };
    Some((cap(&c, 1).to_string(), false, lasting))
}

fn cap<'a>(c: &'a regex::Captures, i: usize) -> &'a str {
    c.get(i).map_or("", |m| m.as_str())
}

impl Templates {
    #[allow(clippy::too_many_lines)]
    fn new() -> Self {
        let r = |s: &str| cached_regex(s).unwrap();
        let rules: Vec<Rule> = vec![
            // "put a loyalty counter on Ral" (CR 306.5).
            (r(r"^[Pp]ut an? loyalty counter on (.+)$"), |c, t| {
                Some(format!(
                    "put 1 loyalty counter on {}",
                    recipient(cap(c, 1), t)?
                ))
            }),
            // "This Vehicle becomes an artifact creature until end of turn."
            (
                r(
                    r"^(?:This Vehicle|this Vehicle|~) becomes an artifact creature until end of turn$",
                ),
                |_, _| Some("this becomes artifact creature until end of turn".to_string()),
            ),
            // "Each player shuffles their hand and graveyard into their
            // library, then draws seven cards."
            (
                r(
                    r"^[Ee]ach player shuffles their hand and graveyard into their library, then draws (\S+) cards$",
                ),
                |c, _| {
                    Some(format!(
                        "each player shuffles hand and graveyard into library then draws {} cards",
                        number(cap(c, 1))?
                    ))
                },
            ),
            // "Target opponent exiles a card from their hand."
            (
                r(r"^(target opponent|each opponent) exiles a card from their hand$"),
                |c, t| {
                    Some(format!(
                        "{} exiles 1 card from hand",
                        recipient(cap(c, 1), t)?
                    ))
                },
            ),
            // "You gain life equal to the sacrificed creature's toughness."
            (
                r(
                    r"^[Yy]ou gain life equal to the sacrificed (creature|artifact)'s (power|toughness|mana value)$",
                ),
                |c, _| {
                    Some(format!(
                        "you gain count equal {} of the sacrificed {} life",
                        cap(c, 2),
                        cap(c, 1)
                    ))
                },
            ),
            // Thirteenth-wave phrases the card language already says.
            // "Target creature must be blocked this turn if able."
            (r(r"^(.+?) must be blocked this turn if able$"), |c, t| {
                let who = recipient(&subject(cap(c, 1)), t)?;
                Some(format!("{who} must be blocked if able"))
            }),
            // "you may have target creature get -1/-1 until end of turn".
            (
                r(r"^[Yy]ou may have (target creature) get ([+-]\d+/[+-]\d+) until end of turn$"),
                |c, t| {
                    let who = recipient(cap(c, 1), t)?;
                    Some(format!("you may {who} gets {}", cap(c, 2)))
                },
            ),
            (r(r"^[Dd]efending player loses (\S+) life$"), |c, _| {
                Some(format!(
                    "defending player loses {} life",
                    number(cap(c, 1))?
                ))
            }),
            (r(r"^[Dd]iscard an? card at random$"), |_, _| {
                Some("discard 1 card at random".to_string())
            }),
            // "Mill two cards, then you may return a creature card from
            // your graveyard to your hand."
            (
                r(r"^[Yy]ou may return an? ([a-z]+) card from your graveyard to your hand$"),
                |c, _| {
                    Some(format!(
                        "you may return 1 {} card from your graveyard to hand chosen",
                        cap(c, 1)
                    ))
                },
            ),
            (
                r(r"^[Ll]ook at the top card of (target player|target opponent)'s library$"),
                |c, t| Some(format!("look at top card of {}", recipient(cap(c, 1), t)?)),
            ),
            (
                r(r"^(target opponent|each opponent|target player) loses that much life$"),
                |c, t| Some(format!("{} loses that much life", recipient(cap(c, 1), t)?)),
            ),
            (r(r"^[Yy]ou gain that much life$"), |_, _| {
                Some("you gain that much life".to_string())
            }),
            // "When this creature dies, you gain life equal to its power."
            // (Packsong Pup): "its" is the triggering object, as `of it`
            // is.
            (
                r(r"^[Yy]ou gain life equal to its (power|toughness)$"),
                |c, _| Some(format!("you gain count equal {} of it life", cap(c, 1))),
            ),
            // "Whenever this creature attacks, you may draw cards equal to
            // its power." (Horrid Shadowspinner).
            (
                r(r"^[Yy]ou (may )?draw cards equal to its (power|toughness)$"),
                |c, _| {
                    Some(format!(
                        "you {}draw count equal {} of it cards",
                        c.get(1).map_or("", |m| m.as_str()),
                        cap(c, 2)
                    ))
                },
            ),
            (
                r(r"^[Dd]raw cards equal to its (power|toughness)$"),
                |c, _| Some(format!("draw count equal {} of it cards", cap(c, 1))),
            ),
            // "Whenever Eivor deals combat damage to a player, you mill
            // that many cards."
            (r(r"^[Yy]ou mill that many cards$"), |_, _| {
                Some("you mill that much cards".to_string())
            }),
            // "{T}: Add {G} or {W}. This land doesn't untap during your next
            // untap step." (the bounce-tap lands, Arbalest Elite).
            (
                r(
                    r"^(?:[Tt]his (?:land|creature|artifact|permanent)|~) doesn't untap during your next untap step$",
                ),
                |_, _| Some("this doesn't untap next untap step".to_string()),
            ),
            // "At the beginning of your upkeep, sacrifice this creature
            // unless you discard a card." (the Masticores).
            (
                r(
                    r"^[Ss]acrifice (?:this (?:creature|enchantment|artifact)|it|~) unless you discard (a|two|three) cards?$",
                ),
                |c, _| {
                    let n = number(cap(c, 1))?;
                    let noun = if n == "1" { "card" } else { "cards" };
                    Some(format!("sacrifice this unless you discard {n} {noun}"))
                },
            ),
            // "Whenever this creature deals combat damage to a player, put
            // that many +1/+1 counters on it." (Westgate Regent): the amount
            // of the trigger's damage, on the triggering object.
            (
                r(r"^[Pp]ut that many \+1/\+1 counters on (it|this creature|~)$"),
                |c, _| {
                    let on = if cap(c, 1) == "it" { "it" } else { "this" };
                    Some(format!("put that much +1/+1 counters on {on}"))
                },
            ),
            // "Whenever this creature is dealt damage, it deals that much
            // damage to any target." (Spitemare, Brash Taunter).
            (
                r(
                    r"^[Ii]t deals that much damage to (any target|any other target|target opponent|target player|you)$",
                ),
                |c, t| {
                    let to = match cap(c, 1) {
                        "any other target" => {
                            *t += 1;
                            "any other target".to_string()
                        }
                        "you" => "you".to_string(),
                        other => recipient(other, t)?,
                    };
                    Some(format!("it deals that much damage to {to}"))
                },
            ),
            // "this Aura deals 2 damage to that creature's controller".
            (
                r(
                    r"^(?:~|this Aura|This Aura|this creature|This creature|this enchantment|This enchantment) deals (\S+) damage to that creature's controller$",
                ),
                |c, _| {
                    Some(format!(
                        "deal {} damage to controller of that creature",
                        number(cap(c, 1))?
                    ))
                },
            ),
            // "Each player discards their hand, then draws seven cards."
            (
                r(r"^[Ee]ach player discards their hand, then draws (\S+) cards?$"),
                |c, _| {
                    Some(format!(
                        "each player discards hand then draws {} cards",
                        number(cap(c, 1))?
                    ))
                },
            ),
            // "Target player draws four cards, then discards three cards."
            (
                r(r"^[Tt]arget player draws (\S+) cards?, then discards (\S+) cards?$"),
                |c, t| {
                    *t += 1;
                    Some(format!(
                        "target player draws {} cards then target {t} discards {} cards",
                        number(cap(c, 1))?,
                        number(cap(c, 2))?
                    ))
                },
            ),
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
                    // "deals 2 damage to any target and 3 damage to you",
                    // "... and 1 damage to any other target".
                    let second = cached_regex(r"^(\S+) damage to (.+)$")
                        .unwrap()
                        .captures(cap(c, 3))
                        .map(|d| (d[1].to_string(), d[2].to_string()));
                    if let Some((m, r)) = second {
                        let m = number(&m)?;
                        let r = if r == "any other target" {
                            *t += 1;
                            "another target any".to_string()
                        } else {
                            recipient(&r, t)?
                        };
                        return Some(format!(
                            "deal {n} damage to {to} and deal {m} damage to {r}"
                        ));
                    }
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
            // "Target creature gets +X/+0 and gains trample until end of
            // turn", "gets -X/-X": the unit pump, X times (`+1/+0 X`).
            (
                r(
                    r"^(.+?) gets? ([+-])X/([+-])(X|0)(?: and gains? ([a-z ,]+?))?(?: until end of turn)?(?:, where X is (.+))?$",
                ),
                |c, t| {
                    let who = all_subject(cap(c, 1), recipient(&subject(cap(c, 1)), t)?);
                    let y = if cap(c, 4) == "X" { "1" } else { "0" };
                    // "where X is the number of lands you control": the
                    // count in place of X.
                    let x = match c.get(6) {
                        Some(w) => {
                            let w = w.as_str();
                            let e = bound_amount(w)
                                .or_else(|| count_expr(w.strip_prefix("the number of ")?))?;
                            format!("count where {e}")
                        }
                        None => "X".to_string(),
                    };
                    let mut s = format!("{who} gets {}1/{}{y} {x}", cap(c, 2), cap(c, 3));
                    if !cap(c, 5).is_empty() {
                        let _ = write!(s, " and gains {}", keyword_list(cap(c, 5))?);
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
            // "That creature doesn't untap during its controller's untap
            // step" (with a duration after it, CR 611.2b).
            (
                r(
                    r"^(.+?) (?:doesn't untap during its controller's untap step|don't untap during their controllers' untap steps)$",
                ),
                |c, t| {
                    let who = all_subject(cap(c, 1), recipient(&subject(cap(c, 1)), t)?);
                    Some(format!("{who} gains doesn't untap"))
                },
            ),
            // CR 608.2g: "You may cast a noncreature spell [with mana value
            // 4 or less] from your hand without paying its mana cost."
            (
                r(
                    r"^[Yy]ou may cast an? (?:(noncreature|creature|artifact|instant|sorcery|instant or sorcery|enchantment|permanent) )?spell(?: with mana value (\d+|X) or less)? from your hand without paying its mana cost$",
                ),
                |c, _| {
                    let f = cap(c, 1);
                    let mut out = "cast free ".to_string();
                    if !f.is_empty() {
                        let _ = write!(out, "{f} ");
                    }
                    out.push_str("spell from hand");
                    if !cap(c, 2).is_empty() {
                        let _ = write!(out, " with mana value up to {}", cap(c, 2));
                    }
                    Some(out)
                },
            ),
            // "You may cast target instant or sorcery card [with mana value
            // 3 or less] from your graveyard without paying its mana cost[.
            // If that spell would be put into your graveyard, exile it
            // instead.]"
            (
                r(
                    r"^[Yy]ou may cast target (instant or sorcery|instant|sorcery) card( with mana value \d+ or less)? from your graveyard without paying its mana cost(\x{2})?$",
                ),
                |c, t| {
                    *t += 1;
                    let exile = if cap(c, 3).is_empty() {
                        ""
                    } else {
                        " exile instead"
                    };
                    Some(format!(
                        "cast free target {} card{} from your graveyard{exile}",
                        cap(c, 1),
                        cap(c, 2)
                    ))
                },
            ),
            // CR 614.1a: "If it would leave the battlefield, exile it
            // instead of putting it anywhere else." about a returned card.
            (
                r(
                    r"^If (it|that creature) would leave the battlefield, exile it instead of putting it anywhere else$",
                ),
                |c, t| {
                    if *t == 0 {
                        return None;
                    }
                    Some(if cap(c, 1) == "it" {
                        format!("leaving exiles target {}", *t)
                    } else {
                        "leaving exiles that creature".to_string()
                    })
                },
            ),
            // "It loses all abilities" (with a duration after it).
            (r(r"^(.+?) loses? all abilities$"), |c, t| {
                let who = all_subject(cap(c, 1), recipient(&subject(cap(c, 1)), t)?);
                Some(format!("{who} loses all abilities"))
            }),
            (
                r(r"^(.+?) gains? ([a-z ,]+?)(?: until end of turn)?$"),
                |c, t| {
                    if cap(c, 2).ends_with(" life") {
                        return None;
                    }
                    let who = all_subject(cap(c, 1), recipient(&subject(cap(c, 1)), t)?);
                    // "It gains haste." with no duration (a returned
                    // creature, Whip of Erebos): for good (CR 611.2a).
                    let forever = !c[0].ends_with(" until end of turn")
                        && matches!(
                            cap(c, 1),
                            "It" | "That creature" | "They" | "Those creatures"
                        );
                    Some(format!(
                        "{who} gains {}{}",
                        keyword_list(cap(c, 2))?,
                        if forever { " indefinitely" } else { "" }
                    ))
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
            // A processor (CR 400.3): "put a card an opponent owns from
            // exile into that player's graveyard", "put two cards your
            // opponents own from exile into their owners' graveyards".
            (
                r(
                    r"^Put (a card an opponent owns from exile into that player's graveyard|(\w+) cards your opponents own from exile into their owners' graveyards)$",
                ),
                |c, _| {
                    let n = if cap(c, 2).is_empty() {
                        "1".to_string()
                    } else {
                        number(cap(c, 2))?
                    };
                    Some(format!(
                        "put {n} card{} an opponent owns from exile into graveyard chosen",
                        if n == "1" { "" } else { "s" }
                    ))
                },
            ),
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
            // "For each land you control, create a Treasure token." (Brass's
            // Bounty), "For each creature you control, create a 1/2 white
            // Moogle creature token with lifelink."
            (
                r(
                    r"^For each (.+?), create an? (Treasure|Food|Clue|Blood|Map|Powerstone|Lander) token$",
                ),
                |c, _| {
                    Some(format!(
                        "create count foreach {} \"{}\"",
                        count_expr(cap(c, 1))?,
                        cap(c, 2)
                    ))
                },
            ),
            (
                r(
                    r"^For each (.+?), (?:you )?create an? ((?:\d+/\d+) (?:colorless|(?:white|blue|black|red|green)(?: and (?:white|blue|black|red|green))?) (?:[A-Z][a-z]+ )+(?:artifact )?creature) token((?: with [a-z ,]+)?)$",
                ),
                |c, _| {
                    let key = format!("{}{}", cap(c, 2), cap(c, 3));
                    token_block(&key)?;
                    // Parley: "For each ..., you create ...".
                    let you = if cap(c, 0).contains(", you create ") {
                        "you "
                    } else {
                        ""
                    };
                    Some(format!(
                        "{you}create count foreach {} \"{key}\"",
                        count_expr(cap(c, 1))?
                    ))
                },
            ),
            // CR 111.3: "Create an X/X green Elemental creature token,
            // where X is the number of lands you control."
            (
                r(
                    r"^[Cc]reate an X/X ((?:colorless|(?:white|blue|black|red|green)(?: and (?:white|blue|black|red|green))?) (?:[A-Z][a-z]+ )+(?:artifact )?creature) token((?: with [a-z ,]+)?)(?:, where X is (the number of .+|the greatest power among creatures you control))?$",
                ),
                |c, _| {
                    let key = format!("0/0 {}{}", cap(c, 1), cap(c, 2));
                    token_block(&key)?;
                    // Without "where", X is the spell's or ability's X.
                    let n = match cap(c, 3) {
                        "" => "x".to_string(),
                        "the greatest power among creatures you control" => {
                            "count where greatest power".to_string()
                        }
                        s => format!(
                            "count where {}",
                            count_expr(s.strip_prefix("the number of ")?)?
                        ),
                    };
                    Some(format!("create 1 \"{key}\" sized {n}"))
                },
            ),
            (r(LEGENDARY_TOKEN_PHRASE), |c, _| {
                let key = format!("{}{}", cap(c, 2), cap(c, 3));
                legendary_token_block(&key)?;
                let may = if cap(c, 1).starts_with(['Y', 'y']) {
                    "you may "
                } else {
                    ""
                };
                Some(format!("{may}create 1 \"{key}\""))
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
            // "Draw cards equal to the greatest power among creatures you
            // control", "... to the sacrificed creature's power".
            (r(r"^Draw cards equal to (.+)$"), |c, _| {
                let e = match cap(c, 1) {
                    "the sacrificed creature's power" => {
                        "power of the sacrificed creature".to_string()
                    }
                    "the sacrificed creature's toughness" => {
                        "toughness of the sacrificed creature".to_string()
                    }
                    a => bound_amount(a)?,
                };
                Some(format!("draw count equal {e} cards"))
            }),
            (r(r"^Draw a card for each (.+)$"), |c, _| {
                Some(format!("draw count each {} cards", count_expr(cap(c, 1))?))
            }),
            // CR 701.20a: "Reveal any number of green cards in your hand."
            (
                r(
                    r"^Reveal any number of (white|blue|black|red|green|[A-Z][a-z]+) cards in your hand$",
                ),
                |c, _| {
                    Some(format!(
                        "reveal any number {} card in hand chosen",
                        cap(c, 1)
                    ))
                },
            ),
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
                r(
                    r"^[Tt]ransform (this creature|this artifact|this permanent|this enchantment|this land|this Equipment|this Vehicle|it|~)$",
                ),
                |_, _| Some("transform this".to_string()),
            ),
            // "Until end of turn, target land becomes a 3/3 creature that's
            // still a land" (CR 205.1b).
            (
                r(
                    r"^Until end of turn, (target land(?: you control)?) becomes an? (\d+)/(\d+) creature that's still a land$",
                ),
                |c, t| {
                    Some(format!(
                        "until end of turn, {} becomes {}/{} creature",
                        recipient(cap(c, 1), t)?,
                        cap(c, 2),
                        cap(c, 3)
                    ))
                },
            ),
            // The Mimics: "this creature has base power and toughness 4/4
            // until end of turn and gains flying until end of turn".
            (
                r(
                    r"^(?:[Tt]his creature|~) has base power and toughness (\d+)/(\d+) until end of turn and (?:gains ([a-z ]+) until end of turn|(can't be blocked) this turn)$",
                ),
                |c, _| {
                    let rest = match c.get(4) {
                        Some(_) => "can't be blocked this turn".to_string(),
                        None => format!("gains {}", cap(c, 3)),
                    };
                    Some(format!(
                        "this has base power and toughness {}/{} and {rest}",
                        cap(c, 1),
                        cap(c, 2)
                    ))
                },
            ),
            // CR 701.27: "Transform target Incubator token you control",
            // "transform an Incubator token you control" (chosen as it
            // resolves), "transform all Humans".
            (r(r"^[Tt]ransform (target .+|all .+)$"), |c, t| {
                Some(format!("transform {}", recipient(cap(c, 1), t)?))
            }),
            (
                r(r"^[Tt]ransform (an?|any number of) (.+? you control)$"),
                |c, _| {
                    let n = if cap(c, 1) == "any number of" {
                        "any number"
                    } else {
                        "1"
                    };
                    Some(format!("transform {n} {} chosen", singular(cap(c, 2))))
                },
            ),
            // CR 701.42a: a meld card's "exile them, then meld them".
            (
                r(
                    r"^[Ii]f you both own and control (?:this creature|this land|~) and a creature named (.+?), exile them, then meld them into (.+)$",
                ),
                |c, _| Some(format!("meld with {:?} into {:?}", cap(c, 1), cap(c, 2))),
            ),
            // CR 710: a flip card's "flip it".
            (r(r"^(?:you may )?[Ff]lip (this creature|it|~)$"), |_, _| {
                Some("flip this".to_string())
            }),
            // "Search your library for up to X basic land cards, where X
            // is the number of lands you control, put them onto the
            // battlefield tapped, then shuffle."; "for up to that many land
            // cards" (the triggering amount).
            (
                r(
                    r"^[Ss]earch your library for (a|an|up to that many|up to \S+) ([A-Za-z][A-Za-z -]*?|[a-z]+, [a-z]+, and/or [a-z]+)? ?cards?( with [^,]+)?(?:, where X is ([^,]+))?, (reveal (?:it|them|that card|those cards), )?(?:put (it|them|that card|those cards|one) (into your hand or graveyard|into your hand|into your graveyard|onto the battlefield tapped and the other into your hand|onto the battlefield tapped|onto the battlefield), then shuffle|then shuffle and put that card on top)$",
                ),
                |c, _| {
                    let n = cap(c, 1);
                    let (up_to, mut count) = match n.strip_prefix("up to ") {
                        Some("that many") => ("up to ", "that much".to_string()),
                        Some(k) => ("up to ", number(k)?),
                        None => ("", "1".to_string()),
                    };
                    if !cap(c, 4).is_empty() {
                        if count != "X" {
                            return None;
                        }
                        count = format!("{},", x_count(cap(c, 4))?);
                    }
                    let mut t = format!("search {up_to}");
                    if count != "1" || !up_to.is_empty() {
                        let _ = write!(t, "{count} ");
                    }
                    // "artifact, creature, and/or enchantment cards":
                    // `andor artifact or creature or enchantment`.
                    if cap(c, 2).contains(", and/or ") {
                        let types = cap(c, 2).replace(", and/or ", " or ").replace(", ", " or ");
                        let _ = write!(t, "andor {types} ");
                    } else if !cap(c, 2).is_empty() {
                        let _ = write!(t, "{} ", cap(c, 2));
                    }
                    // "with power 2 or less": part of the filter, before
                    // `card`; "with mana value 1 or less" after it.
                    let with = cap(c, 3);
                    if with.starts_with(" with power ") || with.starts_with(" with toughness ") {
                        let _ = write!(t, "{} ", with.trim_start());
                        t.push_str("card");
                    } else {
                        t.push_str("card");
                        t.push_str(with);
                    }
                    if !cap(c, 5).is_empty() {
                        t.push_str(" reveal");
                    }
                    t.push_str(match cap(c, 7) {
                        "into your hand" => " to hand",
                        "into your hand or graveyard" => " to hand or graveyard",
                        // "put one onto the battlefield tapped and the other
                        // into your hand" (Cultivate).
                        "onto the battlefield tapped and the other into your hand" => {
                            " to battlefield tapped and hand"
                        }
                        "into your graveyard" => " to graveyard",
                        "onto the battlefield tapped" => " to battlefield tapped",
                        "onto the battlefield" => " to battlefield",
                        _ => " to top",
                    });
                    // "then shuffle and put that card on top"; "put those
                    // cards onto the battlefield".
                    if matches!(cap(c, 6), "that card" | "those cards")
                        || matches!(cap(c, 5), "reveal that card, " | "reveal those cards, ")
                        || cap(c, 7).is_empty()
                    {
                        t.push_str(" that card");
                    }
                    Some(t)
                },
            ),
            // "Search your library and/or graveyard for a creature card
            // with no abilities, reveal it, and put it into your hand. If
            // you search your library this way, shuffle." (the second
            // sentence joined as `\u{2}` above): one card, either zone.
            (
                r(
                    r"^[Ss]earch your library and/or graveyard for an? ((?:[A-Z][a-z]+ |non[a-z]+ |artifact |creature |enchantment |land |instant |sorcery |planeswalker |legendary |basic |permanent )+)card( with (?:mana value \d+ or (?:less|greater)|power \d+ or (?:less|greater)|toughness \d+ or (?:less|greater)|no abilities))?(, reveal it, and put it into your hand| and put it onto the battlefield)\x{2}$",
                ),
                |c, _| {
                    let what = cap(c, 1).trim_end();
                    let with = cap(c, 2);
                    // "with mana value 2 or less" after `card`; the rest
                    // of the filter before it.
                    let (before, after) = if with.starts_with(" with mana value") {
                        ("", with)
                    } else {
                        (with, "")
                    };
                    let to = if cap(c, 3).starts_with(", reveal") {
                        "reveal to hand"
                    } else {
                        "to battlefield"
                    };
                    Some(format!(
                        "search library or graveyard {what}{before} card{after} {to}"
                    ))
                },
            ),
            // "Target player reveals three cards from their hand and you
            // choose one of them. That player discards that card." (joined
            // by `\u{2}` above); "..., where X is the number of Faeries you
            // control. You choose one of those cards. ..."; "... You choose
            // two of them. That player discards those cards."
            (
                r(
                    r"^([Tt]arget player|[Tt]arget opponent) reveals (\w+) cards from their hand(?:, where X is ([^\x{2}]+))?(?: and you choose one of them|\x{2}You choose (one|two) of (?:them|those cards))\x{2}That player discards (that card|those cards)$",
                ),
                |c, t| {
                    let who = recipient(&lowercase_first(cap(c, 1)), t)?;
                    let n = if cap(c, 3).is_empty() {
                        number(cap(c, 2))?
                    } else {
                        if cap(c, 2) != "X" {
                            return None;
                        }
                        format!("{},", x_count(cap(c, 3))?)
                    };
                    let take = match cap(c, 4) {
                        "two" if cap(c, 5) == "those cards" => "2 ",
                        "" | "one" if cap(c, 5) == "that card" => "",
                        _ => return None,
                    };
                    Some(format!("{who} reveals {n} cards discarding {take}card"))
                },
            ),
            // "You may cast target Zombie creature card from your graveyard
            // this turn." (CR 601.3).
            (
                r(r"^[Yy]ou may cast (target [^,]+? card from your graveyard) this turn$"),
                |c, t| {
                    let what = recipient(cap(c, 1), t)?;
                    Some(format!("may cast {what} this turn"))
                },
            ),
            // "Target player shuffles their library." (CR 701.24).
            (
                r(
                    r"^(Target player|Target opponent|Each player|Each opponent) shuffles their library$",
                ),
                |c, t| {
                    let who = recipient(&subject(cap(c, 1)), t)?;
                    Some(format!("{who} shuffles library"))
                },
            ),
            (
                r(r"^Each player shuffles their hand and graveyard into their library$"),
                |_, _| Some("each player shuffles hand and graveyard into library".to_string()),
            ),
            // "Shuffle ~ into its owner's library." (the Zeniths, the
            // Beacons): the spell, as it resolves.
            (r(r"^Shuffle ~ into its owner's library$"), |_, _| {
                Some("shuffle this into library".to_string())
            }),
            // Another player searches their own library: "Target player
            // searches their library for a basic land card, puts it onto
            // the battlefield tapped, then shuffles." (Fertilid), "Its
            // controller may search their library for ...".
            (
                r(
                    r"^([Tt]arget player|[Ii]ts controller|[Tt]hat player|[Ee]ach player|[Ee]ach opponent) (searches|may search) their library for (?:a|an) ([A-Za-z][A-Za-z -]*?) card, (?:put|puts) it (onto the battlefield tapped|onto the battlefield|into their hand), then (?:shuffle|shuffles)$",
                ),
                |c, t| {
                    let who = recipient(&lowercase_first(cap(c, 1)), t)?;
                    let verb = if cap(c, 2) == "searches" {
                        "searches"
                    } else {
                        "may search"
                    };
                    let to = match cap(c, 4) {
                        "onto the battlefield tapped" => "battlefield tapped",
                        "onto the battlefield" => "battlefield",
                        _ => "hand",
                    };
                    Some(format!("{who} {verb} {} card to {to}", cap(c, 3)))
                },
            ),
            (r(r"^You may have that player shuffle$"), |_, _| {
                Some("you may have that player shuffle".to_string())
            }),
            // "Search your library for three cards, exile them, then
            // shuffle." (Foresight).
            (
                r(
                    r"^Search your library for (a|an|one|two|three|four|five|up to \S+|any number of) ([A-Za-z][A-Za-z -]*?)? ?cards?, exile (?:it|them), then shuffle$",
                ),
                |c, _| {
                    let n = cap(c, 1);
                    let count = match n {
                        "a" | "an" => String::new(),
                        "any number of" => "any number ".to_string(),
                        _ => match n.strip_prefix("up to ") {
                            Some(k) => format!("up to {} ", number(k)?),
                            None => format!("{} ", number(n)?),
                        },
                    };
                    let what = cap(c, 2);
                    let what = if what.is_empty() {
                        String::new()
                    } else {
                        format!("{what} ")
                    };
                    let noun = if count.is_empty() { "card" } else { "cards" };
                    let noun = if what.is_empty() { noun } else { "card" };
                    Some(format!("search {count}{what}{noun} to exile"))
                },
            ),
            (r(r"^Add ((?:\{[WUBRGC]\})+)$"), |c, _| {
                Some(format!("add {}", cap(c, 1)))
            }),
            // "Add {W}{W}, {W}{U}, or {U}{U}." (the filter lands).
            (
                r(
                    r"^Add (\{[WUBRG]\}\{[WUBRG]\}), (\{[WUBRG]\}\{[WUBRG]\}), or (\{[WUBRG]\}\{[WUBRG]\})$",
                ),
                |c, _| {
                    Some(format!(
                        "add {} or {} or {}",
                        cap(c, 1),
                        cap(c, 2),
                        cap(c, 3)
                    ))
                },
            ),
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
            // CR 701.70a.
            (r(r"^[Rr]ecruit$"), |_, _| Some("recruit".to_string())),
            (r(r"^you recruit$"), |_, _| Some("you recruit".to_string())),
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
            // "attach it to": in a trigger about another object (Sigarda's
            // Aid's "Whenever an Equipment you control enters, you may
            // attach it to ..."), "it" is that object, not the source.
            (r(r"^Attach (it|this Equipment) to (.+)$"), |c, t| {
                let it = if cap(c, 1) == "it" { "it " } else { "" };
                Some(format!("attach {it}to {}", recipient(cap(c, 2), t)?))
            }),
            (
                r(
                    r"^Look at the top (\w+) cards of your library, then put them back in any order$",
                ),
                |c, _| Some(format!("look at top {}", number(cap(c, 1))?)),
            ),
            // Parley.
            (
                r(r"^Each player reveals the top card of their library$"),
                |_, _| Some("each player reveals top card".to_string()),
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
            (
                r(r"^Add (two|three|four) mana of any one color$"),
                |c, _| Some(format!("add any {}", number(cap(c, 1))?)),
            ),
            // "Add X mana of any one color, where X is this creature's
            // power."
            (
                r(r"^Add X mana of any one color, where X is (?:this creature|~)'s power$"),
                |_, _| Some("add any count where power".to_string()),
            ),
            (
                r(r"^Add X mana of any one color, where X is the number of (.+)$"),
                |c, _| Some(format!("add any count where {}", count_expr(cap(c, 1))?)),
            ),
            (
                r(
                    r"^Add X mana of any one color, where X is the amount of life you gained this turn$",
                ),
                |_, _| Some("add any count where life you gained this turn".to_string()),
            ),
            // CR 903.4f: no commander, no mana.
            (
                r(r"^Add one mana of any color in your commander's color identity$"),
                |_, _| Some("add commander".to_string()),
            ),
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
            // "Create a tapped [and attacking] token that's a copy of ...",
            // "..., except it's an artifact in addition to its other types"
            // (CR 707.9b: the exceptions go to the card's `except` block).
            (
                r(
                    r"^[Cc]reate an? (tapped |tapped and attacking )?token that's a copy of ([^,]+?)(?:, except (.+))?$",
                ),
                |c, t| token_copy("1", cap(c, 1), cap(c, 2), cap(c, 3), t),
            ),
            // "Create two tokens that are copies of target creature you
            // control", "Create X tokens that are copies of ...", "create
            // two tapped tokens that are copies of this artifact".
            (
                r(
                    r"^[Cc]reate (\S+) (tapped |tapped and attacking )?tokens that are copies of ([^,]+?)(?:, except (.+))?$",
                ),
                |c, t| token_copy(cap(c, 1), cap(c, 2), cap(c, 3), cap(c, 4), t),
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
            // "All creatures able to block target creature this turn do so."
            // (a lure until end of turn).
            (
                r(r"^[Aa]ll creatures able to block (.+) this turn do so$"),
                |c, t| {
                    let who = recipient(cap(c, 1), t)?;
                    Some(format!("{who} gains lure"))
                },
            ),
            // "This creature deals damage equal to the sacrificed creature's
            // power to any target" (Fling's, after a sacrifice cost).
            (
                r(
                    r"^(?:~|This creature|this creature|It|it) deals damage equal to the sacrificed creature's power to (.+)$",
                ),
                |c, t| {
                    let to = recipient(cap(c, 1), t)?;
                    Some(format!(
                        "deal damage equal power of the sacrificed creature to {to}"
                    ))
                },
            ),
            // "put two cards from your hand on top of your library in any
            // order" (Brainstorm's).
            (
                r(r"^[Pp]ut (\w+) cards from your hand on top of your library in any order$"),
                |c, _| {
                    Some(format!(
                        "put {} cards from hand on top of library",
                        number(cap(c, 1))?
                    ))
                },
            ),
            (r(r"^Attach (it|~|this) to (.+)$"), |c, t| {
                let it = if cap(c, 1) == "it" { "it " } else { "" };
                Some(format!("attach {it}to {}", recipient(cap(c, 2), t)?))
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
                    r"^Exile (.+) until (?:this (?:creature|enchantment|artifact|permanent|Aura|land|Vehicle|Equipment)|~) leaves the battlefield$",
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
            // CR 701.12a.
            (
                r(
                    r"^[Ee]xchange control of two target (creature|artifact|enchantment|land|permanent)s$",
                ),
                |c, t| {
                    *t += 2;
                    Some(format!("exchange control of 2 target {}", cap(c, 1)))
                },
            ),
            // CR 205.3m: "Return two target creature cards that share a
            // creature type from your graveyard to your hand."
            (
                r(
                    r"^Return two target (creature|[A-Z][a-z]+) cards that share a creature type from your graveyard to your hand$",
                ),
                |c, t| {
                    *t += 2;
                    Some(format!(
                        "return 2 target {} that share creature type card from your graveyard to hand",
                        cap(c, 1)
                    ))
                },
            ),
            // CR 205.2a: "two target permanents that share a card type".
            (
                r(
                    r"^[Ee]xchange control of two target (nonland )?permanents that share a card type$",
                ),
                |c, t| {
                    *t += 2;
                    Some(format!(
                        "exchange control of 2 target {}permanent that share card type",
                        cap(c, 1)
                    ))
                },
            ),
            (
                r(r"^[Ee]xchange control of (.+?) and (target .+)$"),
                |c, t| {
                    let a = recipient(cap(c, 1), t)?;
                    let b = recipient(cap(c, 2), t)?;
                    Some(format!("exchange control of {a} and {b}"))
                },
            ),
            // CR 613.1b: another player gains control.
            (
                r(r"^(Target opponent|Target player) gains control of (.+)$"),
                |c, t| {
                    *t += 1;
                    let who = lowercase_first(cap(c, 1));
                    // "it" is the source, not the player just targeted.
                    let of = match cap(c, 2) {
                        "it" => "it".to_string(),
                        w => recipient(w, t)?,
                    };
                    Some(format!("{who} gains control of {of}"))
                },
            ),
            // CR 701.12c.
            (
                r(r"^[Ee]xchange life totals with target opponent$"),
                |_, t| {
                    *t += 1;
                    Some("exchange life totals with target opponent".to_string())
                },
            ),
            // CR 608.2d: a punisher.
            (
                r(
                    r"^(Target opponent|Each opponent) loses (\d+) life unless (?:they sacrifice|that player sacrifices) a nonland permanent of their choice or (?:discard|discards) a card$",
                ),
                |c, t| {
                    let who = lowercase_first(cap(c, 1));
                    if who == "target opponent" {
                        *t += 1;
                        Some(format!(
                            "target opponent loses {} life unless they sacrifice nonland permanent else discard 1 card",
                            cap(c, 2)
                        ))
                    } else {
                        Some(format!(
                            "each opponent loses {} life unless each opponent sacrifices nonland permanent else discards 1 card",
                            cap(c, 2)
                        ))
                    }
                },
            ),
            // CR 613.4b.
            (
                r(
                    r"^[Cc]reatures you control have base power and toughness X/X until end of turn$",
                ),
                |_, _| Some("each creature you control has base power and toughness X".to_string()),
            ),
            // CR 613.4b: "target creature you control has base power 2
            // until end of turn" (the toughness stays).
            (
                r(r"^(.+?) has base power (\d+) until end of turn$"),
                |c, t| {
                    Some(format!(
                        "{} has base power {} until end of turn",
                        recipient(cap(c, 1), t)?,
                        cap(c, 2)
                    ))
                },
            ),
            // CR 724.1.
            (r(r"^[Ee]nd the turn$"), |_, _| {
                Some("end the turn".to_string())
            }),
            // CR 701.58a.
            (r(r"^[Cc]loak the top card of your library$"), |_, _| {
                Some("cloak top".to_string())
            }),
            // CR 701.65a.
            (r(r"^[Aa]irbend (.+)$"), |c, t| {
                Some(format!("airbend {}", recipient(cap(c, 1), t)?))
            }),
            // A spell or legendary permanent returning itself (Molten Birth).
            (r(r"^Return ~ to its owner's hand$"), |_, _| {
                Some("return this to its owner's hand".to_string())
            }),
            (
                r(r"^Return (.+?) to (?:its owner's|your|their owners') hands?$"),
                |c, t| Some(format!("return {} to hand", recipient(cap(c, 1), t)?)),
            ),
            // "Return all creature cards from your graveyard to the
            // battlefield."
            (
                r(
                    r"^Return all (creature|artifact|enchantment|land|permanent) cards from your graveyard to the battlefield$",
                ),
                |c, _| {
                    Some(format!(
                        "return all {} card from your graveyard to battlefield",
                        cap(c, 1)
                    ))
                },
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
            // "... to the battlefield with a lifelink counter on it", "with
            // an additional +1/+1 counter on it" (CR 122.1b).
            (
                r(
                    r"^Return (.+? from your graveyard) to the battlefield( tapped)? with (a|an|two|three) (additional )?(\S+(?: strike)?) counters? on it$",
                ),
                |c, t| {
                    let what = recipient(cap(c, 1), t)?;
                    if cap(c, 5) == "finality" {
                        return None;
                    }
                    Some(format!(
                        "return {what} to battlefield{} with {} {}{} counter{}",
                        cap(c, 2),
                        number(cap(c, 3))?,
                        cap(c, 4),
                        cap(c, 5),
                        if matches!(cap(c, 3), "a" | "an") {
                            ""
                        } else {
                            "s"
                        }
                    ))
                },
            ),
            // "Put target artifact card from your graveyard on top of your
            // library" (Academy Ruins).
            (
                r(r"^[Pp]ut (target .+? card from your graveyard) on top of your library$"),
                |c, t| {
                    Some(format!(
                        "put {} on top of library",
                        recipient(cap(c, 1), t)?
                    ))
                },
            ),
            // "Mill two cards, then you may return a creature card from
            // your graveyard to your hand."
            (
                r(
                    r"^[Mm]ill (\w+) cards?, then (you may )?return an? (.+?) card from your graveyard to your hand$",
                ),
                |c, _| {
                    let n = number(cap(c, 1))?;
                    Some(format!(
                        "mill {n} cards, then {}return 1 {} card from your graveyard to hand chosen",
                        cap(c, 2),
                        cap(c, 3)
                    ))
                },
            ),
            // "Return a creature card from your graveyard to the
            // battlefield", "... a land card ... tapped" (chosen as it
            // resolves).
            (
                r(
                    r"^[Rr]eturn an? ((?:[a-z]+|[A-Z][a-z]+)(?: or (?:[a-z]+|[A-Z][a-z]+))?) card from your graveyard to the battlefield( tapped)?$",
                ),
                |c, _| {
                    Some(format!(
                        "return 1 {} card from your graveyard to battlefield{} chosen",
                        cap(c, 1),
                        cap(c, 2)
                    ))
                },
            ),
            // "Put a creature card from among them onto the battlefield",
            // "... from among the milled cards ...".
            (
                r(
                    r"^[Pp]ut an? ((?:[a-z]+|[A-Z][a-z]+)(?:, (?:[a-z]+|[A-Z][a-z]+))*,? (?:or (?:[a-z]+|[A-Z][a-z]+))?|[a-z]+|[A-Z][a-z]+) card from among (them|the milled cards) onto the battlefield$",
                ),
                |c, _| {
                    Some(format!(
                        "put 1 {} card from among {} onto battlefield chosen",
                        cap(c, 1),
                        if cap(c, 2) == "them" {
                            "them"
                        } else {
                            "milled cards"
                        }
                    ))
                },
            ),
            // "Exile a land you control", "exile a creature you control
            // other than this creature" (chosen as it resolves).
            (
                r(
                    r"^[Ee]xile an? ([a-z]+) you control( other than this (?:creature|artifact|enchantment|permanent))?$",
                ),
                |c, _| {
                    Some(format!(
                        "exile 1 {}{} you control chosen",
                        if cap(c, 2).is_empty() { "" } else { "other " },
                        cap(c, 1)
                    ))
                },
            ),
            // CR 404.2: "Exile the top creature card of your graveyard".
            (
                r(r"^[Ee]xile the top (?:([a-z]+) )?card of your graveyard$"),
                |c, _| {
                    Some(if cap(c, 1).is_empty() {
                        "exile top 1 card from your graveyard chosen".to_string()
                    } else {
                        format!("exile top 1 {} card from your graveyard chosen", cap(c, 1))
                    })
                },
            ),
            // "Exile a creature card from your graveyard", "exile two
            // cards from your graveyard" (chosen as it resolves).
            (
                r(r"^[Ee]xile (an?|two|three|four) (?:([a-z]+) )?cards? from your graveyard$"),
                |c, _| {
                    let n = number(cap(c, 1))?;
                    Some(if cap(c, 2).is_empty() {
                        format!("exile {n} cards from your graveyard chosen")
                    } else {
                        format!("exile {n} {} card from your graveyard chosen", cap(c, 2))
                    })
                },
            ),
            // "That player exiles two cards from their graveyard".
            (
                r(
                    r"^(Target player|That player|that player|Target opponent|Each player|Each opponent) exiles (a|two|three|four) cards? from their graveyard$",
                ),
                |c, t| {
                    let who = recipient(cap(c, 1), t)?;
                    Some(format!(
                        "{who} exiles {} cards from graveyard",
                        number(cap(c, 2))?
                    ))
                },
            ),
            // "Put target creature card from an opponent's graveyard onto
            // the battlefield under your control".
            (
                r(
                    r"^Put target ([a-z]+) card from (a|an opponent's) graveyard onto the battlefield under your control( tapped)?$",
                ),
                |c, t| {
                    *t += 1;
                    Some(format!(
                        "put target {} card from {} graveyard onto battlefield under your control{}",
                        cap(c, 1),
                        cap(c, 2),
                        cap(c, 3)
                    ))
                },
            ),
            // "Target player shuffles up to three target cards from their
            // graveyard into their library".
            (
                r(
                    r"^Target player shuffles up to (\w+) target cards from their graveyard into their library$",
                ),
                |c, t| {
                    *t += 2;
                    Some(format!(
                        "target player shuffles up to {} target card from their graveyard into library",
                        number(cap(c, 1))?
                    ))
                },
            ),
            // "Exile all creature cards from target player's graveyard".
            (
                r(r"^[Ee]xile all ([a-z]+) cards from target player's graveyard$"),
                |c, t| {
                    *t += 1;
                    Some(format!(
                        "exile all {} card from graveyard of target player",
                        cap(c, 1)
                    ))
                },
            ),
            // "Return two cards at random from your graveyard to your hand."
            (
                r(r"^Return (\w+) cards at random from your graveyard to your hand$"),
                |c, _| {
                    let n = number(cap(c, 1))?;
                    Some(format!(
                        "return {n} card from your graveyard to hand at random"
                    ))
                },
            ),
            (
                r(
                    r"^Look at the top (\w+) cards of target (opponent|player)'s library, then put them back in any order$",
                ),
                |c, t| {
                    let n = number(cap(c, 1))?;
                    *t += 1;
                    Some(format!("look at top {n} of target {}", cap(c, 2)))
                },
            ),
            // "Put target creature card from a graveyard onto the
            // battlefield under your control" (Rise from the Grave).
            (
                r(
                    r"^[Pp]ut (target .+? card from a graveyard) onto the battlefield( tapped)? under your control$",
                ),
                |c, t| {
                    Some(format!(
                        "return {} to battlefield{} under your control",
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
            // A transforming card's back face returning itself: "Exile
            // Shiva, then return it to the battlefield (front face up)."
            // (CR 712.14; Oracle names no controller only there).
            // A flicker: "Exile target creature you control, then return
            // that card to the battlefield under its owner's control"
            // (Blur, Cloudshift).
            (
                r(
                    r"^[Ee]xile (.+), then return that card to the battlefield under (your|its owner's) control$",
                ),
                |c, t| {
                    let what = recipient(cap(c, 1), t)?;
                    let under = if cap(c, 2) == "your" {
                        " under your control"
                    } else {
                        ""
                    };
                    Some(format!(
                        "exile {what} then return that card to battlefield{under}"
                    ))
                },
            ),
            (
                r(r"^Exile ~, then return it to the battlefield$"),
                |_, _| Some("exile this then return this to battlefield front face up".to_string()),
            ),
            // CR 712.14a: "Exile this Saga, then return it to the
            // battlefield transformed under your control", "When this
            // creature dies, return it to the battlefield tapped and
            // transformed under its owner's control", "Return this card
            // from your graveyard to the battlefield transformed". "It" is
            // the source: where it isn't, the source's name or noun prints
            // and the gate refuses the card.
            (
                r(
                    r"^[Rr]eturn (?:it|him|her|this card from your graveyard) to the battlefield( tapped and)? transformed( under your control| under (?:its|his|her) owner's control)?$",
                ),
                |c, _| {
                    let tapped = if c.get(1).is_some() {
                        " tapped and"
                    } else {
                        ""
                    };
                    let under = if cap(c, 2) == " under your control" {
                        " under your control"
                    } else {
                        ""
                    };
                    Some(format!(
                        "return this to battlefield{tapped} transformed{under}"
                    ))
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
            // "Discard any number of cards, then draw that many cards plus
            // one", "discard up to two cards, then draw that many cards".
            (
                r(
                    r"^[Dd]iscard (any number of|up to \w+) cards, then draw that many cards( plus one)?$",
                ),
                |c, _| {
                    let how = match cap(c, 1) {
                        "any number of" => "any number".to_string(),
                        up => format!("up to {}", number(up.strip_prefix("up to ")?)?),
                    };
                    let plus = if cap(c, 2).is_empty() { "" } else { " plus 1" };
                    Some(format!(
                        "discard {how} cards chosen then draw count that many{plus} discarded this way cards"
                    ))
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
                    if *t > 0 {
                        Some(format!(
                            "you gain count equal {} of that target {t} life",
                            cap(c, 1)
                        ))
                    } else if c[0].contains("that creature's") {
                        // No target: the creature a trigger is about ("Whenever a
                        // creature you control dies, you gain life equal to that
                        // creature's toughness", as it last existed).
                        Some(format!(
                            "you gain count equal {} of that creature life",
                            cap(c, 1)
                        ))
                    } else {
                        None
                    }
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
            // "You gain 2 life for each card in your hand": `count each 2
            // hand` (Shepherd of Heroes).
            (r(r"^[Yy]ou gain (\S+) life for each (.+)$"), |c, _| {
                let n = number(cap(c, 1))?;
                let e = count_expr(cap(c, 2))?;
                Some(if n == "1" {
                    format!("you gain count each {e} life")
                } else {
                    format!("you gain count each {n} {e} life")
                })
            }),
            // "You and target opponent each draw two cards."
            (
                r(r"^[Yy]ou and target opponent each draw (\S+) cards?$"),
                |c, t| {
                    *t += 1;
                    Some(format!(
                        "you and target opponent each draw {} cards",
                        number(cap(c, 1))?
                    ))
                },
            ),
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
                r(r"^(.+?) blocks (?:this creature|~|it) this turn if able$"),
                |c, t| Some(format!("{} blocks this if able", recipient(cap(c, 1), t)?)),
            ),
            (r(r"^[Tt]ap or untap (.+)$"), |c, t| {
                Some(format!("tap or untap {}", recipient(cap(c, 1), t)?))
            }),
            // CR 506.4: "Remove target attacking or blocking creature from
            // combat."
            (r(r"^[Rr]emove (.+) from combat$"), |c, t| {
                Some(format!("remove {} from combat", recipient(cap(c, 1), t)?))
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
            // "Put X +1/+1 counters on target creature, where X is your
            // life total": the count in place of X.
            (
                r(&format!(
                    r"^[Pp]ut X ([+-]\d/[+-]\d|{}) counters on (.+?), where X is (.+)$",
                    counter_names()
                )),
                |c, t| {
                    let to = recipient(cap(c, 2), t)?;
                    let e = x_amount(cap(c, 3))?;
                    Some(format!(
                        "put count where {e} {} counters on {to}",
                        cap(c, 1)
                    ))
                },
            ),
            (
                r(&format!(
                    r"^Put (\S+) ([+-]\d/[+-]\d|{}) counters? on (.+)$",
                    counter_names()
                )),
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
                r(&format!(
                    r"^Remove (\S+) ([+-]\d/[+-]\d|{}) counters? from (.+)$",
                    counter_names()
                )),
                |c, t| {
                    Some(format!(
                        "remove {} {} counters from {}",
                        number(cap(c, 1))?,
                        cap(c, 2),
                        recipient(cap(c, 3), t)?
                    ))
                },
            ),
            // "Remove up to three counters from target creature": each of
            // a kind the controller chooses.
            (r(r"^Remove up to (\w+) counters from (.+)$"), |c, t| {
                Some(format!(
                    "remove up to {} counters from {}",
                    number(cap(c, 1))?,
                    recipient(cap(c, 2), t)?
                ))
            }),
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
                    // "for as long as you control this creature": the
                    // sentence's duration, not part of the target.
                    if cap(c, 1).contains(" for as long as ") {
                        return None;
                    }
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
        Templates {
            rules,
            about_this: std::cell::Cell::new(false),
        }
    }

    /// Two imperative clauses joined by "and": "Tap target creature an
    /// opponent controls and put a stun counter on it", "Put a +1/+1
    /// counter on target creature and a +1/+1 counter on up to one target
    /// Merfolk" (the second "put" left out). Each clause is authored on
    /// its own, so a target's phrase doesn't swallow the second clause.
    fn and_split(&self, s: &str, targets: &mut usize) -> Option<String> {
        const VERBS: [&str; 14] = [
            "put ",
            "tap ",
            "untap ",
            "draw ",
            "create ",
            "exile ",
            "destroy ",
            "return ",
            "sacrifice ",
            "scry ",
            "surveil ",
            "mill ",
            // "~ deals 3 damage to any target and you gain 3 life".
            "you gain ",
            "you lose ",
        ];
        let counters = cached_regex(
            r"^(a|an|one|two|three|four|five|X) ([+-]\d/[+-]\d|stun|loyalty|charge|oil|shield|lore|time|level) counters? on ",
        )
        .unwrap();
        let s = lowercase_first(s);
        let mut from = 0;
        while let Some(k) = s[from..].find(" and ") {
            let at = from + k;
            from = at + 5;
            let (left, right) = (&s[..at], &s[at + 5..]);
            let right = if VERBS.iter().any(|v| right.starts_with(v)) {
                right.to_string()
            } else if left.starts_with("put ") && counters.is_match(right) {
                format!("put {right}")
            } else {
                continue;
            };
            let before = *targets;
            let Some(a) = self.sentence(&capitalize(left), targets) else {
                *targets = before;
                continue;
            };
            let Some(b) = self.sentence(&capitalize(&right), targets) else {
                *targets = before;
                continue;
            };
            if a.contains(';') || b.contains(';') {
                *targets = before;
                continue;
            }
            // "You draw a card and you lose 1 life": the subject repeated,
            // `, and` (a plain `and` prints "and lose 1 life").
            if left.starts_with("you ") && right.starts_with("you ") {
                return Some(format!("{a}, and {b}"));
            }
            return Some(format!("{a} and {b}"));
        }
        None
    }

    fn sentence(&self, s: &str, targets: &mut usize) -> Option<String> {
        if let Some(out) = self.sentence_rules(s, targets) {
            return Some(out);
        }
        // CR 611.2b: "Until your next turn, <sentence>", "<sentence> until
        // your next turn", "<sentence> for as long as this creature
        // remains on the battlefield": the sentence, with its duration.
        let (inner, first, lasting) = lasting_split(s)?;
        let before = *targets;
        // "can't attack or block", "prevent all damage it would deal":
        // the one-turn forms, with "this turn".
        let out = self.sentence_rules(&inner, targets).or_else(|| {
            *targets = before;
            self.sentence_rules(&format!("{inner} this turn"), targets)
        });
        let Some(out) = out.filter(|o| !o.contains(';') && !o.contains("until end of turn")) else {
            *targets = before;
            return None;
        };
        // "It gains haste until your next turn": the duration replaces the
        // one-word "for good" of a bare grant.
        let out = match out.strip_suffix(" indefinitely") {
            Some(o) if !out.starts_with("gain control of ") => o.to_string(),
            _ => out,
        };
        // "Gain control of target creature for as long as ...": the
        // duration takes the place of "for good".
        let out = match out.strip_prefix("gain control of ") {
            Some(rest) => format!(
                "gain control of {}",
                rest.strip_suffix(" indefinitely").unwrap_or(rest)
            ),
            None => out,
        };
        Some(if first {
            format!("{lasting}, {out}")
        } else {
            format!("{out} {lasting}")
        })
    }

    fn sentence_rules(&self, s: &str, targets: &mut usize) -> Option<String> {
        // "deals twice X damage": one word, so a number's `(\w+)` takes it.
        let twice;
        let s = if s.contains("twice X ") {
            twice = s.replace("twice X ", "twiceX ");
            twice.as_str()
        } else {
            s
        };
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
    #[allow(clippy::too_many_lines)] // one arm per dig wording
    fn dig(&self, text: &str, targets: &mut usize) -> Result<Option<String>, Skip> {
        // "and the rest of the revealed cards into your graveyard"
        // (Kruphix's Insight): the dig with `revealed`.
        if text.contains(" the rest of the revealed cards ") {
            let plain = text.replacen(" the rest of the revealed cards ", " the rest ", 1);
            let Some(d) = self.dig(&plain, targets)? else {
                return Ok(None);
            };
            let (head, tail) = d
                .split_once("; ")
                .map_or((d.as_str(), None), |(a, b)| (a, Some(b)));
            if !head.starts_with("dig ") {
                return Ok(None);
            }
            return Ok(Some(match tail {
                Some(t) => format!("{head} revealed; {t}"),
                None => format!("{head} revealed"),
            }));
        }
        // A dig after other sentences ("Creative Outburst deals 5 damage
        // to any target. Look at the top five cards ..."): those first,
        // then the dig with what follows it.
        for at in [
            ". Look at the top ",
            ". Reveal the top ",
            ". Reveal cards from the top ",
        ] {
            if let Some(i) = text.find(at)
                && !text[..i].contains(" top ")
            {
                let (head, rest) = (&text[..=i], &text[i + 2..]);
                let mut t = *targets;
                if let Some(dig) = self.dig(rest, &mut t)? {
                    let first = self.effects(head.trim_end_matches('.'), targets)?;
                    let dig = self.dig(rest, targets)?.unwrap_or(dig);
                    return Ok(Some(format!("{first}; {dig}")));
                }
            }
        }
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
        let may = cached_regex(
            r"^(Look at|Reveal) the top (\w+) cards of your library\. You may (reveal|put) (an? .+?) card from among them (?:and put it )?into your hand\. Put the rest (on the bottom of your library(?: in any order| in a random order)?|into your graveyard)\.?\s*(.*)$",
        )
        .unwrap();
        let one = cached_regex(
            r"^Look at the top (\w+) cards of your library\. Put one of them into your hand and the (?:rest|other) (on the bottom of your library(?: in any order| in a random order)?|into your graveyard)\.?\s*(.*)$",
        )
        .unwrap();
        let all = cached_regex(
            r"^Reveal the top (\w+) cards of your library\. Put all (.+?) cards revealed this way into your hand and the rest (into your graveyard|on the bottom of your library in (?:any|a random) order)\.?\s*(.*)$",
        )
        .unwrap();
        // "You may put a creature card from among them onto the
        // battlefield [tapped]. Put the rest on the bottom ..."
        let onto = cached_regex(
            r"^(Look at|Reveal) the top (\w+) cards of your library\. You may put (an? .+?) card from among them onto the battlefield( tapped)?\. Put the rest (on the bottom of your library(?: in any order| in a random order)?|into your graveyard)\.?\s*(.*)$",
        )
        .unwrap();
        // "Put two of them into your hand and the rest ...".
        let several = cached_regex(
            r"^Look at the top (\w+) cards of your library\. Put (two|three) of them into your hand and the rest (on the bottom of your library(?: in any order| in a random order)?|into your graveyard)\.?\s*(.*)$",
        )
        .unwrap();
        // "look at the top three cards of your library, then put one of
        // them into your hand and the rest ..." (Court Hussar).
        let then = cached_regex(
            r"^Look at the top (\w+) cards of your library, then put one of them into your hand and the rest (on the bottom of your library(?: in any order| in a random order)?|into your graveyard)\.?\s*(.*)$",
        )
        .unwrap();
        // "Put one into your hand and the other into your graveyard."
        let short = cached_regex(
            r"^Look at the top two cards of your library\. Put one into your hand and the other (on the bottom of your library|into your graveyard)\.?\s*(.*)$",
        )
        .unwrap();
        let (dsl, tail) = if let Some(c) = onto.captures(&text) {
            let n = number(cap(&c, 2)).ok_or_else(|| Skip::Sentence(text.clone()))?;
            let reveal_top = if cap(&c, 1) == "Reveal" {
                " reveal"
            } else {
                ""
            };
            let what = cap(&c, 3)
                .trim_start_matches("an ")
                .trim_start_matches("a ")
                .to_string();
            let tapped = if c.get(4).is_some() { " tapped" } else { "" };
            (
                format!(
                    "dig {n}{reveal_top} take {what} card may onto battlefield{tapped} rest {}",
                    rest_of(cap(&c, 5))
                ),
                cap(&c, 6).to_string(),
            )
        } else if let Some(c) = several.captures(&text) {
            let n = number(cap(&c, 1)).ok_or_else(|| Skip::Sentence(text.clone()))?;
            let k = number(cap(&c, 2)).ok_or_else(|| Skip::Sentence(text.clone()))?;
            (
                format!("dig {n} take {k} card rest {} reword", rest_of(cap(&c, 3))),
                cap(&c, 4).to_string(),
            )
        } else if let Some(c) = short.captures(&text) {
            (
                format!("dig 2 take card rest {} short", rest_of(cap(&c, 1))),
                cap(&c, 2).to_string(),
            )
        } else if let Some(c) = may.captures(&text) {
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
        } else if let Some(c) = then.captures(&text) {
            let n = number(cap(&c, 1)).ok_or_else(|| Skip::Sentence(text.clone()))?;
            (
                format!("dig {n} then take card rest {}", rest_of(cap(&c, 2))),
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
        } else if let Some((dsl, tail)) = dig_split(&text, targets) {
            (dsl, tail)
        } else if let Some((dsl, tail)) = dig_general(&text) {
            (dsl, tail)
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
        // "Add two mana in any combination of colors. Spend this mana only
        // to cast legendary spells." (CR 106.6).
        if let Some((head, rule)) = text.split_once(" Spend this mana only to ") {
            let rule = rule.trim_end_matches('.');
            let inner = self.effects(head, targets)?;
            if inner.starts_with("add ")
                && !inner.contains(';')
                && let Some(r) = spend_rule(rule)
            {
                return Ok(format!("{inner} {r}"));
            }
            return Err(Skip::Sentence(text.to_string()));
        }
        // "Add {G}. When you spend this mana to cast a Dragon creature
        // spell, you gain 2 life."
        if let Some(c) = cached_regex(
            r"^(.+?\.) When you spend this mana to cast an? ([A-Za-z ]+?) spell, (.+)$",
        )
        .unwrap()
        .captures(text)
        {
            let inner = self.effects(cap(&c, 1), targets)?;
            let then = self.effects(cap(&c, 3), targets)?;
            if inner.starts_with("add ") && !inner.contains(';') && !then.contains(';') {
                return Ok(format!("{inner} spent on {} spell: {then}", cap(&c, 2)));
            }
            return Err(Skip::Sentence(text.to_string()));
        }
        // "Add {R}. Until end of turn, you don't lose this mana as steps
        // and phases end." (CR 500.4).
        if let Some(head) = text
            .strip_suffix(" Until end of turn, you don't lose this mana as steps and phases end.")
            .or_else(|| {
                text.strip_suffix(
                    " Until end of turn, you don't lose this mana as steps and phases end",
                )
            })
        {
            let inner = self.effects(head, targets)?;
            if inner.starts_with("add ") && !inner.contains(';') {
                return Ok(format!("keep mana {inner}"));
            }
            return Err(Skip::Sentence(text.to_string()));
        }
        // "Target opponent reveals their hand. You choose a nonland card
        // from it. That player discards that card." is one effect.
        let duress = cached_regex(
            r"^(.+?) reveals their hand\. You (may )?choose (a|an) (.+? )?card from it( with mana value \d+ or (?:less|greater))?\. That player discards that card\.?(.*)$",
        )
        .unwrap();
        if let Some(c) = duress.captures(text) {
            let who = recipient(&subject(cap(&c, 1)), targets)
                .ok_or_else(|| Skip::Sentence(text.to_string()))?;
            let may = if c.get(2).is_some() { "may " } else { "" };
            let what = cap(&c, 4).trim();
            let what = what.replace(", ", " ");
            let mut out = if what.is_empty() {
                format!("{who} reveals hand discarding {may}card")
            } else {
                format!("{who} reveals hand discarding {may}{what} card")
            };
            if let Some(mv) = c.get(5) {
                if what.is_empty() {
                    return Err(Skip::Sentence(text.to_string()));
                }
                out = format!(
                    "{} card",
                    out.trim_end_matches(" card").to_string() + mv.as_str()
                );
            }
            let rest = cap(&c, 6).trim();
            if !rest.is_empty() {
                out.push_str("; ");
                out.push_str(&self.effects(rest, targets)?);
            }
            return Ok(out);
        }
        // "look at the top card of your library. You may reveal that card.
        // If an instant or sorcery card is revealed this way, ..."
        // (Delver of Secrets); "Scry 3, then you may reveal the top card of
        // your library. If a creature card is revealed this way, ..."
        if let Some(c) = cached_regex(
            r"^(?:([Ll]ook at the top card of your library\. You may reveal that card)|[Ss]cry (\w+), then you may reveal the top card of your library)\. If an? ([a-z][a-z ]*?) card is revealed this way, (.+?)\.?$",
        )
        .unwrap()
        .captures(text)
        {
            let then = self.effects(&capitalize(cap(&c, 4)), targets)?;
            let head = if c.get(1).is_some() {
                "look at top card; you may reveal that card".to_string()
            } else {
                let n = number(cap(&c, 2)).ok_or_else(|| Skip::Sentence(text.to_string()))?;
                format!("scry {n}, then you may reveal top card")
            };
            return Ok(format!("{head}; if revealed {} card, {then}", cap(&c, 3)));
        }
        // "Target player reveals their hand and you choose a card of that
        // color from it. That player discards that card." (Addle).
        if let Some(c) = cached_regex(
            r"^(Choose a color\. )?(.+?) reveals their hand and you choose (?:a|an) (.+? )?card( of that color)? from it\. That player discards that card\.?(.*)$",
        )
        .unwrap()
        .captures(text)
        {
            // "of that color" needs the color chosen first.
            if c.get(1).is_some() != c.get(4).is_some() {
                return Err(Skip::Sentence(text.to_string()));
            }
            let who = recipient(&subject(cap(&c, 2)), targets)
                .ok_or_else(|| Skip::Sentence(text.to_string()))?;
            let what = format!(
                "{}{}",
                cap(&c, 3),
                if c.get(4).is_some() { "of that color " } else { "" }
            );
            let lead = if c.get(1).is_some() { "choose color; " } else { "" };
            let mut out = format!("{lead}{who} reveals hand and discarding {what}card");
            let rest = cap(&c, 5).trim();
            if !rest.is_empty() {
                out.push_str("; ");
                out.push_str(&self.effects(rest, targets)?);
            }
            return Ok(out);
        }
        // "Look at target opponent's hand and choose a card from it. Put
        // that card on top of that player's library." (Painful Memories).
        if let Some(c) = cached_regex(
            r"^Look at (target opponent|target player)'s hand and choose (?:a|an) (.+? )?card from it\. (Put that card on top of that player's library|That player discards that card)\.?(.*)$",
        )
        .unwrap()
        .captures(text)
        {
            *targets += 1;
            let verb = if cap(&c, 3).starts_with("Put") {
                "topping"
            } else {
                "discarding"
            };
            let mut out = format!("look at hand of {} {verb} {}card", cap(&c, 1), cap(&c, 2));
            let rest = cap(&c, 4).trim();
            if !rest.is_empty() {
                out.push_str("; ");
                out.push_str(&self.effects(rest, targets)?);
            }
            return Ok(out);
        }
        // Kinship: "you may look at the top card of your library. If it
        // shares a creature type with this creature, you may reveal it. If
        // you do, ...".
        if let Some(c) = cached_regex(
            r"^[Yy]ou may look at the top card of your library\. If it shares a creature type with this creature, you may reveal it\. If you do, (.+?)\.?$",
        )
        .unwrap()
        .captures(text)
        {
            let then = self.effects(cap(&c, 1), targets)?;
            return Ok(format!(
                "you may look at top card; if it shares creature type with this, you may reveal it; if you do, {then}"
            ));
        }
        // "Target opponent reveals their hand. You may choose a nonland
        // card from it. If you do, that player discards that card. If you
        // don't, that player discards two cards." (Nightsnare).
        if let Some(c) = cached_regex(
            r"^(.+?) reveals their hand\. You may choose (a|an) (.+? )?card from it\. If you do, that player discards that card\. If you don't, (.+?)\.?$",
        )
        .unwrap()
        .captures(text)
        {
            let who = recipient(&subject(cap(&c, 1)), targets)
                .ok_or_else(|| Skip::Sentence(text.to_string()))?;
            let what = cap(&c, 3).trim().replace(", ", " ");
            let what = if what.is_empty() {
                "card".to_string()
            } else {
                format!("{what} card")
            };
            let rest = self.effects(cap(&c, 4), targets)?;
            return Ok(format!(
                "{who} reveals hand discarding may {what}; if you don't, {rest}"
            ));
        }
        // "Target opponent reveals their hand. You choose a nonland card
        // from it. Exile that card." / "... and exile that card."
        let exile_chosen = cached_regex(
            r"^(.+?) reveals their hand\. You choose (a|an) (.+? )?card from it(\. Exile| and exile) that card\.?(.*)$",
        )
        .unwrap();
        if let Some(c) = exile_chosen.captures(text) {
            let who = recipient(&subject(cap(&c, 1)), targets)
                .ok_or_else(|| Skip::Sentence(text.to_string()))?;
            let what = cap(&c, 3).trim().replace(", ", " ");
            let what = if what.is_empty() {
                "card".to_string()
            } else {
                format!("{what} card")
            };
            let split = if cap(&c, 4) == ". Exile" {
                " split"
            } else {
                ""
            };
            let mut out = format!("{who} reveals hand exiling {what}{split}");
            let rest = cap(&c, 5).trim();
            if !rest.is_empty() {
                out.push_str("; ");
                out.push_str(&self.effects(rest, targets)?);
            }
            return Ok(out);
        }
        let unless = cached_regex(
            r"^(.+) unless (you pay|you waterbend|they pay|its controller pays|that player pays) ((?:\{[0-9WUBRGC]+\})+|\d+ life)$",
        )
        .unwrap();
        // "Look at the top four cards of your library. You may reveal a
        // creature card from among them ..." and the other dig forms.
        if let Some(out) = self.dig(text, targets)? {
            return Ok(out);
        }
        // CR 702.34a: "Target instant or sorcery card in your graveyard
        // gains flashback until end of turn. The flashback cost is equal
        // to its mana cost."
        if let Some(c) = cached_regex(
            r"^[Tt]arget instant or sorcery card in your graveyard gains flashback until end of turn\. The flashback cost is equal to (its|that card's) mana cost\.?$",
        )
        .unwrap()
        .captures(text)
        {
            *targets += 1;
            let that = if cap(&c, 1) == "its" { "" } else { " that card" };
            return Ok(format!(
                "target instant or sorcery card from your graveyard gains flashback{that}"
            ));
        }
        // "Target creature gains \"...\" until end of turn."
        if let Some(c) = cached_regex(r#"^(.+?) gains? "(.+)" until end of turn\.?$"#)
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
                cleave: None,
            }
            .quoted(cap(&c, 2));
            let who = recipient(&lowercase_first(cap(&c, 1)), targets);
            return match (quoted, who) {
                (Some(inner), Some(who)) => Ok(format!("{who} gains ability {{ {inner} }}")),
                _ => Err(Skip::Sentence(text.to_string())),
            };
        }
        let manifest_then = cached_regex(
            r"^Manifest the top card of your library, then put (a|two|three) \+1/\+1 counters? on it$",
        )
        .unwrap();
        let delayed = cached_regex(
            r"^([Ss]acrifice|[Ee]xile|[Dd]estroy|[Rr]eturn) (it|them|that card|that creature|that token|the token|those tokens|this creature|~)( to (?:its owner's|their owners') hand| to the battlefield under (?:its|their) owner's control)? at (the beginning of the next end step|end of combat|the beginning of your next upkeep)$",
        )
        .unwrap();
        // "Put a -1/-1 counter on it at end of combat", "remove a +1/+1
        // counter from it at end of combat" (CR 603.7).
        let delayed_counter = cached_regex(
            r"^([Pp]ut|[Rr]emove) (a|an|two|three) (\S+) counters? (on|from) (it|this creature|~) at end of combat$",
        )
        .unwrap();
        // A token whose quoted ability spans sentences: the corpus's
        // "Mercenary" token.
        let text = text.replace(
            "reate a 1/1 red Mercenary creature token with \"{T}: Target creature you control gets +1/+0 until end of turn. Activate only as a sorcery.\"",
            "\u{1}create 1 \"Mercenary\"",
        );
        let text = text.replace("C\u{1}", "\u{1}").replace("c\u{1}", "\u{1}");
        // "You may search your library and/or graveyard for a card named
        // X, reveal it, and put it into your hand. If you search your
        // library this way, shuffle."
        let text = cached_regex(
            r"(^|\. )([Yy]ou may s|[Ss])earch your library and/or graveyard for a card named (.+?), reveal it, and put it into your hand\. If you search your library this way, shuffle\.?",
        )
        .map_or(text.clone(), |re| {
            re.replace_all(&text, |c: &regex::Captures| {
                let may = if c[2].len() > 1 { "you may " } else { "" };
                format!(
                    "{}\u{1}{may}search library or graveyard card named \"{}\" reveal to hand.",
                    &c[1], &c[3]
                )
            })
            .into_owned()
        });
        // "... search your library and/or graveyard for a creature card
        // with no abilities, reveal it, and put it into your hand. If you
        // search your library this way, shuffle." (Fang-Druid Summoner):
        // the shuffle sentence joins the search's as `\u{2}`, which the
        // search template below reads.
        let text = cached_regex(
            r"(earch your library and/or graveyard for [^.]+)\. If you search your library this way, shuffle",
        )
        .map_or(text.clone(), |re| {
            re.replace_all(&text, "$1\u{2}").into_owned()
        });
        // "Target player reveals three cards from their hand and you
        // choose one of them. That player discards that card." (Blackmail,
        // Thieving Sprite, Noggin Whack): one sentence for the template
        // below, joined by `\u{2}`.
        let text = cached_regex(
            r"(reveals (?:\w+) cards from their hand(?:, where X is [^.]+| and you choose one of them)?)\.( You choose (?:one|two) of (?:them|those cards)\.)? That player discards (that card|those cards)",
        )
        .map_or(text.clone(), |re| {
            re.replace_all(&text, |c: &regex::Captures| {
                let mid = c.get(2).map_or(String::new(), |m| {
                    format!("\u{2}{}", m.as_str().trim().trim_end_matches('.'))
                });
                format!("{}{mid}\u{2}That player discards {}", &c[1], &c[3])
            })
            .into_owned()
        });
        // "Exile target artifact. Search its controller's graveyard, hand,
        // and library for all cards with the same name as that artifact
        // and exile them. Then that player shuffles." (Splinter).
        let text = cached_regex(
            r"(^|\. )Search its controller's graveyard, hand, and library for all cards with the same name as that [a-z]+ and exile them\. Then that player shuffles\.?",
        )
        .map_or(text.clone(), |re| {
            re.replace_all(&text, "$1\u{1}exile same name as target 1 from its controller zones.")
                .into_owned()
        });
        let text = text.as_str();
        let mut out: Vec<String> = Vec::new();
        let extra_land =
            cached_regex(r"^[Yy]ou may play (an|up to two) additional lands? this turn$").unwrap();
        let ordeal = cached_regex(
            r"^Then if it has (\w+) or more \+1/\+1 counters on it, sacrifice this Aura$",
        )
        .unwrap();
        let gain_that =
            cached_regex(r"^[Yy]ou gain life equal to that creature's (toughness|power)$").unwrap();
        for sentence in merge_impulse(&split_sentences(text)) {
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
            // After a token copy, "it" and "that token" are the token, not
            // the target it copied: "It gains haste." (indefinitely), "It
            // gains haste until end of turn.", "That token gains haste."
            // (`that created token` prints "that token").
            if out
                .last()
                .is_some_and(|l| l.contains("token copy of ") || l.contains("token copies of "))
            {
                let haste = match sentence.as_str() {
                    "It gains haste" => Some("that token gains haste indefinitely"),
                    "It gains haste until end of turn" => {
                        Some("that token gains haste until end of turn")
                    }
                    "That token gains haste" => Some("that created token gains haste indefinitely"),
                    "That token gains haste until end of turn" => {
                        Some("that created token gains haste until end of turn")
                    }
                    _ => None,
                };
                if let Some(h) = haste {
                    out.push(h.to_string());
                    continue;
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
            // "When Purity is put into a graveyard from anywhere, shuffle it
            // into its owner's library."
            if *targets == 0
                && self.about_this.get()
                && sentence.eq_ignore_ascii_case("shuffle it into its owner's library")
            {
                out.push("shuffle this into library".to_string());
                continue;
            }
            if let Some(c) = delayed_counter.captures(&sentence)
                && (cap(&c, 5) != "it" || (*targets == 0 && self.about_this.get()))
            {
                let n = number(cap(&c, 2)).ok_or_else(|| Skip::Sentence(sentence.clone()))?;
                let (verb, prep) = if cap(&c, 1).eq_ignore_ascii_case("put") {
                    ("put", "on")
                } else {
                    ("remove", "from")
                };
                out.push(format!(
                    "delayed this: {verb} {n} {} counter {prep} it at end of combat",
                    cap(&c, 3)
                ));
                continue;
            }
            if let Some(c) = delayed.captures(&sentence) {
                let obj = match cap(&c, 2) {
                    "the token" => "that token",
                    o => o,
                };
                // "Create a token that's a copy of ... It gains haste.
                // Sacrifice it at the beginning of the next end step.": the
                // token, also after a sentence about it.
                let made_token = out.last().is_some_and(|l| {
                    l.contains("create ")
                        || (l.starts_with("that token ") || l.starts_with("that created token "))
                            && out
                                .iter()
                                .rev()
                                .nth(1)
                                .is_some_and(|p| p.contains("create "))
                });
                let who = if made_token && matches!(obj, "it" | "that token") {
                    "that token".to_string()
                } else if made_token && matches!(obj, "them" | "those tokens") {
                    "those tokens".to_string()
                } else if matches!(obj, "this creature" | "~")
                    && !cap(&c, 1).eq_ignore_ascii_case("return")
                {
                    // Sacrificed or destroyed where it is; a card returning
                    // from exile is a new object (CR 400.7).
                    "this".to_string()
                } else if obj == "it"
                    && cap(&c, 1) == "Return"
                    && out.last().is_some_and(|l| l == "exile this")
                {
                    // "Exile this creature. Return it to the battlefield
                    // under its owner's control at the beginning of the next
                    // end step." (Aetherling): the card exiled.
                    "this".to_string()
                } else if *targets == 0 && obj == "that creature" {
                    // "Whenever this creature blocks or becomes blocked by
                    // a creature, destroy that creature at end of combat":
                    // the triggering object.
                    "that creature".to_string()
                } else if *targets == 0 && obj == "it" && !made_token && self.about_this.get() {
                    // "When this creature attacks, sacrifice it at end of
                    // combat."
                    "this".to_string()
                } else if *targets > 0
                    && matches!(obj, "it" | "that card" | "that creature")
                    && (!out.iter().any(|l| l.contains("to battlefield"))
                        || (cap(&c, 1) != "Return"
                            && out.iter().any(|l| {
                                l.starts_with("return target ") || l.starts_with("put target ")
                            })))
                {
                    // The target: the card in the graveyard, or the
                    // permanent a single target card became ("Return target
                    // creature card ... to the battlefield. It gains haste.
                    // Sacrifice it at the beginning of the next end step.").
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
                } else if cap(&c, 4) == "the beginning of your next upkeep" {
                    "beginning of your next upkeep"
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
            // The Ordeals: "Whenever enchanted creature attacks, put a +1/+1
            // counter on it. Then if it has three or more +1/+1 counters on
            // it, sacrifice this Aura." ("it" is the enchanted creature).
            if let Some(c) = ordeal.captures(&sentence)
                && out
                    .last()
                    .is_some_and(|p| p.starts_with("put 1 +1/+1 counter"))
            {
                // The counter goes on the attacking creature, `it` (the
                // templates read the first "it" as the source).
                if let Some(p) = out.last_mut() {
                    *p = "put 1 +1/+1 counter on it".to_string();
                }
                let n = number(cap(&c, 1)).ok_or_else(|| Skip::Sentence(sentence.clone()))?;
                out.push(format!(
                    "then if attached is with {n} or more +1/+1 counters, sacrifice this"
                ));
                continue;
            }
            // "Sacrifice a creature. You gain life equal to that creature's
            // toughness." (Doomgape): the creature sacrificed this way.
            if let Some(c) = gain_that.captures(&sentence)
                && out
                    .last()
                    .is_some_and(|p| p.starts_with("sacrifice 1 creature"))
            {
                out.push(format!(
                    "you gain count equal {} of that sacrificed creature life",
                    cap(&c, 1)
                ));
                continue;
            }
            let if_cond;
            // "You may cast ... without paying its mana cost": the DSL's
            // `cast free` keeps its "you may" (CR 608.2g).
            let (prefix, body) = if let Some(rest) = sentence
                .strip_prefix("you may ")
                .or_else(|| sentence.strip_prefix("You may "))
                .filter(|r| !(r.starts_with("cast ") && r.contains("without paying its mana cost")))
                // "You may cast target ... card from your graveyard this
                // turn": the DSL's `may cast target` keeps it too.
                .filter(|r| !(r.starts_with("cast target ") && r.ends_with(" this turn")))
            {
                ("you may ", rest.to_string())
            } else if let Some(rest) = sentence.strip_prefix("If you do, ") {
                ("if you do, ", rest.to_string())
            } else if let Some(rest) =
                sentence.strip_prefix("If you didn't put a card into your hand this way, ")
            {
                ("if you didnt put into hand this way, ", rest.to_string())
            } else if let Some(rest) =
                sentence.strip_prefix("If you don't put a card into your hand this way, ")
            {
                ("if you dont put into hand this way, ", rest.to_string())
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
            } else if let Some(rest) = sentence
                .strip_prefix("Then ")
                .filter(|_| !out.is_empty())
                .filter(|r| r.starts_with("return ") || r.starts_with("put "))
            {
                // "You may mill three cards. Then return a creature card
                // from your graveyard to the battlefield."
                ("then ", rest.to_string())
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
                || body.contains("earches their library")
                || body == "Discard all the cards in your hand, then draw that many cards"
                || (body.contains("iscard ") && body.contains(", then draw that many cards"))
                || body.ends_with("explores, then it explores again")
                || body.contains(", then meld them into ")
                || body == "Exile ~, then return it to the battlefield"
                || body.contains(", then return that card to the battlefield under ")
                || body.contains("ach player discards their hand, then draws ")
                || (body.starts_with("Target player draws ") && body.contains(", then discards "))
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
                // A rule whose target phrase took in a second clause ("tap
                // target creature ... and put a stun counter on it" as one
                // target) gives way to the clauses authored apart.
                let before = *targets;
                let whole = self.sentence(&capitalize(&head), targets);
                let swallowed = whole.as_ref().is_some_and(|w| {
                    head.split(" and ")
                        .skip(1)
                        .filter_map(|r| {
                            let w3: Vec<&str> = r.split(' ').take(3).collect();
                            (w3.len() == 3).then(|| w3.join(" "))
                        })
                        .any(|r| w.contains(&format!(" and {r}")))
                });
                let e = if swallowed || whole.is_none() {
                    let after = *targets;
                    *targets = before;
                    self.and_split(&head, targets).or_else(|| {
                        *targets = after;
                        whole
                    })
                } else {
                    whole
                };
                let Some(e) = e else {
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
            // "Create a 0/0 ... Fractal creature token. Put X +1/+1
            // counters on it." (Manifestation Sage, Additive Evolution):
            // "it" is the token, not this card.
            if body.starts_with("Put ")
                && (body.trim_end_matches('.').ends_with(" counters on it")
                    || body.contains(" counters on it, where X is "))
                && out.last().is_some_and(|l| l.contains("create 1 "))
                && let Some(head) = e.strip_suffix(" counters on this")
            {
                e = format!("{head} counters on that token");
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
fn merge_impulse(sentences: &[String]) -> Vec<String> {
    let exile = cached_regex(
        r"^(If you do, |you may |You may )?[Ee]xile the top (card|\w+ cards) of your library(?:, where X is (.+))?$",
    )
    .unwrap();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < sentences.len() {
        let s = &sentences[i];
        // "Choose one of them." between the exile and the permission:
        // `choose`.
        let chosen = sentences
            .get(i + 1)
            .is_some_and(|n| n == "Choose one of them");
        let next_at = if chosen { i + 2 } else { i + 1 };
        let merged = exile.captures(s).and_then(|c| {
            let n = match (cap(&c, 2), c.get(3)) {
                ("X cards", Some(x)) => x_count(x.as_str())?,
                (_, Some(_)) => return None,
                ("card", None) => "1".to_string(),
                (w, None) => number(w.strip_suffix(" cards")?)?,
            };
            let next = sentences.get(next_at)?;
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
                "You may play those cards until your next end step" => "next end step",
                "You may play cards exiled this way until the end of your next turn" => {
                    "next turn this way"
                }
                "You may play cards exiled this way this turn" => "this turn this way",
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
            let choose = if chosen { " choose" } else { "" };
            Some(format!("\u{1}{prefix}impulse {n} {when}{choose}"))
        });
        if let Some(m) = merged {
            out.push(m);
            i = next_at + 1;
            continue;
        }
        // "... without paying its mana cost. If that spell would be put
        // into your graveyard, exile it instead.": one sentence, marked.
        if s.ends_with("without paying its mana cost")
            && sentences.get(i + 1).is_some_and(|n| {
                n.trim_end_matches('.')
                    == "If that spell would be put into your graveyard, exile it instead"
            })
        {
            out.push(format!("{s}\u{2}"));
            i += 2;
            continue;
        }
        out.push(s.clone());
        i += 1;
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
#[allow(clippy::too_many_lines)] // one arm per cost item
fn cost_text(s: &str) -> Option<String> {
    let remove = cached_regex(
        r"^Remove (an?|two|three|\w+) ([a-z+/1-]+(?: strike)?) counters? from (?:this (?:artifact|creature|land|enchantment|permanent)|~)$",
    )
    .unwrap();
    // Hybrid symbols too ("{R/W}{R/W}"): the caller has checked that each
    // parses.
    let symbols = cached_regex(r"^(\{[0-9XWUBRGCSTP/]+\})+$").unwrap();
    let mut items = Vec::new();
    let two = cached_regex(r"^[a-z]+ or [a-z]+$").unwrap();
    let exile_gy = cached_regex(
        r"^Exile (an?|two|three|four|five|six|X) (?:([a-z]+) )?cards? from your graveyard$",
    )
    .unwrap();
    let adj_creature = cached_regex(r"^(?:colorless|nontoken) creature$").unwrap();
    // "Tap three untapped creatures you control".
    // "Tap two untapped Merfolk you control" (a subtype too).
    let tap_n =
        cached_regex(r"^Tap (two|three|four|five|six|seven) untapped ([A-Za-z]+) you control$")
            .unwrap();
    // "Remove three quest counters from this enchantment and sacrifice it"
    // (the Expeditions and Quests): two costs joined by "and".
    let remove_and_sac = cached_regex(
        r"^Remove (an?|two|three|four|five|six|seven|eight|nine|ten) ([a-z]+) counters? from this (?:artifact|creature|land|enchantment|permanent) and sacrifice it$",
    )
    .unwrap();
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
                | "Sacrifice this Equipment"
                | "Sacrifice this Vehicle"
                | "Sacrifice ~"
        ) {
            "sacrifice this".to_string()
        } else if matches!(
            part,
            "Exile this creature"
                | "Exile this artifact"
                | "Exile this enchantment"
                | "Exile this permanent"
                | "Exile this land"
                | "Exile ~"
        ) {
            "exile this".to_string()
        } else if let Some(rest) = part.strip_prefix("Sacrifice another ") {
            // "Sacrifice another creature", "... another creature or
            // enchantment", "... another colorless creature".
            if rest.contains(' ') && !two.is_match(rest) && !adj_creature.is_match(rest) {
                return None;
            }
            format!("sacrifice other {rest}")
        } else if let Some(c) = tap_n.captures(part) {
            let what = singular(cap(&c, 2));
            if what.chars().next().is_some_and(char::is_lowercase)
                && !matches!(what.as_str(), "creature" | "artifact" | "land")
            {
                return None;
            }
            format!("tap {} untapped {what} you control", number(cap(&c, 1))?)
        } else if part == "Discard a card at random" {
            "discard 1 card at random".to_string()
        } else if let Some(c) = remove_and_sac.captures(part) {
            let n = if matches!(cap(&c, 1), "a" | "an") {
                "1".to_string()
            } else {
                number(cap(&c, 1))?
            };
            format!(
                "remove {n} {} counters from this, sacrifice this",
                cap(&c, 2)
            )
        } else if let Some(n) = part
            .strip_prefix("Discard ")
            .and_then(|r| r.strip_suffix(" cards"))
            .filter(|n| *n != "X")
            .and_then(number)
        {
            // "Discard two cards".
            format!("discard {n} cards")
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
        } else if let Some((n, rest)) = part
            .strip_prefix("Sacrifice ")
            .and_then(|r| r.split_once(' '))
            .and_then(|(n, rest)| Some((number(n)?, rest)))
            .filter(|(n, rest)| {
                n != "1" && n.bytes().all(|b| b.is_ascii_digit()) && !rest.contains(' ')
            })
        {
            // "Sacrifice two Mountains".
            format!("sacrifice {n} {}", singular(rest))
        } else if part == "Discard another card named ~" {
            // Grandeur (an ability word, CR 207.2c).
            "discard other named this card".to_string()
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
        } else if part == "Put a -1/-1 counter on a creature you control" {
            // Blight's older wording (CR 701.68a).
            "put 1 -1/-1 counter on creature you control".to_string()
        } else if part == "Put a card an opponent owns from exile into that player's graveyard" {
            // A processor.
            "put 1 card an opponent owns from exile into graveyard".to_string()
        } else if let Some(kind) = part
            .strip_prefix("Remove a ")
            .and_then(|r| r.strip_suffix(" counter from a creature you control"))
            .filter(|k| !k.contains(' '))
        {
            format!("remove 1 {kind} counter from creature you control")
        } else if part == "Discard this card" {
            "discard this".to_string()
        } else if part == "Reveal this card from your hand"
            || part == "Reveal this creature from your hand"
        {
            "reveal this".to_string()
        } else if let Some(c) = remove.captures(part) {
            let n = if matches!(cap(&c, 1), "a" | "an") {
                "1".to_string()
            } else {
                number(cap(&c, 1))?
            };
            format!("remove {n} {} counters from this", cap(&c, 2))
        } else if let Some(c) = exile_gy.captures(part) {
            // "Exile a creature card from your graveyard", "Exile two cards
            // from your graveyard".
            let n = number(cap(&c, 1))?;
            let n = if n == "1" {
                String::new()
            } else {
                format!("{n} ")
            };
            if cap(&c, 2).is_empty() {
                format!("exile {n}cards from graveyard")
            } else {
                format!("exile {n}{} card from graveyard", cap(&c, 2))
            }
        } else if let Some(n) = part
            .strip_prefix("Exile ")
            .and_then(|r| r.strip_suffix(" cards from your graveyard and this creature"))
            .and_then(number)
        {
            // "Exile two cards from your graveyard and this creature".
            format!("exile {n} cards from graveyard and this")
        } else if part == "Exile this card from your graveyard" {
            "exile this from graveyard".to_string()
        } else if let Some(kind) = part
            .strip_prefix("Exile the top ")
            .and_then(|r| r.strip_suffix("card of your graveyard"))
            .filter(|k| k.is_empty() || (k.ends_with(' ') && !k.trim_end().contains(' ')))
        {
            // CR 404.2: "Exile the top creature card of your graveyard".
            format!("exile top {kind}card from graveyard")
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
        } else if let Some(rest) = part
            .strip_prefix("Tap another ")
            .filter(|r| r.starts_with("untapped "))
        {
            // "Tap another untapped creature you control".
            format!("tap other {rest}")
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
        // The Ordeals.
        "When you sacrifice this Aura" | "When you sacrifice this enchantment" => {
            "when you sacrifice this"
        }
        // CR 603.6a: the card itself, from any zone.
        "When this creature is put into a graveyard from anywhere" => {
            "when this is put into a graveyard from anywhere"
        }
        // CR 726.3: the day/night switch.
        "Whenever day becomes night or night becomes day" => {
            "whenever day becomes night or night becomes day"
        }
        // CR 702.55b-c: haunt.
        "When this creature enters or the creature it haunts dies" => {
            "when this enters or haunted dies"
        }
        "When the creature this card haunts dies" => "when haunted dies",
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
        // A Curse (CR 303.4): "enchanted controller" is the player.
        "At the beginning of enchanted player's upkeep" => {
            "at beginning of enchanted controller's upkeep"
        }
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
        "Whenever this creature deals combat damage to a player or planeswalker" => {
            "whenever this deals combat damage to a player or planeswalker"
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
        | "Whenever an opponent casts their first noncreature spell each turn"
        | "Whenever you cast your first noncreature spell each turn"
        | "Whenever you cast a spell from exile"
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
        // The per-object and batched forms (CR 603.2c).
        "Whenever a card leaves your graveyard" => "whenever a card leaves your graveyard",
        "Whenever one or more players discard one or more cards" => {
            "whenever one or more players discard one or more cards"
        }
        "Whenever you put a +1/+1 counter on a creature" => {
            "whenever you put a +1/+1 counter on creature"
        }
        "Whenever you put a +1/+1 counter on another creature" => {
            "whenever you put a +1/+1 counter on other creature"
        }
        "Whenever you put a +1/+1 counter on a creature you control" => {
            "whenever you put a +1/+1 counter on creature you control"
        }
        "Whenever you put a counter on a creature you control" => {
            "whenever you put a counter on creature you control"
        }
        "Whenever one or more creature cards are put into your graveyard from anywhere" => {
            "whenever one or more creature card put into your graveyard from anywhere"
        }
        "Whenever one or more creature cards are put into your graveyard from anywhere during your turn" => {
            "whenever one or more creature card put into your graveyard from anywhere during your turn"
        }
        "Whenever one or more creatures you control become tapped" => {
            "whenever one or more creature you control become tapped"
        }
        "Whenever you tap one or more untapped creatures your opponents control" => {
            "whenever you tap one or more untapped creature your opponents control"
        }
        // CR 701.27: a transforming card's face names itself.
        "Whenever this creature enters or transforms into ~" => {
            "whenever this enters or transforms"
        }
        "When this creature transforms into ~" | "When this permanent transforms into ~" => {
            "when this transforms"
        }
        "Whenever this creature transforms into ~" => "whenever this transforms",
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
            let none = cached_regex(
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
            let mv = cached_regex(
                r"^Whenever you cast a spell with mana value (\d+) or (greater|less)$",
            )
            .unwrap();
            if let Some(c) = mv.captures(head) {
                return Some(format!(
                    "whenever you cast with mana value {} or {} spell",
                    cap(&c, 1),
                    cap(&c, 2)
                ));
            }
            // "Whenever you cast a spell that's both white and black" (CR
            // 105.2: every one of the colors).
            let both = cached_regex(
                r"^Whenever you cast a spell that's both (white|blue|black|red|green) and (white|blue|black|red|green)$",
            )
            .unwrap();
            if let Some(c) = both.captures(head) {
                return Some(format!(
                    "whenever you cast both {} or {} spell",
                    cap(&c, 1),
                    cap(&c, 2)
                ));
            }
            // "Whenever this creature or another Ally you control enters".
            let this_or = cached_regex(
                r"^Whenever this creature or another (creature|Ally|artifact|enchantment) you control enters$",
            )
            .unwrap();
            if let Some(c) = this_or.captures(head) {
                return Some(format!(
                    "whenever this or {} you control enters",
                    cap(&c, 1)
                ));
            }
            // "Whenever a creature you control with power 4 or greater
            // enters" (Garruk's Uprising).
            if let Some(c) = cached_regex(
                r"^Whenever a(nother)? creature you control with (power|toughness) (\d+) or (greater|less) enters$",
            )
            .unwrap()
            .captures(head)
            {
                let other = if c.get(1).is_some() { "other " } else { "" };
                return Some(format!(
                    "whenever {other}creature you control with {} {} or {} enters",
                    cap(&c, 2),
                    cap(&c, 3),
                    cap(&c, 4)
                ));
            }
            // "Whenever this creature blocks a creature with flying",
            // "... becomes blocked by a creature".
            let pair = cached_regex(
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
            let re = cached_regex(
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
        // A Room's door (CR 709.5h, 709.5i).
        "When you unlock this door" => Some("when you unlock this door"),
        "Whenever you fully unlock a Room" => Some("whenever you fully unlock a Room"),
        "Whenever you discard one or more cards" => Some("whenever you discard one or more cards"),
        "Whenever a player discards one or more cards" => {
            Some("whenever a player discards one or more cards")
        }
        "Whenever an opponent discards one or more cards" => {
            Some("whenever an opponent discards one or more cards")
        }
        "Whenever you gain or lose life" => Some("whenever you gain or lose life"),
        "Whenever you gain or lose life during your turn" => {
            Some("whenever you gain or lose life during your turn")
        }
        "Whenever you scry or surveil" => Some("whenever you scry or surveil"),
        "Whenever a creature you control explores" => {
            Some("whenever creature you control explores")
        }
        "Whenever you tap an untapped creature an opponent controls" => {
            Some("whenever you tap untapped creature an opponent controls")
        }
        "Whenever you activate an exhaust ability" => {
            Some("whenever you activate an exhaust ability")
        }
        "Whenever you activate a loyalty ability" => {
            Some("whenever you activate a loyalty ability")
        }
        "Whenever an opponent activates a loyalty ability" => {
            Some("whenever an opponent activates a loyalty ability")
        }
        "Whenever you activate an ability of a creature" => {
            Some("whenever you activate an ability of a creature")
        }
        "Whenever you cast your third spell each turn" => {
            Some("whenever you cast your third spell each turn")
        }
        "Whenever you sacrifice one or more Foods" => {
            Some("whenever you sacrifice one or more Food")
        }
        "Whenever you attack with one or more creatures with power 4 or greater" => {
            Some("whenever you attack with one or more creature with power 4 or greater")
        }
        "Whenever you attack a player with one or more creatures with power 4 or greater" => {
            Some("whenever you attack a player with one or more creature with power 4 or greater")
        }
        "Whenever one or more creatures you control attack" => {
            Some("whenever one or more creatures you control attack")
        }
        "Whenever two or more creatures your opponents control attack" => {
            Some("whenever 2 or more creatures your opponents control attack")
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
    let attached = cached_regex(
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
    let class = cached_regex(
        r"^(When|Whenever) (a|an|another) ((?:[a-z]+ )*?(?:[A-Z][a-z]+ )*?(?:creature|artifact|enchantment|land|permanent|planeswalker|token|[A-Z][a-z]+)(?: creature)?(?: token)?(?: you control| an opponent controls| your opponents control)?) (enters|dies|attacks|blocks|attacks alone|becomes blocked|deals combat damage to a player)$",
    )
    .unwrap();
    // "Whenever you sacrifice a Food", "Whenever you sacrifice another
    // creature".
    if let Some(c) = cached_regex(r"^Whenever you sacrifice (a|an|another) ([A-Za-z]+)$")
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
    // CR 103.4: against the starting life total.
    match s {
        "your life total is greater than your starting life total" => {
            return Some("life above starting".to_string());
        }
        "your life total is greater than or equal to your starting life total" => {
            return Some("life at least starting".to_string());
        }
        "your life total is less than your starting life total" => {
            return Some("life below starting".to_string());
        }
        _ => {}
    }
    if let Some(n) = s
        .strip_prefix("you have at least ")
        .and_then(|r| r.strip_suffix(" life more than your starting life total"))
        .and_then(|n| n.parse::<u32>().ok())
    {
        return Some(format!("life {n} more than starting"));
    }
    if let Some(n) = s
        .strip_prefix("your life total is at least ")
        .and_then(|r| r.strip_suffix(" greater than your starting life total"))
        .and_then(|n| n.parse::<u32>().ok())
    {
        return Some(format!("life {n} above starting"));
    }
    // Pack tactics.
    if let Some(n) = s
        .strip_prefix("you attacked with creatures with total power ")
        .and_then(|r| r.strip_suffix(" or greater this combat"))
        .and_then(|n| n.parse::<u16>().ok())
    {
        return Some(format!("attacked total power {n}"));
    }
    match s {
        "you attacked this turn" => return Some("attacked this turn".to_string()),
        "you haven't cast a spell this turn" => return Some("not cast this turn".to_string()),
        "an opponent controls more creatures than you" => {
            return Some("opponent controls more creatures".to_string());
        }
        "an opponent has more life than you" => return Some("opponent has more life".to_string()),
        "an opponent has more cards in hand than you" => {
            return Some("opponent has more cards in hand".to_string());
        }
        "you haven't cast a spell from your hand this turn" => {
            return Some("not cast from hand this turn".to_string());
        }
        "a creature died this turn" => return Some("creature died this turn".to_string()),
        "you have no cards in hand" => return Some("hand 0 or less".to_string()),
        "it's your turn" => return Some("your turn".to_string()),
        "it's attacking" | "this creature is attacking" => return Some("attacking".to_string()),
        "it's enchanted" | "this creature is enchanted" => return Some("enchanted".to_string()),
        "it's equipped" | "this creature is equipped" => return Some("equipped".to_string()),
        "this spell's sneak cost was paid" | "its sneak cost was paid" => {
            return Some("sneak paid".to_string());
        }
        "this spell's prowl cost was paid" | "its prowl cost was paid" => {
            return Some("prowl paid".to_string());
        }
        "his sneak cost was paid" => return Some("his sneak paid".to_string()),
        "her sneak cost was paid" => return Some("her sneak paid".to_string()),
        "this spell was kicked" => return Some("kicked".to_string()),
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
        "you have an enduring story" => return Some("enduring story".to_string()),
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
        "a creature died under your control this turn" => {
            return Some("creature died under your control this turn".to_string());
        }
        "an opponent controls more lands than you" => {
            return Some("opponent controls more lands".to_string());
        }
        "a nonland permanent left the battlefield this turn or a spell was warped this turn" => {
            return Some("void".to_string());
        }
        "it isn't your main phase" => return Some("not your main phase".to_string()),
        "this spell was cast from anywhere other than your hand"
        | "it was cast from anywhere other than your hand" => {
            return Some("cast from anywhere other than hand".to_string());
        }
        "you put a counter on a creature this turn" => {
            return Some("put any counter this turn".to_string());
        }
        _ => {}
    }
    // "there is an Elf card in your graveyard and this creature has a -1/-1
    // counter on it": two conditions, both holding.
    if let Some((a, b)) = s.split_once(" and ")
        && let (Some(a), Some(b)) = (static_condition(a), static_condition(b))
    {
        return Some(format!("{a} and {b}"));
    }
    if let Some(c) = cached_regex(r"^(\w+) or more creatures died this turn$")
        .unwrap()
        .captures(s)
    {
        return Some(format!("{} creatures died this turn", number(cap(&c, 1))?));
    }
    if let Some(c) = cached_regex(r"^creatures you control have total toughness (\d+) or greater$")
        .unwrap()
        .captures(s)
    {
        return Some(format!("total toughness {} or more", cap(&c, 1)));
    }
    if let Some(c) = cached_regex(r"^a player has (\d+) or less life$")
        .unwrap()
        .captures(s)
    {
        return Some(format!("player life {} or less", cap(&c, 1)));
    }
    if let Some(c) = cached_regex(r"^you have (\w+) or (fewer|more) cards in hand$")
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
    if let Some(c) = cached_regex(r"^an opponent has (\d+) or less life$")
        .unwrap()
        .captures(s)
    {
        return Some(format!("opponent life {} or less", cap(&c, 1)));
    }
    if let Some(c) = cached_regex(r"^you have (\d+) or (more|less) life$")
        .unwrap()
        .captures(s)
    {
        return Some(format!("your life {} or {}", cap(&c, 1), cap(&c, 2)));
    }
    let more =
        cached_regex(r"^there are (\S+) or more ((?:[a-z]+ )*)cards in your graveyard$").unwrap();
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
    if let Some(c) = cached_regex(r"^creatures you control have total power (\d+) or greater$")
        .unwrap()
        .captures(s)
    {
        return Some(format!("total power {} or more", cap(&c, 1)));
    }
    if let Some(c) = cached_regex(r"^an opponent has (\w+) or more poison counters$")
        .unwrap()
        .captures(s)
    {
        return Some(format!("opponent poison {} or more", number(cap(&c, 1))?));
    }
    // Counters on the source (CR 122.1): "there are five or more growth
    // counters on this enchantment", "this creature has four or more oil
    // counters on it".
    let self_word = r"this (?:creature|artifact|enchantment|land|permanent|Vehicle|Equipment)";
    if let Some(c) = cached_regex(&format!(
        r"^there are (\S+) or more ([+-]\d/[+-]\d|{}) counters on {self_word}$",
        counter_names()
    ))
    .unwrap()
    .captures(s)
    {
        return Some(format!(
            "there are {} or more {} counters on this",
            number(cap(&c, 1))?,
            cap(&c, 2)
        ));
    }
    if let Some(c) = cached_regex(&format!(
        r"^{self_word} has (\S+) or more ([+-]\d/[+-]\d|{}) counters on it$",
        counter_names()
    ))
    .unwrap()
    .captures(s)
        && cap(&c, 1) != "one"
    {
        return Some(format!(
            "this has {} or more {} counters",
            number(cap(&c, 1))?,
            cap(&c, 2)
        ));
    }
    let types = cached_regex(r"^there are (\S+) or more card types among cards in your graveyard$")
        .unwrap();
    if let Some(c) = types.captures(s) {
        return Some(format!(
            "{} card types in your graveyard",
            number(cap(&c, 1))?
        ));
    }
    // The fast lands: "you control two or fewer other lands".
    let fewer = cached_regex(r"^you control (\S+) or fewer ([a-zA-Z ]+)$").unwrap();
    if let Some(c) = fewer.captures(s) {
        let f = control_filter(&singular(cap(&c, 2)))?;
        return Some(format!("you control at most {} {f}", number(cap(&c, 1))?));
    }
    // The check lands: "you control a Mountain or a Plains".
    let either = cached_regex(r"^you control an? ([A-Z][a-z]+) or an? ([A-Z][a-z]+)$").unwrap();
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
    let control = cached_regex(r"^you control (\S+) or more ([a-zA-Z ]+)$").unwrap();
    if let Some(c) = control.captures(s) {
        let f = control_filter(&singular(cap(&c, 2)))?;
        return Some(format!("you control {} {f}", number(cap(&c, 1))?));
    }
    let one = cached_regex(r"^you control an? ([a-zA-Z ]+)$").unwrap();
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
    let gets = cached_regex(r"^gets? ([+-]\d+/[+-]\d+)$").unwrap();
    let per = cached_regex(r"^gets? ([+-]\d+/[+-]\d+) for (each|every) (.+)$").unwrap();
    let mut mods = Vec::new();
    for part in s.split(" and ") {
        if let Some(c) = per.captures(part) {
            // "for every seven cards in your graveyard": `every 7 ...`.
            let what = if cap(&c, 2) == "every" {
                format!("every {}", cap(&c, 3))
            } else {
                cap(&c, 3).to_string()
            };
            mods.push(format!("{} count each {}", cap(&c, 1), count_expr(&what)?));
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
            "this creature" | "This creature" | "this permanent" | "This permanent" | "~" => "this",
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
        for n in [4, 3, 2, 1] {
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
        // "This creature gets +2/+2 for each Aura attached to it." (Kor
        // Spiritdancer), "... for each oil counter on it." (Trawler
        // Drake): "it" is the source.
        if f == "this"
            && let Some(c) = cached_regex(&format!(
                r"^gets ([+-]\d+/[+-]\d+) for each (Aura attached to it|({}) counter on it)\.?$",
                counter_names()
            ))
            .unwrap()
            .captures(&pred)
        {
            let what = match c.get(3) {
                Some(k) => format!("{} counter on it", k.as_str()),
                None => "Aura attached to this".to_string(),
            };
            return Some(format!("static this: {} count each {what}", cap(&c, 1)));
        }
        return Some(format!("static {f}: {}", static_mods(&pred)?));
    };
    let (f, pred) = split_subject(body)?;
    // "~ has indestructible as long as it has a divinity counter on it"
    // (the Myojin): "it" is the subject, the source.
    let one_counter = cached_regex(&format!(
        r"^it has an? ([+-]\d/[+-]\d|{}) counter on it$",
        counter_names()
    ))
    .unwrap();
    let cond = match one_counter.captures(cond) {
        Some(c) if f == "this" => format!("this has 1 or more {} counters", cap(&c, 1)),
        _ => static_condition(cond)?,
    };
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
    /// Cleave (CR 702.148a): its cost, and the spell's paragraphs with the
    /// bracketed words removed.
    cleave: Option<(String, Vec<String>)>,
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
            cleave: None,
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
        // CR 711.2: "LEVEL 1-4 2/2" (joined above), "LEVEL 5+ 5/5".
        if let Some(c) = cached_regex(r"^LEVEL (\d+)(?:-(\d+)|\+) (\d+/\d+)$")
            .unwrap()
            .captures(line)
        {
            let range = match c.get(2) {
                Some(m) => format!("{} to {}", cap(&c, 1), m.as_str()),
                None => format!("{} or more", cap(&c, 1)),
            };
            self.statements
                .push(format!("levels {range} pt {}", cap(&c, 3)));
            return Ok(());
        }
        // CR 702.152a: "Blitz {3}{G}", "Blitz—{2}{B}{B}, Pay 2 life."
        if let Some(c) = cached_regex(r"^Blitz ((?:\{[0-9WUBRGC]\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("blitz {}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(c) = cached_regex(r"^Blitz\x{2014}((?:\{[0-9WUBRGC]\})+), Pay (\d+) life\.$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("blitz {}, pay {} life", cap(&c, 1), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.160a: "Prototype {2}{R} — 3/2".
        if let Some(c) = cached_regex(r"^Prototype ((?:\{[0-9WUBRGC]\})+) \x{2014} (\d+/\d+)$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("prototype {} {}", cap(&c, 1), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.87a.
        if let Some(c) = cached_regex(r"^Level up ((?:\{[0-9WUBRGC]\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("level up {}", cap(&c, 1)));
            return Ok(());
        }
        // CR 716.2a: "{R}{G}: Level 2".
        if let Some(c) = cached_regex(r"^((?:\{[0-9WUBRGC]+\})+): Level (\d)$")
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
        if let Some(c) = cached_regex(
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
        if let Some(c) = cached_regex(r"^Mobilize (\d+)$").unwrap().captures(line) {
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
        if let Some(c) = cached_regex(r"^(\d+)\+ \| (.+)$").unwrap().captures(line) {
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
        if let Some(c) = cached_regex(
            r"^(Other [A-Za-z ]+?|Creature tokens|Attacking creatures|Nontoken creatures|Tapped creatures|Untapped creatures|Artifact creatures|Legendary creatures) you control ((?:get|have) .+)\.$",
        )
        .unwrap()
        .captures(line)
            && let Some(mods) = static_mods(cap(&c, 2))
        {
            // "Other Orcs and Goblins you control" (Warg Rider): either
            // creature type, `or` in the DSL.
            let class = singular(&lowercase_first(cap(&c, 1)));
            let class = match cached_regex(r"^other ([A-Z][a-z]+) and ([A-Z][a-z]+)$")
                .unwrap()
                .captures(&class)
            {
                Some(t) => format!("other {} or {}", cap(&t, 1), cap(&t, 2)),
                None => class,
            };
            self.statements
                .push(format!("static {class} you control: {mods}"));
            return Ok(());
        }
        // "Sliver creatures you control get +3/+3."
        if let Some(c) = cached_regex(r"^([A-Z][a-z]+) creatures you control ((?:get|have) .+)\.$")
            .unwrap()
            .captures(line)
            && let Some(mods) = static_mods(cap(&c, 2))
        {
            // "Red creatures", "Modified creatures": a quality, not a
            // creature type.
            let w = cap(&c, 1);
            let lower = w.to_ascii_lowercase();
            let class = if [
                "white",
                "blue",
                "black",
                "red",
                "green",
                "multicolored",
                "monocolored",
                "colorless",
                "modified",
                "legendary",
                "nontoken",
                "artifact",
                "enchantment",
            ]
            .contains(&lower.as_str())
            {
                lower
            } else {
                w.to_string()
            };
            self.statements
                .push(format!("static {class} creature you control: {mods}"));
            return Ok(());
        }
        // "Creatures you control with flying get +1/+1.", "Other white
        // creatures get +1/+1."
        if let Some(c) = cached_regex(
            r"^(Creatures you control with flying|Other (?:white|blue|black|red|green) creatures) ((?:get|have) .+)\.$",
        )
        .unwrap()
        .captures(line)
            && let Some(mods) = static_mods(cap(&c, 2))
        {
            let class = singular(&lowercase_first(cap(&c, 1)));
            self.statements.push(format!("static {class}: {mods}"));
            return Ok(());
        }
        // "This creature has hexproof as long as it's untapped."
        if let Some(c) =
            cached_regex(r"^This creature has ([a-z ]+) as long as it's (untapped|tapped)\.$")
                .unwrap()
                .captures(line)
            && let Some(ks) = keyword_list(cap(&c, 1))
        {
            self.statements.push(format!(
                "static this: has {ks} as long as this is {}",
                cap(&c, 2)
            ));
            return Ok(());
        }
        // "This creature can't be blocked as long as it's attacking alone."
        if line == "This creature can't be blocked as long as it's attacking alone." {
            self.statements
                .push("static this: has can't be blocked as long as attacking alone".to_string());
            return Ok(());
        }
        // "Zombies you control get +1/+0.", "Skeletons, Vampires, and
        // Zombies you control get +1/+1.": creature types, one or a list.
        if let Some(c) = cached_regex(
            r"^((?:[A-Z][a-z]+, )*(?:[A-Z][a-z]+,? and )?[A-Z][a-z]+) you control ((?:get|have) .+)\.$",
        )
        .unwrap()
        .captures(line)
            && let Some(mods) = static_mods(cap(&c, 2))
        {
            let types: Vec<String> = cap(&c, 1)
                .split([',', ' '])
                .filter(|w| !w.is_empty() && *w != "and")
                .map(singular)
                .collect();
            // Each a creature type some card has: the singular of an
            // irregular plural must be known ("Werewolves").
            let known = |t: &String| self.corpus.iter().any(|c| c.subtypes.contains(t));
            if types
                .iter()
                .all(|t| !card_type_word(&t.to_lowercase()) && known(t))
            {
                self.statements
                    .push(format!("static {} you control: {mods}", types.join(" or ")));
                return Ok(());
            }
        }
        // CR 615: "Prevent all combat damage that would be dealt to this
        // creature." (Seraph of the Sword), "Prevent all damage ..."
        // (Glittering Lynx's first line).
        if line == "Prevent all combat damage that would be dealt to this creature." {
            self.statements
                .push("replace combat damage to this: prevent".to_string());
            return Ok(());
        }
        if line == "Prevent all damage that would be dealt to this creature." {
            self.statements
                .push("replace damage to this: prevent".to_string());
            return Ok(());
        }
        if line == "If you would gain life, you gain twice that much life instead." {
            self.statements
                .push("replace you gain life: double".to_string());
            return Ok(());
        }
        // CR 614.1a, 111.1: token doublers.
        if line
            == "If one or more tokens would be created under your control, twice that many of those tokens are created instead."
        {
            self.statements
                .push("replace tokens created under your control: double".to_string());
            return Ok(());
        }
        if line
            == "If an effect would create one or more tokens under your control, it creates twice that many of those tokens instead."
        {
            self.statements
                .push("replace tokens created under your control by an effect: double".to_string());
            return Ok(());
        }
        if line == "If you would gain life, you gain that much life plus 1 instead." {
            self.statements
                .push("replace you gain life: plus one".to_string());
            return Ok(());
        }
        // CR 113.6b: works in every zone (Darksteel Colossus).
        if let Some(c) = cached_regex(
            r"^If (.+) would be put into a graveyard from anywhere, reveal (.+) and shuffle it into its owner's library instead\.$",
        )
        .unwrap()
        .captures(line)
            && cap(&c, 1) == cap(&c, 2)
        {
            self.statements
                .push("replace this put into a graveyard: shuffle into library".to_string());
            return Ok(());
        }
        // CR 614.1a: "If a creature an opponent controls would die, exile
        // it instead." (Misery's Shadow, Stone of Erech).
        if let Some(c) = cached_regex(
            r"^If a (creature an opponent controls|creature you don't control|nontoken creature an opponent controls) would die, exile it instead\.$",
        )
        .unwrap()
        .captures(line)
        {
            self.statements
                .push(format!("replace {} dies: exile", cap(&c, 1)));
            return Ok(());
        }
        // "If this creature would die, put it on top of its owner's
        // library instead." (Gravebane Zombie), "exile it instead".
        if let Some(c) = cached_regex(
            r"^If this creature would die, (exile it|return it to its owner's hand|put it on top of its owner's library|put it on the bottom of its owner's library) instead\.$",
        )
        .unwrap()
        .captures(line)
        {
            let outcome = match cap(&c, 1) {
                "exile it" => "exile",
                "return it to its owner's hand" => "hand",
                "put it on top of its owner's library" => "library top",
                _ => "library bottom",
            };
            self.statements
                .push(format!("replace this dies: {outcome}"));
            return Ok(());
        }
        if line
            == "If one or more +1/+1 counters would be put on a creature you control, twice that many +1/+1 counters are put on that creature instead."
        {
            self.statements
                .push("replace +1/+1 counters put on creature you control: double".to_string());
            return Ok(());
        }
        // "If a red source you control would deal damage to an opponent or
        // a permanent an opponent controls, it deals that much damage plus
        // 2 instead." (Torbran).
        if let Some(c) = cached_regex(
            r"^If (a|another) (red |white |blue |black |green )?source you control would deal (noncombat )?damage to an opponent or a permanent an opponent controls, it deals (double that damage|that much damage plus (\d+)) instead\.$",
        )
        .unwrap()
        .captures(line)
        {
            let other = if cap(&c, 1) == "another" { "other " } else { "" };
            let color = cap(&c, 2);
            let kind = if c.get(3).is_some() { "noncombat " } else { "" };
            let outcome = match c.get(5) {
                Some(n) => format!("plus {}", n.as_str()),
                None => "double".to_string(),
            };
            self.statements.push(format!(
                "replace {kind}damage from {other}{color}permanent you control to opponent and permanent an opponent controls: {outcome}"
            ));
            return Ok(());
        }
        if let Some(c) = cached_regex(
            r"^Enchanted (?:creature|permanent) can't (attack or block|block|attack), and its activated abilities can't be activated( unless they're mana abilities)?\.$",
        )
        .unwrap()
        .captures(line)
        {
            let bars = match cap(&c, 1) {
                "attack or block" => "can't attack, can't block",
                "block" => "can't block",
                _ => "can't attack",
            };
            self.statements.push(format!(
                "static enchanted: has {bars}, can't activate{}",
                if c.get(2).is_some() { " except mana" } else { "" }
            ));
            return Ok(());
        }
        // "Each creature you control with a +1/+1 counter on it has
        // trample." (printed one at a time).
        if let Some(c) = cached_regex(
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
        if line == "Players play with their hands revealed." {
            self.statements.push("hands revealed players".to_string());
            return Ok(());
        }
        if line == "Play with the top card of your library revealed." {
            self.statements.push("top revealed".to_string());
            return Ok(());
        }
        if line == "Players play with the top card of their libraries revealed." {
            self.statements.push("top revealed players".to_string());
            return Ok(());
        }
        // CR 601.3: "You may play lands and cast creature spells from the
        // top of your library."
        if let Some(c) = cached_regex(
            r"^You may (play lands|cast (?:(\w+) )?spells|play lands and cast (?:(\w+) )?spells) from the top of your library\.$",
        )
        .unwrap()
        .captures(line)
        {
            let what = cap(&c, 1);
            let kind = c.get(2).or_else(|| c.get(3)).map(|m| m.as_str());
            let lands = what.starts_with("play lands");
            let spells = what.contains("spells");
            let mut s = "play from top".to_string();
            if lands {
                s.push_str(" lands");
            }
            if spells {
                s.push_str(if lands { " and spells" } else { " spells" });
                if let Some(k) = kind {
                    let k = match k {
                        "Ally" | "artifact" | "creature" | "instant" | "sorcery" | "enchantment"
                        | "Goblin" | "Elf" | "Bird" | "Angel" | "Spider" | "noncreature" => k,
                        _ => return Err(Skip::Sentence(line.to_string())),
                    };
                    s.push(' ');
                    s.push_str(k);
                }
            }
            self.statements.push(s);
            return Ok(());
        }
        // CR 118.9: "You may cast spells from your hand without paying
        // their mana costs." (Omniscience), "You may cast Dragon spells
        // without paying their mana costs."
        if let Some(c) = cached_regex(
            r"^You may cast (?:(\w+) )?spells( from your hand)? without paying their mana costs\.$",
        )
        .unwrap()
        .captures(line)
        {
            let mut s = "cast free".to_string();
            if c.get(2).is_some() {
                s.push_str(" from hand");
            }
            s.push_str(" spells");
            if let Some(k) = c.get(1) {
                let k = k.as_str();
                if k.chars().next().is_some_and(char::is_uppercase)
                    || matches!(
                        k,
                        "artifact"
                            | "creature"
                            | "instant"
                            | "sorcery"
                            | "enchantment"
                            | "noncreature"
                    )
                {
                    s.push(' ');
                    s.push_str(k);
                } else {
                    return Err(Skip::Sentence(line.to_string()));
                }
            }
            self.statements.push(s);
            return Ok(());
        }
        // CR 702.131: "Ascend" as an ability line.
        // CR 726.2: "If it's neither day nor night, it becomes day as this
        // creature enters."
        if line == "If it's neither day nor night, it becomes day as this creature enters."
            || line == "If it's neither day nor night, it becomes day as ~ enters."
        {
            self.statements
                .push("becomes day as this enters".to_string());
            return Ok(());
        }
        if line == "Ascend" {
            self.statements.push("ascend".to_string());
            return Ok(());
        }
        // CR 702.195: "Storied".
        if line == "Storied" {
            self.statements.push("storied".to_string());
            return Ok(());
        }
        // "Enchanted creature has \"{T}: ...\"", "Creatures you control
        // have \"...\"", "Equipped creature gets +1/+1 and has \"...\"".
        if let Some(c) = cached_regex(
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
            // 'Enchanted land has "Untap this land during each other
            // player's untap step."': the keyword granted (Urban
            // Burgeoning).
            let untaps = "untaps during each other player's untap step";
            if filter == "enchanted"
                && pump.is_empty()
                && (inner == format!("keywords {untaps}") || inner == format!("keyword {untaps}"))
            {
                self.statements
                    .push(format!("static enchanted: has {untaps}"));
                return Ok(());
            }
            self.statements
                .push(format!("static {filter}: {pump}has ability {{ {inner} }}"));
            return Ok(());
        }
        // "Elves you control have \"{T}: Add {G}{G}.\"", "Sliver creatures
        // you control have \"...\"", "Artifacts you control have \"...\"".
        if let Some(c) =
            cached_regex(r#"^([A-Z][a-z]+(?: creatures)?|Artifacts) you control have "(.+)"$"#)
                .unwrap()
                .captures(line)
        {
            let what = singular(cap(&c, 1));
            let what = if what == "Artifact" {
                "artifact".to_string()
            } else {
                what
            };
            let inner = self
                .quoted(cap(&c, 2))
                .ok_or_else(|| Skip::Sentence(line.to_string()))?;
            self.statements.push(format!(
                "static {what} you control: has ability {{ {inner} }}"
            ));
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
            // "Flying, ward {2}", "Menace, ward {2}, haste": ward with its
            // cost among plain keywords (CR 702.21).
            let ward = cached_regex(r"^(?:[Ww]ard \{\d+\}|[Ff]irebending \d+)$").unwrap();
            if parts.iter().all(|p| {
                keyword_list(p).is_some()
                    || p.to_ascii_lowercase().starts_with("protection from ")
                    || p.to_ascii_lowercase().starts_with("hexproof from ")
                    || ward.is_match(p)
            }) {
                for p in parts {
                    let p = capitalize(p);
                    self.line(&p)?;
                }
                return Ok(());
            }
        }
        // "Trample; rampage 2": an older keyword line, one keyword each.
        if line.contains("; ")
            && !line.contains(", ")
            && !line.ends_with('.')
            && line.chars().next().is_some_and(char::is_uppercase)
        {
            for p in line.split("; ") {
                self.line(&capitalize(p))?;
            }
            return Ok(());
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
        // "This creature can't block and can't be blocked." (Inkfathom
        // Infiltrator): two restrictions in one sentence.
        if line == "This creature can't block and can't be blocked." {
            self.statements
                .push("static this: has can't block, has can't be blocked".to_string());
            return Ok(());
        }
        // Blocking restrictions with a filter.
        if let Some(rest) = line
            .strip_prefix("This creature ")
            .and_then(|r| r.strip_suffix('.'))
        {
            let min =
                cached_regex(r"^can't be blocked except by (\w+) or more creatures$").unwrap();
            let rule = if rest == "can't be blocked by more than one creature" {
                Some("can't be blocked by more than one creature".to_string())
            } else if let Some(c) = min.captures(rest) {
                number(cap(&c, 1))
                    .map(|n| format!("can't be blocked except by {n} or more creatures"))
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
        if let Some(c) = cached_regex(r"^(Basic land|[A-Z][a-z]+)cycling ((?:\{[0-9XWUBRGC]+\})+)$")
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
        // CR 702.148a: the spell's text follows.
        if let Some(c) = cached_regex(r"^Cleave ((?:\{[0-9WUBRGCX]+\})+)$")
            .unwrap()
            .captures(line)
            && self.is_spell
        {
            self.cleave = Some((cap(&c, 1).to_string(), Vec::new()));
            return Ok(());
        }
        // CR 702.72a: "Champion a Goblin", "Champion a Goblin or Shaman".
        if let Some(c) = cached_regex(r"^Champion an? (creature|[A-Z][a-z]+(?: or [A-Z][a-z]+)?)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("champion {}", cap(&c, 1)));
            return Ok(());
        }
        // CR 702.155a.
        if line == "Read ahead" {
            self.statements.push("read ahead".to_string());
            return Ok(());
        }
        // CR 702.55a.
        if line == "Haunt" {
            self.statements.push("haunt".to_string());
            return Ok(());
        }
        if let Some(c) = cached_regex(r"^Cycling ((?:\{[0-9XWUBRGC]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("cycling {}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(c) = cached_regex(r"^Kicker ((?:\{[0-9XWUBRGC]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("kicker {}", cap(&c, 1)));
            return Ok(());
        }
        // CR 702.33a: "Kicker—Sacrifice a creature.", "Kicker—{2}{R}, Sacrifice a
        // land." (no {T}).
        if let Some(c) = line
            .strip_prefix("Kicker\u{2014}")
            .and_then(|r| r.strip_suffix('.'))
            .filter(|r| !r.contains("{T}"))
            .and_then(cost_text)
        {
            self.statements.push(format!("kicker {c}"));
            return Ok(());
        }
        if let Some(c) = cached_regex(r"^Flashback ((?:\{[0-9XWUBRGC]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("flashback {}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(c) = cached_regex(r"^Flashback—((?:\{[0-9XWUBRGC]+\})+), Pay (\d+) life\.$")
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
        if let Some(c) =
            cached_regex(r"^\{T\}: Add \{([WUBRG])\} or one mana of the chosen color\.$")
                .unwrap()
                .captures(line)
        {
            self.statements
                .push(format!("mana {{{}}} or chosen", cap(&c, 1)));
            return Ok(());
        }
        // "As this land enters, choose a color." (CR 614.12), alone or after
        // "This land enters tapped." on the same line.
        if let Some(c) = cached_regex(
            r"^(This (?:land|artifact|creature|permanent) enters tapped\. )?As (?:it|this (?:land|artifact|creature|enchantment|permanent|Aura|Vehicle|Equipment)) enters, choose a (color|creature type|basic land type|card name|nonland card name)(?: other than (white|blue|black|red|green))?\.$",
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
        // Chosen-name bans (CR 201.3).
        let ban = match line {
            "As this artifact enters, look at an opponent's hand, then choose any card name." => {
                Some("as this enters look at opponent's hand then choose card name")
            }
            "Spells with the chosen name can't be cast." => {
                Some("spells with chosen name can't be cast")
            }
            "Activated abilities of sources with the chosen name can't be activated unless they're mana abilities." => {
                Some(
                    "activated abilities of sources with chosen name can't be activated unless mana",
                )
            }
            "Activated abilities of sources with the chosen name can't be activated." => {
                Some("activated abilities of sources with chosen name can't be activated")
            }
            _ => None,
        };
        if let Some(b) = ban {
            self.statements.push(b.to_string());
            return Ok(());
        }
        // "As an additional cost to cast this spell, you may collect
        // evidence 6." (CR 601.2b), and "This spell costs {2} less to cast
        // if evidence was collected." after it.
        if let Some(c) = cached_regex(
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
        // "Equip abilities you activate that target this creature cost {2}
        // less to activate."
        if let Some(c) = cached_regex(
            r"^Equip abilities you activate that target (?:this creature|~) cost \{(\d+)\} less to activate\.$",
        )
        .unwrap()
        .captures(line)
        {
            self.statements.push(format!(
                "equip abilities targeting this cost {{{}}} less",
                cap(&c, 1)
            ));
            return Ok(());
        }
        // An alternative cost (CR 118.9): "You may pay {2}{U} rather than
        // pay this spell's mana cost.", "If it's not your turn, you may
        // exile a blue card from your hand rather than ...".
        if let Some(c) = cached_regex(
            r"^(?:If (.+?), )?[Yy]ou may (.+?) rather than pay this spell's mana cost\.$",
        )
        .unwrap()
        .captures(line)
        {
            let skip = || Skip::Sentence(line.to_string());
            let mut parts = Vec::new();
            let hand =
                cached_regex(r"^exile (a|an|two|three) ((?:[a-z]+ )*?)cards? from your hand$")
                    .unwrap();
            let sac = cached_regex(r"^sacrifice (a|an|two|three|four|five) ([A-Za-z ]+)$").unwrap();
            for part in cap(&c, 2).split(" and ") {
                let part = part
                    .strip_prefix("pay ")
                    .filter(|p| p.starts_with('{'))
                    .unwrap_or(part);
                // "exile a blue card from your hand" (a pitch spell).
                if let Some(h) = hand.captures(part) {
                    let n = match cap(&h, 1) {
                        "a" | "an" => String::new(),
                        w => format!("{} ", number(w).ok_or_else(skip)?),
                    };
                    let cards = if n.is_empty() { "card" } else { "cards" };
                    parts.push(format!("exile {n}{}{cards} from hand", cap(&h, 2)));
                    continue;
                }
                // "sacrifice a nontoken blue creature", "sacrifice three
                // artifacts".
                if let Some(s) = sac.captures(part)
                    && !cap(&s, 2).contains(" you ")
                    && !cap(&s, 2).contains(" with ")
                {
                    let what = cap(&s, 2);
                    parts.push(match cap(&s, 1) {
                        "a" | "an" => format!("sacrifice {what}"),
                        w => {
                            let (head, last) = what.rsplit_once(' ').unwrap_or(("", what));
                            let one = if head.is_empty() {
                                singular(last)
                            } else {
                                format!("{head} {}", singular(last))
                            };
                            format!("sacrifice {} {one}", number(w).ok_or_else(skip)?)
                        }
                    });
                    continue;
                }
                parts.push(cost_text(&capitalize(part)).ok_or_else(skip)?);
            }
            let mut cost = format!("alternative cost {}", parts.join(", "));
            if !cap(&c, 1).is_empty() {
                let cond = static_condition(cap(&c, 1)).ok_or_else(skip)?;
                let _ = write!(cost, ", if {cond}");
            }
            self.statements.push(cost);
            return Ok(());
        }
        if let Some(rest) = line.strip_prefix("As an additional cost to cast this spell, ") {
            let rest = rest.trim_end_matches('.');
            // CR 701.69a: "behold a Kithkin or pay {2}", "behold an Elf
            // and exile it", "you may behold a Dragon".
            if let Some(c) = cached_regex(
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
                "player" => "player",
                // "Enchant creature an opponent controls", "Enchant
                // creature or land", "Enchant artifact, creature, or
                // planeswalker": any filter the DSL parses and prints so.
                other
                    if other
                        .chars()
                        .all(|ch| ch.is_ascii_alphabetic() || ch == ' ' || ch == ',') =>
                {
                    other
                }
                _ => return Err(Skip::Sentence(line.to_string())),
            };
            self.statements.push(format!("enchant {spec}"));
            return Ok(());
        }
        if let Some(c) = cached_regex(r"^Equip ((?:\{[0-9WUBRGC]\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("equip {}", cap(&c, 1)));
            return Ok(());
        }
        // A flavor word before equip ("Perseus's Bow — Equip {6}", job
        // select Equipment).
        if let Some(c) = cached_regex(r#"^([A-Z][^"—]*) — Equip \{(\d+)\}$"#)
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("label \"{}\" equip {{{}}}", cap(&c, 1), cap(&c, 2)));
            return Ok(());
        }
        if let Some(c) = cached_regex(r"^\{T\}: Add ((?:\{[WUBRGC]\})(?: or \{[WUBRGC]\})*)\.$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("mana {}", cap(&c, 1)));
            return Ok(());
        }
        // "{T}: Add {C}. Spend this mana only to cast an instant or
        // sorcery spell." and the other restricted mana (CR 106.6).
        if let Some(c) = cached_regex(
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
        if let Some(c) = cached_regex(r"^\{T\}: Add ((?:\{[WUBRGC]\}, )+)or (\{[WUBRGC]\})\.$")
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
        if let Some(c) = cached_regex(
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
            cached_regex(r"^This creature can't (attack or block|attack|block) unless (.+)\.$")
                .unwrap()
                .captures(line)
            && let Some(cond) = static_condition(cap(&c, 2))
        {
            self.statements
                .push(format!("can't {} unless {cond}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(c) = cached_regex(
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
        if let Some(c) = cached_regex(
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
        if line == "Enchanted creature can't attack, block, or transform." {
            self.statements.push(
                "static enchanted: has can't attack, can't block, can't transform".to_string(),
            );
            return Ok(());
        }
        // The Zendikons: "Enchanted land is a 4/2 red Beast creature with
        // trample. It's still a land." (CR 205.1b).
        if let Some(c) = cached_regex(
            r"^Enchanted land is an? (\d+)/(\d+) (white|blue|black|red|green) ([A-Z][a-z]+) creature(?: with ([a-z ]+))?\. It's still a land\.$",
        )
        .unwrap()
        .captures(line)
        {
            let has = c
                .get(5)
                .map_or(String::new(), |k| format!(", has {}", k.as_str()));
            self.statements.push(format!(
                "static enchanted: base {}/{}, set color {}, add subtype {}, add type creature{has}",
                cap(&c, 1),
                cap(&c, 2),
                cap(&c, 3),
                cap(&c, 4)
            ));
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
        // CR 502.3: "You may choose not to untap this artifact during your
        // untap step."
        if cached_regex(
            r"^You may choose not to untap (?:this (?:creature|artifact|land|permanent)|~) during your untap step\.$",
        )
        .unwrap()
        .is_match(line)
        {
            if self.statements.is_empty() {
                self.keywords.push("may choose not to untap".to_string());
            } else {
                self.statements
                    .push("keyword may choose not to untap".to_string());
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
            "You can't lose the game and your opponents can't win the game." => {
                Some("you can't lose the game")
            }
            "You have shroud." => Some("you have shroud"),
            "Your opponents can't cast spells during your turn." => {
                Some("opponents can't cast spells during your turn")
            }
            "Players can't gain life." => Some("players can't gain life"),
            "You may play lands from your graveyard." => {
                Some("you may play lands from your graveyard")
            }
            "Damage can't be prevented." => Some("damage can't be prevented"),
            "Cards in graveyards can't be the targets of spells or abilities." => {
                Some("graveyard cards can't be targeted")
            }
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
        // "You may reveal this card from your opening hand. If you do, at
        // the beginning of the first upkeep, ..." (the Chancellors).
        if let Some(rest) =
            line.strip_prefix("You may reveal this card from your opening hand. If you do, ")
        {
            let rest = rest.trim_end_matches('.');
            let heads = [
                (
                    "at the beginning of the first upkeep, ",
                    "first upkeep",
                    false,
                ),
                (
                    "at the beginning of your first upkeep, ",
                    "your first upkeep",
                    false,
                ),
                (
                    "at the beginning of your first main phase of the game, ",
                    "your first main",
                    false,
                ),
            ];
            let found = heads
                .iter()
                .find_map(|(h, d, a)| rest.strip_prefix(h).map(|body| (body.to_string(), *d, *a)));
            // "scry 3 at the beginning of your first upkeep".
            let found = found.or_else(|| {
                rest.strip_suffix(" at the beginning of your first upkeep")
                    .map(|body| (body.to_string(), "your first upkeep after", true))
            });
            if let Some((body, when, _)) = found {
                let mut t = 0;
                let effects = self
                    .templates
                    .effects(&format!("{}.", capitalize(&body)), &mut t)?;
                if t == 0 {
                    self.statements
                        .push(format!("opening hand reveal {when}: {effects}"));
                    return Ok(());
                }
            }
            return Err(Skip::Sentence(line.to_string()));
        }
        if let Some(s) = player_static {
            self.statements.push(s.to_string());
            return Ok(());
        }
        // CR 601.3: "You may cast this card from your graveyard [as long
        // as ...] [by paying {2}{W} rather than paying its mana cost | by
        // discarding a card in addition to paying its other costs]."
        // CR 707.5: "You may have this creature enter as a copy of any
        // creature on the battlefield[, except it's an artifact in addition
        // to its other types]."
        if let Some(c) = cached_regex(
            r"^You may have this (?:creature|artifact|enchantment|land|permanent) enter as a copy of (?:any ([a-z]+(?: or [a-z]+)?|nonland permanent|[A-Z][a-z]+ creature) on the battlefield|an? ([a-z]+(?: or [a-z]+)?) you control)(?:, except (.+))?\.$",
        )
        .unwrap()
        .captures(line)
        {
            let what = if cap(&c, 1).is_empty() {
                format!("{} you control", cap(&c, 2))
            } else {
                cap(&c, 1).to_string()
            };
            let mut st = format!("enters as copy of {what}");
            let x = cap(&c, 3);
            if !x.is_empty() {
                let inner = if let Some(pt) = x
                    .strip_prefix("it's ")
                    .filter(|r| cached_regex(r"^\d+/\d+$").unwrap().is_match(r))
                {
                    format!("pt {pt}")
                } else if let Some(t) = x
                    .strip_prefix("it's an? ")
                    .and_then(|r| r.strip_suffix(" in addition to its other types"))
                {
                    let words: Vec<&str> = t.split(' ').collect();
                    if words.iter().all(|w| w.chars().next().is_some_and(char::is_uppercase)) {
                        format!("subtypes {t}")
                    } else if words.len() == 1 && card_type_word(words[0]) {
                        format!("types {}", capitalize(words[0]))
                    } else {
                        return Err(Skip::Sentence(line.to_string()));
                    }
                } else {
                    return Err(Skip::Sentence(line.to_string()));
                };
                let _ = write!(st, "\n  except {{\n    {inner}\n  }}");
            }
            self.statements.push(st);
            return Ok(());
        }
        // "If this card is in a graveyard, effects from spells named Flame
        // Burst count it as a card named Flame Burst."
        if let Some(c) = cached_regex(
            r"^If this card is in a graveyard, effects from spells named (.+) count it as a card named (.+)\.$",
        )
        .unwrap()
        .captures(line)
            && cap(&c, 1) == cap(&c, 2)
        {
            self.statements
                .push(format!("counts as named {:?} in graveyard", cap(&c, 1)));
            return Ok(());
        }
        if line == "Max speed \u{2014} You may cast this card from your graveyard." {
            self.statements
                .push("cast from graveyard at max speed".to_string());
            return Ok(());
        }
        if let Some(rest) = line
            .strip_prefix("You may cast this card from your graveyard")
            .and_then(|r| r.strip_suffix('.'))
        {
            let fail = || Skip::Sentence(line.to_string());
            let (cond, how) = match rest.find(" by ") {
                Some(i) => (&rest[..i], &rest[i + 4..]),
                None => (rest, ""),
            };
            let mut out = "cast from graveyard".to_string();
            if let Some(c) = cond.strip_prefix(" as long as ") {
                let _ = write!(out, " while {}", static_condition(c).ok_or_else(fail)?);
            } else if let Some(c) = cond.strip_prefix(" if ") {
                let _ = write!(out, " if {}", static_condition(c).ok_or_else(fail)?);
            } else if !cond.is_empty() {
                return Err(fail());
            }
            if let Some(m) = how
                .strip_prefix("paying ")
                .and_then(|r| r.strip_suffix(" rather than paying its mana cost"))
                .filter(|m| m.starts_with('{'))
            {
                let _ = write!(out, " for {m}");
            } else if let Some(parts) = how.strip_suffix(" in addition to paying its other costs") {
                let mut costs = Vec::new();
                for part in parts.split(" and ") {
                    costs.push(
                        if let Some(n) = part
                            .strip_prefix("paying ")
                            .and_then(|r| r.strip_suffix(" life"))
                            .filter(|n| n.bytes().all(|b| b.is_ascii_digit()))
                        {
                            format!("pay {n} life")
                        } else if part == "discarding a card" {
                            "discard 1 card".to_string()
                        } else if let Some(n) = part
                            .strip_prefix("discarding ")
                            .and_then(|r| r.strip_suffix(" cards"))
                            .and_then(number)
                        {
                            format!("discard {n} cards")
                        } else if let Some(what) = part
                            .strip_prefix("sacrificing an ")
                            .or_else(|| part.strip_prefix("sacrificing a "))
                        {
                            format!("sacrifice {what}")
                        } else {
                            return Err(fail());
                        },
                    );
                }
                let _ = write!(out, " with {}", costs.join(", "));
            } else if !how.is_empty() {
                return Err(fail());
            }
            self.statements.push(out);
            return Ok(());
        }
        if let Some(c) = cached_regex(
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
        if let Some(c) = cached_regex(
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
        if let Some(c) = cached_regex(
            r"^(Bushido|Soulshift|Afflict|Toxic|Crew|Bloodthirst|Modular|Backup|Saddle|Fabricate|Renown|Afterlife|Casualty|Teamwork|Ripple|Dredge|Firebending|Graft|Vanishing|Fading|Annihilator|Rampage|Amplify|Tribute|Devour|Devour artifact|Devour land) (\d+)$",
        )
        .unwrap()
        .captures(line)
        {
            self.statements
                .push(format!("{} {}", cap(&c, 1).to_lowercase(), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.189a: "Firebending X, where X is the number of ...".
        if let Some(c) = cached_regex(r"^Firebending X, where X is the number of (.+)\.$")
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
        if let Some(c) = cached_regex(
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
        // "You may cast this spell as though it had flash if you pay {2}
        // more to cast it."
        if let Some(c) = cached_regex(
            r"^You may cast this spell as though it had flash if you pay \{(\d+)\} more to cast it\.$",
        )
        .unwrap()
        .captures(line)
        {
            self.statements.push(format!("flash if pay {{{}}} more", cap(&c, 1)));
            return Ok(());
        }
        // CR 510.1a: "Each creature you control [with toughness greater
        // than its power] assigns combat damage equal to its toughness
        // rather than its power."
        if let Some(c) = cached_regex(
            r"^(Each creature|Each creature you control|This creature)( with toughness greater than its power)? assigns combat damage equal to its toughness rather than its power\.$",
        )
        .unwrap()
        .captures(line)
        {
            let who = match cap(&c, 1) {
                "Each creature" => "each creature",
                "This creature" => "this",
                _ => "each creature you control",
            };
            let with = if c.get(2).is_some() {
                " with toughness greater than power"
            } else {
                ""
            };
            self.statements.push(format!(
                "static {who}{with}: has assigns combat damage equal to toughness"
            ));
            return Ok(());
        }
        // CR 611.3a: "Enchanted creature gets +2/+2 as long as it's a
        // Vampire. Otherwise, it gets -2/-2."
        if let Some(c) = cached_regex(
            r"^Enchanted creature gets ([+-]\d+/[+-]\d+) as long as it's (?:an? )?([A-Za-z]+)\. Otherwise, it (.+)\.$",
        )
        .unwrap()
        .captures(line)
        {
            let made = self
                .quoted(&format!("Enchanted creature {}.", cap(&c, 3)))
                .ok_or_else(|| Skip::Sentence(line.to_string()))?;
            let rest = made
                .strip_prefix("static enchanted: ")
                .filter(|r| !r.contains(" as long as ") && !r.contains(" if "))
                .ok_or_else(|| Skip::Sentence(line.to_string()))?;
            self.statements.push(format!(
                "static enchanted: {} as long as attached is {} otherwise {rest}",
                cap(&c, 1),
                cap(&c, 2)
            ));
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
            cached_regex(r"^(Ward|Echo|Unearth|Buyback|Outlast|Morph|Megamorph|Disguise|Evoke|Madness|Mayhem|Foretell|Entwine|Ninjutsu|Scavenge|Dash|Warp|Embalm|Eternalize|Offspring|Bestow|Overload|Surge|Spectacle|Replicate|Encore|Plot|Sneak|Harmonize|Freerunning|Multikicker|Transmute|Disturb|Recover|Escalate|Prowl|Miracle|Emerge|Reconfigure) ((?:\{[0-9WUBRGCX/]+\})+)$")
                .unwrap()
                .captures(line)
        {
            self.statements
                .push(format!("{} {}", cap(&c, 1).to_lowercase(), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.176: "Impending 4—{1}{U}{U}".
        if let Some(c) = cached_regex(r"^Impending (\d+)—((?:\{[0-9WUBRGCX/]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("impending {} {}", cap(&c, 1), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.188: "Web-slinging {W}".
        if let Some(c) = cached_regex(r"^Web-slinging ((?:\{[0-9WUBRGCX/]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("web slinging {}", cap(&c, 1)));
            return Ok(());
        }
        // CR 702.174: "Gift a card", "Gift a tapped Fish".
        if let Some(c) = cached_regex(r"^Gift (a card|a Food|a tapped Fish|a Treasure)$")
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
            cached_regex(r"^Craft with (artifact|creature|[A-Z][a-z]+) ((?:\{[0-9WUBRGC]+\})+)$")
                .unwrap()
                .captures(line)
        {
            self.statements
                .push(format!("craft with {} {}", cap(&c, 1), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.21: "Ward—Pay 2 life."
        if let Some(c) = cached_regex(r"^Ward\u{2014}Pay (\d+) life\.?$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("ward pay {} life", cap(&c, 1)));
            return Ok(());
        }
        // "Ward—{2}, Pay 2 life.": both parts.
        if let Some(c) = cached_regex(r"^Ward\u{2014}(\{\d+\}), Pay (\d+) life\.$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("ward {}, pay {} life", cap(&c, 1), cap(&c, 2)));
            return Ok(());
        }
        // "Ward—Sacrifice a creature." (one permanent of a kind).
        if let Some(c) = cached_regex(r"^Ward\u{2014}Sacrifice an? ([a-zA-Z]+)\.$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("ward sacrifice {}", cap(&c, 1)));
            return Ok(());
        }
        if line == "Ward\u{2014}Discard a card." {
            self.statements.push("ward discard 1 card".to_string());
            return Ok(());
        }
        // CR 702.21a: other ward costs: "Ward—Sacrifice three permanents.",
        // "Ward—Pay life equal to this creature's power.", "Ward—Get five
        // poison counters.", "Ward—Collect evidence 4.", "Ward—Discard a
        // card or pay {2}."
        let n = |w: &str| match w {
            "two" => "2",
            "three" => "3",
            "four" => "4",
            "five" => "5",
            "six" => "6",
            _ => "1",
        };
        if let Some(c) = cached_regex(
            r"^Ward\u{2014}Sacrifice (two|three|four|five|six) (nonland )?permanents\.$",
        )
        .unwrap()
        .captures(line)
        {
            let nonland = if c.get(2).is_some() { "nonland " } else { "" };
            self.statements
                .push(format!("ward sacrifice {} {nonland}permanent", n(&c[1])));
            return Ok(());
        }
        if cached_regex(r"^Ward\u{2014}Pay life equal to (?:this creature|[A-Z][^']*)'s power\.$")
            .unwrap()
            .is_match(line)
        {
            self.statements
                .push("ward pay count equal power life".to_string());
            return Ok(());
        }
        if let Some(c) =
            cached_regex(r"^Ward\u{2014}(You g|G)et (two|three|four|five|six) poison counters\.$")
                .unwrap()
                .captures(line)
        {
            let you = if &c[1] == "You g" { "you " } else { "" };
            self.statements
                .push(format!("ward {you}get {} poison counters", n(&c[2])));
            return Ok(());
        }
        if let Some(c) = cached_regex(r"^Ward\u{2014}Collect evidence (\d+)\.$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("ward collect evidence {}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(c) = cached_regex(r"^Ward\u{2014}Discard a card or pay (\{\d+\})\.$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("ward discard 1 card or {}", cap(&c, 1)));
            return Ok(());
        }
        // CR 702.113: "Awaken 3—{5}{B}".
        if let Some(c) = cached_regex(r"^Awaken (\d+)\u{2014}((?:\{[0-9WUBRGCX/]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("awaken {} {}", cap(&c, 1), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.77: "Reinforce 1—{1}{R}".
        if let Some(c) = cached_regex(r"^Reinforce (\d+)\u{2014}((?:\{[0-9WUBRGCX/]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("reinforce {} {}", cap(&c, 1), cap(&c, 2)));
            return Ok(());
        }
        // CR 702.47: "Splice onto Arcane {1}{R}".
        if let Some(c) = cached_regex(r"^Splice onto Arcane ((?:\{[0-9WUBRGCX/]+\})+)$")
            .unwrap()
            .captures(line)
        {
            self.statements.push(format!("splice {}", cap(&c, 1)));
            return Ok(());
        }
        if let Some(c) = cached_regex(
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
        if let Some(c) = cached_regex(
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
        if let Some(c) = cached_regex(
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
        if let Some(c) = cached_regex(r"^Suspend (\d+)\u{2014}((?:\{[0-9WUBRGCX/]+\})+)$")
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
        if line == "Compleated" {
            self.statements.push("compleated".to_string());
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
            "Evolve"
                | "Unleash"
                | "Extort"
                | "Mentor"
                | "Riot"
                | "Retrace"
                | "Ingest"
                | "Increment"
                | "Provoke"
                | "Battle cry"
                | "Training"
                | "Melee"
                | "Dethrone"
                | "Myriad"
        ) {
            self.statements.push(line.to_lowercase());
            return Ok(());
        }
        if line == "For Mirrodin!" {
            self.statements.push("for mirrodin".to_string());
            return Ok(());
        }
        if let Some(c) = cached_regex(r"^Affinity for (\w+)$")
            .unwrap()
            .captures(line)
        {
            self.statements
                .push(format!("affinity for {}", singular(cap(&c, 1))));
            return Ok(());
        }
        if let Some(c) =
            cached_regex(r"^This spell costs \{(\d+)\} (less|more) to cast if (.+?)\.?$")
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
        if let Some(c) = cached_regex(
            r"^This spell costs \{(\d+)\} (less|more) to cast if it targets an? (.+)\.$",
        )
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
        if let Some(c) = cached_regex(r"^This spell costs \{(\d+)\} less to cast for each (.+)\.$")
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
        if let Some(c) = cached_regex(
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
        // "Activated abilities of artifacts you control cost {1} less to
        // activate.", "Power-up abilities of other creatures you control
        // cost {3} less to activate.", "Activated abilities cost {2} more
        // to activate unless they're mana abilities.", "Abilities you
        // activate that aren't mana abilities cost {2} less to activate."
        if let Some(c) = cached_regex(
            r"^(Activated a|Power-up a|Exhaust a|Boast a|Ninjutsu a|Equip a|Loyalty a|A)bilities( you activate| your opponents activate)?(?: of (.+?))?( that aren't mana abilities)? cost \{(\d+)\} (less|more) to activate( unless they're mana abilities)?\.( This effect can't reduce the mana in that cost to less than one mana\.)?$",
        )
        .unwrap()
        .captures(line)
        {
            let head = match cap(&c, 1) {
                "A" => "abilities".to_string(),
                h => format!("{}bilities", h.to_ascii_lowercase()),
            };
            let of = match c.get(3).map(|m| m.as_str()) {
                None => String::new(),
                Some("sources with the chosen name") => " of sources with chosen name".to_string(),
                Some(what) => {
                    let (what, whose) = if let Some(w) = what.strip_suffix(" you control") {
                        (w, " you control")
                    } else if let Some(w) = what.strip_suffix(" your opponents control") {
                        (w, " your opponents control")
                    } else {
                        (what, "")
                    };
                    let what = what.replace(" and ", " or ");
                    let subject = if let Some(rest) = what.strip_prefix("other ") {
                        format!("other {}", singular(rest))
                    } else {
                        singular(&what)
                    };
                    format!(" of {subject}{whose}")
                }
            };
            let nonmana = c.get(4).is_some() || c.get(7).is_some();
            self.statements.push(format!(
                "{head}{}{of} cost {{{}}} {}{}{}",
                cap(&c, 2),
                cap(&c, 5),
                cap(&c, 6),
                if nonmana { " unless mana" } else { "" },
                if c.get(8).is_some() { " min one" } else { "" }
            ));
            return Ok(());
        }
        // "Activated abilities of artifacts can't be activated.",
        // "... of creatures your opponents control can't be activated
        // unless they're mana abilities." (CR 602.5, 605.1a)
        if let Some(c) = cached_regex(
            r"^Activated abilities of (.+?) can't be activated( unless they're mana abilities)?\.$",
        )
        .unwrap()
        .captures(line)
            && !cap(&c, 1).starts_with("sources with")
        {
            let what = cap(&c, 1);
            let (what, whose) = if let Some(w) = what.strip_suffix(" you control") {
                (w, " you control")
            } else if let Some(w) = what.strip_suffix(" your opponents control") {
                (w, " your opponents control")
            } else {
                (what, "")
            };
            let what = what
                .replace(", and ", " or ")
                .replace(", ", " or ")
                .replace(" and ", " or ");
            self.statements.push(format!(
                "static {}{whose}: has can't activate{}",
                singular(&what),
                if c.get(2).is_some() {
                    " except mana"
                } else {
                    ""
                }
            ));
            return Ok(());
        }
        // "Untap this creature during each other player's untap step."
        // (CR 502.3); "Untap all artifacts you control during ...".
        if cached_regex(r"^Untap this (?:creature|artifact|land|permanent) during each other player's untap step\.$")
            .unwrap()
            .is_match(line)
        {
            self.statements
                .push("keywords untaps during each other player's untap step".to_string());
            return Ok(());
        }
        if let Some(c) =
            cached_regex(r"^Untap all (.+?) you control during each other player's untap step\.$")
                .unwrap()
                .captures(line)
        {
            let what = cap(&c, 1);
            let subject = if what.contains(" and/or ") {
                format!("andor {}", singular(&what.replace(" and/or ", " or ")))
            } else if what.contains(" and ") {
                format!("andlist {}", singular(&what.replace(" and ", " or ")))
            } else {
                singular(what)
            };
            self.statements.push(format!(
                "static {subject} you control: has untaps during each other player's untap step"
            ));
            return Ok(());
        }
        // "You may cast noncreature spells as though they had flash.",
        // "Creature spells you control can't be countered."
        let flash = cached_regex(r"^You may cast (?:(.+) )?spells as though they had flash\.$")
            .unwrap()
            .captures(line);
        let uncounterable = cached_regex(r"^(?:(.+) )?[Ss]pells you control can't be countered\.$")
            .unwrap()
            .captures(line);
        if let Some(c) = flash.as_ref().or(uncounterable.as_ref()) {
            let subject = lowercase_first(&cap(c, 1).replace(" spells and ", " or "));
            let subject = subject.replace(" and ", " or ");
            let subject = if subject.is_empty() {
                String::new()
            } else {
                format!(" {}", singular(&subject))
            };
            self.statements.push(if flash.is_some() {
                format!("spells{subject} you cast have flash")
            } else {
                format!("spells{subject} you control can't be countered")
            });
            return Ok(());
        }
        // "Instant and sorcery spells you cast [from your hand] have
        // storm." (convoke, improvise, delve, cascade, affinity for ...).
        if let Some(c) = cached_regex(
            r"^(?:(.+) )?[Ss]pells you cast( from your hand)? have (convoke|improvise|delve|cascade|storm|affinity for (artifacts|creatures|enchantments|Auras))\.$",
        )
        .unwrap()
        .captures(line)
        {
            // "Instant and sorcery spells" (types lowercase), "Sliver
            // spells" (a subtype keeps its capital).
            let subject = cap(&c, 1).replace(" and ", " or ");
            let typed = subject.split(' ').next().is_some_and(|w| {
                matches!(
                    w,
                    "Instant"
                        | "Sorcery"
                        | "Creature"
                        | "Artifact"
                        | "Enchantment"
                        | "Noncreature"
                        | "Nonartifact"
                        | "Permanent"
                        | "Multicolored"
                        | "Colorless"
                        | "Legendary"
                )
            });
            let subject = if typed {
                lowercase_first(&subject)
            } else {
                subject
            };
            let subject = if subject.is_empty() {
                String::new()
            } else {
                format!(" {}", singular(&subject))
            };
            let from = if cap(&c, 2).is_empty() {
                ""
            } else {
                " from hand"
            };
            let what = match cap(&c, 4) {
                "" => cap(&c, 3).to_string(),
                kind => format!("affinity for {}", singular(kind)),
            };
            self.statements
                .push(format!("spells{subject} you cast{from} have {what}"));
            return Ok(());
        }
        if let Some(c) = cached_regex(
            r"^As this land enters, you may pay (\d+) life\. If you don't, it enters tapped\.$",
        )
        .unwrap()
        .captures(line)
        {
            self.statements
                .push(format!("enters tapped unless pay {} life", cap(&c, 1)));
            return Ok(());
        }
        // "As this land enters, you may reveal a Mountain or Forest card
        // from your hand. If you don't, this land enters tapped." (the
        // snarls, the Lorwyn tribal lands); "... This land enters tapped
        // unless you revealed a Dragon card this way or you control a
        // Dragon."
        if let Some(c) = cached_regex(
            r"^As this land enters, you may reveal an? ([A-Z][a-z]+(?: or [A-Z][a-z]+)?) card from your hand\. (?:If you don't, this land enters tapped|This land enters tapped unless you revealed an? ([A-Z][a-z]+) card this way or you control an? ([A-Z][a-z]+))\.$",
        )
        .unwrap()
        .captures(line)
        {
            let mut s = format!("enters tapped unless reveal {} card", cap(&c, 1));
            if !cap(&c, 2).is_empty() {
                if cap(&c, 2) != cap(&c, 1) || cap(&c, 3) != cap(&c, 1) {
                    return Err(Skip::Sentence(line.to_string()));
                }
                let _ = write!(s, " or you control {}", cap(&c, 3));
            }
            self.statements.push(s);
            return Ok(());
        }
        if line == "This creature enters prepared." || line == "~ enters prepared." {
            self.statements.push("enters prepared".to_string());
            return Ok(());
        }
        // CR 604.3: "~'s power and toughness are each equal to the number
        // of creatures you control."
        if let Some(c) = cached_regex(
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
        if let Some(c) = cached_regex(&format!(
            r"^(?:This (?:creature|permanent|artifact|land|enchantment|Vehicle)|~) enters with (\S+) (\+1/\+1|-1/-1|{}) counters? on it(?: if (.+))?\.$",
            counter_names()
        ))
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
                    .or_else(|| (cap(&c, 3) == "you cast it").then(|| "you cast it".to_string()))
                    .ok_or_else(|| Skip::Sentence(line.to_string()))?;
                let s = if n == "1" { "" } else { "s" };
                self.statements
                    .push(format!("enters with {n} {} counter{s} if {cond}", cap(&c, 2)));
            }
            return Ok(());
        }
        // Multikicker (CR 702.33d): "This creature enters with a +1/+1
        // counter on it for each time it was kicked."
        if let Some(c) = cached_regex(
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
        if let Some(c) = cached_regex(
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
        if let Some(c) = cached_regex(
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
            let gets = cached_regex(r"^get ([+-]\d+/[+-]\d+)$").unwrap();
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
        // Statics on the attached or on a class of permanents ("All
        // creatures get -2/-2 until end of turn." is a spell's effect).
        if !line.contains(" until end of turn")
            && !line.contains(" this turn do so")
            && let Some(c) = cached_regex(
            r"^(Enchanted creature|Equipped creature|Creatures you control|Other creatures you control with flying|Other creatures you control|Creatures your opponents control|All creatures|White creatures|Blue creatures|Black creatures|Red creatures|Green creatures|Enchanted land|Enchanted permanent) (.+)\.$",
        )
        .unwrap()
        .captures(line)
        {
            let filter = match cap(&c, 1) {
                "Enchanted creature" | "Equipped creature" => "enchanted".to_string(),
                // "Enchanted permanent gets +1/+1 and has vigilance."
                // (Lightwheel Enhancements: "Enchant creature or Vehicle").
                "Enchanted permanent" => "enchanted permanent".to_string(),
                "Enchanted land" => return Err(Skip::Sentence(line.to_string())),
                "Creatures you control" => "creature you control".to_string(),
                "Other creatures you control" => "other creature you control".to_string(),
                "Other creatures you control with flying" => {
                    "other creature you control with flying".to_string()
                }
                "Creatures your opponents control" => "creature your opponents control".to_string(),
                "All creatures" => "creature".to_string(),
                colored => format!(
                    "{} creature",
                    colored.split(' ').next().unwrap_or("").to_ascii_lowercase()
                ),
            };
            let mut mods = Vec::new();
            let gets = cached_regex(r"^gets? ([+-]\d+/[+-]\d+)$").unwrap();
            // "gets +2/+2, has reach, and is a Bard in addition to its
            // other types" (job select Equipment): a list of three.
            let body = cap(&c, 2);
            let parts: Vec<&str> = if body.ends_with(" in addition to its other types") {
                cached_regex(r",? and |, ").unwrap().split(body).collect()
            } else {
                body.split(" and ").collect()
            };
            let subtype = cached_regex(r"^is an? ([A-Z][a-z]+) in addition to its other types$").unwrap();
            for part in parts {
                if let Some(c2) = gets.captures(part) {
                    mods.push(cap(&c2, 1).to_string());
                } else if let Some(c2) = subtype.captures(part) {
                    mods.push(format!("add subtype {}", cap(&c2, 1)));
                } else if let Some(rest) = part.strip_prefix("has ").or_else(|| part.strip_prefix("have ")) {
                    let ks = keyword_list(rest).ok_or_else(|| Skip::Sentence(line.to_string()))?;
                    mods.push(format!("has {ks}"));
                } else if let Some(k) = keyword_list(part)
                    && mods.last().is_some_and(|m: &String| m.starts_with("has "))
                {
                    // "has trample and lifelink": the list goes on.
                    let last = mods.pop().expect("checked");
                    mods.push(format!("{last}, {k}"));
                } else if let Some(k) = part.strip_prefix("loses ").and_then(keyword_list) {
                    // "gets +10/+10 and loses flying" (Colossus Hammer).
                    mods.push(format!("loses {k}"));
                } else if RESTRICTIONS.contains(&part) {
                    // "gets +1/+0 and can't be blocked", "gets +2/+2 and
                    // attacks each combat if able".
                    mods.push(format!("has {part}"));
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
        if let Some(c) = cached_regex(r"^(All )?([A-Z][a-z]+) creatures (.+)\.$")
            .unwrap()
            .captures(line)
        {
            let all = if cap(&c, 1).is_empty() { "" } else { "all " };
            let filter = format!("{all}{} creature", cap(&c, 2));
            let mut mods = Vec::new();
            let gets = cached_regex(r"^gets? ([+-]\d+/[+-]\d+)$").unwrap();
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
        if let Some(c) = cached_regex(r"^([+\u{2212}]?)(\d+|X): (.+)$")
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
            // CR 602.1: who may activate it, printed last.
            let activators =
                if let Some(r) = rest.strip_suffix(" Any player may activate this ability.") {
                    Some((r, "any player"))
                } else {
                    rest.strip_suffix(" Only your opponents may activate this ability.")
                        .map(|r| (r, "opponents only"))
                };
            let rest = match activators {
                Some((r, _)) => r,
                None => rest,
            };
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
            // "This ability costs {1} less to activate for each Shrine you
            // control." (or "... if <condition>."), before the
            // restrictions in the cost.
            let reduced = cached_regex(
                r"^(.*?) This ability costs (\{\d+\}) less to activate (for each|if) (.+)\.$",
            )
            .unwrap();
            let rest = if let Some(c) = reduced.captures(rest) {
                let by = cap(&c, 2);
                let what = if cap(&c, 3) == "if" {
                    static_condition(cap(&c, 4)).map(|s| format!("if {s}"))
                } else {
                    count_expr(cap(&c, 4)).map(|s| format!("each {s}"))
                };
                let what = what.ok_or_else(|| Skip::Sentence(line.to_string()))?;
                cost.insert_str(base_len, &format!(", reduced {by} {what}"));
                c.get(1).map_or("", |m| m.as_str())
            } else {
                rest
            };
            if let Some((_, who)) = activators {
                let _ = write!(cost, ", {who}");
            }
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
                // CR 712.14a.
                "Return this card from your graveyard to the battlefield transformed" => {
                    Some("return this to battlefield transformed")
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
        if let Some(c) = cached_regex(r"^Cumulative upkeep ((?:\{[0-9WUBRGC]+\})+)$")
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
            // Targets belong to the reflexive "When you do" (CR 603.12).
            let mut targets = 0;
            let effects = self.templates.effects(rest, &mut targets)?;
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
            } else if let Some(r) = rest
                .strip_prefix("if you cast it, ")
                .filter(|_| trigger == "when this enters")
            {
                // The source's own cast (CR 601.2a), from any zone.
                (" if you cast it", r)
            } else if let Some(r) = rest
                .strip_prefix("if it was a creature, ")
                .filter(|_| trigger == "when this dies")
            {
                // As it last existed (CR 603.10a): the Enduring cycle.
                (" if this is creature", r)
            } else if let Some(r) = rest.strip_prefix("if tribute wasn't paid, ") {
                // CR 702.104b.
                (" if tribute not paid", r)
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
            self.templates.about_this.set(
                (trigger.starts_with("when this ") || trigger.starts_with("whenever this "))
                    && !trigger.contains(" by "),
            );
            // The Zendikons: "When enchanted land dies, return that card to
            // its owner's hand." ("that card" is the land that died).
            let effects = if trigger.starts_with("when enchanted ")
                && trigger.ends_with(" dies")
                && rest.trim_end_matches('.') == "return that card to its owner's hand"
            {
                "return triggering card to hand".to_string()
            } else if trigger == "when this dies"
                && condition == " if this is creature"
                && rest.trim_end_matches('.')
                    == "return it to the battlefield under its owner's control. It's an enchantment"
            {
                // The Enduring cycle (CR 205.1a: no longer a creature).
                "return this to battlefield; it is now enchantment".to_string()
            } else {
                self.templates.effects(rest, &mut targets)?
            };
            self.templates.about_this.set(false);
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
        if self.is_spell
            && let Some((cost, cut)) = &mut self.cleave
        {
            // CR 702.148a: the text as printed, and with the words in
            // square brackets removed.
            let full = line.replace(['[', ']'], "");
            let short = cached_regex(r" ?\[[^\]]*\]")
                .unwrap()
                .replace_all(line, "")
                .into_owned();
            let short = short.trim_start();
            let mut t = 0;
            let cleaved = self.templates.effects(short, &mut t)?;
            let _ = cost;
            cut.push(cleaved);
            let mut targets = self.spell_targets;
            let effects = self.templates.effects(&full, &mut targets)?;
            self.spell_targets = targets;
            self.spell_at.get_or_insert(self.statements.len());
            self.spell_paragraphs.push(effects);
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
        few_modes(modes.len())?;
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
        few_modes(modes.len())?;
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
    fn modal_trigger(&mut self, trigger: &str, head: &str, modes: &[String]) -> Result<(), Skip> {
        few_modes(modes.len())?;
        let mut lines = vec![format!("triggered {trigger}: {head} {{")];
        for m in modes {
            lines.push(format!("  {}", self.mode(m)?));
        }
        lines.push("}".to_string());
        self.statements.push(lines.join("\n  "));
        Ok(())
    }
}

/// Five or more modes: written by hand, where a scenario checks each mode
/// (the lead dropped Sublime Epiphany and Casualties of War).
fn few_modes(n: usize) -> Result<(), Skip> {
    if n >= 5 {
        return Err(Skip::Sentence(format!(
            "{n} modes (five or more are left to a card agent, to test each)"
        )));
    }
    Ok(())
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
        "flip" => "flip",
        // A Room's doors are a split card's halves (CR 709.5).
        _ => "split",
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

/// A plural card type made singular, for a filter: "artifacts" →
/// "artifact".
fn singular_type(w: &str) -> &str {
    match w {
        "sorceries" => "sorcery",
        "artifacts" | "creatures" | "enchantments" | "planeswalkers" | "lands" | "instants"
        | "permanents" | "battles" => &w[..w.len() - 1],
        _ => w,
    }
}

/// Oracle's filter phrases a template copied into a statement, rewritten
/// in the DSL's words: "with a +1/+1 counter on it" (`with +1/+1
/// counter`), "named ~" (`named this`), "artifacts and/or enchantments"
/// (`andor artifact or enchantment`), "creature or planeswalker that's
/// black or red" (the colors on each type), "target spell with mana value
/// 2 or less" (the qualities before `spell`), "each of up to two target
/// creatures" (`up to 2 target creature`), "from a single graveyard",
/// "target player's graveyard", "all artifacts and enchantments"
/// (`andlist`), and a subtype the template lowercased ("Aura spells").
/// The render check decides whether the result is the card's text.
#[allow(clippy::too_many_lines)] // one rewrite per phrase
fn filter_phrases(st: &str, oracle: &str) -> String {
    const COLORS: &str = "white|blue|black|red|green";
    let re = |p: &str| cached_regex(p).expect("a filter phrase pattern");
    // "Counter target spell if it's red.": the line's one target as it
    // resolves.
    let its = re(r"^(.*\btarget [a-z].*) if it's (?:an? )?([a-z]+)$");
    let mut s = st
        .lines()
        .map(|l| {
            let l = bound_phrases(l);
            if l.matches("target ").count() == 1 {
                its.replace(&l, "$1 if target 1 is $2").into_owned()
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    s = re(r"with an? ([+-]\d+/[+-]\d+|[a-z]+) counter on (?:it|them)")
        .replace_all(&s, "with $1 counter")
        .into_owned();
    s = s
        .replace("with a counter on it", "with counter")
        .replace("with no counters on it", "with no counters")
        .replace("not named ~", "not named this")
        .replace("named ~", "named this");
    // "all legendary permanent cards" (CR 110.4a): the DSL names the
    // types a permanent card isn't.
    s = re(r"\b(legendary|snow) permanent card")
        .replace_all(&s, "$1 noninstant nonsorcery permanent card")
        .into_owned();
    // "each permanent of the chosen type you control" (Pack's Disdain):
    // the choice before the controller, `late`.
    s = re(r"\b(permanent|creature)s? of the chosen type you control")
        .replace_all(&s, "$1 you control late of chosen type")
        .into_owned();
    s = re(r"(\w+), (\w+), and/or (\w+)")
        .replace_all(&s, |c: &regex::Captures| {
            format!(
                "andor {} or {} or {}",
                singular_type(&c[1]),
                singular_type(&c[2]),
                singular_type(&c[3])
            )
        })
        .into_owned();
    s = re(r"(\w+) and/or (\w+)")
        .replace_all(&s, |c: &regex::Captures| {
            format!("andor {} or {}", singular_type(&c[1]), singular_type(&c[2]))
        })
        .into_owned();
    s = re(&format!(r"(\w+) or (\w+) that's ({COLORS}) or ({COLORS})"))
        .replace_all(&s, "$3 or $4 $1 or $3 or $4 $2")
        .into_owned();
    s = re(r"counter target ((?:\w+ )?)spell (with (?:mana value|power or toughness|power|toughness) (?:\d+|X)(?: or (?:less|greater))?)")
        .replace_all(&s, "counter target $1$2 spell")
        .into_owned();
    let num = |w: &str| match w {
        "one" => "1",
        "two" => "2",
        "three" => "3",
        "four" => "4",
        _ => "5",
    };
    s = re(r"each of (up to |one or )?(one|two|three|four|five) (other )?target")
        .replace_all(&s, |c: &regex::Captures| {
            let pre = match c.get(1).map(|m| m.as_str()) {
                Some("one or ") => "1 to ",
                Some(p) => p,
                None => "",
            };
            let other = if c.get(3).is_some() { "another " } else { "" };
            format!("{pre}{} {other}target", num(&c[2]))
        })
        .into_owned();
    s = re(r"up to (one|two|three|four|five) (other )?target")
        .replace_all(&s, |c: &regex::Captures| {
            let other = if c.get(2).is_some() { "another " } else { "" };
            format!("up to {} {other}target", num(&c[1]))
        })
        .into_owned();
    s = s
        .replace("from a single graveyard", "from a graveyard")
        .replace(
            "exile target player's graveyard",
            "exile graveyard of target player",
        )
        .replace(
            "exile each opponents' graveyards",
            "exile graveyard of each opponent",
        )
        .replace("exile each graveyards", "exile all graveyards")
        .replace("to each of two targets", "to 2 targets");
    // "Radiance — {T}: This creature deals 1 damage to target creature and
    // each other creature that shares a color with it." (Wojek
    // Embermage): "it" is the line's one target.
    s = s
        .lines()
        .map(|l| {
            if l.matches("target ").count() == 1 {
                re(r"\beach other ([a-z]+) that shares a color with it\b")
                    .replace_all(l, "each other $1 that shares color with target 1")
                    .into_owned()
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    // "Nonwhite creatures get -1/-1." (Crovax, Ascendant Hero): a color
    // the class lowercased.
    s = re(r"\bstatic Non(white|blue|black|red|green) ")
        .replace_all(&s, "static non$1 ")
        .into_owned();
    // "Return two target cards from your graveyard", "Exile five target
    // cards from an opponent's graveyard": the DSL's noun after a count is
    // singular.
    s = re(r"\b((?:\d+|X) (?:another )?target (?:[\w-]+ )*?)cards\b")
        .replace_all(&s, "${1}card")
        .into_owned();
    // "deals 4 damage to target player or planeswalker and each creature
    // that player or that planeswalker's controller controls" (Flame
    // Wave): the first target's player, either way.
    s = s
        .lines()
        .map(|l| {
            let first = l.find("target ");
            if first.is_some() && first == l.find("target player or planeswalker") {
                l.replace(
                    "that player or that planeswalker's controller controls",
                    "that player or planeswalker 1 controls",
                )
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    // "Put X +1/+1 counters on target creature, where X is that creature's
    // power": the line's one target, declared by the counters phrase.
    s = re(r"put X (\S+) counters on (target [^,;]+), where X is that creature's (power|toughness|mana value)$")
        .replace_all(&s, |c: &regex::Captures| {
            if c[0].matches("target").count() == 1 {
                format!(
                    "put count where {} of that target 1 {} counters on {}",
                    &c[3], &c[1], &c[2]
                )
            } else {
                c[0].to_string()
            }
        })
        .into_owned();
    // "Return all land cards from your graveyard to the battlefield
    // tapped": every one of them (`return all ... card`).
    s = re(r"return each (.+?) cards from your graveyard to battlefield")
        .replace_all(&s, |c: &regex::Captures| {
            let f = re(r"^(\w+) and (\w+)$")
                .replace(&c[1], "andlist $1 or $2")
                .into_owned();
            format!("return all {f} card from your graveyard to battlefield")
        })
        .into_owned();
    // "Those creatures gain trample": the group named before.
    s = s
        .replace("each those creature gains", "those creatures gain")
        .replace("each those creature gets", "those creatures get")
        .replace("untap each those creature", "untap those creatures")
        .replace("tap each those creature", "tap those creatures");
    // "and each other creature with the same name as that creature": the
    // line's one target.
    if s.matches("target ").count() == 1 {
        s = re(r"with the same name as that (creature|permanent)")
            .replace_all(&s, "with same name as target 1")
            .into_owned();
    }
    // "Return up to one target instant card and up to one target sorcery
    // card from your graveyard to your hand": each target names the
    // graveyard.
    s = re(r"\breturn ((?:up to \d+ )?target [^;,]+? card) and ((?:up to \d+ )?target [^;,]+? card) from your graveyard to hand")
        .replace_all(&s, |c: &regex::Captures| {
            // "a noncreature permanent card" (CR 110.4a), as `recipient`
            // writes it.
            let nonperm = re(r"\b(non[a-z]+) permanent card");
            let item = |i: usize| {
                nonperm
                    .replace_all(&c[i], "$1 noninstant nonsorcery permanent card")
                    .into_owned()
            };
            format!(
                "return {} from your graveyard and {} from your graveyard to hand",
                item(1),
                item(2)
            )
        })
        .into_owned();
    // "Exile target nonland permanent an opponent controls and all other
    // nonland permanents that player controls with the same name as that
    // permanent": "that player" is the target's controller (or the target
    // player), not a triggering player.
    s = s
        .lines()
        .map(|l| {
            let Some(c) = re(r"^(.*?\btarget ([a-z]+)\b[^;]*? and [^;]*?)\bthat player controls\b")
                .captures(l)
            else {
                return l.to_string();
            };
            if re(r"\btarget [a-zA-Z]").find_iter(l).count() != 1 {
                return l.to_string();
            }
            let who = if matches!(&c[2], "player" | "opponent") {
                "that player 1 controls"
            } else {
                "controller of target 1 controls"
            };
            format!("{}{who}{}", &c[1], &l[c[0].len()..])
        })
        .collect::<Vec<_>>()
        .join("\n");
    // "Put X +1/+1 counters on target creature, where X is your life
    // total"; "Counter target spell unless its controller pays {X}, where X
    // is this creature's power": the count after the amount.
    s = re(r"put X (\S+) counters on (target [^,;]+), where X is ([^;]+)$")
        .replace_all(&s, |c: &regex::Captures| match bound_amount(&c[3]) {
            Some(e) if !e.contains("target") => {
                format!("put count where {e} {} counters on {}", &c[1], &c[2])
            }
            _ => c[0].to_string(),
        })
        .into_owned();
    s = re(r"unless its controller pays \{X\}, where X is ([^;]+)$")
        .replace_all(&s, |c: &regex::Captures| match bound_amount(&c[1]) {
            Some(e) => format!("unless its controller pays {{X}} count where {e}"),
            None => c[0].to_string(),
        })
        .into_owned();
    // "Put a +1/+1 counter on target creature for each Elf you control".
    s = re(r"put 1 (\S+) counters on (target [^;]+?) for each ([^;]+)$")
        .replace_all(&s, |c: &regex::Captures| {
            let what = c[3].to_string();
            let e = bound_amount(&format!("the number of {what}"))
                .or_else(|| count_expr(&what))
                .or_else(|| {
                    cached_regex(r"^(\S+) counter on (?:this creature|~)$")
                        .expect("a counter pattern")
                        .captures(&what)
                        .map(|k| format!("{} counters on this", &k[1]))
                });
            match e {
                Some(e) => format!("put count each {e} {} counters on {}", &c[1], &c[2]),
                None => c[0].to_string(),
            }
        })
        .into_owned();
    // "for each tapped artifact, creature, and land you control": any of
    // them, the list printed with "and".
    s = re(r"count each (tapped |untapped )?(\w+), (\w+), and (\w+) you control")
        .replace_all(&s, "count each andlist $1$2, $3, or $4 you control")
        .into_owned();
    // "Destroy all artifacts and enchantments": either type, every one.
    s = re(r"(destroy|exile|return) each (\w+), (\w+), and (\w+)\b")
        .replace_all(&s, |c: &regex::Captures| {
            format!(
                "{} all andlist {} or {} or {}",
                &c[1],
                singular_type(&c[2]),
                singular_type(&c[3]),
                singular_type(&c[4])
            )
        })
        .into_owned();
    s = re(r"(destroy|exile|return) each (\w+) and (\w+)\b")
        .replace_all(&s, |c: &regex::Captures| {
            format!(
                "{} all andlist {} or {}",
                &c[1],
                singular_type(&c[2]),
                singular_type(&c[3])
            )
        })
        .into_owned();
    // "Aura spells you cast", "each Warrior creature you control": a
    // subtype the template lowercased, as Oracle capitalizes it.
    s = re(r"\b(spells|each|every|sacrifice) ([a-z]+)\b")
        .replace_all(&s, |c: &regex::Captures| {
            let w = &c[2];
            let mut cap = w.to_string();
            cap[..1].make_ascii_uppercase();
            let printed = re(&format!(r"\b{}s?\b", regex::escape(&cap)));
            if card_type_word(w) || !printed.is_match(oracle) {
                c[0].to_string()
            } else {
                format!("{} {cap}", &c[1])
            }
        })
        .into_owned();
    s
}

/// An amount a filter compares a characteristic with, in the DSL's words:
/// "this creature's power" (`power`), "the number of creatures you
/// control" (`creature you control`).
fn bound_amount(a: &str) -> Option<String> {
    let fixed = match a {
        "this creature's power" | "~'s power" => Some("power"),
        "this creature's toughness" | "~'s toughness" => Some("toughness"),
        "that creature's power" => Some("power of that target 1"),
        "the greatest mana value among permanents you control" => Some("greatest mana value"),
        "the greatest mana value among cards in your graveyard" => {
            Some("greatest mana value graveyard")
        }
        "the greatest power among creatures you control" => Some("greatest power"),
        "your life total" => Some("life"),
        "your devotion to blue" => Some("devotion blue"),
        "your devotion to black" => Some("devotion black"),
        "your devotion to red" => Some("devotion red"),
        "your devotion to green" => Some("devotion green"),
        "your devotion to white" => Some("devotion white"),
        _ => None,
    };
    // "the number of charge counters on this artifact".
    if let Some(c) = cached_regex(
        r"^the number of (\S+) counters on (?:this (?:artifact|creature|land|enchantment)|~)$",
    )
    .expect("a counters pattern")
    .captures(a)
    {
        return Some(format!("{} counters on this", &c[1]));
    }
    // The templates write "lands" singular before a count.
    let fixed = fixed
        .or((a == "the number of basic land types among land you control").then_some("domain"));
    if let Some(f) = fixed {
        return Some(f.to_string());
    }
    count_expr(a.strip_prefix("the number of ")?)
}

/// "with power less than or equal to the number of creatures you
/// control" (`with power at most count equal creature you control`),
/// "with power less than this creature's power" (`less than`), "with mana
/// value X or less, where X is ..." (`count where`), and "if its toughness
/// is less than or equal to ..." (`if target 1 is with toughness at most
/// ...`, the statement's one target): a characteristic compared with a
/// count.
fn bound_phrases(st: &str) -> String {
    let head = cached_regex(
        r"(with|if its) (power|toughness|mana value) (is )?(less than or equal to|less than|equal to|X or less, where X is) ",
    )
    .expect("a bound pattern");
    let mut s = st.to_string();
    let mut from = 0;
    while let Some(c) = head.captures_at(&s, from) {
        let m = c.get(0).expect("the match");
        let (lead, stat) = (c[1].to_string(), c[2].to_string());
        let how = c[4].to_string();
        let rest = &s[m.end()..];
        // The shortest run of words that is an amount ("the number of
        // Vampires you control", not "... you control; untap ...").
        let words: Vec<(usize, &str)> = rest
            .match_indices(' ')
            .map(|(i, _)| (i, &rest[..i]))
            .chain(std::iter::once((rest.len(), rest)))
            .collect();
        let found = words.iter().find_map(|(_, a)| {
            let a = a.trim_end_matches([';', ',']);
            bound_amount(a).map(|e| (a.len(), e))
        });
        let Some((end, expr)) = found else {
            from = m.end();
            continue;
        };
        let (cmp, style) = match how.as_str() {
            "less than" => ("less than", "equal"),
            "equal to" => ("equal to", "equal"),
            "X or less, where X is" => ("at most", "where"),
            _ => ("at most", "equal"),
        };
        // A "where X is" amount ends the sentence.
        if style == "where" && end != rest.len() {
            from = m.end();
            continue;
        }
        let dsl = if lead == "with" {
            format!("with {stat} {cmp} count {style} {expr}")
        } else {
            if st.matches("target ").count() != 1 || c.get(3).is_none() {
                from = m.end();
                continue;
            }
            format!("if target 1 is with {stat} {cmp} count {style} {expr}")
        };
        let mut tail = rest[end..].to_string();
        // "target creature with power ... an opponent controls": the
        // controller after the quality, as Oracle has it.
        if let Some(t) = tail.strip_prefix(" an opponent controls") {
            tail = format!(" an opponent controls late{t}");
        }
        let at = m.start();
        s = format!("{}{dsl}{tail}", &s[..at]);
        from = at + dsl.len();
    }
    // "target creature with power X or less": X of the spell.
    let s = cached_regex(r"with (power|toughness) X or less\b(,? where)?")
        .expect("a bound pattern")
        .replace_all(&s, |c: &regex::Captures| {
            if c.get(2).is_some() {
                c[0].to_string()
            } else {
                format!("with {} at most X", &c[1])
            }
        })
        .into_owned();
    // "counter target spell with mana value at most ...": the qualities
    // before `spell`.
    cached_regex(r"counter target spell (with (?:power|toughness|mana value) (?:at most|less than) count [a-z]+ .+)$")
        .expect("a bound pattern")
        .replace(&s, "counter target $1 spell")
        .into_owned()
}

/// A card type or another lowercase filter word, never a subtype.
fn card_type_word(w: &str) -> bool {
    matches!(
        w,
        "artifact"
            | "creature"
            | "enchantment"
            | "land"
            | "planeswalker"
            | "instant"
            | "sorcery"
            | "battle"
            | "permanent"
            | "card"
            | "cards"
            | "player"
            | "opponent"
            | "other"
            | "single"
            | "those"
            | "these"
            | "that"
            | "this"
            | "the"
            | "of"
    )
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
    const VERBS: [&str; 11] = [
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
        // "gain control of all Dragons".
        "of",
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
                && !ability.starts_with("triggered whenever this"))
            // "Whenever this creature attacks, put a +1/+1 counter on it":
            // the triggering object is the source.
            || ability.starts_with("triggered whenever this attacks")
            || ability.starts_with("triggered when this enters");
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
            // "if you do, each creature you control gains flying"
            // (Waterspout Weavers): after a condition's comma too.
            let after_clause = prev.ends_with(':')
                || prev.ends_with(';')
                || (prev.ends_with(',') && w == &"each")
                || VERBS.contains(&prev);
            let mut alts: Vec<String> = Vec::new();
            match *w {
                "each" if after_clause && next != "counters" => {
                    alts.push("all".to_string());
                    // "each creature gets -X/-X" (The Meathook Massacre):
                    // one at a time, wording only.
                    if next != "single" {
                        alts.push("each single".to_string());
                    }
                }
                // "put two +1/+1 counters on each creature you control
                // instead": the group named again, not "those creatures".
                "each" if prev == "on" && next != "single" => alts.push("each single".to_string()),
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
                    // "create two tokens that are copies of it" (Skitterbeam
                    // Battalion): the copy names nothing before "it".
                    let copy_of = prev == "of"
                        && j >= 2
                        && matches!(words[j - 2], "copy" | "copies")
                        && words[start..j].first() == Some(&"create");
                    let named = [
                        "create", "target", "that", "return", "exile", "search", "reveal", "dig",
                        "put", "attach", "copy", "mill", "discard",
                    ]
                    .iter()
                    .any(|w| before.split(' ').any(|b| b == *w))
                        && !(copy_of && !before.contains("target") && !before.contains(';'));
                    if it_ok && !named {
                        alts.push("it".to_string());
                    }
                }
                "this" if next == "creature" => alts.push(String::new()),
                // "{1}{W}, Discard this card: It deals 4 damage ...": the
                // source, named "it" where `it` is the source.
                "deal" if it_ok && prev.ends_with(':') => alts.push("it deals".to_string()),
                // "Whenever this creature and at least two other creatures
                // attack, this creature deals 3 damage ..." (Firemane
                // Avenger): the source named in full.
                "deal" if prev.ends_with(':') || prev.ends_with(';') => {
                    alts.push("this creature deal".to_string());
                }
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
    let he = cached_regex(r"\b(?:[Hh]e|[Hh]im|[Hh]is)\b").unwrap();
    let she = cached_regex(r"\b(?:[Ss]he|[Hh]er)\b").unwrap();
    let pronoun = match (he.is_match(oracle), she.is_match(oracle)) {
        (true, false) => "he",
        (false, true) => "she",
        _ => return (oracle.to_string(), None),
    };
    let word = cached_regex(r"\b([Hh]e|[Hh]im|[Hh]is|[Ss]he|[Hh]er)\b( ?)(\w*)").unwrap();
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
    // A battle (CR 310) is left to a card agent, to test its defense and
    // protector.
    if s.type_line.as_deref().unwrap_or("").contains("Battle") {
        return Err(Skip::Type(
            "Battle (left to a card agent, to test)".to_string(),
        ));
    }
    if matches!(
        s.layout.as_str(),
        "adventure" | "transform" | "split" | "prepare" | "modal_dfc" | "flip"
    ) && s.card_faces.len() == 2
    {
        return author_faces(s, corpus);
    }
    if s.name.contains(" // ")
        || !matches!(
            s.layout.as_str(),
            "normal" | "saga" | "class" | "case" | "leveler" | "prototype" | "meld"
        )
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
        // Typecycling of any type (CR 702.29e): "Wizardcycling {2}".
        let typecycling = k.ends_with("cycling") && k.len() > "cycling".len();
        if !KEYWORDS.contains(&k.as_str())
            && !TEMPLATED_TAGS.contains(&k.as_str())
            && !typecycling
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
    let symbol = cached_regex(r"\{([^}]*)\}").unwrap();
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
        let word = cached_regex(&format!(r"\b{}\b", regex::escape(short))).unwrap();
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
        let word = cached_regex(&format!(r"\b{}\b('s )?", regex::escape(first))).unwrap();
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
    // "When Enduring Courage dies, ...": a nonlegendary permanent whose
    // text calls it by its full name (the Enduring cycle) gets that name
    // as its `short_name`, so `this` prints it.
    if short.is_none()
        && !type_line.contains("Legendary")
        && oracle.contains("When ~ dies")
        && !oracle.contains("named ~")
    {
        short = Some(s.name.clone());
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
        cleave: None,
    };
    // CR 711.2: a leveler's "LEVEL 1-4" and the P/T box under it, one line.
    let leveler = cached_regex(r"(?m)^(LEVEL \d+(?:-\d+|\+))\n(\d+/\d+)$").unwrap();
    let oracle = leveler.replace_all(&oracle, "$1 $2").into_owned();
    let lines: Vec<&str> = oracle.lines().collect();
    let mut i = 0;
    // "Choose one. If ..., choose both instead." (CR 700.2).
    let choose_if = cached_regex(
        r"^Choose one\. If (.+), (you may )?choose (both|one or more|any number) instead\.$",
    )
    .unwrap();
    let strive = cached_regex(
        r"^Strive — This spell costs (\{[^ ]+\}) more to cast for each target beyond the first\.$",
    )
    .unwrap();
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
        // Strive: "This spell costs {2}{B} more to cast for each target
        // beyond the first." (the ability word's cost, `strive {2}{B}`).
        if let Some(c) = strive.captures(line) {
            b.statements.push(format!("strive {}", cap(&c, 1)));
            i += 1;
            continue;
        }
        // "Choose one. If this spell was cast using teamwork, choose both
        // instead." (CR 700.2), with an ability word before it.
        let (cond_label, cond_line) = match line.split_once(" — ") {
            Some((word, h)) if ABILITY_WORDS.contains(&word) => (Some(word), h),
            _ => (None, line),
        };
        if let Some(c) = choose_if.captures(cond_line) {
            let text = cap(&c, 1);
            let cond = static_condition(text)
                .or_else(|| {
                    let n = cached_regex(
                        r"^there are (\w+) or more card types among cards in your graveyard$",
                    )
                    .unwrap()
                    .captures(text)?;
                    Some(format!(
                        "{} card types in your graveyard",
                        number(cap(&n, 1))?
                    ))
                })
                .ok_or_else(|| Skip::Sentence(line.to_string()))?;
            let may = if c.get(2).is_some() { "may " } else { "" };
            let more = match cap(&c, 3) {
                "one or more" => "more",
                m => m,
            };
            let head = format!("choose one or {more} if {may}{cond}");
            let mut modes = Vec::new();
            i += 1;
            while i < lines.len() && lines[i].starts_with("• ") {
                modes.push(lines[i][3..].to_string());
                i += 1;
            }
            b.modal(&modes, &head)?;
            if let Some(l) = cond_label
                && let Some(st) = b.statements.last_mut()
            {
                *st = format!("label \"{l}\" {st}");
            }
            continue;
        }
        let head = match line {
            "Choose one —" => Some("choose one"),
            "Choose one or both —" => Some("choose one or both"),
            "Choose two —" => Some("choose two"),
            "Choose one or more —" => Some("choose one or more"),
            "Choose up to one —" => Some("choose up to 1"),
            "Choose up to two —" => Some("choose up to 2"),
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
        // Other modal heads (CR 700.2): "choose one or more —", "choose up
        // to one —", "choose one that hasn't been chosen —".
        let trigger_head = [
            (", choose one —", "choose one"),
            (", choose one or both —", "choose one or both"),
            (", choose two —", "choose two"),
            (", choose one or more —", "choose one or more"),
            (", choose up to one —", "choose up to 1"),
            (", choose up to two —", "choose up to 2"),
            (
                ", choose one that hasn't been chosen —",
                "choose one unchosen",
            ),
            (
                ", choose one that hasn't been chosen this turn —",
                "choose one unchosen this turn",
            ),
        ]
        .iter()
        .find_map(|(suffix, dsl)| line.strip_suffix(suffix).map(|h| (h, *dsl)));
        if let Some((head, dsl_head)) = trigger_head {
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
            b.modal_trigger(&trigger, dsl_head, &modes)?;
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
    // A copy effect's exceptions (`token_copy`): the card's one `except`
    // block.
    let mut except: Option<String> = None;
    for st in b.statements.iter_mut().chain(b.spell_paragraphs.iter_mut()) {
        while let Some(i) = st.find(EXCEPT_MARK) {
            let Some(len) = st[i + 1..].find(EXCEPT_MARK) else {
                break;
            };
            let lines = st[i + 1..i + 1 + len].to_string();
            st.replace_range(i..i + 2 + len, "");
            if except.as_ref().is_some_and(|x| *x != lines) {
                return Err(Skip::Sentence(
                    "two copy effects with different exceptions".to_string(),
                ));
            }
            except = Some(lines);
        }
    }
    // Oracle's filter phrases the templates copied, in the DSL's words.
    for st in b.statements.iter_mut().chain(b.spell_paragraphs.iter_mut()) {
        *st = filter_phrases(st, &oracle);
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
    // CR 702.148a: the cleaved spell, before the spell.
    if let Some((cost, cut)) = &b.cleave {
        match cut.as_slice() {
            [one] => {
                let _ = writeln!(text, "  cleave {cost}: {one}");
            }
            many => {
                let _ = writeln!(text, "  cleave {cost} {{");
                for p in many {
                    let _ = writeln!(text, "    {p}");
                }
                text.push_str("  }\n");
            }
        }
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
    if let Some(x) = &except {
        text.push_str("  except {\n");
        for line in x.split(EXCEPT_LINE) {
            let _ = writeln!(text, "    {line}");
        }
        text.push_str("  }\n");
    }
    text.push_str("}\n");
    // CR 712.4: a meld card needs its melded permanent, the back face both
    // cards share, in the corpus.
    let meld = cached_regex(r#"meld with "[^"]+" into "([^"]+)""#).unwrap();
    for c in meld.captures_iter(&text) {
        if !corpus.iter().any(|t| t.name == c[1]) {
            return Err(Skip::Sentence(format!(
                "meld result {} is not in the corpus",
                &c[1]
            )));
        }
    }
    // Tokens it creates that the corpus lacks are generated from their key.
    let created = cached_regex(r#"create \S+ "([^"]+)""#).unwrap();
    let mut tokens = Vec::new();
    let mut token_cards = Vec::new();
    for c in created.captures_iter(&text) {
        let key = &c[1];
        if corpus.iter().any(|t| t.name == key) || tokens.iter().any(|(k, _)| k == key) {
            continue;
        }
        let block = token_block(key)
            .or_else(|| legendary_token_block(key))
            .ok_or_else(|| Skip::Sentence(format!("unknown token {key}")))?;
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
    let mut rendered = render_of(&text).inspect_err(|_| {
        // `MTG_AUTHOR_SHOW=1`: print the draft that failed to parse.
        if std::env::var_os("MTG_AUTHOR_SHOW").is_some() {
            eprintln!("{text}");
        }
    })?;
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
