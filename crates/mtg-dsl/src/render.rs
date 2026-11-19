//! Oracle-text printer: renders a card's syntax tree back to the English
//! Scryfall prints, for the round-trip check (design.md §6.3) and for the
//! text the engine shows players. Templates follow current Oracle wording
//! ("this creature", "enters", "any target").

use std::collections::HashSet;
use std::fmt::Write as _;

use mtg_core::card::{
    CardType, Condition, CounterKind, Keyword, Num, PtValue, Relation, Replacement as Outcome,
    RuleKind, Supertype, TargetSpec, Who,
};
use mtg_core::mana::{Color, Mana};
use mtg_core::turn::Step;

use crate::ast::{
    Ability, Affects, AttachedRef, Card, CdaCount, ChoiceKind, Cost, CostItem, DamageWording,
    Effect, Filter, Join, Kind, Mod, OfChosen, Recipient, Replacement, Replaces, Restrict, SacCost,
    SameRef, Sentence, StaticCond, Target, Trigger, TriggerObj,
};

/// "+3/+3", "-3/-0": a zero part takes the other part's sign, as Oracle
/// prints it.
pub fn pt_mod(power: i32, toughness: i32) -> String {
    let sign = |n: i32, other: i32| {
        if n > 0 || (n == 0 && other >= 0) {
            '+'
        } else {
            '-'
        }
    };
    format!(
        "{}{}/{}{}",
        sign(power, toughness),
        power.abs(),
        sign(toughness, power),
        toughness.abs()
    )
}

/// Finds other definitions (tokens, emblems) by name.
pub trait Lookup {
    fn get(&self, name: &str) -> Option<&Card>;
}

impl Lookup for () {
    fn get(&self, _name: &str) -> Option<&Card> {
        None
    }
}

#[allow(clippy::implicit_hasher)]
impl Lookup for std::collections::HashMap<&str, &Card> {
    fn get(&self, name: &str) -> Option<&Card> {
        std::collections::HashMap::get(self, name).copied()
    }
}

#[allow(clippy::implicit_hasher)]
impl Lookup for std::collections::HashMap<String, Card> {
    fn get(&self, name: &str) -> Option<&Card> {
        std::collections::HashMap::get(self, name)
    }
}

pub(crate) const NUMBER_WORDS: [&str; 20] = [
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

/// A count in words ("a", "two", "X").
fn count_words(n: Num, article: &str) -> String {
    match n {
        // "Mill that many cards" (a counted noun); "that much life".
        Num::ThatMuch => "that many".to_string(),
        Num::X | Num::Count(_) => "X".to_string(),
        Num::N(1) => article.to_string(),
        Num::N(n) => NUMBER_WORDS
            .get(n as usize - 1)
            .map_or_else(|| n.to_string(), |w| (*w).to_string()),
    }
}

fn digits(n: Num) -> String {
    match n {
        Num::ThatMuch => "that much".to_string(),
        Num::X | Num::Count(_) => "X".to_string(),
        Num::N(n) => n.to_string(),
    }
}

fn plural(n: Num, word: &str) -> String {
    if n == Num::N(1) {
        word.to_string()
    } else {
        pluralize(word)
    }
}

fn pluralize(word: &str) -> String {
    // Creature types with irregular or unchanged plurals.
    match word {
        "Plains" | "Equipment" | "Kavu" | "Aurochs" | "Sheep" | "Fish" | "Moonfolk" | "Kithkin"
        | "Merfolk" | "Samurai" | "Kor" | "Zubera" | "Vedalken" | "Aetherborn" | "Eldrazi"
        | "Treefolk" => {
            return word.to_string();
        }
        "Elf" => return "Elves".to_string(),
        "Dwarf" => return "Dwarves".to_string(),
        "Wolf" => return "Wolves".to_string(),
        "Werewolf" => return "Werewolves".to_string(),
        "Sphinx" => return "Sphinxes".to_string(),
        "Fox" => return "Foxes".to_string(),
        "Ox" => return "Oxen".to_string(),
        "Mouse" => return "Mice".to_string(),
        "Homunculus" => return "Homunculi".to_string(),
        _ => {}
    }
    if word.ends_with('y') && !word.ends_with("ey") {
        format!("{}ies", &word[..word.len() - 1])
    } else {
        format!("{word}s")
    }
}

/// The type a typecycling ability searches for, as its keyword prints
/// it ("Plains", "Basic land", "Land"), if `effects` are one search.
fn typecycling_word(effects: &[Sentence]) -> Option<String> {
    let [s] = effects else { return None };
    let [Effect::Search { filter, .. }] = s.effects.as_slice() else {
        return None;
    };
    if let [sub] = filter.subtypes.as_slice() {
        return Some(sub.clone());
    }
    if filter
        .supertypes
        .contains(&mtg_core::card::Supertype::Basic)
    {
        return Some("Basic land".to_string());
    }
    Some("Land".to_string())
}

/// "{B}{B}", or `any` for one mana of any color.
pub fn mana_symbols(m: &[Mana], any: &str) -> String {
    if m == [Mana::Any] {
        return any.to_string();
    }
    m.iter()
        .map(|m| match m {
            Mana::Colorless => "{C}".to_string(),
            Mana::Colored(c) => format!("{{{}}}", c.letter()),
            Mana::Any | Mana::Chosen => "{?}".to_string(),
        })
        .collect()
}

pub fn keyword_name(k: Keyword) -> &'static str {
    match k {
        Keyword::Deathtouch => "deathtouch",
        Keyword::Defender => "defender",
        Keyword::DoubleStrike => "double strike",
        Keyword::FirstStrike => "first strike",
        Keyword::Flash => "flash",
        Keyword::Flying => "flying",
        Keyword::Haste => "haste",
        Keyword::Hexproof => "hexproof",
        Keyword::Indestructible => "indestructible",
        Keyword::Lifelink => "lifelink",
        Keyword::Menace => "menace",
        Keyword::Prowess => "prowess",
        Keyword::Reach => "reach",
        Keyword::Trample => "trample",
        Keyword::Vigilance => "vigilance",
        Keyword::CantAttack => "can't attack",
        Keyword::CantBlock => "can't block",
        Keyword::MustAttack => "attacks each combat if able",
        Keyword::Unblockable => "can't be blocked",
        Keyword::Plainswalk => "plainswalk",
        Keyword::Islandwalk => "islandwalk",
        Keyword::Swampwalk => "swampwalk",
        Keyword::Mountainwalk => "mountainwalk",
        Keyword::Forestwalk => "forestwalk",
        // Written as restrictions; Oracle's sentence is `activation_clause`.
        Keyword::CantActivate => "can't activate",
        Keyword::CantActivateNonMana => "can't activate except mana",
        k => k.name(),
    }
}

/// Whether the keyword bars activated abilities (CR 602.5).
fn is_activation_bar(k: Keyword) -> bool {
    matches!(k, Keyword::CantActivate | Keyword::CantActivateNonMana)
}

/// "its activated abilities can't be activated[ unless they're mana
/// abilities]": the last of the restrictions in `ks` that bars activation.
fn activation_clause(ks: &[Keyword]) -> Option<&'static str> {
    ks.iter()
        .rev()
        .find(|k| is_activation_bar(**k))
        .map(|k| k.name())
}

/// Restrictions the engine keeps with the keywords but Oracle prints as
/// sentences ("This creature can't block.").
pub fn is_restriction(k: Keyword) -> bool {
    matches!(
        k,
        Keyword::CantAttack
            | Keyword::CantBlock
            | Keyword::MustAttack
            | Keyword::Unblockable
            | Keyword::CantAttackAlone
            | Keyword::CantBlockAlone
            | Keyword::DoesntUntap
            | Keyword::CantBecomeUntapped
            | Keyword::MustBlock
            | Keyword::MustBeBlocked
            | Keyword::BlockAdditional
            | Keyword::BlockAny
            | Keyword::Lure
            | Keyword::AttackDespiteDefender
            | Keyword::CantActivate
            | Keyword::CantActivateNonMana
    )
}

/// Blocking requirements and allowances (CR 509.1a, 509.1c), whose
/// one-turn form puts "this turn" inside the phrase.
fn is_block_rule(k: Keyword) -> bool {
    matches!(
        k,
        Keyword::MustAttack
            | Keyword::MustBlock
            | Keyword::MustBeBlocked
            | Keyword::BlockAdditional
            | Keyword::BlockAny
            | Keyword::AttackDespiteDefender
    )
}

/// A blocking requirement or allowance for this turn only ("blocks this
/// turn if able").
fn block_rule_this_turn(k: Keyword) -> &'static str {
    match k {
        Keyword::MustAttack => "attacks this turn if able",
        Keyword::MustBlock => "blocks this turn if able",
        Keyword::MustBeBlocked => "must be blocked this turn if able",
        Keyword::BlockAdditional => "can block an additional creature this turn",
        Keyword::BlockAny => "can block any number of creatures this turn",
        Keyword::AttackDespiteDefender => "can attack this turn as though it didn't have defender",
        k => keyword_name(k),
    }
}

/// "All creatures able to block this creature do so." (a lure).
fn lure_sentence(what: &str, this_turn: bool) -> String {
    let turn = if this_turn { " this turn" } else { "" };
    format!("all creatures able to block {what}{turn} do so")
}

/// "can't attack or block", "can't block", ...
fn restriction_phrase(ks: &[Keyword]) -> String {
    let mut parts = Vec::new();
    if ks.contains(&Keyword::CantAttack) && ks.contains(&Keyword::CantBlock) {
        parts.push("can't attack or block".to_string());
    } else {
        for k in ks {
            if matches!(k, Keyword::CantAttack | Keyword::CantBlock) {
                parts.push(keyword_name(*k).to_string());
            }
        }
    }
    if ks.contains(&Keyword::CantAttackAlone) && ks.contains(&Keyword::CantBlockAlone) {
        parts.push("can't attack or block alone".to_string());
    } else {
        for k in ks {
            if matches!(k, Keyword::CantAttackAlone | Keyword::CantBlockAlone) {
                parts.push(keyword_name(*k).to_string());
            }
        }
    }
    for k in ks {
        if matches!(
            k,
            Keyword::MustAttack
                | Keyword::Unblockable
                | Keyword::DoesntUntap
                | Keyword::CantBecomeUntapped
        ) || is_block_rule(*k)
        {
            parts.push(keyword_name(*k).to_string());
        }
    }
    list(&parts)
}

/// A keyword as a line of text: the name, or a restriction as a sentence
/// about the card.
fn keyword_line(card: &Card, k: Keyword) -> String {
    if is_activation_bar(k) {
        return format!("{}'s {}.", capitalize(&this_ref(card)), &k.name()[4..]);
    }
    if k == Keyword::DoesntUntap {
        return format!(
            "{} doesn't untap during your untap step.",
            capitalize(&this_ref(card))
        );
    }
    if k == Keyword::Lure {
        let what = this_ref(card);
        return format!("{}.", capitalize(&lure_sentence(&what, false)));
    }
    if is_restriction(k) {
        format!("{} {}.", capitalize(&this_ref(card)), keyword_name(k))
    } else {
        capitalize(keyword_name(k))
    }
}

pub fn color_name(c: Color) -> &'static str {
    match c {
        Color::White => "white",
        Color::Blue => "blue",
        Color::Black => "black",
        Color::Red => "red",
        Color::Green => "green",
    }
}

pub fn basic_land_name(c: Color) -> &'static str {
    match c {
        Color::White => "Plains",
        Color::Blue => "Island",
        Color::Black => "Swamp",
        Color::Red => "Mountain",
        Color::Green => "Forest",
    }
}

fn type_noun(t: CardType) -> &'static str {
    match t {
        CardType::Artifact => "artifact",
        CardType::Battle => "battle",
        CardType::Creature => "creature",
        CardType::Enchantment => "enchantment",
        CardType::Instant => "instant",
        CardType::Kindred => "kindred",
        CardType::Land => "land",
        CardType::Planeswalker => "planeswalker",
        CardType::Sorcery => "sorcery",
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// "an instant or sorcery spell", "an artifact spell", "a Giant spell",
/// "an Elemental spell or a Chandra planeswalker spell".
/// "creature spell", "spell with mana value 4 or greater": the noun
/// "spell" goes before a filter's "with" qualities.
fn spell_noun(what: &str) -> String {
    let what = what.trim();
    if let Some(rest) = what.strip_prefix("with ") {
        return format!("spell with {rest}");
    }
    match what.find(" with ") {
        Some(i) => format!("{} spell{}", &what[..i], &what[i..]),
        None if what.is_empty() => "spell".to_string(),
        None => format!("{what} spell"),
    }
}

fn a_spell(f: &Filter, card: &Card) -> String {
    if let Some(alt) = &f.or {
        let (b, a) = crate::format::factor_qualifiers(f, alt);
        return format!("{} or {}", a_spell(&b, card), a_spell(&a, card));
    }
    if !f.any_types.is_empty() {
        let types: Vec<String> = f
            .any_types
            .iter()
            .map(|t| type_noun(*t).to_string())
            .collect();
        let mut g = f.clone();
        g.any_types.clear();
        let rest = card_filter_phrase(&g, card);
        let types = or_list(&types);
        let what = if rest.trim().is_empty() {
            types
        } else {
            format!("{} {types}", rest.trim())
        };
        return format!("{} spell", with_article(&what));
    }
    let what = spells_subject(f, card);
    let what = what.strip_suffix(" spells").unwrap_or(&what);
    // "an artifact spell", but "a Giant spell".
    let what = if f.subtypes.is_empty() {
        lowercase_first(what)
    } else {
        what.to_string()
    };
    with_article(&spell_noun(&what))
}

/// "{T}: Add {C}. Spend this mana only to cast an instant or sorcery
/// spell." (CR 106.6).
fn restricted_mana_text(card: &Card, m: Mana, r: &crate::ast::ManaRestriction) -> String {
    use mtg_core::card::SpendWording;
    let what = match m {
        Mana::Colorless => "{C}".to_string(),
        Mana::Colored(c) => format!("{{{}}}", c.letter()),
        Mana::Any => "one mana of any color".to_string(),
        Mana::Chosen => "one mana of the chosen color".to_string(),
    };
    let spells = r.spells.as_ref().map(|f| {
        let s = format!("cast {}", a_spell(f, card));
        if r.foretell {
            format!("foretell a card from your hand or {s}")
        } else {
            s
        }
    });
    // "an ability of an artifact source".
    let ability = match &r.ability_source {
        Some(f) => format!(
            "an ability of {} source",
            with_article(&filter_phrase(f, card, false))
        ),
        None => "an ability".to_string(),
    };
    let rule = match (r.wording, spells) {
        (SpendWording::CantCast, Some(s)) => format!("This mana can't be spent to {s}."),
        (SpendWording::ActivateFirst, Some(s)) => {
            format!("Spend this mana only to activate {ability} or {s}.")
        }
        (SpendWording::CastFirstTo, Some(s)) if r.abilities => {
            format!("Spend this mana only to {s} or to activate {ability}.")
        }
        (_, Some(s)) if r.abilities => {
            format!("Spend this mana only to {s} or activate {ability}.")
        }
        (_, Some(s)) => format!("Spend this mana only to {s}."),
        (_, None) => format!("Spend this mana only to activate {ability}."),
    };
    format!("{{T}}: Add {what}. {rule}")
}

/// "a" or "an".
fn article(word: &str) -> &'static str {
    match word.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u' | 'A' | 'E' | 'I' | 'O' | 'U') => "an",
        _ => "a",
    }
}

fn with_article(s: &str) -> String {
    // "sacrifice another creature".
    if let Some(rest) = s.strip_prefix("other ") {
        return format!("another {rest}");
    }
    format!("{} {s}", article(s))
}

/// "a, b, and c".
fn cmp_words(c: mtg_core::card::Cmp) -> &'static str {
    match c {
        mtg_core::card::Cmp::AtMost => " or less",
        mtg_core::card::Cmp::AtLeast => " or greater",
        mtg_core::card::Cmp::Exactly => "",
    }
}

/// A filter naming cards rather than permanents ("nonland card", "instant
/// card"): without a type it has no noun of its own.
fn card_filter_phrase(f: &Filter, card: &Card) -> String {
    if let Some(alt) = &f.or
        && crate::format::attacking_or_blocking(f, alt).is_none()
    {
        let (b, a) = crate::format::factor_qualifiers(f, alt);
        return format!(
            "{} or {}",
            card_filter_phrase(&b, card),
            card_filter_phrase(&a, card)
        );
    }
    // "a permanent card" (CR 110.4a).
    if f.not_types == [CardType::Instant, CardType::Sorcery]
        && f.types.is_empty()
        && f.any_types.is_empty()
    {
        let mut g = f.clone();
        g.not_types.clear();
        let p = card_filter_phrase(&g, card);
        return if p.is_empty() {
            "permanent".to_string()
        } else {
            format!("{p} permanent")
        };
    }
    let p = filter_phrase(f, card, false);
    if f.types.is_empty() && f.subtypes.is_empty() && f.any_types.is_empty() {
        p.replace(" permanent", "").replace("permanent", "")
    } else {
        p
    }
}

/// The subject of a cost change: "Artifact spells", "Instant and
/// sorcery spells", "Spells".
/// A cost changer's subject: "Goblin spells and Rogue spells", "Red
/// spells and white spells" for two alternatives.
fn cost_spells_subject(f: &Filter, card: &Card) -> String {
    let second = |s: String, keep_case: bool| if keep_case { s } else { lowercase_first(&s) };
    if let Some(alt) = &f.or {
        let (b, a) = crate::format::factor_qualifiers(f, alt);
        return format!(
            "{} and {}",
            spells_subject(&b, card),
            second(spells_subject(&a, card), !a.subtypes.is_empty())
        );
    }
    if let Some(cs) = &f.colors
        && cs.len() == 2
    {
        let mut one = f.clone();
        let words: Vec<String> = cs
            .iter()
            .map(|c| {
                one.colors = Some(vec![*c]);
                spells_subject(&one, card)
            })
            .collect();
        return format!("{} and {}", words[0], lowercase_first(&words[1]));
    }
    spells_subject(f, card)
}

fn spells_subject(f: &Filter, card: &Card) -> String {
    if *f == Filter::default() {
        return "Spells".to_string();
    }
    let mut g = f.clone();
    let noun = if g.any_types.is_empty() {
        card_filter_phrase(&g, card)
    } else {
        let types: Vec<String> = g
            .any_types
            .iter()
            .map(|t| type_noun(*t).to_string())
            .collect();
        g.any_types.clear();
        let rest = card_filter_phrase(&g, card);
        let joined = list(&types);
        if rest.is_empty() {
            joined
        } else {
            format!("{rest} {joined}")
        }
    };
    capitalize(&format!("{} spells", noun.trim()))
}

/// "red or green", "artifact, creature, or land".
fn or_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} or {b}"),
        _ => {
            let (last, rest) = items.split_last().expect("nonempty");
            format!("{}, or {last}", rest.join(", "))
        }
    }
}

fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} and {b}"),
        _ => {
            let (last, rest) = items.split_last().expect("nonempty");
            format!("{}, and {last}", rest.join(", "))
        }
    }
}

/// Ends a sentence unless it already carries its own terminator (a quoted
/// ability ends with `."`).
fn sentence(s: &str) -> String {
    let s = capitalize(s);
    if s.ends_with('.') || s.ends_with(".\"") {
        s
    } else {
        format!("{s}.")
    }
}

/// How a permanent refers to itself: "this creature", "this Aura"; a
/// legendary permanent by its name, or the part of it before a comma
/// ("Sacrifice Lunatic Pandora", "When Pol Jamaar enters"; CR 201.5).
pub fn this_ref(card: &Card) -> String {
    if card.kind != Kind::Token && card.supertypes.contains(&Supertype::Legendary) {
        short_name(card).to_string()
    } else {
        format!("this {}", self_noun(card))
    }
}

/// The name a card's text uses for itself: a legendary card's name up to
/// its first comma ("Pol Jamaar" for "Pol Jamaar, Illusionist"; CR
/// 201.5), else the whole name.
pub fn short_name(card: &Card) -> &str {
    if card.supertypes.contains(&Supertype::Legendary) {
        card.name.split(", ").next().unwrap_or(&card.name)
    } else {
        &card.name
    }
}

/// Whether text starts with the card's name for itself, which keeps its
/// capital after a comma.
fn starts_with_name(s: &str, card: &Card) -> bool {
    s.starts_with(&card.name) || s.starts_with(short_name(card))
}

/// The noun a permanent uses for itself: "this creature", "this Aura".
pub fn self_noun(card: &Card) -> String {
    if card.kind == Kind::Token {
        return "token".to_string();
    }
    if card.types.contains(&CardType::Creature) {
        return "creature".to_string();
    }
    if card.types.contains(&CardType::Planeswalker) {
        return "planeswalker".to_string();
    }
    if card.types.contains(&CardType::Land) {
        return "land".to_string();
    }
    if card.types.contains(&CardType::Artifact) {
        // "this Vehicle", "this Equipment" (Oracle's templating since
        // 2024).
        if card.subtypes.iter().any(|s| s == "Vehicle") {
            return "Vehicle".to_string();
        }
        if card.subtypes.iter().any(|s| s == "Equipment") {
            return "Equipment".to_string();
        }
        return "artifact".to_string();
    }
    if card.types.contains(&CardType::Enchantment) {
        if card.subtypes.iter().any(|s| s == "Aura") {
            return "Aura".to_string();
        }
        return "enchantment".to_string();
    }
    "permanent".to_string()
}

fn is_spell(card: &Card) -> bool {
    card.types
        .iter()
        .any(|t| matches!(t, CardType::Instant | CardType::Sorcery))
}

fn is_equipment(card: &Card) -> bool {
    card.subtypes.iter().any(|s| s == "Equipment")
}

/// Rendering state for one ability's text.
#[allow(clippy::struct_excessive_bools)] // independent mention flags
struct Ctx<'a> {
    card: &'a Card,
    lookup: &'a dyn Lookup,
    targets: &'a [Target],
    source_mentioned: bool,
    target_mentioned: Vec<bool>,
    /// What each target is now called after its first mention.
    target_noun: Vec<String>,
    groups_mentioned: HashSet<Recipient>,
    /// Inside a delayed trigger's effects.
    in_delayed: bool,
    /// Inside a delayed trigger about the triggering object, which its
    /// effects call "that creature" ("destroy that creature at end of
    /// combat").
    delayed_that: bool,
    /// The ability being rendered, for "except it has this ability".
    current: Option<&'a Ability>,
    /// The trigger of the ability, for wording that depends on it.
    trigger: Option<&'a Trigger>,
    /// The last token created, and, inside an "instead" sentence, the
    /// token the sentence before created ("create four of those tokens
    /// instead").
    last_token: Option<String>,
    those_tokens: Option<String>,
    /// What this is attached to has been named in the ability, so it is
    /// "it" as an object ("tap enchanted creature and remove all counters
    /// from it"; "if enchanted creature is untapped, tap it").
    attached_mentioned: bool,
}

impl<'a> Ctx<'a> {
    fn new(card: &'a Card, lookup: &'a dyn Lookup, targets: &'a [Target]) -> Self {
        Ctx {
            card,
            lookup,
            targets,
            source_mentioned: false,
            target_mentioned: vec![false; targets.len()],
            target_noun: targets.iter().map(target_noun).collect(),
            groups_mentioned: HashSet::new(),
            in_delayed: false,
            delayed_that: false,
            current: None,
            trigger: None,
            last_token: None,
            those_tokens: None,
            attached_mentioned: false,
        }
    }

    /// The source in subject position ("this creature", "Lightning Bolt",
    /// then "it").
    fn source(&mut self) -> String {
        if self.source_mentioned {
            return "it".to_string();
        }
        self.source_mentioned = true;
        self.source_name()
    }

    /// The source in full: a spell's name, "this creature", or a legendary
    /// permanent's name.
    fn source_name(&self) -> String {
        if is_spell(self.card) {
            short_name(self.card).to_string()
        } else {
            this_ref(self.card)
        }
    }

    fn target_first(&mut self, i: usize) -> String {
        self.target_mentioned[i] = true;
        // "X target creatures", "one or two targets", "any number of
        // target creature cards from your graveyard".
        if let Target::Many { min, max, .. } = &self.targets[i] {
            let count = match max {
                None => "any number of".to_string(),
                Some(m) if m == min => count_words(*m, "one"),
                Some(Num::N(m)) => {
                    let lo = match min {
                        Num::N(n) => *n,
                        _ => 0,
                    };
                    let words: Vec<String> = (lo.max(1)..=*m)
                        .map(|n| count_words(Num::N(n), "one"))
                        .collect();
                    or_list(&words)
                }
                Some(m) => count_words(*m, "one"),
            };
            return match self.targets[i].base() {
                Target::Spec(TargetSpec::Any) => format!("{count} targets"),
                _ => format!("{count} target {}", self.plural_target_noun(i)),
            };
        }
        let phrase = self.target_phrase_of(i);
        if matches!(self.targets[i], Target::UpTo(_)) {
            format!("up to one {phrase}")
        } else {
            phrase
        }
    }

    /// "target spell that targets a creature".
    fn spell_target_phrase(&self, spell: &Filter) -> String {
        let mut base_filter = spell.clone();
        let targeting = base_filter.targeting.take();
        let base = card_filter_phrase(&base_filter, self.card);
        let mut out = format!("target {}", spell_noun(&base));
        if let Some(what) = targeting {
            out.push_str(" that targets ");
            // "that targets this creature".
            out.push_str(&crate::format::targeting(&what, |g| {
                if g.relation == Relation::This {
                    this_ref(self.card)
                } else {
                    filter_phrase(g, self.card, false)
                }
            }));
        }
        out
    }

    /// Whether a recipient is a variable target group (CR 601.2c).
    fn is_many(&self, r: Recipient) -> bool {
        matches!(r, Recipient::Target(i) | Recipient::ThatTarget(i)
            if matches!(self.targets.get(i), Some(Target::Many { .. })))
    }

    fn target_phrase_of(&self, i: usize) -> String {
        match self.targets[i].base() {
            Target::UpTo(_) | Target::Many { .. } => unreachable!("base"),
            Target::Spec(s) => target_phrase(*s),
            Target::CreatureOfSubtype(sub) => format!("target {sub} creature"),
            Target::Filter(f) if f.relation == Relation::Other => {
                let mut g = f.clone();
                g.relation = Relation::Any;
                format!(
                    "another target {}",
                    controller_first(&g, filter_phrase(&g, self.card, false))
                )
            }
            // "target creature you control with power 2 or less".
            Target::Filter(f) => format!(
                "target {}",
                controller_first(f, filter_phrase(f, self.card, false))
            ),
            Target::Spell(f) => self.spell_target_phrase(f),
            Target::GraveyardCard {
                filter,
                who,
                graveyards,
            } => {
                let whose = match who {
                    Who::You => "your graveyard",
                    Who::Opponents => "an opponent's graveyard",
                    Who::Any if *graveyards => "graveyards",
                    Who::Any => "a graveyard",
                };
                let mut g = filter.clone();
                let tgt = if g.relation == Relation::Other {
                    g.relation = Relation::Any;
                    "another target"
                } else {
                    "target"
                };
                format!("{tgt} {} from {whose}", cards_phrase(&g, self.card, false))
            }
        }
    }

    /// A recipient as the object of a verb.
    fn obj(&mut self, r: Recipient) -> String {
        match r {
            Recipient::Targets(a, n) => {
                if self.target_mentioned[a] {
                    "them".to_string()
                } else {
                    let rs: Vec<Recipient> = (a..a + n).map(Recipient::Target).collect();
                    self.obj_list(&rs, false)
                }
            }
            Recipient::Target(i) => {
                if self.target_mentioned[i] {
                    if self.is_many(r) { "them" } else { "it" }.to_string()
                } else {
                    self.target_first(i)
                }
            }
            Recipient::ThatTarget(i) => {
                self.target_mentioned[i] = true;
                format!("that {}", self.target_noun[i])
            }
            Recipient::You => "you".to_string(),
            Recipient::EachOpponent => "each opponent".to_string(),
            Recipient::EachOtherPlayer => "each other player".to_string(),
            Recipient::Source => self.source(),
            Recipient::SourceNamed => {
                self.source_mentioned = true;
                self.source_name()
            }
            Recipient::SourceIt => {
                self.source_mentioned = true;
                "it".to_string()
            }
            // "When enchanted creature is dealt damage, destroy it."
            Recipient::TriggeringObject if self.trigger_is_enchanted() => "it".to_string(),
            Recipient::TriggeringObject | Recipient::SameThat => "that creature".to_string(),
            Recipient::TriggeringPlayer => "that player".to_string(),
            Recipient::TriggeringCard => "that card".to_string(),
            Recipient::TriggeringController { that: true } => {
                let noun = if self.trigger_is_enchanted() {
                    enchanted_noun(self.card)
                } else {
                    "creature"
                };
                format!("that {noun}'s controller")
            }
            Recipient::ControllerOf(_) | Recipient::TriggeringController { that: false } => {
                "its controller".to_string()
            }
            Recipient::ThatControllerOf(i) => format!("that {}'s controller", self.target_noun[i]),
            Recipient::TheControllerOf(i) => format!("the {}'s controller", self.target_noun[i]),
            Recipient::OwnerOf(_) => "its owner".to_string(),
            Recipient::DefendingPlayer => "the player or planeswalker it's attacking".to_string(),
            Recipient::DefendingPlayerWord => "defending player".to_string(),
            Recipient::EachCreature => "each creature".to_string(),
            Recipient::EachPlayer => "each player".to_string(),
            Recipient::EachCreatureYouControl => {
                self.groups_mentioned.insert(r);
                "each creature you control".to_string()
            }
            Recipient::EachPermanentYouControl => {
                self.groups_mentioned.insert(r);
                "each permanent you control".to_string()
            }
            Recipient::SameObject if self.delayed_that => "that creature".to_string(),
            Recipient::SameObject | Recipient::CreatedObject | Recipient::TriggeringIt => {
                "it".to_string()
            }
            Recipient::Group(i) => {
                self.groups_mentioned.insert(r);
                format!("each {}", self.group_phrase(i, false))
            }
            Recipient::Attached if self.attached_mentioned => "it".to_string(),
            Recipient::Attached => {
                self.attached_mentioned = true;
                filter_phrase(&Filter::enchanted(), self.card, false)
            }
            Recipient::BlockingIt => "each creature blocking it".to_string(),
            Recipient::TargetAnd(t, i) => self.target_and(t, i),
        }
    }

    /// "target creature and all other creatures with the same name as that
    /// creature", "target creature and each other creature that shares a
    /// color with it".
    fn target_and(&mut self, target: usize, group: u16) -> String {
        let first = self.obj(Recipient::Target(target));
        let Some(filter) = self.card.groups.get(usize::from(group)) else {
            return first;
        };
        let mut rest = filter.clone();
        rest.same = None;
        let plural_form = filter.all;
        let phrase = filter_phrase(&rest, self.card, plural_form);
        let rel = match filter.same {
            Some(SameRef::SameNameAs(n)) => format!(
                " with the same name as that {}",
                self.target_noun.get(n).map_or("permanent", String::as_str)
            ),
            Some(SameRef::SharesColorWith(_)) => format!(
                " that {} a color with it",
                if plural_form { "share" } else { "shares" }
            ),
            _ => String::new(),
        };
        let det = if plural_form { "all" } else { "each" };
        format!("{first} and {det} {phrase}{rel}")
    }

    /// The filter of the card's `i`th group, as a noun phrase.
    fn group_phrase(&mut self, i: u16, plural_form: bool) -> String {
        let Some(f) = self.card.groups.get(usize::from(i)) else {
            return "permanent".to_string();
        };
        let phrase = filter_phrase(f, self.card, plural_form);
        // "all Auras attached to it", "all Equipment attached to that
        // creature", "all Auras and Equipment attached to target creature".
        match f.attached_to {
            Some(AttachedRef::Target(n)) => {
                let what = self.obj(Recipient::Target(n));
                format!("{phrase} attached to {what}")
            }
            Some(AttachedRef::ThatTarget(n)) => {
                self.target_mentioned[n] = true;
                format!("{phrase} attached to that {}", self.target_noun[n])
            }
            _ => phrase,
        }
    }

    /// A recipient as the subject of a sentence.
    fn trigger_is_enchanted(&self) -> bool {
        self.trigger.is_some_and(|t| {
            let t = match t {
                Trigger::When(inner) => inner,
                t => t,
            };
            matches!(
                t,
                Trigger::Dies(TriggerObj::Filter(f))
                    | Trigger::DealtDamage(TriggerObj::Filter(f))
                    | Trigger::DealtCombatDamage(TriggerObj::Filter(f))
                    | Trigger::Blocks(TriggerObj::Filter(f))
                    | Trigger::Attacks(TriggerObj::Filter(f))
                    | Trigger::BecomesTapped(TriggerObj::Filter(f))
                    if f.enchanted
            )
        })
    }

    fn subj(&mut self, r: Recipient) -> String {
        match r {
            // As a subject Oracle keeps "defending player" ("defending
            // player loses 4 life").
            Recipient::DefendingPlayer => "defending player".to_string(),
            Recipient::Targets(a, n) => {
                if self.target_mentioned[a] {
                    "they".to_string()
                } else {
                    let rs: Vec<Recipient> = (a..a + n).map(Recipient::Target).collect();
                    self.obj_list(&rs, false)
                }
            }
            // "That creature becomes black": named again on purpose.
            Recipient::ThatTarget(i) if self.target_mentioned[i] => {
                format!("that {}", self.target_noun[i])
            }
            Recipient::Target(i) | Recipient::ThatTarget(i) => {
                if self.target_mentioned[i] {
                    if self.is_many(r) { "they" } else { "it" }.to_string()
                } else {
                    self.target_first(i)
                }
            }
            Recipient::EachCreature => "all creatures".to_string(),
            Recipient::EachCreatureYouControl => {
                if self.groups_mentioned.insert(r) {
                    "creatures you control".to_string()
                } else {
                    "those creatures".to_string()
                }
            }
            Recipient::EachPermanentYouControl => {
                if self.groups_mentioned.insert(r) {
                    "permanents you control".to_string()
                } else {
                    "those permanents".to_string()
                }
            }
            Recipient::Group(i) => {
                if !self.groups_mentioned.insert(r) {
                    "they".to_string()
                } else if self.each_group(r) {
                    format!("each {}", self.group_phrase(i, false))
                } else {
                    self.group_phrase(i, true)
                }
            }
            Recipient::Source => self.source(),
            other => self.obj(other),
        }
    }

    /// A group in a combat relation to the source is a subject one at a
    /// time: "Each creature blocking this creature gets -1/-1".
    fn each_group(&self, r: Recipient) -> bool {
        matches!(r, Recipient::Group(i)
            if self.card.groups.get(usize::from(i)).is_some_and(|f| f.combat_with.is_some()))
    }

    /// Objects of destruction and exile ("all creatures").
    fn obj_all(&mut self, r: Recipient) -> String {
        match r {
            Recipient::EachCreature => "all creatures".to_string(),
            Recipient::EachCreatureYouControl => "all creatures you control".to_string(),
            Recipient::EachPermanentYouControl => "all permanents you control".to_string(),
            Recipient::Group(i) => format!("all {}", self.group_phrase(i, true)),
            other => self.obj(other),
        }
    }

    /// A list of recipients, merging fresh targets of one kind ("two target
    /// lands").
    fn obj_list(&mut self, rs: &[Recipient], all: bool) -> String {
        let fresh: Vec<usize> = rs
            .iter()
            .filter_map(|r| match r {
                Recipient::Target(i) if !self.target_mentioned[*i] => Some(*i),
                _ => None,
            })
            .collect();
        if fresh.len() == rs.len()
            && fresh.len() > 1
            && fresh
                .iter()
                .all(|i| self.targets[*i] == self.targets[fresh[0]])
        {
            for i in &fresh {
                self.target_mentioned[*i] = true;
            }
            let up_to = if matches!(self.targets[fresh[0]], Target::UpTo(_)) {
                "up to "
            } else {
                ""
            };
            let noun = self.plural_target_noun(fresh[0]);
            return format!(
                "{up_to}{} target {noun}",
                count_words(Num::N(u32::try_from(fresh.len()).expect("small")), "a")
            );
        }
        let items: Vec<String> = rs
            .iter()
            .map(|r| {
                // "target creature and all Auras attached to it".
                let all_group = rs.len() > 1
                    && matches!(r, Recipient::Group(i)
                    if self.card.groups.get(usize::from(*i)).is_some_and(|f| f.all));
                if all || all_group {
                    self.obj_all(*r)
                } else {
                    self.obj(*r)
                }
            })
            .collect();
        list(&items)
    }

    /// The plural noun of target `i`'s kind: "lands", "creature cards
    /// from your graveyard".
    fn plural_target_noun(&self, i: usize) -> String {
        match self.targets[i].base() {
            Target::UpTo(_) | Target::Many { .. } => unreachable!("base"),
            Target::Spec(TargetSpec::CreatureCardInYourGraveyard) => {
                "creature cards from your graveyard".to_string()
            }
            Target::Spec(TargetSpec::ArtifactCreatureOrLand) => {
                "artifacts, creatures, and/or lands".to_string()
            }
            // "creatures you control".
            Target::Spec(TargetSpec::CreatureYouControl) => "creatures you control".to_string(),
            Target::Spec(s) => pluralize(&target_phrase(*s).replace("target ", "")),
            Target::CreatureOfSubtype(sub) => format!("{sub} creatures"),
            Target::Filter(f) => filter_phrase(f, self.card, true),
            Target::Spell(f) => format!("{} spells", filter_phrase(f, self.card, false)),
            Target::GraveyardCard {
                filter,
                who,
                graveyards,
            } => {
                let whose = match who {
                    Who::You => "your graveyard",
                    Who::Opponents => "an opponent's graveyard",
                    Who::Any if *graveyards => "graveyards",
                    Who::Any => "a single graveyard",
                };
                if *filter == Filter::default() {
                    format!("cards from {whose}")
                } else {
                    format!(
                        "{} cards from {whose}",
                        filter_phrase(filter, self.card, false)
                    )
                }
            }
        }
    }

    /// Objects of tap and untap: Oracle says "tap all artifacts" for a
    /// group but "untap each creature you control" for the built-in
    /// groups.
    fn obj_list_tap(&mut self, rs: &[Recipient]) -> String {
        if !rs.iter().any(|r| matches!(r, Recipient::Group(_))) {
            return self.obj_list(rs, false);
        }
        let items: Vec<String> = rs
            .iter()
            .map(|r| match r {
                Recipient::Group(i) => format!("all {}", self.group_phrase(*i, true)),
                other => self.obj(*other),
            })
            .collect();
        list(&items)
    }

    fn is_plural(r: Recipient) -> bool {
        matches!(
            r,
            Recipient::EachCreature
                | Recipient::EachCreatureYouControl
                | Recipient::EachPermanentYouControl
                | Recipient::Group(_)
                | Recipient::Targets(..)
                | Recipient::TargetAnd(..)
        )
    }

    fn verb(&self, r: Recipient, singular: &'static str, plural: &'static str) -> &'static str {
        if r == Recipient::You || (Self::is_plural(r) && !self.each_group(r)) || self.is_many(r) {
            plural
        } else {
            singular
        }
    }
}

fn target_noun(t: &Target) -> String {
    match t.base() {
        Target::UpTo(_) | Target::Many { .. } => unreachable!("base"),
        Target::CreatureOfSubtype(_) => "creature".to_string(),
        Target::Spell(_) => "spell".to_string(),
        Target::GraveyardCard { .. } => "card".to_string(),
        Target::Spec(s) => match s {
            TargetSpec::Any => "permanent or player".to_string(),
            TargetSpec::Creature
            | TargetSpec::TappedCreature
            | TargetSpec::CreatureYouControl
            | TargetSpec::OtherCreature
            | TargetSpec::NonlegendaryCreatureYouControl
            | TargetSpec::CreatureOrPlaneswalker
            | TargetSpec::CreatureWithSubtype(_)
            | TargetSpec::AttackingOrBlockingCreature
            | TargetSpec::AttackingCreature
            | TargetSpec::BlockingCreature => "creature".to_string(),
            TargetSpec::PlayerOrPlaneswalker
            | TargetSpec::Player
            | TargetSpec::Opponent
            | TargetSpec::OpponentOrPlaneswalker => "player".to_string(),
            TargetSpec::Land => "land".to_string(),
            TargetSpec::ArtifactCreatureOrLand
            | TargetSpec::ArtifactOrEnchantment
            | TargetSpec::Permanent
            | TargetSpec::OtherPermanent
            | TargetSpec::ArtifactOrCreature
            | TargetSpec::UpTo(_)
            | TargetSpec::Many(_)
            | TargetSpec::Filtered(_) => "permanent".to_string(),
            TargetSpec::Artifact => "artifact".to_string(),
            TargetSpec::Enchantment => "enchantment".to_string(),
            TargetSpec::CreatureCardInYourGraveyard | TargetSpec::GraveyardCard { .. } => {
                "card".to_string()
            }
            TargetSpec::Spell
            | TargetSpec::CreatureSpell
            | TargetSpec::NoncreatureSpell
            | TargetSpec::SpellFiltered(_)
            | TargetSpec::OnStack { spells: true, .. } => "spell".to_string(),
            TargetSpec::OnStack { .. } => "ability".to_string(),
        },
        Target::Filter(f) => {
            if f.types.contains(&CardType::Creature) {
                "creature".to_string()
            } else if f.types.contains(&CardType::Land) {
                "land".to_string()
            } else {
                "permanent".to_string()
            }
        }
    }
}

pub fn target_phrase(s: TargetSpec) -> String {
    // "target spell or ability", "target activated or triggered ability",
    // "target activated ability from an artifact source".
    if let TargetSpec::OnStack {
        spells,
        activated,
        triggered,
        source,
    } = s
    {
        let kinds = match (activated, triggered) {
            (true, true) if spells => "spell or ability",
            (true, true) => "activated or triggered ability",
            (true, false) => "activated ability",
            (false, true) => "triggered ability",
            (false, false) => "spell",
        };
        let from = source.map_or_else(String::new, |t| {
            let t = format!("{t:?}").to_lowercase();
            format!(" from {}", with_article(&format!("{t} source")))
        });
        return format!("target {kinds}{from}");
    }
    match s {
        TargetSpec::OnStack { .. } => unreachable!("above"),
        TargetSpec::Any => "any target",
        TargetSpec::Creature | TargetSpec::CreatureWithSubtype(_) => "target creature",
        TargetSpec::PlayerOrPlaneswalker => "target player or planeswalker",
        TargetSpec::CreatureOrPlaneswalker => "target creature or planeswalker",
        TargetSpec::Land => "target land",
        TargetSpec::Player => "target player",
        TargetSpec::TappedCreature => "target tapped creature",
        TargetSpec::CreatureYouControl => "target creature you control",
        TargetSpec::ArtifactCreatureOrLand => "target artifact, creature, or land",
        TargetSpec::ArtifactOrEnchantment => "target artifact or enchantment",
        TargetSpec::Artifact => "target artifact",
        TargetSpec::Enchantment => "target enchantment",
        TargetSpec::Permanent | TargetSpec::Filtered(_) => "target permanent",
        TargetSpec::OtherCreature => "another target creature",
        TargetSpec::OtherPermanent => "another target permanent",
        TargetSpec::NonlegendaryCreatureYouControl => "target nonlegendary creature you control",
        TargetSpec::CreatureCardInYourGraveyard => "target creature card from your graveyard",
        TargetSpec::Spell | TargetSpec::SpellFiltered(_) => "target spell",
        TargetSpec::CreatureSpell => "target creature spell",
        TargetSpec::NoncreatureSpell => "target noncreature spell",
        TargetSpec::Opponent => "target opponent",
        TargetSpec::OpponentOrPlaneswalker => "target opponent or planeswalker",
        TargetSpec::ArtifactOrCreature => "target artifact or creature",
        TargetSpec::AttackingOrBlockingCreature => "target attacking or blocking creature",
        TargetSpec::AttackingCreature => "target attacking creature",
        TargetSpec::BlockingCreature => "target blocking creature",
        TargetSpec::GraveyardCard { .. } => "target card from a graveyard",
        TargetSpec::UpTo(_) => "up to one target permanent",
        TargetSpec::Many(_) => "any number of target permanents",
    }
    .to_string()
}

/// What an Aura calls the permanent it enchants: "creature", "land",
/// "artifact", or "permanent" when it can enchant several kinds.
fn enchanted_noun(card: &Card) -> &'static str {
    let enchant = card.abilities.iter().find_map(|a| match a {
        Ability::Enchant(t) => Some(t),
        _ => None,
    });
    match enchant.map(Target::base) {
        Some(Target::Spec(TargetSpec::Land)) => "land",
        Some(Target::Spec(TargetSpec::Artifact)) => "artifact",
        Some(Target::Spec(TargetSpec::Enchantment)) => "enchantment",
        Some(Target::Spec(
            TargetSpec::Permanent
            | TargetSpec::ArtifactOrCreature
            | TargetSpec::ArtifactOrEnchantment
            | TargetSpec::ArtifactCreatureOrLand
            | TargetSpec::CreatureOrPlaneswalker,
        )) => "permanent",
        Some(Target::Filter(f)) if f.types == [CardType::Land] => "land",
        // "Enchant nonland permanent".
        Some(Target::Filter(f)) if f.types.is_empty() && f.subtypes.is_empty() => "permanent",
        Some(Target::Filter(f)) if f.types.len() + f.any_types.len() > 1 => "permanent",
        _ => "creature",
    }
}

// ---- filters ----

/// The noun phrase of a filter without an article, singular ("creature you
/// control", "nonbasic land") or plural.
#[allow(clippy::too_many_lines)] // one block per filter part
fn filter_phrase(f: &Filter, card: &Card, plural_form: bool) -> String {
    // "for each Aura and Equipment attached to it"; a target's phrase is
    // the sentence's (`group_phrase`).
    if let Some(a) = f.attached_to {
        let g = Filter {
            attached_to: None,
            ..f.clone()
        };
        let p = filter_phrase(&g, card, plural_form);
        // "for each Aura and Equipment attached to it".
        let p = if plural_form {
            p
        } else {
            p.replace(" or ", " and ")
        };
        return match a {
            AttachedRef::Source | AttachedRef::Host => format!("{p} attached to it"),
            AttachedRef::Target(_) | AttachedRef::ThatTarget(_) => p,
        };
    }
    let Some(alt) = &f.or else {
        return filter_phrase_one(f, card, plural_form);
    };
    let mut base = f.clone();
    base.or = None;
    if let Some(rest) = crate::format::attacking_or_blocking(&base, alt) {
        return format!(
            "attacking or blocking {}",
            filter_phrase_one(&rest, card, plural_form)
        );
    }
    // "creatures with flying or reach".
    if let Some(k) = crate::format::keyword_alternative(&base, alt) {
        let one = filter_phrase_one(&base, card, plural_form);
        let kw = keyword_name(base.keywords[0]);
        return one.replacen(
            &format!("with {kw}"),
            &format!("with {kw} or {}", keyword_name(k)),
            1,
        );
    }
    // A chain of three or more prints as a list: "Bear, Spider, or Wolf".
    let mut sides = Vec::new();
    let mut cur = f.clone();
    loop {
        let Some(next) = cur.or.clone() else {
            sides.push(filter_phrase_one(&cur, card, plural_form));
            break;
        };
        let (b, mut a) = crate::format::factor_qualifiers(&cur, &next);
        // "basic Forest, Plains, or Island": a supertype every side shares
        // prints once, before the first.
        if !f.supertypes.is_empty()
            && a.supertypes == f.supertypes
            && !a.subtypes.is_empty()
            && b.subtypes.len() == 1
        {
            a.supertypes.clear();
        }
        sides.push(filter_phrase_one(&b, card, plural_form));
        cur = a;
    }
    // In the plural the alternatives join with "and": "Ninjas and Rogues
    // you control", "Bears, Spiders, and Wolves".
    let conj = if plural_form { "and" } else { "or" };
    if sides.len() == 2 {
        format!("{} {conj} {}", sides[0], sides[1])
    } else {
        let last = sides.pop().expect("three or more");
        format!("{}, {conj} {last}", sides.join(", "))
    }
}

#[allow(clippy::too_many_lines)] // one clause per qualifier
fn filter_phrase_one(f: &Filter, card: &Card, plural_form: bool) -> String {
    if f.enchanted {
        if is_equipment(card) {
            return "equipped creature".to_string();
        }
        return format!("enchanted {}", enchanted_noun(card));
    }
    if f.types.is_empty() && f.not_types.contains(&CardType::Planeswalker) {
        let mut s = "artifacts, creatures, enchantments, and lands".to_string();
        if f.who == Some(Who::You) {
            s.push_str(" you control");
        }
        return s;
    }
    let mut words: Vec<String> = Vec::new();
    if f.relation == Relation::Other {
        words.push("other".to_string());
    }
    if f.status.enchanted == Some(true) {
        words.push("enchanted".to_string());
    }
    if let Some(t) = f.tapped {
        words.push(if t { "tapped" } else { "untapped" }.to_string());
    }
    if let Some(b) = f.status.blocked {
        words.push(if b { "blocked" } else { "unblocked" }.to_string());
    }
    if f.attacking {
        words.push("attacking".to_string());
    }
    if f.blocking {
        words.push("blocking".to_string());
    }
    if let Some(cc) = f.color_count {
        words.push(format!("{cc:?}").to_lowercase());
    }
    if let Some(colors) = &f.colors {
        words.push(or_list(
            &colors
                .iter()
                .map(|c| color_name(*c).to_string())
                .collect::<Vec<_>>(),
        ));
    }
    for s in &f.supertypes {
        words.push(format!("{s:?}").to_lowercase());
    }
    for s in &f.not_supertypes {
        words.push(match s {
            Supertype::Basic => "nonbasic".to_string(),
            Supertype::Legendary => "nonlegendary".to_string(),
            Supertype::Snow => "nonsnow".to_string(),
            Supertype::World => "nonworld".to_string(),
        });
    }
    // "noncreature, nonland card", "nonartifact, nonblack creature":
    // several negations are listed, types before colors.
    let mut negs: Vec<String> = f
        .not_types
        .iter()
        .map(|t| format!("non{}", type_noun(*t)))
        .collect();
    negs.extend(
        f.not_colors
            .iter()
            .map(|c| format!("non{}", color_name(*c))),
    );
    negs.extend(f.not_subtypes.iter().map(|s| format!("non-{s}")));
    if !negs.is_empty() {
        words.push(negs.join(", "));
    }
    let mut noun: Vec<String> = Vec::new();
    for s in &f.subtypes {
        noun.push(s.clone());
    }
    for t in &f.types {
        noun.push(type_noun(*t).to_string());
    }
    let any_types = !f.any_types.is_empty();
    if noun.is_empty() && !any_types {
        noun.push("permanent".to_string());
    }
    if plural_form && !noun.is_empty() {
        let last = noun.pop().expect("nonempty");
        noun.push(pluralize(&last));
    }
    if any_types {
        let names: Vec<String> = f
            .any_types
            .iter()
            .map(|t| {
                let n = type_noun(*t).to_string();
                if plural_form { pluralize(&n) } else { n }
            })
            .collect();
        noun.push(or_list(&names));
    }
    words.extend(noun);
    let mut s = words.join(" ");
    for (name, v) in [("power", f.power), ("toughness", f.toughness)] {
        if let Some((cmp, n)) = v {
            let _ = write!(s, " with {name} {n}{}", cmp_words(cmp));
        }
    }
    if let Some((cmp, n)) = f.mana_value {
        if f.status.mana_value_x {
            let _ = write!(s, " with mana value X{}", cmp_words(cmp));
        } else {
            let _ = write!(s, " with mana value {n}{}", cmp_words(cmp));
        }
    }
    if !f.keywords.is_empty() {
        let _ = write!(s, " with {}", keywords_list(&f.keywords));
    }
    if !f.not_keywords.is_empty() {
        let _ = write!(s, " without {}", keywords_list(&f.not_keywords));
    }
    if f.named_first {
        s.push_str(&same_phrase(f, card, plural_form));
    }
    match f.who {
        Some(Who::You) => s.push_str(" you control"),
        Some(Who::Opponents) if f.dont_control => s.push_str(" you don't control"),
        Some(Who::Opponents) if f.your_opponents => s.push_str(" your opponents control"),
        Some(Who::Opponents) if f.defending => s.push_str(" defending player controls"),
        Some(Who::Opponents) => s.push_str(" an opponent controls"),
        Some(Who::Any) | None => {}
    }
    if f.no_abilities {
        s.push_str(" with no abilities");
    }
    if f.damaged_short {
        s.push_str(" dealt damage this turn");
    } else if f.damaged {
        s.push_str(" that was dealt damage this turn");
    }
    // "that entered this turn", "that isn't enchanted".
    let that = |s: &mut String, one: &str, many: &str| {
        s.push_str(if plural_form { many } else { one });
    };
    if f.status.enchanted == Some(false) {
        that(&mut s, " that isn't enchanted", " that aren't enchanted");
    }
    if let Some(r) = f.combat_with {
        let this = this_ref(card);
        let _ = match r {
            crate::ast::CombatRef::BlockingThis => write!(s, " blocking {this}"),
            crate::ast::CombatRef::BlockedByThis => write!(s, " {this} is blocking"),
            crate::ast::CombatRef::BlockedByIt => write!(s, " it's blocking"),
            crate::ast::CombatRef::BlockingOrBlockedByThis => {
                write!(s, " blocking or blocked by {this}")
            }
        };
    }
    if f.status.entered_this_turn {
        s.push_str(" that entered this turn");
    }
    if f.status.dealt_damage {
        s.push_str(" that dealt damage this turn");
    }
    if f.status.blocked_this_turn {
        s.push_str(" that blocked this turn");
    }
    // "a creature dealt damage by this creature this turn".
    if f.status.damaged_by_source {
        let _ = write!(s, " dealt damage by {} this turn", this_ref(card));
    }
    if let Some(c) = f.chosen {
        s.push_str(match c {
            OfChosen::ChosenColor => " of the chosen color",
            OfChosen::ChosenType => " of the chosen type",
            OfChosen::ThatType => " of that type",
            OfChosen::ThatColor => " of that color",
            OfChosen::ColorOfChoice => " of the color of your choice",
            OfChosen::CreatureTypeOfChoice => " of the creature type of your choice",
        });
    }
    if !f.named_first {
        s.push_str(&same_phrase(f, card, plural_form));
    }
    s
}

/// " named ~", " not named ~" (CR 201.4b: the card's own name), and the
/// relations to a target outside a recipient that names it (" with the
/// same name as it").
fn same_phrase(f: &Filter, card: &Card, plural_form: bool) -> String {
    match f.same {
        None => String::new(),
        Some(SameRef::NamedThis) => format!(" named {}", card.name),
        Some(SameRef::NotNamedThis) => format!(" not named {}", card.name),
        Some(SameRef::SameNameAs(_)) => " with the same name as it".to_string(),
        Some(SameRef::SharesColorWith(_)) => format!(
            " that {} a color with it",
            if plural_form { "share" } else { "shares" }
        ),
    }
}

/// "card named ~", "cards named ~", "creature card": a filter on cards
/// (in a library or graveyard) as a noun phrase, the name after the noun.
fn cards_phrase(f: &Filter, card: &Card, plural_form: bool) -> String {
    let mut g = f.clone();
    g.same = None;
    // "creature card with mana value 3 or less": the mana value after the
    // noun.
    let mut mv = String::new();
    if g.or.is_none()
        && let Some((cmp, n)) = g.mana_value.take()
    {
        let n = if g.status.mana_value_x {
            "X".to_string()
        } else {
            n.to_string()
        };
        g.status.mana_value_x = false;
        mv = format!(" with mana value {n}{}", cmp_words(cmp));
    }
    let p = if g == Filter::default() {
        String::new()
    } else {
        format!("{} ", card_filter_phrase(&g, card))
    };
    let noun = if plural_form { "cards" } else { "card" };
    // "a card with flash": only qualities after the noun.
    if let Some(after) = p.trim().strip_prefix("with ") {
        return format!(
            "{noun} with {after}{mv}{}",
            same_phrase(f, card, plural_form)
        );
    }
    format!("{p}{noun}{mv}{}", same_phrase(f, card, plural_form))
}

/// What "Choose ..." chooses: "a creature type other than Wall", "a
/// color", "a basic land type".
fn choice_noun(k: &ChoiceKind) -> String {
    match k {
        ChoiceKind::Color { except: None } => "a color".to_string(),
        ChoiceKind::Colors => "one or more colors".to_string(),
        ChoiceKind::Color { except: Some(c) } => {
            format!("a color other than {}", format!("{c:?}").to_lowercase())
        }
        ChoiceKind::CreatureType { except: None } => "a creature type".to_string(),
        ChoiceKind::CreatureType { except: Some(t) } => {
            format!("a creature type other than {t}")
        }
        ChoiceKind::LandType(lands) if lands.len() == 5 => "a basic land type".to_string(),
        ChoiceKind::LandType(lands) => or_list(
            &lands
                .iter()
                .map(|c| basic_land_name(*c).to_string())
                .collect::<Vec<_>>(),
        ),
    }
}

/// A filter as a kind of card, with an article: "a creature card", "a
/// nonblack card".
fn card_filter_a(f: &Filter, card: &Card) -> String {
    // "a permanent card", "a Rebel permanent card" (CR 110.4a).
    if f.or.is_none() && f.not_types == [CardType::Instant, CardType::Sorcery] {
        return with_article(&format!("{} card", card_filter_phrase(f, card)));
    }
    let p = filter_phrase(f, card, false);
    let p = p.strip_suffix(" permanent").unwrap_or(&p);
    let p = if p == "permanent" { "" } else { p };
    if p.is_empty() {
        "a card".to_string()
    } else if f.or.is_none()
        && let Some((kind, quality)) = p.split_once(" with ")
    {
        // "a creature card with power 2 or less".
        with_article(&format!("{kind} card with {quality}"))
    } else {
        with_article(&format!("{p} card"))
    }
}

/// A filter with an article: "a creature you control", "another creature".
fn filter_a(f: &Filter, card: &Card) -> String {
    let p = filter_phrase(f, card, false);
    // "When enchanted creature dies": the one it's attached to, no article.
    if f.enchanted && f.or.is_none() {
        return p;
    }
    if f.relation == Relation::Other {
        return format!("another {}", p.trim_start_matches("other "));
    }
    with_article(&p)
}

// ---- costs ----

fn cost_text(cost: &Cost, ctx: &mut Ctx) -> String {
    if let Some(n) = cost.loyalty {
        return if n >= 0 {
            format!("+{n}")
        } else {
            format!("−{}", -n)
        };
    }
    let default_order = [
        CostItem::Mana,
        CostItem::Tap,
        CostItem::Sacrifice,
        CostItem::Discard,
        CostItem::Life,
    ];
    let order: &[CostItem] = if cost.order.is_empty() {
        &default_order
    } else {
        &cost.order
    };
    let mut parts = Vec::new();
    for item in order {
        match item {
            CostItem::Mana => {
                let mana = cost.mana.to_string();
                if !mana.is_empty() {
                    parts.push(mana);
                }
            }
            CostItem::Tap => {
                if cost.tap {
                    parts.push("{T}".to_string());
                }
            }
            CostItem::Sacrifice => match &cost.sacrifice {
                Some(SacCost::This) => parts.push(format!("Sacrifice {}", ctx.source())),
                Some(SacCost::Filter(f)) => parts.push(match cost.chosen {
                    mtg_core::card::ChosenCost::Sacrifice => {
                        format!("Sacrifice {}", filter_a(f, ctx.card))
                    }
                    mtg_core::card::ChosenCost::Return => {
                        format!("Return {} to its owner's hand", filter_a(f, ctx.card))
                    }
                    mtg_core::card::ChosenCost::Tap => format!("Tap {}", filter_a(f, ctx.card)),
                }),
                None => {}
            },
            CostItem::RemoveCounters => {
                if let Some((k, n)) = cost.remove_counters {
                    let what = counters(Num::N(n), k);
                    let src = ctx.source();
                    parts.push(format!("Remove {what} from {src}"));
                }
            }
            CostItem::FromGraveyard => {}
            CostItem::Exile => {
                if cost.exile_self {
                    parts.push("Exile this card from your graveyard".to_string());
                }
            }
            CostItem::Discard => {
                if cost.discard_self {
                    parts.push("Discard this card".to_string());
                } else if cost.discard_x {
                    parts.push("Discard X cards".to_string());
                } else if let Some(f) = &cost.discard_filter {
                    parts.push(format!("Discard {}", card_filter_a(f, ctx.card)));
                } else if cost.discard > 0 {
                    parts.push(format!(
                        "Discard {} {}",
                        count_words(Num::N(cost.discard), "a"),
                        plural(Num::N(cost.discard), "card")
                    ));
                }
            }
            CostItem::Life => {
                if cost.life > 0 {
                    parts.push(format!("Pay {} life", cost.life));
                }
            }
            CostItem::Energy => parts.push(format!("Pay {}", energy(cost.energy))),
        }
    }
    parts.join(", ")
}

// ---- effects ----

/// One clause of a sentence.
struct Clause {
    subject: Option<(Recipient, String)>,
    text: String,
    suffix: Option<&'static str>,
    /// After the suffix: ", where X is ...", " for each ...".
    tail: String,
}

impl Clause {
    fn imperative(text: String) -> Self {
        Clause {
            subject: None,
            text,
            suffix: None,
            tail: String::new(),
        }
    }

    fn with_subject(r: Recipient, subject: String, text: String) -> Self {
        Clause {
            subject: Some((r, subject)),
            text,
            suffix: None,
            tail: String::new(),
        }
    }

    fn until_eot(mut self) -> Self {
        self.suffix = Some("until end of turn");
        self
    }
}

const EOT: &str = "until end of turn";

/// "{E}{E}{E}".
fn energy(n: u32) -> String {
    "{E}".repeat(n as usize)
}

fn counter_name(k: CounterKind) -> &'static str {
    k.name()
}

fn counters(n: Num, k: CounterKind) -> String {
    // "an oil counter", "a stun counter".
    let a = if n == Num::N(1) && article(counter_name(k)) == "an" {
        "an"
    } else {
        "a"
    };
    format!(
        "{} {} {}",
        count_words(n, a),
        counter_name(k),
        plural(n, "counter")
    )
}

fn keywords_list(ks: &[Keyword]) -> String {
    list(
        &ks.iter()
            .filter(|k| !is_restriction(**k))
            .map(|k| keyword_name(*k).to_string())
            .collect::<Vec<_>>(),
    )
}

#[allow(clippy::too_many_lines)] // one part of the description after another
fn token_description(name: &str, count: Num, tapped: bool, ctx: &Ctx) -> String {
    let tap = if tapped { "tapped " } else { "" };
    let Some(token) = ctx.lookup.get(name) else {
        return format!(
            "{} {tap}{name} {}",
            count_words(count, "a"),
            plural(count, "token")
        );
    };
    // Predefined tokens (CR 111.10: Treasure, Food, Clue, ...) are named,
    // not described: "create a Treasure token", "create a tapped
    // Powerstone token".
    if token.pt.is_none()
        && !token.types.contains(&CardType::Creature)
        && token.subtypes.len() == 1
        && token.subtypes[0] == token.display_name.as_deref().unwrap_or(&token.name)
        && token.keywords.is_empty()
    {
        let sub = &token.subtypes[0];
        return format!(
            "{} {tap}{sub} {}",
            count_words(count, if tapped { "a" } else { article(sub) }),
            plural(count, "token")
        );
    }
    let mut words: Vec<String> = Vec::new();
    if tapped {
        words.push("tapped".to_string());
    }
    let has_cda_pt = token.abilities.iter().any(|a| {
        matches!(a, Ability::Static { mods, .. } if mods.iter().any(|m| matches!(m, Mod::CdaPt { .. })))
    });
    if let Some((p, t)) = token.pt
        && !has_cda_pt
    {
        words.push(format!("{p}/{t}"));
    }
    if let Some(colors) = &token.color_indicator {
        if colors.is_empty() {
            words.push("colorless".to_string());
        } else {
            words.push(
                colors
                    .iter()
                    .map(|c| color_name(*c))
                    .collect::<Vec<_>>()
                    .join(" and "),
            );
        }
    } else {
        words.push("colorless".to_string());
    }
    words.extend(token.subtypes.iter().cloned());
    words.extend(token.types.iter().map(|t| type_noun(*t).to_string()));
    let mut s = format!(
        "{} {} {}",
        count_words(count, article(&words[0])),
        words.join(" "),
        plural(count, "token")
    );
    // A creature token whose name is not its subtypes is named (CR
    // 111.4): "a 2/2 colorless Wizard Soldier creature token named Cadet".
    let shown = token.display_name.as_deref().unwrap_or(&token.name);
    let named = (token.types.contains(&CardType::Creature) && shown != token.subtypes.join(" "))
        .then(|| format!(" named {shown}"));
    // Keywords and keyword abilities print bare ("with flying", "with
    // toxic 1"); restrictions and other abilities print quoted ("with
    // \"This token can't block.\"").
    let mut bare: Vec<String> = token
        .keywords
        .iter()
        .filter(|k| !is_restriction(**k))
        .map(|k| keyword_name(*k).to_string())
        .collect();
    let mut quoted: Vec<String> = token
        .keywords
        .iter()
        .filter(|k| is_restriction(**k))
        .map(|k| keyword_line(token, *k))
        .collect();
    for a in &token.abilities {
        match a {
            Ability::Mechanic(crate::ast::Mechanic::Toxic(n)) => bare.push(format!("toxic {n}")),
            Ability::Keyword(k) if !is_restriction(*k) => {
                bare.push(keyword_name(*k).to_string());
            }
            _ => quoted.push(ability_text(token, a, ctx.lookup)),
        }
    }
    // Keywords alone print before the name ("a 1/1 green Insect creature
    // token with flying named Butterfly"); quoted abilities after it.
    if let Some(named) = &named
        && !(quoted.is_empty() && !bare.is_empty() && !has_cda_pt)
    {
        s.push_str(named);
    }
    if has_cda_pt {
        // "create a white Avatar creature token. It has \"This token's
        // power and toughness are each equal to your life total.\""
        if !bare.is_empty() {
            let _ = write!(s, " with {}", list(&bare));
        }
        if !quoted.is_empty() {
            let _ = write!(s, ". It has \"{}\"", quoted.join("\n"));
        }
        return s;
    }
    let keywords_only = quoted.is_empty() && !bare.is_empty();
    let parts: Vec<String> = bare
        .into_iter()
        .chain(quoted.into_iter().map(|q| format!("\"{q}\"")))
        .collect();
    if !parts.is_empty() {
        let _ = write!(s, " with {}", list(&parts));
    }
    if keywords_only && let Some(named) = named {
        s.push_str(&named);
    }
    s
}

fn except_text(card: &Card, current: Option<&Ability>) -> String {
    let Some(x) = &card.except else {
        return String::new();
    };
    let mut parts = Vec::new();
    if !x.subtypes.is_empty() {
        parts.push(format!(
            "it's {} in addition to its other types",
            with_article(&x.subtypes.join(" "))
        ));
    }
    if !x.keywords.is_empty() {
        parts.push(format!("it has {}", keywords_list(&x.keywords)));
    }
    for a in &x.abilities {
        if current == Some(a) {
            parts.push("it has this ability".to_string());
        } else {
            parts.push(format!("it has \"{}\"", ability_text(card, a, &())));
        }
    }
    list(&parts)
}

#[allow(clippy::too_many_lines)]
/// "+X", "-X", or "+0" for a scaled pump's part.
/// One half of "+X/+0": a zero takes the other half's sign ("-X/-0").
fn x_sign(v: i32, other: i32) -> &'static str {
    match v.cmp(&0) {
        std::cmp::Ordering::Equal if other < 0 => "-0",
        std::cmp::Ordering::Equal => "+0",
        std::cmp::Ordering::Greater => "+X",
        std::cmp::Ordering::Less => "-X",
    }
}

/// The variable amounts an effect uses.
fn nums_of(e: &Effect) -> Vec<Num> {
    match e {
        Effect::DealDamage { amount, .. }
        | Effect::DealDamageDivided { amount, .. }
        | Effect::GainLife { amount, .. }
        | Effect::LoseLife { amount, .. } => vec![*amount],
        Effect::DrawCards { count, .. }
        | Effect::Mill { count, .. }
        | Effect::Discard { count, .. }
        | Effect::AddCounters { count, .. }
        | Effect::DistributeCounters { count, .. }
        | Effect::AddManaTimes { count, .. }
        | Effect::CreateToken { count, .. } => vec![*count],
        Effect::AddCounterList { counters, .. } => counters.iter().map(|(_, n)| *n).collect(),
        Effect::CounterUnlessPays { x, .. } => vec![*x],
        Effect::Scry(n) => vec![*n],
        Effect::CreateReplacement(r) => match r.outcome {
            mtg_core::card::Replacement::PreventNext(n) => vec![n],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// "the number of creatures you control" (or "for each creature you
/// control" with `each`).
#[allow(clippy::too_many_lines)] // one arm per count
fn count_phrase(c: &crate::ast::CountExpr, card: &Card, each: bool) -> String {
    use crate::ast::CountExpr;
    if let CountExpr::Times(_, inner) = c {
        return count_phrase(inner, card, each);
    }
    let (one, many) = match c {
        // "the greatest power among creatures you control".
        CountExpr::GreatestPower(f) => {
            return format!("the greatest power among {}", filter_phrase(f, card, true));
        }
        CountExpr::GreatestPowerInGraveyard(f) => {
            return format!(
                "the greatest power among {} in your graveyard",
                cards_phrase(f, card, true)
            );
        }
        // "for each Zubera that died this turn".
        CountExpr::DiedThisTurn(f) => (
            format!("{} that died this turn", filter_phrase(f, card, false)),
            format!("{} that died this turn", filter_phrase(f, card, true)),
        ),
        // "creatures named ~ on the battlefield" (`named_first`), "other
        // creature on the battlefield named ~".
        CountExpr::Permanents(f) if f.who.is_none() && f.same.is_some() && !f.named_first => {
            let mut g = f.clone();
            g.same = None;
            let place = " on the battlefield";
            (
                format!(
                    "{}{place}{}",
                    filter_phrase(&g, card, false),
                    same_phrase(f, card, false)
                ),
                format!(
                    "{}{place}{}",
                    filter_phrase(&g, card, true),
                    same_phrase(f, card, true)
                ),
            )
        }
        CountExpr::Permanents(f) => {
            // "for each other attacking creature": attacking and blocking
            // creatures are on the battlefield without saying so.
            let place = if f.who.is_none() && !f.attacking && !f.blocking && f.attached_to.is_none()
            {
                " on the battlefield"
            } else {
                ""
            };
            (
                format!("{}{place}", filter_phrase(f, card, false)),
                format!("{}{place}", filter_phrase(f, card, true)),
            )
        }
        CountExpr::Domain => (
            "basic land type among lands you control".to_string(),
            "basic land types among lands you control".to_string(),
        ),
        CountExpr::CardsInHand => (
            "card in your hand".to_string(),
            "cards in your hand".to_string(),
        ),
        CountExpr::CardsInGraveyard(f) => match f {
            None => (
                "card in your graveyard".to_string(),
                "cards in your graveyard".to_string(),
            ),
            Some(f) => {
                let (mut one, mut many) =
                    (cards_phrase(f, card, false), cards_phrase(f, card, true));
                // "the number of instant and sorcery cards in your
                // graveyard".
                if !f.any_types.is_empty() {
                    one = one.replace(" or ", " and ");
                    many = many.replace(" or ", " and ");
                }
                (
                    format!("{one} in your graveyard"),
                    format!("{many} in your graveyard"),
                )
            }
        },
        // "cards named ~ in all graveyards", "for each card named ~ in each
        // graveyard".
        CountExpr::CardsInGraveyards { filter, each } => {
            let f = filter.clone().unwrap_or_default();
            let place = if *each {
                "each graveyard"
            } else {
                "all graveyards"
            };
            (
                format!("{} in {place}", cards_phrase(&f, card, false)),
                format!("{} in {place}", cards_phrase(&f, card, true)),
            )
        }
        CountExpr::Plus { n, word, inner } => {
            let n = if *word {
                count_words(Num::N(*n), "one")
            } else {
                n.to_string()
            };
            return format!("{n} plus {}", count_phrase(inner, card, each));
        }
        CountExpr::SourcePower => {
            return if each {
                "for each power".to_string()
            } else {
                "its power".to_string()
            };
        }
        CountExpr::TargetStat {
            toughness, that, ..
        } => {
            let stat = if *toughness { "toughness" } else { "power" };
            let whose = if *that { "that creature's" } else { "its" };
            return format!("{whose} {stat}");
        }
        CountExpr::Life => return "your life total".to_string(),
        CountExpr::BlockingIt { beyond } => {
            let rest = match beyond {
                0 => String::new(),
                1 => " beyond the first".to_string(),
                n => format!(" beyond the {}", count_words(Num::N(*n), "one")),
            };
            (
                format!("creature blocking it{rest}"),
                format!("creatures blocking it{rest}"),
            )
        }
        CountExpr::Times(..) => unreachable!("unwrapped above"),
        // "for each enchantment destroyed this way".
        CountExpr::ThisWay { verb, filter } => {
            let verb = crate::ast::this_way_word(*verb);
            match filter {
                None => (
                    format!("card {verb} this way"),
                    format!("cards {verb} this way"),
                ),
                Some(f) => (
                    format!("{} {verb} this way", filter_phrase(f, card, false)),
                    format!("{} {verb} this way", filter_phrase(f, card, true)),
                ),
            }
        }
        CountExpr::LifeLostThisWay => return "the life lost this way".to_string(),
        CountExpr::Party => (
            "creature in your party".to_string(),
            "creatures in your party".to_string(),
        ),
        CountExpr::GreatestManaValue => {
            return "the greatest mana value among permanents you control".to_string();
        }
        CountExpr::CardsDrawnThisTurn => (
            "card you've drawn this turn".to_string(),
            "cards you've drawn this turn".to_string(),
        ),
        CountExpr::SourceToughness => return "its toughness".to_string(),
        CountExpr::TargetManaValue {
            that, the_spell, ..
        } => {
            let whose = if *the_spell {
                "the spell's"
            } else if *that {
                "that creature's"
            } else {
                "its"
            };
            return format!("{whose} mana value");
        }
        CountExpr::ColorsAmongPermanents => (
            "color among permanents you control".to_string(),
            "colors among permanents you control".to_string(),
        ),
        CountExpr::CardsInOpponentsGraveyards(f) => {
            let f = f.clone().unwrap_or_default();
            (
                format!(
                    "{} in your opponents' graveyards",
                    cards_phrase(&f, card, false)
                ),
                format!(
                    "{} in your opponents' graveyards",
                    cards_phrase(&f, card, true)
                ),
            )
        }
        CountExpr::OpponentsAttacked => (
            "opponent you're attacking".to_string(),
            "opponents you're attacking".to_string(),
        ),
        CountExpr::AttackedWithThisTurn => (
            "creature you attacked with this turn".to_string(),
            "creatures you attacked with this turn".to_string(),
        ),
        CountExpr::SpentColors | CountExpr::SpentTreasure => {
            let it = if is_instant_or_sorcery(card) {
                "this spell"
            } else {
                "it"
            };
            if matches!(c, CountExpr::SpentColors) {
                (
                    format!("color of mana spent to cast {it}"),
                    format!("colors of mana spent to cast {it}"),
                )
            } else {
                (
                    format!("mana from a Treasure spent to cast {it}"),
                    format!("mana from Treasures spent to cast {it}"),
                )
            }
        }
        // "for each artifact and/or creature card in your graveyard".
        CountExpr::AndOr(inner) => {
            return count_phrase(inner, card, each).replacen(" and ", " and/or ", 1);
        }
        CountExpr::TriggerStat { stat, that } => {
            let whose = if *that { "that creature's" } else { "its" };
            return format!("{whose} {}", stat_word(*stat));
        }
        // "the sacrificed creature's toughness", "that creature's
        // toughness".
        CountExpr::MovedStat {
            verb,
            stat,
            noun,
            that,
        } => {
            let whose = if *that {
                format!("that {noun}'s")
            } else {
                format!("the {} {noun}'s", crate::ast::this_way_word(*verb))
            };
            return format!("{whose} {}", stat_word(*stat));
        }
        CountExpr::TargetHand(_) => (
            "card in that player's hand".to_string(),
            "cards in that player's hand".to_string(),
        ),
        CountExpr::GreatestToughness(f) => {
            return format!(
                "the greatest toughness among {}",
                filter_phrase(f, card, true)
            );
        }
        CountExpr::Sum(a, b) => {
            return format!(
                "{} plus {}",
                count_phrase(a, card, false),
                count_phrase(b, card, false)
            );
        }
        CountExpr::LifeLostThisTurn => return "the life you've lost this turn".to_string(),
        CountExpr::DamageThisWay { excess } => {
            return if *excess {
                "the excess damage dealt this way".to_string()
            } else {
                "the damage dealt this way".to_string()
            };
        }
    };
    if each {
        format!("for each {one}")
    } else {
        format!("the number of {many}")
    }
}

/// The power, toughness, or mana value a count reads.
fn stat_word(s: mtg_core::card::Stat) -> &'static str {
    match s {
        mtg_core::card::Stat::Power => "power",
        mtg_core::card::Stat::Toughness => "toughness",
        mtg_core::card::Stat::ManaValue => "mana value",
    }
}

/// `count_phrase` with the clause's targets known: "that card's
/// toughness" for a card targeted in a graveyard, "that permanent's mana
/// value" for a target of several permanent types.
fn count_phrase_in(c: &crate::ast::CountExpr, ctx: &Ctx, each: bool) -> String {
    use crate::ast::CountExpr;
    let phrase = count_phrase(c, ctx.card, each);
    match c {
        CountExpr::TargetStat { n, that: true, .. }
        | CountExpr::TargetManaValue { n, that: true, .. }
            if ctx
                .target_noun
                .get(*n)
                .is_some_and(|t| t == "card" || t == "permanent") =>
        {
            phrase.replacen(
                "that creature's",
                &format!("that {}'s", ctx.target_noun[*n]),
                1,
            )
        }
        _ => phrase,
    }
}

fn clause(e: &Effect, ctx: &mut Ctx) -> Clause {
    use crate::ast::CountStyle;
    let mut c = clause_inner(e, ctx);
    for n in nums_of(e) {
        let Num::Count(i) = n else { continue };
        let Some(count) = ctx.card.counts.get(usize::from(i)) else {
            continue;
        };
        match count.style {
            // "deals damage equal to that creature's power to ...".
            CountStyle::EqualBefore => {
                let phrase = count_phrase_in(&count.expr, ctx, false);
                c.text = c
                    .text
                    .replacen("X damage", &format!("damage equal to {phrase}"), 1);
            }
            CountStyle::Where => {
                let _ = write!(
                    c.tail,
                    ", where X is {}",
                    count_phrase_in(&count.expr, ctx, false)
                );
            }
            CountStyle::Equal => {
                // "Create a number of 1/1 red Goblin creature tokens equal
                // to ...".
                c.text = if matches!(e, Effect::CreateToken { .. }) {
                    c.text.replacen("X ", "a number of ", 1)
                } else {
                    c.text.replacen("X ", "", 1)
                };
                let _ = write!(
                    c.tail,
                    " equal to {}",
                    count_phrase_in(&count.expr, ctx, false)
                );
            }
            // "then draw that many cards".
            CountStyle::ThatMany => c.text = c.text.replacen("X ", "that many ", 1),
            // "unless its controller pays {2} for each Wizard on the
            // battlefield", "{2} plus an additional {1} for each Faerie you
            // control".
            CountStyle::Each if matches!(e, Effect::CounterUnlessPays { .. }) => {
                let (pays, inner) = match &count.expr {
                    crate::ast::CountExpr::Times(n, inner) => (format!("{{{n}}}"), &**inner),
                    crate::ast::CountExpr::Plus { n, inner, .. } => {
                        (format!("{{{n}}} plus an additional {{1}}"), &**inner)
                    }
                    other => ("{1}".to_string(), other),
                };
                c.text = c.text.replacen("{X}", &pays, 1);
                let _ = write!(c.tail, " {}", count_phrase(inner, ctx.card, true));
            }
            CountStyle::Each => {
                // "gain 1 life for each", "deals 1 damage ... for each".
                let per = match &count.expr {
                    crate::ast::CountExpr::Times(n, _) => *n,
                    _ => 1,
                };
                c.text = c
                    .text
                    .replacen("X life", &format!("{per} life"), 1)
                    .replacen("X damage", &format!("{per} damage"), 1)
                    .replacen("X ", "a ", 1)
                    .replacen("a cards", "a card", 1);
                // "create a 2/2 black Zombie creature token for each ...".
                if matches!(e, Effect::CreateToken { .. }) {
                    c.text = c.text.replacen(" tokens", " token", 1);
                }
                // "put a +1/+1 counter on this creature for each ...".
                if matches!(e, Effect::AddCounters { .. }) {
                    c.text = c.text.replacen(" counters on ", " counter on ", 1);
                }
                let _ = write!(c.tail, " {}", count_phrase(&count.expr, ctx.card, true));
            }
        }
    }
    if let Effect::Counter { dest, .. } | Effect::CounterUnlessPays { dest, .. } = e {
        c.tail.push_str(match dest {
            crate::ast::CounteredTo::Graveyard => "",
            crate::ast::CounteredTo::Exile => {
                ". If that spell is countered this way, exile it instead of putting it into its owner's graveyard"
            }
            crate::ast::CounteredTo::LibraryTop => {
                ". If that spell is countered this way, put it on top of its owner's library instead of into that player's graveyard"
            }
        });
    }
    c
}

#[allow(clippy::too_many_lines)] // one arm per effect kind
fn clause_inner(e: &Effect, ctx: &mut Ctx) -> Clause {
    match e {
        // "it deals 4 damage instead": to what the sentence before named.
        Effect::DealDamage { amount, to, .. } if to.is_empty() => {
            let s = ctx.source();
            Clause::with_subject(
                Recipient::Source,
                s,
                format!("deals {} damage", digits(*amount)),
            )
        }
        Effect::DealDamage { amount, to, by } => {
            let s = ctx.subj(*by);
            // "deals X damage to any target and X damage to itself".
            let objs = if *by == Recipient::Source && to.as_slice() == [Recipient::Source] {
                "itself".to_string()
            } else {
                ctx.obj_list(to, false)
            };
            Clause::with_subject(
                Recipient::Source,
                s,
                format!("deals {} damage to {objs}", digits(*amount)),
            )
        }
        Effect::PumpVar {
            power,
            toughness,
            times,
            to,
        } => {
            use crate::ast::CountStyle;
            let s = ctx.subj(*to);
            let count = match times {
                Num::Count(i) => ctx.card.counts.get(usize::from(*i)).cloned(),
                _ => None,
            };
            let (mods, tail) = match &count {
                Some(c) if c.style == CountStyle::Each => (
                    pt_mod(*power, *toughness),
                    format!(" {}", count_phrase(&c.expr, ctx.card, true)),
                ),
                Some(c) => (
                    format!(
                        "{}/{}",
                        x_sign(*power, *toughness),
                        x_sign(*toughness, *power)
                    ),
                    format!(", where X is {}", count_phrase(&c.expr, ctx.card, false)),
                ),
                None => (
                    format!(
                        "{}/{}",
                        x_sign(*power, *toughness),
                        x_sign(*toughness, *power)
                    ),
                    String::new(),
                ),
            };
            let mut c =
                Clause::with_subject(*to, s, format!("{} {mods}", ctx.verb(*to, "gets", "get")))
                    .until_eot();
            c.tail = tail;
            c
        }
        Effect::Pump {
            power,
            toughness,
            to,
        } => {
            let s = ctx.subj(*to);
            // "Up to two target creatures each get +1/+0".
            let each = if matches!(to, Recipient::Targets(..)) || ctx.is_many(*to) {
                "each "
            } else {
                ""
            };
            Clause::with_subject(
                *to,
                s,
                format!(
                    "{each}{} {}",
                    ctx.verb(*to, "gets", "get"),
                    pt_mod(*power, *toughness)
                ),
            )
            .until_eot()
        }
        Effect::Destroy { to, no_regen } => {
            let objs = ctx.obj_list(to, true);
            let mut text = format!("destroy {objs}");
            if *no_regen {
                let pronoun = if to.len() > 1 || to.iter().any(|r| Ctx::is_plural(*r)) {
                    "They"
                } else {
                    "It"
                };
                let _ = write!(text, ". {pronoun} can't be regenerated");
            }
            Clause::imperative(text)
        }
        Effect::GainLife { amount, who } => {
            // "Any number of target players each gain 6 life."
            let each = if ctx.is_many(*who) { "each " } else { "" };
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{each}{} {} life",
                    ctx.verb(*who, "gains", "gain"),
                    digits(*amount)
                ),
            )
        }
        Effect::Proliferate => Clause::imperative("proliferate".to_string()),
        Effect::Manifest => Clause::imperative("manifest the top card of your library".to_string()),
        Effect::Adapt(n) => Clause::imperative(format!("adapt {n}")),
        Effect::Discover(n) => Clause::imperative(format!("discover {n}")),
        Effect::MoveSourceCounters { to } => {
            let o = ctx.obj(*to);
            Clause::imperative(format!("put its +1/+1 counters on {o}"))
        }
        Effect::CumulativeUpkeep(c) => Clause::imperative(format!(
            "put an age counter on this permanent, then sacrifice it unless you pay {c} for each age counter on it"
        )),
        Effect::Connive { what } => {
            let s = ctx.subj(*what);
            Clause::with_subject(*what, s, "connives".to_string())
        }
        Effect::ExilePlayable {
            count,
            until,
            first,
            it,
        } => {
            use mtg_core::card::PlayUntil;
            let one = *count == Num::N(1);
            let head = if one {
                "exile the top card of your library".to_string()
            } else {
                format!(
                    "exile the top {} cards of your library",
                    count_words(*count, "one")
                )
            };
            let what = match (one, *it) {
                (true, true) => "it",
                (true, false) => "that card",
                (false, _) => "those cards",
            };
            let body = if *first {
                let when = match until {
                    PlayUntil::ThisTurn => "Until end of turn",
                    PlayUntil::EndOfYourNextTurn => "Until the end of your next turn",
                    PlayUntil::YourNextEndStep => "Until your next end step",
                };
                format!("{when}, you may play {what}")
            } else {
                let when = match until {
                    PlayUntil::ThisTurn => "this turn",
                    PlayUntil::EndOfYourNextTurn => "until the end of your next turn",
                    PlayUntil::YourNextEndStep => "until your next end step",
                };
                format!("You may play {what} {when}")
            };
            Clause::imperative(format!("{head}. {body}"))
        }
        Effect::LookAtTop(n) => Clause::imperative(format!(
            "look at the top {} cards of your library, then put them back in any order",
            count_words(*n, "one")
        )),
        Effect::Dig {
            count,
            filter,
            optional,
            all,
            rest_graveyard,
            reveal_top,
            revealing,
            take,
            also,
            random,
            then,
        } => {
            let n = count_words(*count, "one");
            let head = if *reveal_top {
                format!("reveal the top {n} cards of your library")
            } else {
                format!("look at the top {n} cards of your library")
            };
            let two = *count == Num::N(2);
            let rest = |word: &str| {
                if *rest_graveyard {
                    format!("{word} into your graveyard")
                } else if *random {
                    format!("{word} on the bottom of your library in a random order")
                } else if two {
                    format!("{word} on the bottom of your library")
                } else {
                    format!("{word} on the bottom of your library in any order")
                }
            };
            let several = *take != 1 || also.is_some();
            // "a creature card and/or a land card", "up to two instant
            // and/or sorcery cards", "any number of creature cards".
            let what = filter.as_ref().map(|f| {
                if let Some(a) = also {
                    format!(
                        "{} and/or {}",
                        card_filter_a(f, ctx.card),
                        card_filter_a(a, ctx.card)
                    )
                } else if *take == 1 {
                    card_filter_a(f, ctx.card)
                } else {
                    let kind = card_filter_phrase(f, ctx.card).replace(" or ", " and/or ");
                    if *take == crate::ast::ANY_NUMBER {
                        format!("any number of {kind} cards")
                    } else {
                        format!(
                            "{}{} {kind} cards",
                            if *optional { "up to " } else { "" },
                            count_words(Num::N(*take), "one")
                        )
                    }
                }
            });
            let it = if several { "the revealed cards" } else { "it" };
            if filter.is_none() && *take != 1 {
                // "Put two of those cards into your hand and the rest into
                // your graveyard."
                let k = count_words(Num::N(*take), "one");
                return Clause::imperative(format!(
                    "{head}. Put {k} of those cards into your hand and {}",
                    rest("the rest")
                ));
            }
            let body = match (what, all, optional) {
                (Some(w), false, true)
                    if *take > 1 && *take != crate::ast::ANY_NUMBER && !*revealing =>
                {
                    format!(
                        "Put {w} from among them into your hand and {}",
                        rest("the rest")
                    )
                }
                (Some(w), true, _) => {
                    let plural = w
                        .trim_start_matches("a ")
                        .trim_start_matches("an ")
                        .replacen(" card", " cards", 1);
                    format!(
                        "Put all {plural} revealed this way into your hand and {}",
                        rest("the rest")
                    )
                }
                (Some(w), false, true) if *revealing => format!(
                    "You may reveal {w} from among them and put {it} into your hand. Put {}",
                    rest("the rest")
                ),
                (Some(w), false, true) => format!(
                    "You may put {w} from among them into your hand. Put {}",
                    rest("the rest")
                ),
                (Some(w), false, false) => format!(
                    "Put {w} from among them into your hand and {}",
                    rest("the rest")
                ),
                (None, _, _) => format!(
                    "Put one of them into your hand and {}",
                    rest(if two { "the other" } else { "the rest" })
                ),
            };
            if *then {
                // "look at the top three cards of your library, then put
                // one of them into your hand and the rest into your
                // graveyard".
                let mut b = body;
                if let Some(c) = b.get_mut(0..1) {
                    c.make_ascii_lowercase();
                }
                return Clause::imperative(format!("{head}, then {b}"));
            }
            Clause::imperative(format!("{head}. {body}"))
        }
        Effect::Modes(modes) => {
            let mut out = "choose one —".to_string();
            for m in modes {
                let mut inner = Ctx::new(ctx.card, ctx.lookup, &m.targets);
                inner.current = ctx.current;
                out.push_str("\n• ");
                push_mode_name(&mut out, m);
                out.push_str(&sentences(&m.effects, &mut inner));
            }
            Clause::imperative(out)
        }
        Effect::GrantAbility { ability, to } => {
            let s = ctx.subj(*to);
            // "Target creature can't be blocked this turn except by
            // artifact creatures and/or red creatures."
            if let Ability::BlockRule(r) = ability.as_ref() {
                return Clause::with_subject(*to, s, block_rule_phrase(r, ctx.card, true));
            }
            let q = quoted(ability, ctx.card, CardType::Creature, ctx.lookup);
            Clause::with_subject(*to, s, format!("{} {q}", ctx.verb(*to, "gains", "gain")))
                .until_eot()
        }
        Effect::ReturnChosen { count, filter } => {
            let what = if *count == Num::N(1) {
                filter_a(filter, ctx.card)
            } else {
                format!(
                    "{} {}",
                    count_words(*count, "a"),
                    filter_phrase(filter, ctx.card, true)
                )
            };
            Clause::imperative(format!("return {what} to its owner's hand"))
        }
        Effect::BlocksIfAble { blocker, attacker } => {
            let s = ctx.subj(*blocker);
            let a = ctx.obj(*attacker);
            Clause::with_subject(*blocker, s, format!("blocks {a} this turn if able"))
        }
        Effect::SkipNextUntap { to } => {
            let s = match *to {
                // "Tap up to two target creatures. Those creatures don't
                // untap ...".
                Recipient::Targets(a, _) if ctx.target_mentioned[a] => {
                    format!("those {}", ctx.plural_target_noun(a))
                }
                // "Each attacking creature doesn't untap ..." (Tangle).
                Recipient::Group(i) if !ctx.groups_mentioned.contains(to) => {
                    ctx.groups_mentioned.insert(*to);
                    format!("each {}", ctx.group_phrase(i, false))
                }
                r => ctx.subj(r),
            };
            let plural =
                !s.starts_with("each ") && (Ctx::is_plural(*to) || ctx.is_many(*to) || s == "they");
            let text = if plural {
                "don't untap during their controller's next untap step"
            } else {
                "doesn't untap during its controller's next untap step"
            };
            Clause::with_subject(*to, s, text.to_string())
        }
        Effect::SwitchPt { to } => {
            let o = ctx.obj(*to);
            Clause::imperative(format!("switch {o}'s power and toughness")).until_eot()
        }
        Effect::PutFromHand { filter, tapped } => {
            let what = if *filter == Filter::default() {
                "a card".to_string()
            } else {
                with_article(&format!("{} card", card_filter_phrase(filter, ctx.card)))
            };
            Clause::imperative(format!(
                "put {what} from your hand onto the battlefield{}",
                if *tapped { " tapped" } else { "" }
            ))
        }
        Effect::Explore { what } => {
            let s = ctx.subj(*what);
            Clause::with_subject(*what, s, "explores".to_string())
        }
        Effect::Amass { kind, count } => {
            Clause::imperative(format!("amass {kind} {}", digits(*count)))
        }
        Effect::ClearEcho => Clause::imperative(String::new()),
        Effect::AddEnergy { count, who } => {
            let s = ctx.subj(*who);
            let n = match count {
                Num::N(n) => *n,
                _ => 1,
            };
            Clause::with_subject(
                *who,
                s,
                format!("{} {}", ctx.verb(*who, "gets", "get"), energy(n)),
            )
        }
        Effect::PayEnergy(n) => Clause::imperative(format!("pay {}", energy(*n))),
        Effect::AddPoison { count, who } => {
            let s = ctx.subj(*who);
            let what = if *count == Num::N(1) {
                "a poison counter".to_string()
            } else {
                format!("{} poison counters", count_words(*count, "a"))
            };
            Clause::with_subject(*who, s, format!("{} {what}", ctx.verb(*who, "gets", "get")))
        }
        Effect::LoseLife { amount, who } => {
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{} {} life",
                    ctx.verb(*who, "loses", "lose"),
                    digits(*amount)
                ),
            )
        }
        Effect::DrawCards { count, who } => {
            let additional = matches!(
                ctx.trigger,
                Some(Trigger::BeginningOfStep {
                    step: Step::Draw,
                    ..
                })
            ) && *count == Num::N(1)
                && *who == Some(Recipient::TriggeringPlayer);
            let what = if additional {
                "an additional card".to_string()
            } else {
                format!("{} {}", count_words(*count, "a"), plural(*count, "card"))
            };
            match who {
                None => Clause::imperative(format!("draw {what}")),
                Some(w) => {
                    let s = ctx.subj(*w);
                    Clause::with_subject(*w, s, format!("{} {what}", ctx.verb(*w, "draws", "draw")))
                }
            }
        }
        Effect::Mill { count, who } => {
            let what = format!("{} {}", count_words(*count, "a"), plural(*count, "card"));
            match who {
                None => Clause::imperative(format!("mill {what}")),
                Some(w) => {
                    let s = ctx.subj(*w);
                    Clause::with_subject(*w, s, format!("{} {what}", ctx.verb(*w, "mills", "mill")))
                }
            }
        }
        Effect::AddCounters { kind, count, to } => {
            let c = counters(*count, *kind);
            let o = ctx.obj(*to);
            Clause::imperative(format!("put {c} on {o}"))
        }
        Effect::AddCounterList { counters: cs, to } => {
            let c: Vec<String> = cs.iter().map(|(k, n)| counters(*n, *k)).collect();
            let o = ctx.obj(*to);
            Clause::imperative(format!("put {} on {o}", list(&c)))
        }
        Effect::RemoveCounters { kind, count, to } => {
            let c = counters(*count, *kind);
            let o = ctx.obj(*to);
            Clause::imperative(format!("remove {c} from {o}"))
        }
        // CR 601.2d.
        Effect::DealDamageDivided { amount, to } => {
            let s = ctx.source();
            let objs = ctx.obj(*to);
            Clause::with_subject(
                Recipient::Source,
                s,
                format!(
                    "deals {} damage divided as you choose among {objs}",
                    digits(*amount)
                ),
            )
        }
        Effect::DistributeCounters { kind, count, to } => {
            let c = counters(*count, *kind);
            let o = ctx.obj(*to);
            Clause::imperative(format!("distribute {c} among {o}"))
        }
        Effect::RemoveAllCounters { to } => {
            let o = ctx.obj(*to);
            Clause::imperative(format!("remove all counters from {o}"))
        }
        Effect::ReturnToHand { what } => {
            let yours = what.iter().all(|w| {
                matches!(
                    w,
                    Recipient::Target(i) if matches!(ctx.targets[*i].base(),
                        Target::Spec(TargetSpec::CreatureCardInYourGraveyard)
                        | Target::GraveyardCard { who: Who::You, .. })
                )
            });
            // "each other creature you control ... to its owner's hand";
            // only several recipients take "their owners' hands".
            let all = match what.as_slice() {
                [Recipient::Group(i)] => {
                    ctx.card.groups.get(usize::from(*i)).is_some_and(|f| f.all)
                }
                _ => false,
            };
            let plural = what.len() > 1 || all;
            let o = if all {
                ctx.obj_all(what[0])
            } else if what.len() == 1 {
                ctx.obj(what[0])
            } else {
                ctx.obj_list(what, false)
            };
            // Activated from the graveyard: "Return this card from your
            // graveyard to your hand."
            let from_graveyard = what.as_slice() == [Recipient::Source]
                && matches!(ctx.current, Some(Ability::Activated { cost, .. }) if cost.from_graveyard);
            if from_graveyard {
                return Clause::imperative(
                    "return this card from your graveyard to your hand".to_string(),
                );
            }
            let hand = match (yours, plural) {
                (true, _) => "your hand",
                (false, false) => "its owner's hand",
                (false, true) => "their owners' hands",
            };
            Clause::imperative(format!("return {o} to {hand}"))
        }
        Effect::ReturnToBattlefield {
            what,
            counters: c,
            tapped,
            under_you,
        } => {
            let o = match what {
                Recipient::SameObject | Recipient::CreatedObject => "that card".to_string(),
                other => ctx.obj(*other),
            };
            // A card from your own graveyard is yours: Oracle drops "under
            // its owner's control".
            let yours = matches!(what, Recipient::Target(i) if matches!(
                ctx.targets[*i].base(),
                Target::Spec(TargetSpec::CreatureCardInYourGraveyard)
                    | Target::GraveyardCard { who: Who::You, .. }
            ));
            let tapped = if *tapped { " tapped" } else { "" };
            let mut text = if *under_you {
                format!("return {o} to the battlefield{tapped} under your control")
            } else if yours {
                format!("return {o} to the battlefield{tapped}")
            } else {
                format!("return {o} to the battlefield{tapped} under its owner's control")
            };
            if let Some((kind, n)) = c {
                let _ = write!(text, " with {} on it", counters(Num::N(*n), *kind));
            }
            Clause::imperative(text)
        }
        Effect::Regenerate { to } => {
            let o = ctx.obj(*to);
            Clause::imperative(format!("regenerate {o}"))
        }
        Effect::Exile { what } => {
            let o = if what.len() == 1 {
                ctx.obj_all(what[0])
            } else {
                ctx.obj_list(what, true)
            };
            for w in what {
                if let Recipient::Target(i) = w {
                    ctx.target_noun[*i] = "card".to_string();
                }
            }
            Clause::imperative(format!("exile {o}"))
        }
        Effect::ExileAllGraveyards => Clause::imperative("exile all graveyards".to_string()),
        Effect::ExileGraveyard { who } => {
            let o = ctx.obj(*who);
            let whose = match o.as_str() {
                "you" => "your".to_string(),
                "it" | "them" => "their".to_string(),
                o => format!("{o}'s"),
            };
            Clause::imperative(format!("exile {whose} graveyard"))
        }
        Effect::Sacrifice { what } => {
            let o = ctx.obj(*what);
            Clause::imperative(format!("sacrifice {o}"))
        }
        Effect::SacrificeBy { who, what } => {
            let s = ctx.subj(*who);
            let o = ctx.obj(*what);
            let v = ctx.verb(*who, "sacrifices", "sacrifice");
            Clause::with_subject(*who, s, format!("{v} {o}"))
        }
        Effect::TapOrUntap { to } => {
            let o = ctx.obj(*to);
            Clause::imperative(format!("tap or untap {o}"))
        }
        Effect::Tap { to } => {
            let o = ctx.obj_list_tap(to);
            Clause::imperative(format!("tap {o}"))
        }
        Effect::Untap { to } => {
            let o = ctx.obj_list_tap(to);
            Clause::imperative(format!("untap {o}"))
        }
        Effect::Grant { keywords, to } => {
            if keywords.as_slice() == [Keyword::Lure] {
                let o = ctx.obj(*to);
                return Clause::imperative(lure_sentence(&o, true));
            }
            let s = ctx.subj(*to);
            if keywords.iter().any(|k| is_block_rule(*k))
                && keywords.iter().all(|k| is_restriction(*k))
            {
                // "must be blocked this turn if able": the turn inside.
                let parts: Vec<String> = keywords
                    .iter()
                    .map(|k| {
                        if *k == Keyword::MustAttack {
                            // "Those creatures attack this turn if able."
                            format!("{} this turn if able", ctx.verb(*to, "attacks", "attack"))
                        } else if is_block_rule(*k) {
                            block_rule_this_turn(*k).to_string()
                        } else {
                            format!("{} this turn", restriction_phrase(&[*k]))
                        }
                    })
                    .collect();
                return Clause::with_subject(*to, s, list(&parts));
            }
            if keywords.iter().all(|k| is_restriction(*k)) {
                let mut c = Clause::with_subject(*to, s, restriction_phrase(keywords));
                c.suffix = Some("this turn");
                return c;
            }
            Clause::with_subject(
                *to,
                s,
                format!(
                    "{} {}",
                    ctx.verb(*to, "gains", "gain"),
                    keywords_list(keywords)
                ),
            )
            .until_eot()
        }
        Effect::GainControl { of } => {
            let o = ctx.obj(*of);
            Clause::imperative(format!("gain control of {o}")).until_eot()
        }
        Effect::Attach { to } => {
            let o = ctx.obj(*to);
            // After creating a token, "it" is the token: Oracle names the
            // source again ("attach this Equipment to it").
            let me = if *to == Recipient::CreatedObject && !is_spell(ctx.card) {
                this_ref(ctx.card)
            } else {
                ctx.source()
            };
            Clause::imperative(format!("attach {me} to {o}"))
        }
        Effect::CreateToken {
            token,
            count,
            tapped,
        } => {
            if !*tapped && ctx.those_tokens.as_deref() == Some(token.as_str()) {
                return Clause::imperative(format!(
                    "create {} of those tokens",
                    count_words(*count, "one")
                ));
            }
            ctx.last_token = Some(token.clone());
            Clause::imperative(format!(
                "create {}",
                token_description(token, *count, *tapped, ctx)
            ))
        }
        Effect::CreateTokenCopy { of, except } => {
            let o = ctx.obj(*of);
            let mut text = format!("create a token that's a copy of {o}");
            if *except {
                let _ = write!(text, ", except {}", except_text(ctx.card, ctx.current));
            }
            Clause::imperative(text)
        }
        Effect::BecomeCopy { of, except } => {
            let s = ctx.source();
            let o = ctx.obj(*of);
            let mut text = format!("becomes a copy of {o}");
            if *except {
                let _ = write!(text, ", except {}", except_text(ctx.card, ctx.current));
            }
            Clause::with_subject(Recipient::Source, s, text)
        }
        // Where a countered spell goes is added in `clause`, after a count.
        Effect::Counter { to, .. } => {
            let o = ctx.obj(*to);
            Clause::imperative(format!("counter {o}"))
        }
        Effect::CounterUnlessPays { to, mana, .. } => {
            let o = ctx.obj(*to);
            Clause::imperative(format!("counter {o} unless its controller pays {mana}"))
        }
        Effect::Scry(n) => Clause::imperative(format!("scry {}", digits(*n))),
        Effect::Investigate => Clause::imperative("investigate".to_string()),
        Effect::PutInLibrary {
            what,
            place,
            wording,
        } => {
            use crate::ast::{LibraryPlace as P, LibraryWording as W};
            // A card from your own graveyard goes to "your library".
            let yours = matches!(*what, Recipient::Target(i)
                if matches!(ctx.targets.get(i).map(Target::base),
                    Some(Target::GraveyardCard { who: Who::You, .. }
                        | Target::Spec(TargetSpec::CreatureCardInYourGraveyard))));
            let ordinal = |n: u8| match n {
                2 => "second".to_string(),
                3 => "third".to_string(),
                n => format!("{n}th"),
            };
            if *wording == W::Put {
                let o = ctx.obj(*what);
                let whose = if yours { "your" } else { "its owner's" };
                return Clause::imperative(match place {
                    P::Top => format!("put {o} on top of {whose} library"),
                    P::FromTop(n) => {
                        format!("put {o} into {whose} library {} from the top", ordinal(*n))
                    }
                    P::Shuffle => format!("shuffle {o} into {whose} library"),
                    _ => format!("put {o} on the bottom of {whose} library"),
                });
            }
            // "Target creature's owner puts it ...", "The owner of target
            // nonland permanent puts it ...": the owner makes any choice.
            let owner = if *wording == W::OwnerOf {
                format!("the owner of {}", ctx.obj(*what))
            } else {
                match ctx.subj(*what).as_str() {
                    "it" => "its owner".to_string(),
                    s => format!("{s}'s owner"),
                }
            };
            let rest = match place {
                P::Top => "puts it on top of their library".to_string(),
                P::Bottom => "puts it on the bottom of their library".to_string(),
                P::TopOrBottom => {
                    "puts it on their choice of the top or bottom of their library".to_string()
                }
                P::FromTop(n) => format!("puts it into their library {} from the top", ordinal(*n)),
                P::FromTopOrBottom(n) => format!(
                    "puts it into their library {} from the top or on the bottom",
                    ordinal(*n)
                ),
                P::Shuffle => "shuffles it into their library".to_string(),
            };
            Clause::imperative(format!("{owner} {rest}"))
        }
        Effect::SacrificeChosen { who, count, filter } => {
            let s = ctx.subj(*who);
            let what = if *count == Num::N(1) {
                with_article(&filter_phrase(filter, ctx.card, false))
            } else {
                format!(
                    "{} {}",
                    count_words(*count, "a"),
                    filter_phrase(filter, ctx.card, true)
                )
            };
            // "Sacrifice a creature.": you choose.
            if *who == Recipient::You {
                return Clause::imperative(format!("sacrifice {what}"));
            }
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{} {what} of their choice",
                    ctx.verb(*who, "sacrifices", "sacrifice")
                ),
            )
        }
        Effect::Strip { keywords, to } => {
            let s = ctx.subj(*to);
            Clause::with_subject(
                *to,
                s,
                format!(
                    "{} {}",
                    ctx.verb(*to, "loses", "lose"),
                    keywords_list(keywords)
                ),
            )
            .until_eot()
        }
        Effect::LookAtHand { who } => {
            let o = ctx.obj(*who);
            let whose = match o.as_str() {
                "you" => "your".to_string(),
                "it" | "them" => "their".to_string(),
                o => format!("{o}'s"),
            };
            Clause::imperative(format!("look at {whose} hand"))
        }
        Effect::RevealHand { who } => {
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!("{} their hand", ctx.verb(*who, "reveals", "reveal")),
            )
        }
        Effect::DiscardRandom { count, who } => {
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{} {} at random",
                    ctx.verb(*who, "discards", "discard"),
                    if *count == Num::N(1) {
                        "a card".to_string()
                    } else {
                        format!("{} cards", count_words(*count, "a"))
                    }
                ),
            )
        }
        Effect::DiscardChosen {
            who,
            filter,
            may,
            look,
        } => {
            let what = match filter {
                None => "a card".to_string(),
                Some(f) => with_article(&format!("{} card", card_filter_phrase(f, ctx.card))),
            };
            let choose = match (*look, *may) {
                // "Look at target player's hand and choose a card from it."
                (true, false) => format!(" and choose {what} from it."),
                (_, true) => format!(". You may choose {what} from it."),
                (false, false) => format!(". You choose {what} from it."),
            };
            if *look {
                let o = ctx.obj(*who);
                let whose = match o.as_str() {
                    "you" => "your".to_string(),
                    "it" | "them" => "their".to_string(),
                    o => format!("{o}'s"),
                };
                return Clause::imperative(format!(
                    "look at {whose} hand{choose} That player discards that card"
                ));
            }
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{} their hand{choose} That player discards that card",
                    ctx.verb(*who, "reveals", "reveal")
                ),
            )
        }
        Effect::IfThisWay(cond, inner) => {
            let cond = this_way_text(cond, ctx.card);
            let body = sentences(inner, ctx);
            let body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            Clause::imperative(format!("if {cond}, {}", body.trim_end_matches('.')))
        }
        Effect::ChooseAct {
            act,
            count,
            up_to,
            filter,
            among_them,
            this_way,
        } => {
            use mtg_core::card::ChooseAct;
            let one = *count == Num::N(1) && !*up_to;
            let n = if *up_to {
                format!("up to {}", count_words(*count, "one"))
            } else {
                count_words(*count, "a")
            };
            let permanents = || {
                if one {
                    filter_a(filter, ctx.card)
                } else {
                    format!("{n} {}", filter_phrase(filter, ctx.card, true))
                }
            };
            let cards = || {
                if one {
                    with_article(&cards_phrase(filter, ctx.card, false))
                } else {
                    format!("{n} {}", cards_phrase(filter, ctx.card, true))
                }
            };
            Clause::imperative(match act {
                ChooseAct::Tap => format!("tap {}", permanents()),
                ChooseAct::Sacrifice => format!("sacrifice {}", permanents()),
                ChooseAct::Bounce => format!(
                    "return {} to {} owner's hand",
                    permanents(),
                    if one { "its" } else { "their" }
                ),
                ChooseAct::Discard => format!("discard {}", cards()),
                ChooseAct::ExileFromGraveyard => {
                    format!("exile {} from your graveyard", cards())
                }
                ChooseAct::ReturnFromGraveyard => {
                    format!("return {} from your graveyard to your hand", cards())
                }
                ChooseAct::MilledToHand => format!(
                    "put {} from among {} into your hand",
                    cards(),
                    if *among_them {
                        "them"
                    } else if *this_way {
                        "the cards milled this way"
                    } else {
                        "the milled cards"
                    }
                ),
            })
        }
        Effect::OneOf(options) => {
            // "sacrifice an artifact or discard a card"; "create a Food
            // token or a Treasure token" (the verb once).
            let texts: Vec<String> = options.iter().map(|o| clause(o, ctx).text).collect();
            let verb = texts[0].split(' ').next().unwrap_or_default().to_string();
            let same = texts
                .iter()
                .all(|t| t.split(' ').next() == Some(verb.as_str()));
            let mut out = texts[0].clone();
            for t in &texts[1..] {
                out.push_str(" or ");
                if same && verb == "create" {
                    out.push_str(t[verb.len()..].trim_start());
                } else {
                    out.push_str(t);
                }
            }
            Clause::imperative(out)
        }
        Effect::GrantChoice { to, keywords } => {
            let s = ctx.subj(*to);
            let ks: Vec<String> = keywords
                .iter()
                .map(|k| keywords_list(std::slice::from_ref(k)))
                .collect();
            Clause::with_subject(
                *to,
                s,
                format!(
                    "{} your choice of {}",
                    ctx.verb(*to, "gains", "gain"),
                    ks.join(" or ")
                ),
            )
            .until_eot()
        }
        Effect::DiscardUnless { who, count, filter } => {
            let what = format!("{} {}", count_words(*count, "a"), plural(*count, "card"));
            let alt = with_article(&cards_phrase(filter, ctx.card, false));
            match who {
                None => Clause::imperative(format!("discard {what} unless you discard {alt}")),
                Some(w) => {
                    let s = ctx.subj(*w);
                    Clause::with_subject(
                        *w,
                        s,
                        format!(
                            "{} {what} unless they discard {alt}",
                            ctx.verb(*w, "discards", "discard")
                        ),
                    )
                }
            }
        }
        Effect::DiscardAll {
            who,
            filter,
            all_cards,
        } => {
            let whose = if who.is_none_or(|w| w == Recipient::You) {
                "your"
            } else {
                "their"
            };
            let what = match (filter, all_cards) {
                (Some(f), _) => format!("all {}", cards_phrase(f, ctx.card, true)),
                (None, true) => format!("all the cards in {whose} hand"),
                (None, false) => format!("{whose} hand"),
            };
            match who {
                None => Clause::imperative(format!("discard {what}")),
                Some(w) => {
                    let s = ctx.subj(*w);
                    Clause::with_subject(
                        *w,
                        s,
                        format!("{} {what}", ctx.verb(*w, "discards", "discard")),
                    )
                }
            }
        }
        Effect::Transform { what } => {
            let o = ctx.obj(*what);
            Clause::imperative(format!("transform {o}"))
        }
        Effect::Search {
            filter,
            count,
            up_to,
            to,
            reveal,
            that_card,
            or_card,
        } => {
            let one = *count == Num::N(1) && !*up_to;
            let any = *count == Num::N(crate::ast::ANY_NUMBER);
            let what = if *or_card != crate::ast::OrCard::No
                && let Some(alt) = &filter.or
            {
                // "a basic land card or a Gate card", "up to two basic land
                // cards and/or Gate cards".
                let mut base = filter.clone();
                base.or = None;
                if one {
                    let a = cards_phrase(alt, ctx.card, false);
                    let a = if *or_card == crate::ast::OrCard::Article {
                        with_article(&a)
                    } else {
                        a
                    };
                    format!(
                        "{} or {a}",
                        with_article(&cards_phrase(&base, ctx.card, false))
                    )
                } else {
                    format!(
                        "{}{} {} and/or {}",
                        if *up_to { "up to " } else { "" },
                        count_words(*count, "a"),
                        cards_phrase(&base, ctx.card, true),
                        cards_phrase(alt, ctx.card, true)
                    )
                }
            } else if any {
                format!("any number of {}", cards_phrase(filter, ctx.card, true))
            } else if filter.same.is_some() {
                // "a card named ~", "up to three cards named ~".
                if one {
                    format!("a {}", cards_phrase(filter, ctx.card, false))
                } else {
                    format!(
                        "{}{} {}",
                        if *up_to { "up to " } else { "" },
                        count_words(*count, "a"),
                        cards_phrase(filter, ctx.card, true)
                    )
                }
            } else if filter == &Filter::default() {
                if one {
                    "a card".to_string()
                } else {
                    format!("{} cards", count_words(*count, "a"))
                }
            } else if one && filter.or.is_none() {
                // "a Rebel permanent card with mana value 3 or less".
                with_article(&cards_phrase(filter, ctx.card, false))
            } else if one {
                with_article(&format!("{} card", filter_phrase(filter, ctx.card, false)))
            } else {
                format!(
                    "{}{} {} cards",
                    if *up_to { "up to " } else { "" },
                    count_words(*count, "a"),
                    filter_phrase(filter, ctx.card, false)
                )
            };
            let (it, them) = if *that_card {
                ("that card", "those cards")
            } else {
                ("it", "them")
            };
            let mut pron = if one { it } else { them };
            let mut text = format!("search your library for {what}, ");
            // "reveal that card, put it into your hand", "reveal those
            // cards, put one onto the battlefield tapped".
            let reveal_that = *that_card
                && matches!(
                    to,
                    mtg_core::card::SearchTo::Hand | mtg_core::card::SearchTo::BattlefieldAndHand
                );
            if *reveal {
                let r = if reveal_that {
                    pron = if one { "it" } else { "them" };
                    if one { it } else { them }
                } else if one {
                    "it"
                } else {
                    "them"
                };
                let _ = write!(text, "reveal {r}, ");
            }
            match to {
                mtg_core::card::SearchTo::BattlefieldAndHand => {
                    text.push_str(
                        "put one onto the battlefield tapped and the other into your hand, then shuffle",
                    );
                }
                mtg_core::card::SearchTo::Hand => {
                    let _ = write!(text, "put {pron} into your hand, then shuffle");
                }
                mtg_core::card::SearchTo::Battlefield { tapped } => {
                    let _ = write!(
                        text,
                        "put {pron} onto the battlefield{}, then shuffle",
                        if *tapped { " tapped" } else { "" }
                    );
                }
                mtg_core::card::SearchTo::TopOfLibrary => {
                    let _ = write!(text, "then shuffle and put {it} on top");
                }
            }
            Clause::imperative(text)
        }
        Effect::AddMana(m) => {
            Clause::imperative(format!("add {}", mana_symbols(m, "one mana of any color")))
        }
        Effect::AddManaOneOf(m) => {
            let syms: Vec<String> = m.iter().map(|x| mana_symbols(&[*x], "?")).collect();
            Clause::imperative(format!("add {}", or_list(&syms)))
        }
        Effect::AddManaTimes {
            mana: Mana::Any,
            count: n @ Num::N(_),
        } => Clause::imperative(format!(
            "add {} mana of any one color",
            count_words(*n, "one")
        )),
        Effect::AddManaTimes { mana, .. } => Clause::imperative(format!(
            "add {}",
            mana_symbols(&[*mana], "X mana of any one color")
        )),
        Effect::AddManaCombination {
            count,
            colors,
            different,
        } => {
            let n = count_words(Num::N(*count), "one");
            Clause::imperative(if *different {
                format!("add {n} mana of different colors")
            } else if colors.count() == 5 {
                format!("add {n} mana in any combination of colors")
            } else {
                let syms: Vec<String> = colors
                    .iter()
                    .map(|c| format!("{{{}}}", c.letter()))
                    .collect();
                format!(
                    "add {n} mana in any combination of {}",
                    syms.join(" and/or ")
                )
            })
        }
        Effect::Surveil(n) => Clause::imperative(format!("surveil {}", digits(*n))),
        Effect::Discard { count, who } => {
            let what = format!("{} {}", count_words(*count, "a"), plural(*count, "card"));
            match who {
                None => Clause::imperative(format!("discard {what}")),
                Some(w) => {
                    let s = ctx.subj(*w);
                    Clause::with_subject(
                        *w,
                        s,
                        format!("{} {what}", ctx.verb(*w, "discards", "discard")),
                    )
                }
            }
        }
        Effect::Fight { a, b } => {
            let s = ctx.subj(*a);
            let o = ctx.obj(*b);
            Clause::with_subject(*a, s, format!("fights {o}"))
        }
        Effect::Bite { source, to, .. } if source == to => {
            let s = ctx.subj(*source);
            Clause::with_subject(
                *source,
                s,
                "deals damage to itself equal to its power".to_string(),
            )
        }
        Effect::Bite { source, to, twice } => {
            let s = ctx.subj(*source);
            let o = ctx.obj(*to);
            let twice = if *twice { "twice " } else { "" };
            // "Up to two target creatures you control each deal damage
            // equal to their power to ...".
            let group = matches!(source, Recipient::Targets(..)) || ctx.is_many(*source);
            let text = if group {
                format!("each deal damage equal to {twice}their power to {o}")
            } else {
                format!("deals damage equal to {twice}its power to {o}")
            };
            Clause::with_subject(*source, s, text)
        }
        Effect::CreateEmblem { emblem } => {
            let text = ctx.lookup.get(emblem).map(|e| oracle_text(e, ctx.lookup));
            Clause::with_subject(
                Recipient::You,
                "you".to_string(),
                match text {
                    Some(t) => format!("get an emblem with \"{t}\""),
                    None => "get an emblem".to_string(),
                },
            )
        }
        Effect::Animate {
            to,
            types,
            subtypes,
            power,
            toughness,
            colors,
        } => {
            let s = ctx.subj(*to);
            let mut words = vec![format!("{power}/{toughness}")];
            words.extend(colors.iter().map(|c| format!("{c:?}").to_lowercase()));
            words.extend(subtypes.iter().cloned());
            words.extend(types.iter().map(|t| type_noun(*t).to_string()));
            let mut text = format!(
                "{} {} {} {EOT}",
                ctx.verb(*to, "becomes", "become"),
                article(&words[0]),
                words.join(" ")
            );
            if ctx.card.types.contains(&CardType::Land) && !types.contains(&CardType::Land) {
                text.push_str(". It's still a land");
            }
            Clause::with_subject(*to, s, text)
        }
        Effect::BecomeColor { to, colors } => {
            let s = ctx.subj(*to);
            let c: Vec<String> = colors
                .iter()
                .map(|c| format!("{c:?}").to_lowercase())
                .collect();
            let what = if c.is_empty() {
                "colorless".to_string()
            } else {
                list(&c)
            };
            Clause::with_subject(
                *to,
                s,
                format!("{} {what}", ctx.verb(*to, "becomes", "become")),
            )
            .until_eot()
        }
        Effect::OfChoice {
            to,
            kind,
            protection,
        } => {
            let s = ctx.subj(*to);
            let what = match kind {
                ChoiceKind::Color { .. } => "the color of your choice".to_string(),
                ChoiceKind::Colors => "the color or colors of your choice".to_string(),
                ChoiceKind::LandType(lands) if lands.len() == 5 => {
                    "the basic land type of your choice".to_string()
                }
                ChoiceKind::LandType(lands) => or_list(
                    &lands
                        .iter()
                        .map(|c| {
                            let name = basic_land_name(*c);
                            format!("{} {name}", article(name))
                        })
                        .collect::<Vec<_>>(),
                ),
                ChoiceKind::CreatureType { except: None } => {
                    "the creature type of your choice".to_string()
                }
                // Imagecrafter: "Choose a creature type other than Wall.
                // Target creature becomes that type until end of turn."
                ChoiceKind::CreatureType { except: Some(t) } => {
                    return Clause::imperative(format!(
                        "choose a creature type other than {t}. {} {} that type {EOT}",
                        capitalize(&s),
                        ctx.verb(*to, "becomes", "become")
                    ));
                }
            };
            let verb = if *protection {
                format!("{} protection from", ctx.verb(*to, "gains", "gain"))
            } else {
                ctx.verb(*to, "becomes", "become").to_string()
            };
            Clause::with_subject(*to, s, format!("{verb} {what}")).until_eot()
        }
        Effect::BecomeLandType {
            to,
            land,
            indefinite,
        } => {
            let s = ctx.subj(*to);
            let name = basic_land_name(*land);
            let c = Clause::with_subject(
                *to,
                s,
                format!(
                    "{} {} {name}",
                    ctx.verb(*to, "becomes", "become"),
                    article(name)
                ),
            );
            if *indefinite { c } else { c.until_eot() }
        }
        Effect::Choose(kind) => Clause::imperative(format!("choose {}", choice_noun(kind))),
        Effect::ProtectionFromChosen { to } => {
            let s = ctx.subj(*to);
            let verb = ctx.verb(*to, "gains", "gain");
            Clause::with_subject(*to, s, format!("{verb} protection from the chosen color"))
                .until_eot()
        }
        // "Each land you control becomes that type": a group is "each".
        Effect::BecomeChosen { to } => {
            let (s, verb) = match to {
                Recipient::Group(_)
                | Recipient::EachCreature
                | Recipient::EachCreatureYouControl
                | Recipient::EachPermanentYouControl => (ctx.obj(*to), "becomes"),
                _ => (ctx.subj(*to), ctx.verb(*to, "becomes", "become")),
            };
            Clause::with_subject(*to, s, format!("{verb} that type")).until_eot()
        }
        Effect::WhenYouDo { targets, effects } => {
            // CR 603.12: its own targets; the source and the trigger as the
            // ability around it has them.
            let mut inner = Ctx::new(ctx.card, ctx.lookup, targets);
            inner.current = ctx.current;
            inner.trigger = ctx.trigger;
            inner.source_mentioned = ctx.source_mentioned;
            let body = sentences(effects, &mut inner);
            ctx.source_mentioned |= inner.source_mentioned;
            Clause::imperative(format!(
                "when you do, {}",
                lowercase_first(&body).trim_end_matches('.')
            ))
        }
        Effect::IfYouDo(inner) => {
            let body = sentences(inner, ctx);
            let body = lowercase_first(&body);
            Clause::imperative(format!("if you do, {}", body.trim_end_matches('.')))
        }
        Effect::Pay { mana, life } => Clause::imperative(if *life > 0 {
            format!("pay {life} life")
        } else {
            format!("pay {mana}")
        }),
        Effect::UnlessPays {
            payer,
            mana,
            life,
            effect,
        } => {
            let mut c = clause(effect, ctx);
            let amount = if *life > 0 {
                format!("{life} life")
            } else {
                mana.to_string()
            };
            let who = match (payer, &c.subject) {
                (Recipient::You, _) => "you pay".to_string(),
                (p, Some((s, _))) if p == s => "they pay".to_string(),
                (p, _) => {
                    let w = ctx.obj(*p);
                    format!("{w} pays")
                }
            };
            let _ = write!(c.tail, " unless {who} {amount}");
            c
        }
        Effect::IfCondition(c, inner, also) => {
            let cond = static_cond_text(c, ctx.card);
            // "Adamant — If at least three red mana was spent to cast this
            // spell, Searing Barrage deals 3 damage ...": a spell names
            // itself again after a condition about itself.
            if matches!(c, StaticCond::ManaSpent { .. }) && is_spell(ctx.card) {
                ctx.source_mentioned = false;
            }
            let body = sentences(inner, ctx);
            // "If that land was nonbasic, Molten Rain deals 2 damage": the
            // card's name keeps its capital.
            let mut body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            // "this creature also gains double strike".
            if *also {
                let this = format!("{} ", this_ref(ctx.card));
                let subject = ["it ", "they ", this.as_str()]
                    .into_iter()
                    .find(|s| body.starts_with(s))
                    .map_or(0, str::len);
                body.insert_str(subject, "also ");
            }
            Clause::imperative(format!("if {cond}, {}", body.trim_end_matches('.')))
        }
        Effect::IfTarget(n, f, inner, check) => {
            use crate::ast::TargetTense;
            let noun = ctx.target_noun[*n].clone();
            if check.tense != TargetTense::Was {
                ctx.target_mentioned[*n] = true;
                // "If that creature is an enchantment, Starfall deals 3
                // damage ...": a spell names itself again.
                if is_spell(ctx.card) {
                    ctx.source_mentioned = false;
                }
            }
            let body = sentences(inner, ctx);
            let mut body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            // "it also gains lifelink", "that creature also gets +3/+0".
            if check.also {
                let that = format!("that {noun} ");
                let subject = ["it ", "they ", that.as_str()]
                    .into_iter()
                    .find(|s| body.starts_with(s))
                    .map_or(0, str::len);
                body.insert_str(subject, "also ");
            }
            // "If it's a Goblin or Orc": the target check drops the second
            // article (a static's "is a Human or an Angel" keeps it).
            let mut pred = match &f.or {
                Some(alt)
                    if f.subtypes.len() == 1
                        && alt.subtypes.len() == 1
                        && alt.or.is_none()
                        && f.types.iter().all(|t| *t == CardType::Creature)
                        && alt.types.iter().all(|t| *t == CardType::Creature) =>
                {
                    format!("{} or {}", with_article(&f.subtypes[0]), alt.subtypes[0])
                }
                _ => target_predicate(f, ctx.card),
            };
            // "If it's a Zombie card": a card in a graveyard.
            if matches!(
                ctx.targets.get(*n).map(Target::base),
                Some(
                    Target::GraveyardCard { .. }
                        | Target::Spec(TargetSpec::CreatureCardInYourGraveyard)
                )
            ) && f.types.is_empty()
                && !f.subtypes.is_empty()
            {
                pred.push_str(" card");
            }
            // "If that creature has infect".
            let has = pred.strip_prefix("a permanent with ").map(str::to_string);
            // "If it was dealt damage this turn".
            let damaged_only = (Filter {
                damaged: false,
                ..f.clone()
            }) == Filter::default()
                && f.damaged;
            let cond = match (check.tense, has) {
                _ if damaged_only => "it was dealt damage this turn".to_string(),
                (TargetTense::Was, _) => format!("that {noun} was {pred}"),
                (TargetTense::Is, Some(k)) => format!("it has {k}"),
                (TargetTense::ThatIs, Some(k)) => format!("that {noun} has {k}"),
                (TargetTense::Is, None) => format!("it's {pred}"),
                (TargetTense::ThatIs, None) => format!("that {noun} is {pred}"),
            };
            Clause::imperative(format!("if {cond}, {}", body.trim_end_matches('.')))
        }
        Effect::Instead {
            cond,
            wording,
            label,
            effects,
        } => {
            use crate::ast::{InsteadCond, InsteadWording};
            let kicked = matches!(cond, InsteadCond::Kicked);
            let cond = match cond {
                InsteadCond::Kicked => format!("this {} was kicked", kicked_noun(ctx.card)),
                InsteadCond::Static(c) => static_cond_text(c, ctx.card),
                InsteadCond::ThisWay(c) => this_way_text(c, ctx.card),
                InsteadCond::Target { n, that, filter } => {
                    ctx.target_mentioned[*n] = true;
                    // "If that creature is another Hero".
                    let what = if filter.relation == Relation::Other {
                        let mut g = filter.clone();
                        g.relation = Relation::Any;
                        let p = target_predicate(&g, ctx.card);
                        let bare = p
                            .strip_prefix("a ")
                            .or_else(|| p.strip_prefix("an "))
                            .unwrap_or(&p);
                        format!("another {bare}")
                    } else {
                        target_predicate(filter, ctx.card)
                    };
                    if *that {
                        format!("that {} is {what}", ctx.target_noun[*n])
                    } else {
                        format!("it's {what}")
                    }
                }
            };
            // A spell names itself again unless the upgrade is for kicker
            // ("If this spell was kicked, it deals 4 damage instead";
            // "Metalcraft — Galvanic Blast deals 4 damage instead ...").
            if is_spell(ctx.card) && !kicked {
                ctx.source_mentioned = false;
            }
            // "If ..., instead creatures you control get +2/+2": the group
            // is named again.
            if *wording == InsteadWording::Lead {
                ctx.groups_mentioned.clear();
            }
            ctx.those_tokens = ctx.last_token.clone();
            let body = sentences(effects, ctx);
            ctx.those_tokens = None;
            let body = if body.starts_with(&ctx.card.name) {
                body
            } else {
                lowercase_first(&body)
            };
            let body = body.trim_end_matches('.');
            let text = match wording {
                InsteadWording::Lead => format!("if {cond}, instead {body}"),
                InsteadWording::Trail => format!("if {cond}, {body} instead"),
                InsteadWording::After => format!("{body} instead if {cond}"),
            };
            Clause::imperative(match label {
                Some(l) => format!("{l} \u{2014} {}", capitalize(&text)),
                None => text,
            })
        }
        Effect::IfKicked(inner) => {
            let body = sentences(inner, ctx);
            let body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            Clause::imperative(format!(
                "if this spell was kicked, {}",
                body.trim_end_matches('.')
            ))
        }
        Effect::SetBasePt {
            to,
            power,
            toughness,
        } => {
            let s = ctx.subj(*to);
            Clause::with_subject(
                *to,
                s,
                format!(
                    "{} base power and toughness {power}/{toughness}",
                    ctx.verb(*to, "has", "have")
                ),
            )
            .until_eot()
        }
        Effect::YouMay(inner) => {
            if let [one] = inner.as_slice()
                && let [Effect::BecomeCopy { of, except }] = one.effects.as_slice()
            {
                let s = ctx.source();
                let o = ctx.obj(*of);
                let mut text = format!("you may have {s} become a copy of {o}");
                if *except {
                    let _ = write!(text, ", except {}", except_text(ctx.card, ctx.current));
                }
                return Clause::imperative(text);
            }
            let body = sentences(inner, ctx);
            let body = lowercase_first(&body);
            // "you may gain 3 life", not "you may you gain".
            if let Some(rest) = body.strip_prefix("you ") {
                return Clause::imperative(format!("you may {}", rest.trim_end_matches('.')));
            }
            // "you may have target creature get +2/+0": another subject
            // takes "have" and the verb's base form.
            // "you may have this creature deal 1 damage ...", "you may
            // have Ally creatures you control get +1/+0 ...".
            let group_led = matches!(
                inner.first().and_then(|s| s.effects.first()),
                Some(
                    Effect::Pump {
                        to: Recipient::Group(_),
                        ..
                    } | Effect::Grant {
                        to: Recipient::Group(_),
                        ..
                    }
                )
            );
            let subject_led = group_led
                || [
                    "target ",
                    "that ",
                    "it ",
                    "another target ",
                    "up to ",
                    "this ",
                ]
                .iter()
                .any(|p| body.starts_with(p));
            if subject_led {
                let verbs = [
                    ("gets", "get"),
                    ("gains", "gain"),
                    ("deals", "deal"),
                    ("loses", "lose"),
                    ("mills", "mill"),
                    ("sacrifices", "sacrifice"),
                    ("discards", "discard"),
                    ("becomes", "become"),
                    ("draws", "draw"),
                    ("has", "have"),
                ];
                let mut text = body.trim_end_matches('.').to_string();
                if let Some((at, (from, to))) = verbs
                    .iter()
                    .filter_map(|v| text.find(&format!(" {} ", v.0)).map(|i| (i, *v)))
                    .min_by_key(|(i, _)| *i)
                {
                    text.replace_range(at + 1..at + 1 + from.len(), to);
                }
                return Clause::imperative(format!("you may have {text}"));
            }
            Clause::imperative(format!("you may {}", body.trim_end_matches('.')))
        }
        Effect::Delayed {
            about,
            trigger,
            effects,
            trigger_first,
            this_turn,
        } => {
            let was = (ctx.in_delayed, ctx.delayed_that);
            ctx.in_delayed = true;
            ctx.delayed_that = *about == Recipient::TriggeringObject;
            let body = sentences(effects, ctx);
            (ctx.in_delayed, ctx.delayed_that) = was;
            let body = body.trim_end_matches('.').to_string();
            let t = if *this_turn {
                crate::format::this_turn_trigger(trigger).to_string()
            } else {
                trigger_text(trigger, ctx, true)
            };
            if *trigger_first {
                Clause::imperative(format!("{t}, {}", lowercase_first(&body)))
            } else {
                Clause::imperative(format!("{} {t}", lowercase_first(&body)))
            }
        }
        Effect::CreateReplacement(r) => {
            let text = replacement_text(r, ctx, true);
            // The sentence ends after any ", where X is ..." tail.
            let text = text.strip_suffix('.').unwrap_or(&text);
            Clause::imperative(lowercase_first(text))
        }
        Effect::Rule(RuleKind::NoLifeGain) => {
            Clause::imperative("players can't gain life this turn".to_string())
        }
        Effect::Rule(RuleKind::DamageCantBePrevented) => {
            Clause::imperative("damage can't be prevented this turn".to_string())
        }
    }
}

/// What a target "was": "a Human", "green or white", "nonbasic".
/// "a creature card was exiled this way", "you discarded a card this
/// way", "the sacrificed creature was a Human" (after "if").
fn this_way_text(c: &crate::ast::ThisWayCond, card: &Card) -> String {
    use crate::ast::ThisWayWording;
    let verb = crate::ast::this_way_word(c.verb);
    let what = match (&c.filter, c.card) {
        (None, _) => "a card".to_string(),
        (Some(f), true) => with_article(&cards_phrase(f, card, false)),
        (Some(f), false) => with_article(&filter_phrase(f, card, false)),
    };
    match &c.wording {
        ThisWayWording::Was => format!("{what} was {verb} this way"),
        ThisWayWording::Is => format!("{what} is {verb} this way"),
        ThisWayWording::You if c.verb == mtg_core::card::ThisWay::PutInHand => {
            format!("you put {what} into your hand this way")
        }
        ThisWayWording::You => format!("you {verb} {what} this way"),
        ThisWayWording::The(noun) => {
            let pred = match (&c.filter, c.card) {
                // "If the sacrificed creature was legendary".
                (Some(f), false)
                    if f.types.is_empty()
                        && f.subtypes.is_empty()
                        && f.any_types.is_empty()
                        && !f.supertypes.is_empty() =>
                {
                    filter_phrase(f, card, false)
                        .trim_end_matches(" permanent")
                        .to_string()
                }
                (Some(f), false) => target_predicate(f, card),
                _ => what,
            };
            format!(
                "the {verb} {noun} {} {pred}",
                if c.negated { "wasn't" } else { "was" }
            )
        }
    }
}

fn target_predicate(f: &Filter, card: &Card) -> String {
    // "a Human or an Angel".
    if let Some(alt) = &f.or
        && f.subtypes.len() == 1
        && alt.subtypes.len() == 1
        && f.types.is_empty()
        && alt.types.is_empty()
    {
        let base = Filter {
            or: None,
            ..f.clone()
        };
        return format!(
            "{} or {}",
            target_predicate(&base, card),
            target_predicate(alt, card)
        );
    }
    if f.types.iter().all(|t| *t == CardType::Creature)
        && f.subtypes.is_empty()
        && f.not_supertypes.is_empty()
        && let Some(cs) = &f.colors
    {
        let words: Vec<String> = cs.iter().map(|c| color_name(*c).to_string()).collect();
        return words.join(" or ");
    }
    if f.subtypes.is_empty()
        && f.colors.is_none()
        && f.not_supertypes == [Supertype::Basic]
        && f.types.iter().all(|t| *t == CardType::Land)
    {
        return "nonbasic".to_string();
    }
    if f.types.iter().all(|t| *t == CardType::Creature)
        && f.supertypes.is_empty()
        && f.subtypes.len() == 1
    {
        // "a Goblin or Orc".
        if let Some(g) = &f.or
            && g.types.iter().all(|t| *t == CardType::Creature)
            && g.subtypes.len() == 1
            && g.or.is_none()
        {
            return format!("{} or {}", with_article(&f.subtypes[0]), g.subtypes[0]);
        }
        if f.or.is_none() {
            return with_article(&f.subtypes[0]);
        }
    }
    // "If it's legendary".
    if f.types.is_empty()
        && f.subtypes.is_empty()
        && f.colors.is_none()
        && f.or.is_none()
        && !f.supertypes.is_empty()
        && *f
            == (Filter {
                supertypes: f.supertypes.clone(),
                ..Filter::default()
            })
    {
        let words: Vec<String> = f
            .supertypes
            .iter()
            .map(|s| format!("{s:?}").to_lowercase())
            .collect();
        return words.join(" ");
    }
    if f.types.iter().all(|t| *t == CardType::Creature)
        && f.supertypes.is_empty()
        && f.colors.is_none()
        && f.subtypes.len() > 1
    {
        let each: Vec<String> = f.subtypes.iter().map(|s| with_article(s)).collect();
        return or_list(&each);
    }
    with_article(&filter_phrase(f, card, false))
}

fn lowercase_first(s: &str) -> String {
    // "Goblin creatures you control get +1/+1": a subtype keeps its
    // capital.
    let mut words = s.split(' ');
    if let (Some(first), Some(second)) = (words.next(), words.next())
        && first.chars().next().is_some_and(char::is_uppercase)
        && matches!(
            second,
            "creature" | "creatures" | "spell" | "spells" | "permanents" | "cards" | "tokens"
        )
        && !matches!(
            first,
            "Target"
                | "This"
                | "That"
                | "These"
                | "A"
                | "An"
                | "Up"
                | "Two"
                | "Three"
                | "Four"
                | "Each"
                | "Nonartifact"
                | "Nonblack"
                | "Nonwhite"
                | "Nonblue"
                | "Nonred"
                | "Nongreen"
                | "All"
                | "Other"
                | "Another"
                | "Attacking"
                | "Blocking"
                | "Tapped"
                | "Untapped"
                | "Legendary"
                | "Nonland"
                | "Noncreature"
                | "Nontoken"
                | "Token"
                | "Artifact"
                | "Enchantment"
                | "Land"
                | "Creature"
                | "White"
                | "Blue"
                | "Black"
                | "Red"
                | "Green"
                | "Colorless"
                | "Multicolored"
                | "Monocolored"
                | "Instant"
                | "Sorcery"
                | "Your"
                | "Those"
                | "Their"
                | "Enchanted"
                | "Equipped"
        )
    {
        return s.to_string();
    }
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// One sentence (no final period, lowercase start).
fn sentence_text(s: &Sentence, ctx: &mut Ctx) -> String {
    ctx.groups_mentioned
        .retain(|r| !matches!(r, Recipient::Group(_)));
    let clauses: Vec<Clause> = s.effects.iter().map(|e| clause(e, ctx)).collect();
    let shared_suffix = clauses.len() > 1
        && clauses[0].suffix.is_some()
        && clauses.iter().all(|c| c.suffix == clauses[0].suffix);
    let mut out = String::new();
    if s.then_first {
        out.push_str("then ");
    }
    if s.eot_first {
        out.push_str("until end of turn, ");
    }
    let mut prev_subject: Option<Recipient> = None;
    let mut suffix_done = false;
    for (i, c) in clauses.iter().enumerate() {
        if i > 0 {
            out.push_str(match s.joins.get(i - 1) {
                Some(Join::Then) => ", then ",
                Some(Join::Comma) => ", ",
                // "discards two cards, mills a card, and loses 1 life".
                Some(Join::And) if s.joins[..i - 1].contains(&Join::Comma) => ", and ",
                _ => " and ",
            });
        }
        let repeat = i > 0 && s.joins.get(i - 1) == Some(&Join::AndRepeat);
        let mut same_subject = false;
        match &c.subject {
            Some((r, text)) => {
                if prev_subject != Some(*r) || repeat {
                    out.push_str(text);
                    out.push(' ');
                } else {
                    same_subject = true;
                }
                prev_subject = Some(*r);
            }
            None => prev_subject = None,
        }
        // "deals 1 damage to any target and 1 damage to you".
        let text = if same_subject
            && c.text.starts_with("deals ")
            && clauses[i - 1].text.starts_with("deals ")
        {
            &c.text["deals ".len()..]
        } else {
            &c.text
        };
        out.push_str(text);
        if let Some(suffix) = c.suffix
            && !shared_suffix
            && !s.eot_first
        {
            out.push(' ');
            out.push_str(suffix);
        }
        // "deals X damage to any target and you gain X life, where X is
        // ...": one X, explained once at the end.
        let shared_where = c.tail.starts_with(", where X is ")
            && clauses[i + 1..].iter().any(|d| d.tail == c.tail);
        // "gains menace and gets +X/+0 until end of turn, where X is ...":
        // the shared duration before the last clause's X.
        if shared_suffix
            && !s.eot_first
            && i + 1 == clauses.len()
            && c.tail.starts_with(", where X is ")
        {
            out.push(' ');
            out.push_str(c.suffix.expect("shared"));
            suffix_done = true;
        }
        if !shared_where {
            out.push_str(&c.tail);
        }
    }
    if shared_suffix && !s.eot_first && !suffix_done {
        out.push(' ');
        out.push_str(clauses[0].suffix.expect("shared"));
    }
    out
}

/// Sentences as text: each capitalized and terminated, paragraphs on new
/// lines.
fn sentences(ss: &[Sentence], ctx: &mut Ctx) -> String {
    let mut out = String::new();
    for (i, s) in ss.iter().enumerate() {
        if i > 0 {
            out.push(if s.para { '\n' } else { ' ' });
        }
        if let Some(l) = &s.label {
            out.push_str(l);
            out.push_str(" — ");
        }
        out.push_str(&sentence(&sentence_text(s, ctx)));
    }
    out
}

/// Sentences after a trigger or cost: the first starts lowercase after a
/// trigger's comma, uppercase after a cost's colon.
fn effects_after(ss: &[Sentence], ctx: &mut Ctx, lowercase: bool) -> String {
    let s = sentences(ss, ctx);
    if lowercase && !starts_with_name(&s, ctx.card) {
        lowercase_first(&s)
    } else {
        s
    }
}

// ---- triggers ----

fn step_name(step: Step, whose: Who) -> String {
    // "At the beginning of combat on your turn".
    if step == Step::BeginningOfCombat {
        return match whose {
            Who::You => "combat on your turn".to_string(),
            Who::Opponents => "combat on each opponent's turn".to_string(),
            Who::Any => "each combat".to_string(),
        };
    }
    let name = match step {
        Step::PrecombatMain => "first main phase",
        Step::PostcombatMain => "second main phase",
        Step::Untap => "untap step",
        Step::Upkeep => "upkeep",
        Step::Draw => "draw step",
        Step::BeginningOfCombat => "combat",
        Step::End => "end step",
        _ => "step",
    };
    match whose {
        Who::You => format!("your {name}"),
        Who::Opponents => format!("each opponent's {name}"),
        Who::Any => {
            if step == Step::End {
                format!("the {name}")
            } else {
                format!("each player's {name}")
            }
        }
    }
}

/// "When this creature enters" (no trailing comma); `next` for a delayed
/// trigger's "the next end step".
#[allow(clippy::too_many_lines)] // one arm per trigger kind
fn trigger_text(t: &Trigger, ctx: &mut Ctx, next: bool) -> String {
    if let Trigger::When(inner) = t {
        return trigger_text(inner, ctx, next).replacen("whenever", "when", 1);
    }
    let this = |ctx: &mut Ctx| {
        ctx.source_mentioned = true;
        this_ref(ctx.card)
    };
    match t {
        Trigger::When(_) => unreachable!("unwrapped above"),
        Trigger::Either(a, b) => {
            let a = trigger_text(a, ctx, next);
            format!("{a} and {}", trigger_text(b, ctx, next))
        }
        // "When this creature enters or dies", "Whenever this creature
        // enters or attacks": the subject printed once.
        Trigger::Or {
            first,
            second,
            whenever,
        } => {
            let a = trigger_text(first, ctx, next);
            let b = trigger_text(second, ctx, next);
            let subject = format!("{} ", this_ref(ctx.card));
            let a = a.split_once(&subject).map_or(a.as_str(), |(_, v)| v);
            let b = b.split_once(&subject).map_or(b.as_str(), |(_, v)| v);
            format!(
                "{} {subject}{a} or {b}",
                if *whenever { "whenever" } else { "when" }
            )
        }
        // CR 702.29d: the card is cycled from the hand, so it's "this card".
        Trigger::Cycles { this: true, .. } => {
            ctx.source_mentioned = true;
            "when you cycle this card".to_string()
        }
        Trigger::Cycles {
            this,
            another,
            or_discard,
        } => crate::format::cycles_text(*this, *another, *or_discard),
        Trigger::CastsNth { whose, nth } => {
            format!("whenever {}", crate::format::spell_ordinal(*nth, *whose))
        }
        Trigger::EntersBatch(f) => format!(
            "whenever one or more {} enter",
            controller_first(f, filter_phrase(f, ctx.card, true))
        ),
        Trigger::Dies(o @ TriggerObj::ThisOr(_)) => {
            format!("whenever {} dies", trigger_subject(o, ctx))
        }
        Trigger::Enters(TriggerObj::This) => format!("when {} enters", this(ctx)),
        Trigger::Enters(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!("whenever {} enters", trigger_subject(o, ctx))
        }
        Trigger::Dies(TriggerObj::This) => {
            if ctx.card.types.contains(&CardType::Creature) {
                format!("when {} dies", this(ctx))
            } else {
                format!(
                    "when {} is put into a graveyard from the battlefield",
                    this(ctx)
                )
            }
        }
        Trigger::Dies(TriggerObj::Filter(f)) => {
            // "When enchanted creature dies", "Whenever a creature dies" on
            // an Aura: the filter can't be the card itself.
            let can_be_self = f.types.iter().all(|t| ctx.card.types.contains(t));
            // "Whenever a creature dealt damage by this creature this turn
            // dies": not the source.
            if f.relation == Relation::Other
                || f.enchanted
                || !can_be_self
                || f.status.damaged_by_source
            {
                format!("whenever {} dies", trigger_filter_a(f, ctx.card))
            } else {
                let t = this(ctx);
                format!(
                    "whenever {t} or another {} dies",
                    filter_phrase(f, ctx.card, false)
                )
            }
        }
        Trigger::EveryStep(Step::BeginningOfCombat) => {
            "at the beginning of each combat".to_string()
        }
        Trigger::EveryStep(step) => {
            let name = step_name(*step, Who::You);
            format!(
                "at the beginning of each {}",
                name.trim_start_matches("your ")
            )
        }
        // CR 511.2.
        Trigger::BeginningOfStep {
            step: Step::EndOfCombat,
            ..
        } => "at end of combat".to_string(),
        // "At the beginning of the upkeep of enchanted creature's
        // controller", "... the end step of enchanted artifact's controller".
        Trigger::AttachedStep(step) => format!(
            "at the beginning of the {} of enchanted {}'s controller",
            step_name(*step, Who::You).trim_start_matches("your "),
            enchanted_noun(ctx.card)
        ),
        Trigger::BeginningOfStep { step, whose } => {
            if next && *step == Step::Upkeep && *whose == Who::Any {
                "at the beginning of the next turn's upkeep".to_string()
            } else if next {
                format!(
                    "at the beginning of the next {}",
                    step_name(*step, *whose).trim_start_matches("the ")
                )
            } else {
                format!("at the beginning of {}", step_name(*step, *whose))
            }
        }
        Trigger::Attacks(TriggerObj::This) => format!("whenever {} attacks", this(ctx)),
        Trigger::Attacks(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!("whenever {} attacks", trigger_subject(o, ctx))
        }
        Trigger::AttacksWith {
            this: false,
            min: 1,
        } => "whenever you attack".to_string(),
        Trigger::AttacksWith { this: false, min } => format!(
            "whenever you attack with {} or more creatures",
            count_words(Num::N(u32::from(*min)), "one")
        ),
        Trigger::AttacksWith { this: true, min } => format!(
            "whenever {} and at least {} other creatures attack",
            this(ctx),
            count_words(Num::N(u32::from(min.saturating_sub(1))), "one")
        ),
        Trigger::AttacksUnblocked(o) => {
            format!(
                "whenever {} attacks and isn't blocked",
                trigger_subject(o, ctx)
            )
        }
        Trigger::AttacksAlone(TriggerObj::This) => {
            format!("whenever {} attacks alone", this(ctx))
        }
        Trigger::AttacksAlone(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!("whenever {} attacks alone", trigger_subject(o, ctx))
        }
        Trigger::BecomesBlocked(TriggerObj::This) => {
            format!("whenever {} becomes blocked", this(ctx))
        }
        Trigger::BecomesBlocked(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!("whenever {} becomes blocked", trigger_subject(o, ctx))
        }
        Trigger::AttacksOrBlocks(o) => {
            format!("whenever {} attacks or blocks", trigger_subject(o, ctx))
        }
        Trigger::Leaves(TriggerObj::This) => {
            format!("when {} leaves the battlefield", this(ctx))
        }
        Trigger::Leaves(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!(
                "whenever {} leaves the battlefield",
                trigger_subject(o, ctx)
            )
        }
        Trigger::BlocksOrBlocked(TriggerObj::This) => {
            format!("whenever {} blocks or becomes blocked", this(ctx))
        }
        Trigger::BlocksOrBlocked(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!(
                "whenever {} blocks or becomes blocked",
                trigger_subject(o, ctx)
            )
        }
        Trigger::Blocks(TriggerObj::This) => format!("whenever {} blocks", this(ctx)),
        Trigger::BlockPair {
            obj,
            other,
            blocks,
            blocked,
        } => {
            let s = format!(
                "whenever {} {} {}",
                trigger_subject(obj, ctx),
                crate::format::block_pair_verb(*blocks, *blocked),
                filter_a(other, ctx.card)
            );
            // With another creature named, Oracle names the source again
            // ("..., this creature deals 1 damage to that creature").
            ctx.source_mentioned = false;
            s
        }
        Trigger::Blocks(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!("whenever {} blocks", trigger_subject(o, ctx))
        }
        Trigger::DealsCombatDamageToPlayer(TriggerObj::This) => {
            format!("whenever {} deals combat damage to a player", this(ctx))
        }
        Trigger::DealsCombatDamageToPlayer(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!(
                "whenever {} deals combat damage to a player",
                trigger_subject(o, ctx)
            )
        }
        Trigger::GainsLife { whose } => match whose {
            Who::You => "whenever you gain life".to_string(),
            _ => "whenever a player gains life".to_string(),
        },
        Trigger::CastsSpell {
            whose,
            noncreature,
            instant_or_sorcery,
        } => {
            let who = match whose {
                Who::You => "you cast",
                _ => "an opponent casts",
            };
            let what = if *noncreature {
                "a noncreature spell"
            } else if *instant_or_sorcery {
                "an instant or sorcery spell"
            } else {
                "a spell"
            };
            format!("whenever {who} {what}")
        }
        Trigger::BecomesTargetWhenever(TriggerObj::This) => format!(
            "whenever {} becomes the target of a spell or ability",
            this(ctx)
        ),
        Trigger::BecomesTapped(o) => format!("whenever {} becomes tapped", trigger_subject(o, ctx)),
        Trigger::BecomesUntapped(o) => {
            format!("whenever {} becomes untapped", trigger_subject(o, ctx))
        }
        Trigger::DealtDamage(o) => format!("whenever {} is dealt damage", trigger_subject(o, ctx)),
        Trigger::DealtCombatDamage(o) => {
            format!(
                "whenever {} is dealt combat damage",
                trigger_subject(o, ctx)
            )
        }
        Trigger::DealsDamage { obj, to, combat } => format!(
            "whenever {} deals {}damage{}",
            trigger_subject(obj, ctx),
            if *combat { "combat " } else { "" },
            match to {
                mtg_core::card::DamageTo::Any => "",
                mtg_core::card::DamageTo::Creature => " to a creature",
                mtg_core::card::DamageTo::Player => " to a player",
                mtg_core::card::DamageTo::Opponent => " to an opponent",
            }
        ),
        Trigger::CastsFiltered { whose, filter } => {
            let who = match whose {
                Who::You => "you cast",
                Who::Opponents => "an opponent casts",
                Who::Any => "a player casts",
            };
            let what = spells_subject(filter, ctx.card);
            let what = what.strip_suffix(" spells").unwrap_or(&what);
            // "an artifact spell", but "a Giant spell".
            let what = if filter.subtypes.is_empty() {
                lowercase_first(what)
            } else {
                what.to_string()
            };
            format!("whenever {who} {}", with_article(&spell_noun(&what)))
        }
        Trigger::TurnedFaceUp(TriggerObj::This) => format!("when {} is turned face up", this(ctx)),
        Trigger::TurnedFaceUp(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!("whenever {} is turned face up", trigger_subject(o, ctx))
        }
        Trigger::Exploits => format!("when {} exploits a creature", this(ctx)),
        Trigger::DrawsNth { whose, n } => crate::format::draws_nth(*whose, *n),
        Trigger::TargetedByOpponent => format!(
            "whenever {} becomes the target of a spell or ability an opponent controls",
            this(ctx)
        ),
        Trigger::BecomesTarget(TriggerObj::This) => format!(
            "when {} becomes the target of a spell or ability",
            this(ctx)
        ),
        Trigger::BecomesTarget(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_)))
        | Trigger::BecomesTargetWhenever(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!(
                "whenever {} becomes the target of a spell or ability",
                trigger_subject(o, ctx)
            )
        }
    }
}

/// A trigger's subject: "this creature" or "a creature you control".
fn trigger_subject(o: &TriggerObj, ctx: &mut Ctx) -> String {
    match o {
        TriggerObj::This => {
            ctx.source_mentioned = true;
            this_ref(ctx.card)
        }
        TriggerObj::Filter(f) => trigger_filter_a(f, ctx.card),
        // Oracle names the source again after this ("..., put a +1/+1
        // counter on this creature").
        TriggerObj::ThisOr(f) => {
            let p = trigger_filter_a(f, ctx.card);
            let p = p
                .strip_prefix("an ")
                .or_else(|| p.strip_prefix("a "))
                .unwrap_or(&p);
            format!("this {} or another {p}", self_noun(ctx.card))
        }
    }
}

/// A filter with an article as a trigger's subject, where Oracle names the
/// controller before the qualities: "a creature you control with power 4
/// or greater" (elsewhere "a creature with power 4 or greater you
/// control").
fn trigger_filter_a(f: &Filter, card: &Card) -> String {
    controller_first(f, filter_a(f, card))
}

/// A rendered filter with its "you control" moved before its "with ..."
/// qualities.
fn controller_first(f: &Filter, s: String) -> String {
    let ctl = match f.who {
        Some(Who::You) => " you control",
        Some(Who::Opponents) if f.dont_control => " you don't control",
        Some(Who::Opponents) if f.your_opponents => " your opponents control",
        Some(Who::Opponents) if !f.defending => " an opponent controls",
        _ => return s,
    };
    if f.or.is_some() {
        return s;
    }
    match (s.strip_suffix(ctl), s.find(" with ")) {
        (Some(rest), Some(i)) if i < rest.len() => {
            format!("{}{ctl}{}", &rest[..i], &rest[i..])
        }
        _ => s,
    }
}

fn condition_text(c: Condition, ctx: &mut Ctx) -> String {
    match c {
        Condition::SourceUntapped => {
            format!("if {} is untapped", this_ref(ctx.card))
        }
        Condition::SourceTapped => format!("if {} is tapped", this_ref(ctx.card)),
        Condition::NoCreatures => "if no creatures are on the battlefield".to_string(),
        Condition::Kicked => "if it was kicked".to_string(),
        Condition::HadNoCounters(k) => format!("if it had no {} counters on it", counter_name(k)),
        Condition::NoSpellsLastTurn => "if no spells were cast last turn".to_string(),
        Condition::TwoSpellsLastTurn => "if a player cast two or more spells last turn".to_string(),
        Condition::EchoDue => {
            "if it came under your control since the beginning of your last upkeep".to_string()
        }
        Condition::Static(_) => unreachable!("an AST condition"),
        Condition::Evoked => "if it was evoked".to_string(),
    }
}

// ---- replacements ----

fn affects_to(a: &Affects, ctx: &mut Ctx) -> String {
    match a {
        Affects::Any => "a permanent or player".to_string(),
        Affects::This => {
            ctx.source_mentioned = true;
            this_ref(ctx.card)
        }
        Affects::You => "you".to_string(),
        Affects::Players(Who::Opponents) => "an opponent".to_string(),
        Affects::Players(_) => "a player".to_string(),
        Affects::Objects(f) => filter_phrase(f, ctx.card, true),
        Affects::YouAnd(f) => format!("you and {}", filter_phrase(f, ctx.card, true)),
        Affects::Target(i) => ctx.obj(Recipient::Target(*i)),
        Affects::ThatTarget(i) => ctx.obj(Recipient::ThatTarget(*i)),
        Affects::Targets(a, n) => ctx.obj(Recipient::Targets(*a, *n)),
        Affects::OtherThanTarget(f, i) => format!(
            "{} other than {}",
            filter_phrase(f, ctx.card, true),
            ctx.obj(Recipient::Target(*i))
        ),
        Affects::Chosen(f) => chosen_source(f, ctx.card),
    }
}

/// "a red source of your choice", "a creature of your choice with
/// shadow", "a source of your choice of the chosen color" (CR 609.7a).
fn chosen_source(f: &Filter, card: &Card) -> String {
    let p = filter_phrase(f, card, false);
    let p = if let Some(i) = p.find("permanent") {
        format!(
            "{}source of your choice{}",
            &p[..i],
            &p[i + "permanent".len()..]
        )
    } else if let Some(i) = p.find("creature") {
        let j = i + "creature".len();
        format!("{} of your choice{}", &p[..j], &p[j..])
    } else {
        format!("{p} of your choice")
    };
    with_article(&p)
}

/// What "Prevent all damage that would be dealt to ..." names: a group
/// in the plural ("to players", "to snow creatures").
fn affects_all(a: &Affects, ctx: &mut Ctx) -> String {
    match a {
        Affects::Players(Who::Opponents) => "opponents".to_string(),
        Affects::Players(_) => "players".to_string(),
        other => affects_to(other, ctx),
    }
}

/// A source of damage (CR 609.7): "enchanted creature", "creatures with
/// power 4 or less", and, for a filter that isn't of creatures, "artifact
/// sources".
fn damage_source(a: &Affects, ctx: &mut Ctx) -> String {
    match a {
        Affects::Objects(f)
            if !f.enchanted
                && f.or.is_none()
                && !f.types.contains(&CardType::Creature)
                && f.subtypes.is_empty() =>
        {
            let p = filter_phrase(f, ctx.card, false);
            // "artifact sources", "sources of the chosen color".
            match p.find("permanent") {
                Some(i) => {
                    let pre = &p[..i];
                    let post = &p[i + "permanent".len()..];
                    format!("{pre}sources{post}")
                }
                None if p.is_empty() => "sources".to_string(),
                None => format!("{p} sources"),
            }
        }
        other => affects_all(other, ctx),
    }
}

/// "Prevent all [combat] damage that would be dealt ..." (CR 615): an
/// effect a spell creates (`this_turn`) or a static ability.
fn prevent_all_text(r: &Replaces, ctx: &mut Ctx, this_turn: bool) -> String {
    let turn = if this_turn { " this turn" } else { "" };
    let (Replaces::Damage { combat, .. } | Replaces::DamageToAndBy { combat, .. }) = r else {
        unreachable!("a damage event")
    };
    let kind = match combat {
        Some(true) => "combat damage",
        Some(false) => "noncombat damage",
        None => "damage",
    };
    match r {
        Replaces::DamageToAndBy { what, .. } => {
            let what = affects_all(what, ctx);
            format!("Prevent all {kind} that would be dealt to and dealt by {what}{turn}.")
        }
        Replaces::Damage {
            from, to, wording, ..
        } => {
            let to_text = |ctx: &mut Ctx| match to {
                Affects::Any => String::new(),
                // "When you do, prevent all combat damage that would be
                // dealt to it this turn" (Oketra's Avenger).
                Affects::This if this_turn && ctx.source_mentioned => " to it".to_string(),
                a => format!(" to {}", affects_all(a, ctx)),
            };
            match (from, wording) {
                (_, DamageWording::TurnFirst) => {
                    let to = match to {
                        Affects::Any => String::new(),
                        a => format!(" to {}", affects_all(a, ctx)),
                    };
                    let by = match from {
                        Some(f) => format!(" by {}", damage_source(f, ctx)),
                        None => String::new(),
                    };
                    format!("Prevent all {kind} that would be dealt{turn}{to}{by}.")
                }
                (None, _) => {
                    let to = to_text(ctx);
                    format!("Prevent all {kind} that would be dealt{to}{turn}.")
                }
                (Some(f), DamageWording::WouldDeal) => {
                    let that = if matches!(
                        f,
                        Affects::Target(_) | Affects::ThatTarget(_) | Affects::Chosen(_)
                    ) {
                        ""
                    } else {
                        "that "
                    };
                    let by = damage_source(f, ctx);
                    let to = to_text(ctx);
                    format!("Prevent all {kind} {that}{by} would deal{to}{turn}.")
                }
                (Some(f), DamageWording::DealtLate) => {
                    let to = to_text(ctx);
                    let by = damage_source(f, ctx);
                    format!("Prevent all {kind} that would be dealt{to}{turn} by {by}.")
                }
                (Some(f), DamageWording::Dealt) => {
                    let to = to_text(ctx);
                    let by = damage_source(f, ctx);
                    format!("Prevent all {kind} that would be dealt{to} by {by}{turn}.")
                }
            }
        }
        _ => unreachable!("a damage event"),
    }
}

/// A replacement as a sentence (capitalized, terminated). `this_turn`
/// renders the until-end-of-turn form a spell creates.
#[allow(clippy::too_many_lines)]
fn replacement_text(r: &Replacement, ctx: &mut Ctx, this_turn: bool) -> String {
    let mut out = match (&r.event, &r.outcome) {
        // "Prevent all ...": an effect of a spell, or a static ability with
        // nothing more to do (Phantom Tiger's has: "If damage would be
        // dealt to ~, prevent that damage. Remove ...").
        (Replaces::Damage { .. } | Replaces::DamageToAndBy { .. }, Outcome::Prevent)
            if this_turn || r.then.is_empty() =>
        {
            prevent_all_text(&r.event, ctx, this_turn)
        }
        (
            Replaces::Damage {
                from,
                to,
                combat,
                wording,
            },
            Outcome::PreventNext(n),
        ) => {
            let n = digits(*n);
            // "... to any number of targets, divided as you choose"
            // (CR 601.2d).
            let divided = matches!(to, Affects::Target(i) if ctx.is_many(Recipient::Target(*i)));
            let kind = match combat {
                Some(true) => "combat damage",
                Some(false) => "noncombat damage",
                None => "damage",
            };
            let to = match to {
                Affects::Objects(f) => format!("each {}", filter_phrase(f, ctx.card, false)),
                other => affects_to(other, ctx),
            };
            let by = match from {
                Some(f) => format!(" by {}", damage_source(f, ctx)),
                None => String::new(),
            };
            if *wording == DamageWording::TurnFirst {
                let divided = if divided {
                    ", divided as you choose"
                } else {
                    ""
                };
                format!(
                    "Prevent the next {n} {kind} that would be dealt this turn to {to}{by}{divided}."
                )
            } else {
                format!("Prevent the next {n} {kind} that would be dealt to {to} this turn{by}.")
            }
        }
        // CR 615.7: "The next time a red source of your choice would deal
        // damage to you this turn, prevent that damage."
        (
            Replaces::Damage {
                from, to, combat, ..
            },
            Outcome::PreventNextTime,
        ) => {
            let kind = match combat {
                Some(true) => "combat damage",
                Some(false) => "noncombat damage",
                None => "damage",
            };
            let by = match from {
                Some(f) => affects_to(f, ctx),
                None => "a source".to_string(),
            };
            let to = match to {
                Affects::Any => String::new(),
                a => format!(" to {}", affects_to(a, ctx)),
            };
            let turn = if this_turn { " this turn" } else { "" };
            format!("The next time {by} would deal {kind}{to}{turn}, prevent that damage.")
        }
        // CR 614.9: "The next 1 damage that would be dealt to this creature
        // this turn is dealt to target creature you control instead."
        (
            Replaces::Damage {
                from, to, combat, ..
            },
            Outcome::Redirect { to: dest, next },
        ) => {
            let kind = match combat {
                Some(true) => "combat damage",
                Some(false) => "noncombat damage",
                None => "damage",
            };
            let turn = if this_turn { " this turn" } else { "" };
            let to = affects_to(to, ctx);
            let mut s = match (next, from) {
                (Some(n), _) => format!("The next {n} {kind} that would be dealt to {to}{turn}"),
                (None, None) => format!("All {kind} that would be dealt to {to}{turn}"),
                (None, Some(f)) => {
                    let by = damage_source(f, ctx);
                    format!("All {kind} that would be dealt{turn} to {to} by {by}")
                }
            };
            let dest = match dest {
                mtg_core::card::RedirectTo::This => {
                    ctx.source_mentioned = true;
                    this_ref(ctx.card)
                }
                mtg_core::card::RedirectTo::Target(i) => ctx.obj(Recipient::Target(*i)),
            };
            let _ = write!(s, " is dealt to {dest} instead.");
            s
        }
        (
            Replaces::Damage {
                from, to, combat, ..
            },
            Outcome::Double,
        ) => {
            let kind = match combat {
                Some(true) => "combat damage",
                Some(false) => "noncombat damage",
                None => "damage",
            };
            let to_text = affects_to(to, ctx);
            match from {
                None => format!(
                    "If a source would deal {kind} to {to_text}, it deals double that {kind} to that {} instead.",
                    to_text.trim_start_matches("a ")
                ),
                Some(f) => {
                    let by = match f {
                        Affects::Objects(f) => filter_a(f, ctx.card),
                        other => affects_to(other, ctx),
                    };
                    format!(
                        "If {by} would deal {kind} to {to_text}, it deals double that {kind} instead."
                    )
                }
            }
        }
        (Replaces::Damage { to, combat, .. }, Outcome::Prevent) => {
            let kind = match combat {
                Some(true) => "combat damage",
                Some(false) => "noncombat damage",
                None => "damage",
            };
            let to = affects_to(to, ctx);
            format!("If {kind} would be dealt to {to}, prevent that {kind}.")
        }
        (Replaces::Damage { to, .. }, Outcome::RedirectToAttached) => {
            let to = affects_to(to, ctx);
            format!(
                "All damage that would be dealt to {to} is dealt to enchanted creature instead."
            )
        }
        (Replaces::GainLife { whose }, outcome) => {
            let (subj, obj) = match whose {
                Who::You => ("you", "you"),
                Who::Any => ("a player", "that player"),
                Who::Opponents => ("an opponent", "that player"),
            };
            match outcome {
                Outcome::Prevent => {
                    format!("If {subj} would gain life, {obj} gains no life instead.")
                        .replace("you gains", "you gain")
                }
                Outcome::Double => {
                    format!("If {subj} would gain life, {obj} gains twice that much life instead.")
                        .replace("you gains", "you gain")
                }
                _ => format!("If {subj} would gain life, something else happens instead."),
            }
        }
        (
            Replaces::Draw {
                whose,
                library_empty,
            },
            outcome,
        ) => {
            let subj = match whose {
                Who::You => "you",
                _ => "a player",
            };
            let cond = if *library_empty {
                " while your library has no cards in it"
            } else {
                ""
            };
            match outcome {
                Outcome::WinGame => {
                    format!("If {subj} would draw a card{cond}, {subj} win the game instead.")
                }
                Outcome::DrawTwo => {
                    format!("If {subj} would draw a card{cond}, draw two cards instead.")
                }
                _ => format!("If {subj} would draw a card{cond}, something else happens instead."),
            }
        }
        (
            Replaces::ToGraveyard {
                whose,
                from_battlefield,
            },
            outcome,
        ) => {
            let (what, gy) = match whose {
                Who::Any => ("a card or token", "a graveyard"),
                Who::You => ("a card", "your graveyard"),
                Who::Opponents => ("a card", "an opponent's graveyard"),
            };
            let from = if *from_battlefield {
                "from the battlefield"
            } else {
                "from anywhere"
            };
            let instead = match outcome {
                Outcome::Instead(mtg_core::state::Zone::Exile) => "exile it instead",
                _ => "something else happens instead",
            };
            format!("If {what} would be put into {gy} {from}, {instead}.")
        }
        (Replaces::PlusCounters { on }, Outcome::PlusOne) => format!(
            "If one or more +1/+1 counters would be put on {}, that many plus one +1/+1 counters are put on it instead.",
            filter_a(on, ctx.card)
        ),
        (Replaces::Enters, Outcome::EnterTapped) => {
            format!("{} enters tapped.", capitalize(&this_ref(ctx.card)))
        }
        // "enters with a number of +1/+1 counters on it equal to the number
        // of creature cards in your graveyard".
        (
            Replaces::Enters,
            Outcome::EnterWithCounters {
                kind,
                n: Num::Count(i),
            },
        ) if ctx
            .card
            .counts
            .get(usize::from(*i))
            .is_some_and(|c| c.style == crate::ast::CountStyle::Equal) =>
        {
            format!(
                "{} enters with a number of {} counters on it equal to {}.",
                capitalize(&this_ref(ctx.card)),
                counter_name(*kind),
                count_phrase(&ctx.card.counts[usize::from(*i)].expr, ctx.card, false)
            )
        }
        (Replaces::Enters, Outcome::EnterWithCounters { kind, n }) => format!(
            "{} enters with {} on it.",
            capitalize(&this_ref(ctx.card)),
            counters(*n, *kind)
        ),
        (Replaces::Dies { to }, Outcome::Instead(_)) => {
            let who = match to {
                Affects::Target(i) => {
                    ctx.target_mentioned[*i] = true;
                    format!("that {}", ctx.target_noun[*i])
                }
                other => affects_to(other, ctx),
            };
            let when = if this_turn { " this turn" } else { "" };
            format!("If {who} would die{when}, exile it instead.")
        }
        (Replaces::Destroy { to }, Outcome::Regenerate) => {
            let to = affects_to(to, ctx);
            format!("Regenerate {to}.")
        }
        (event, outcome) => format!("If {event:?} would happen, {outcome:?} instead."),
    };
    if !r.then.is_empty() {
        // A new sentence about the source names it again.
        ctx.source_mentioned = false;
        out.push(' ');
        out.push_str(&sentences(&r.then, ctx));
    }
    out
}

// ---- statics ----

#[allow(clippy::too_many_lines)] // one arm per modification
fn mod_text(m: &Mod, plural_form: bool) -> String {
    let v = |s: &str, p: &str| {
        if plural_form {
            p.to_string()
        } else {
            s.to_string()
        }
    };
    match m {
        Mod::AddAbility(_) => "has \"...\"".to_string(),
        Mod::AddTypes(types) => format!(
            "{} {} in addition to its other types",
            v("is", "are"),
            with_article(
                &types
                    .iter()
                    .map(|t| type_noun(*t))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        ),
        Mod::AddLandType(c) => format!(
            "{} {} in addition to its other land types",
            v("is", "are"),
            with_article(basic_land_name(*c))
        ),
        Mod::SetLandType(c) => {
            if plural_form {
                format!("are {}", pluralize(basic_land_name(*c)))
            } else {
                format!("is {}", with_article(basic_land_name(*c)))
            }
        }
        Mod::SetSubtype(s) => format!("{} {}", v("is", "are"), with_article(s)),
        Mod::AddAllLandTypes => format!(
            "{} every basic land type in addition to its other types",
            v("is", "are")
        ),
        Mod::AddSubtype(s) => format!(
            "{} {} in addition to its other types",
            v("is", "are"),
            with_article(s)
        ),
        Mod::AddChosenType => format!(
            "{} the chosen type in addition to its other types",
            v("is", "are")
        ),
        Mod::ChosenLandType => format!("{} the chosen type", v("is", "are")),
        // "is a Mountain, Forest, and Plains".
        Mod::SetLandTypes(cs) => format!(
            "{} {}",
            v("is", "are"),
            with_article(&list(
                &cs.iter()
                    .map(|c| basic_land_name(*c).to_string())
                    .collect::<Vec<_>>()
            ))
        ),
        Mod::Control => "control".to_string(),
        Mod::AddColors(cs) => format!(
            "{} {} in addition to its other colors",
            v("is", "are"),
            list(
                &cs.iter()
                    .map(|c| color_name(*c).to_string())
                    .collect::<Vec<_>>()
            )
        ),
        Mod::SetColors(cs) => format!(
            "{} {}",
            v("is", "are"),
            list(
                &cs.iter()
                    .map(|c| color_name(*c).to_string())
                    .collect::<Vec<_>>()
            )
        ),
        Mod::AddKeywords(ks) => {
            let real: Vec<Keyword> = ks.iter().copied().filter(|k| !is_restriction(*k)).collect();
            let restricted: Vec<Keyword> =
                ks.iter().copied().filter(|k| is_restriction(*k)).collect();
            let mut parts = Vec::new();
            if !real.is_empty() {
                parts.push(format!("{} {}", v("has", "have"), keywords_list(&real)));
            }
            if restricted.len() > 1 && restricted.contains(&Keyword::AttackDespiteDefender) {
                // "can attack as though it didn't have defender and it
                // can't be blocked" (Expedition Lookout).
                let rest: Vec<Keyword> = restricted
                    .iter()
                    .copied()
                    .filter(|k| *k != Keyword::AttackDespiteDefender)
                    .collect();
                let pronoun = if plural_form { "they" } else { "it" };
                parts.push(format!(
                    "can attack as though {pronoun} didn't have defender and {pronoun} {}",
                    restriction_phrase(&rest)
                ));
            } else if !restricted.is_empty() {
                let r = restriction_phrase(&restricted);
                parts.push(if plural_form {
                    // "can attack as though they didn't have defender".
                    r.replace("though it didn't", "though they didn't")
                } else {
                    r
                });
            }
            list(&parts)
        }
        Mod::RemoveAllAbilities => format!("{} all abilities", v("loses", "lose")),
        Mod::LosesKeywords(ks) => format!("{} {}", v("loses", "lose"), keywords_list(ks)),
        Mod::SetPt {
            power: PtValue::Fixed(p),
            toughness: PtValue::Fixed(t),
        } => format!("{} base power and toughness {p}/{t}", v("has", "have")),
        Mod::SetPt { .. } => format!(
            "{} base power and base toughness each equal to its mana value",
            v("has", "have")
        ),
        Mod::ModifyPt { power, toughness }
        | Mod::ModifyPtPer {
            power, toughness, ..
        } => {
            format!("{} {}", v("gets", "get"), pt_mod(*power, *toughness))
        }
        Mod::SwitchPt => format!("{} its power and toughness switched", v("has", "have")),
        Mod::CdaColors(cs) => {
            if cs.len() == 5 {
                "is all colors".to_string()
            } else {
                format!(
                    "is {}",
                    list(
                        &cs.iter()
                            .map(|c| color_name(*c).to_string())
                            .collect::<Vec<_>>()
                    )
                )
            }
        }
        Mod::CdaPt { .. } => String::new(),
        Mod::ChosenColor => format!("{} the chosen color", v("is", "are")),
        Mod::ProtectionFromChosen => {
            format!("{} protection from the chosen color", v("has", "have"))
        }
    }
}

fn cda_count_text(c: &CdaCount, card: &Card) -> String {
    match c {
        CdaCount::CardTypesInGraveyards => {
            "the number of card types among cards in all graveyards".to_string()
        }
        CdaCount::CreatureCardsInGraveyards => {
            "the number of creature cards in all graveyards".to_string()
        }
        CdaCount::Permanents(f) => format!(
            "the number of {}{}",
            filter_phrase(f, card, true),
            if f.who.is_none() {
                " on the battlefield"
            } else {
                ""
            }
        ),
        CdaCount::ControllerLife => "your life total".to_string(),
    }
}

#[allow(clippy::too_many_lines)] // one case per kind of static
fn static_text(filter: &Filter, mods: &[Mod], card: &Card) -> String {
    // Characteristic-defining abilities name the card.
    if let Some(Mod::CdaPt {
        count,
        plus_toughness,
    }) = mods.iter().find(|m| matches!(m, Mod::CdaPt { .. }))
    {
        let owner = if card.kind == Kind::Token {
            "This token's".to_string()
        } else {
            format!("{}'s", short_name(card))
        };
        let n = cda_count_text(count, card);
        return if *plus_toughness == 0 {
            format!("{owner} power and toughness are each equal to {n}.")
        } else {
            format!(
                "{owner} power is equal to {n} and its toughness is equal to that number plus {plus_toughness}."
            )
        };
    }
    if mods.iter().any(|m| matches!(m, Mod::CdaColors(_))) {
        let subject = if card.kind == Kind::Token {
            "This token".to_string()
        } else {
            short_name(card).to_string()
        };
        return format!(
            "{subject} {}.",
            list(&mods.iter().map(|m| mod_text(m, false)).collect::<Vec<_>>())
        );
    }
    if mods.iter().any(|m| matches!(m, Mod::Control)) {
        return format!("You control {}.", filter_phrase(filter, card, false));
    }
    let each = mods.iter().any(|m| {
        matches!(
            m,
            Mod::AddTypes(_)
                | Mod::AddLandType(_)
                | Mod::AddSubtype(_)
                | Mod::AddColors(_)
                | Mod::AddChosenType
        )
    });
    let (subject, plural_form) = if filter.relation == Relation::This {
        (capitalize(&this_ref(card)), false)
    } else if filter.enchanted {
        (capitalize(&filter_phrase(filter, card, false)), false)
    } else if each {
        (
            format!("Each {}", filter_phrase(filter, card, false)),
            false,
        )
    } else {
        let bare = filter.who.is_none()
            && filter.colors.is_none()
            && filter.not_types.is_empty()
            && filter.not_supertypes.is_empty()
            && filter.not_subtypes.is_empty()
            && filter.subtypes.is_empty()
            && !filter.no_abilities
            && !filter.attacking
            && !filter.blocking
            && filter.tapped.is_none()
            && filter.color_count.is_none()
            && filter.power.is_none()
            && filter.toughness.is_none()
            && filter.mana_value.is_none()
            && filter.chosen.is_none()
            && filter.relation == Relation::Any;
        let p = filter_phrase(filter, card, true);
        if bare || filter.all {
            (format!("All {p}"), true)
        } else {
            (capitalize(&p), true)
        }
    };
    // "All creatures able to block enchanted creature do so."
    if let [Mod::AddKeywords(ks)] = mods
        && ks.as_slice() == [Keyword::Lure]
    {
        let what = lowercase_first(&subject);
        return format!("{}.", capitalize(&lure_sentence(&what, false)));
    }
    // "Enchanted creature can't attack or block, and its activated
    // abilities can't be activated" (CR 602.5): a clause of its own, last.
    let bars: Vec<Keyword> = mods
        .iter()
        .filter_map(|m| match m {
            Mod::AddKeywords(ks) => Some(ks.iter().copied().filter(|k| is_activation_bar(*k))),
            _ => None,
        })
        .flatten()
        .collect();
    let activation = activation_clause(&bars);
    let stripped: Vec<Mod>;
    let mods = if activation.is_some() {
        stripped = mods
            .iter()
            .filter_map(|m| match m {
                Mod::AddKeywords(ks) => {
                    let rest: Vec<Keyword> = ks
                        .iter()
                        .copied()
                        .filter(|k| !is_activation_bar(*k))
                        .collect();
                    (!rest.is_empty()).then_some(Mod::AddKeywords(rest))
                }
                m => Some(m.clone()),
            })
            .collect();
        if stripped.is_empty() {
            // "Enchanted creature's activated abilities can't be activated."
            let clause = activation.unwrap_or_default();
            return format!("{subject}'s {}.", &clause[4..]);
        }
        stripped.as_slice()
    } else {
        mods
    };
    let mut parts: Vec<String> = mods
        .iter()
        .map(|m| {
            if let Mod::AddAbility(a) = m {
                // "Enchanted creature can't be blocked except by Walls": a
                // granted blocking restriction prints unquoted.
                if let Ability::BlockRule(r) = a.as_ref() {
                    return block_rule_phrase(r, card, false);
                }
                let noun = if filter.types == [CardType::Land]
                    || (filter.enchanted && enchanted_noun(card) == "land")
                {
                    CardType::Land
                } else {
                    CardType::Creature
                };
                let have = if plural_form { "have" } else { "has" };
                return format!("{have} {}", quoted(a, card, noun, &()));
            }
            let mut t = mod_text(m, plural_form);
            if let Mod::ModifyPtPer {
                count: Num::Count(i),
                power,
                toughness,
            } = m
                && let Some(c) = card.counts.get(usize::from(*i))
            {
                // "Enchanted creature gets -X/-X, where X is the number of
                // cards in your hand."
                if c.style == crate::ast::CountStyle::Where {
                    t = t.replacen(
                        &pt_mod(*power, *toughness),
                        &format!(
                            "{}/{}",
                            x_sign(*power, *toughness),
                            x_sign(*toughness, *power)
                        ),
                        1,
                    );
                    t.push_str(", where X is ");
                    t.push_str(&count_phrase(&c.expr, card, false));
                } else {
                    t.push(' ');
                    t.push_str(&count_phrase(&c.expr, card, true));
                }
            }
            t
        })
        .collect();
    // "has menace and \"Whenever ...\"": one "has" for both.
    for i in 1..parts.len() {
        if parts[i].starts_with("has \"") && parts[i - 1].starts_with("has ") {
            parts[i] = parts[i]["has ".len()..].to_string();
        }
    }
    // "is a Treefolk with base power and toughness 0/4", "is a creature
    // with base power and toughness 4/4 in addition to its other types":
    // a type and the base P/T that follows it are one phrase.
    for i in (0..mods.len().saturating_sub(1)).rev() {
        let (Mod::SetSubtype(_) | Mod::AddTypes(_) | Mod::AddSubtype(_)) = &mods[i] else {
            continue;
        };
        let Mod::SetPt {
            power: PtValue::Fixed(p),
            toughness: PtValue::Fixed(t),
        } = &mods[i + 1]
        else {
            continue;
        };
        let with = format!(" with base power and toughness {p}/{t}");
        let joined = match parts[i].split_once(" in addition to its other types") {
            Some((head, tail)) => format!("{head}{with} in addition to its other types{tail}"),
            None => format!("{}{with}", parts[i]),
        };
        parts[i] = joined;
        parts.remove(i + 1);
    }
    let mut body = format!("{subject} {}", list(&parts));
    if let Some(clause) = activation {
        // Oracle joins it with ", and", except after "doesn't untap during
        // its controller's untap step" (Encrust).
        let untap_only =
            matches!(mods, [Mod::AddKeywords(ks)] if ks.as_slice() == [Keyword::DoesntUntap]);
        body.push_str(if untap_only { " and " } else { ", and " });
        body.push_str(clause);
    }
    if body.ends_with(".\"") {
        body
    } else {
        format!("{body}.")
    }
}

/// A blocking restriction without its subject: "can't be blocked except
/// by artifact creatures and/or white creatures", with `this_turn` "can't
/// be blocked this turn except by ...", "can't be blocked by Walls this
/// turn".
fn block_rule_phrase(r: &crate::ast::BlockRule, card: &Card, this_turn: bool) -> String {
    use crate::ast::BlockRule;
    // Evasion lists its alternatives with "and/or" ("black and/or red
    // creatures", "artifact creatures and/or white creatures").
    let pl = |f: &Filter| {
        let mut p = filter_phrase(f, card, true);
        if let Some(cs) = &f.colors
            && cs.len() > 1
        {
            let names: Vec<&str> = cs.iter().map(|c| color_name(*c)).collect();
            p = p.replacen(&names.join(" or "), &names.join(" and/or "), 1);
        } else if f.or.is_some() && f.or.as_ref().is_some_and(|o| o.or.is_none()) {
            p = p.replacen(" and ", " and/or ", 1);
        }
        p
    };
    let turn = if this_turn { " this turn" } else { "" };
    match r {
        BlockRule::NotBy(f) => format!("can't be blocked by {}{turn}", pl(f)),
        BlockRule::OnlyBy(f) => format!("can't be blocked{turn} except by {}", pl(f)),
        BlockRule::AtMostOne => format!("can't be blocked by more than one creature{turn}"),
        BlockRule::BlocksOnly(f) => format!("can block only {}{turn}", pl(f)),
        BlockRule::CantBlock(f) => format!("can't block {}{turn}", pl(f)),
    }
}

/// An ability another grants, in quotes, about the object that has it
/// ("\"{T}: This creature deals 1 damage to any target.\"").
fn quoted(a: &Ability, card: &Card, noun: CardType, lookup: &dyn Lookup) -> String {
    let mut holder = card.blank();
    holder.name.clone_from(&card.name);
    holder.types = vec![noun];
    holder.groups.clone_from(&card.groups);
    holder.counts.clone_from(&card.counts);
    format!("\"{}\"", ability_text(&holder, a, lookup))
}

/// "seven or more cards are in your graveyard", "you control an Island".
#[allow(clippy::too_many_lines)] // one arm per condition
fn static_cond_text(c: &StaticCond, card: &Card) -> String {
    let or_more = |n: u32| format!("{} or more", count_words(Num::N(n), "one"));
    match c {
        StaticCond::CardsInGraveyard(n) => {
            format!("there are {} cards in your graveyard", or_more(*n))
        }
        StaticCond::CardTypesInGraveyard(n) => format!(
            "there are {} card types among cards in your graveyard",
            or_more(*n)
        ),
        StaticCond::YouControl { filter, at_least } => {
            let mut f = filter.clone();
            f.who = None;
            let what = if *at_least == 1 && f.relation == Relation::Other {
                // "you control another Goblin".
                let mut g = f.clone();
                g.relation = Relation::Any;
                format!("another {}", filter_phrase(&g, card, false))
            } else if *at_least == 1 {
                with_article(&filter_phrase(&f, card, false))
            } else {
                format!("{} {}", or_more(*at_least), filter_phrase(&f, card, true))
            };
            format!("you control {what}")
        }
        StaticCond::YourTurn | StaticCond::DuringYourTurn => "it's your turn".to_string(),
        StaticCond::AnyPlayerLifeAtMost(n) => format!("a player has {n} or less life"),
        StaticCond::SourceCounters { kind, n, at_least } => {
            if *at_least && *n == 1 {
                // "this creature has a +1/+1 counter on it".
                let c = kind.name();
                format!(
                    "this {} has {} {c} counter on it",
                    self_noun(card),
                    article(c)
                )
            } else if *at_least {
                format!(
                    "this {} has {} or more {} counters on it",
                    self_noun(card),
                    count_words(Num::N(*n), "one"),
                    kind.name()
                )
            } else {
                format!(
                    "there are no {} counters on this {}",
                    kind.name(),
                    self_noun(card)
                )
            }
        }
        StaticCond::OpponentPoisonAtLeast(n) => format!(
            "an opponent has {} or more poison counters",
            count_words(Num::N(*n), "one")
        ),
        StaticCond::DefenderControls { filter, at_least } => {
            let mut f = filter.clone();
            f.who = None;
            if *at_least == 1 {
                format!(
                    "defending player controls {}",
                    with_article(&filter_phrase(&f, card, false))
                )
            } else {
                format!(
                    "defending player controls {} {}",
                    or_more(*at_least),
                    filter_phrase(&f, card, true)
                )
            }
        }
        StaticCond::OnBattlefield { filter, at_least } => {
            if *at_least == 1 {
                format!(
                    "there is {} on the battlefield",
                    with_article(&filter_phrase(filter, card, false))
                )
            } else {
                format!(
                    "there are {} {} on the battlefield",
                    or_more(*at_least),
                    filter_phrase(filter, card, true)
                )
            }
        }
        StaticCond::DefenderPoisoned => "defending player is poisoned".to_string(),
        StaticCond::OthersInCombat {
            filter,
            also: true,
            block,
            ..
        } => format!(
            "{} also {}",
            with_article(&filter_phrase(filter, card, false)),
            if *block { "blocks" } else { "attacks" }
        ),
        StaticCond::OthersInCombat {
            at_least, block, ..
        } => format!(
            "at least {} other creatures {}",
            count_words(Num::N(*at_least), "one"),
            if *block { "block" } else { "attack" }
        ),
        StaticCond::MoreCreaturesThanOpponent { attacking } => format!(
            "you control more creatures than {} player",
            if *attacking { "attacking" } else { "defending" }
        ),
        StaticCond::YouControlNone(filter) => {
            let mut f = filter.clone();
            f.who = None;
            format!("you control no {}", filter_phrase(&f, card, true))
        }
        StaticCond::AttackedThisTurn => "you attacked this turn".to_string(),
        StaticCond::CreatureDiedThisTurn => "a creature died this turn".to_string(),
        StaticCond::OpponentDamagedThisTurn => "an opponent was dealt damage this turn".to_string(),
        StaticCond::Escaped => "it escaped".to_string(),
        StaticCond::Kicked => "it was kicked".to_string(),
        StaticCond::Bargained
            if card.types.iter().any(|t| {
                matches!(
                    t,
                    mtg_core::card::CardType::Instant | mtg_core::card::CardType::Sorcery
                )
            }) =>
        {
            "this spell was bargained".to_string()
        }
        StaticCond::Bargained => "it was bargained".to_string(),
        StaticCond::ManaSpent {
            kind: mtg_core::card::SpentKind::Total,
            at_least,
            that: true,
        } => format!(
            "{} or more mana was spent to cast that spell",
            count_words(Num::N(u32::from(*at_least)), "one")
        ),
        StaticCond::ManaSpent {
            kind,
            at_least,
            that,
        } => {
            use mtg_core::card::SpentKind;
            let what = match kind {
                SpentKind::Color(c) if *at_least <= 2 => {
                    format!("{{{}}}", c.letter()).repeat(*at_least as usize)
                }
                SpentKind::Color(c) => format!(
                    "at least {} {} mana",
                    count_words(Num::N(u32::from(*at_least)), "one"),
                    color_name(*c)
                ),
                SpentKind::Treasure => "mana from a Treasure".to_string(),
                SpentKind::Total => format!(
                    "at least {} mana",
                    count_words(Num::N(u32::from(*at_least)), "one")
                ),
            };
            format!(
                "{what} was spent to cast {}",
                if *that {
                    "that spell"
                } else if is_instant_or_sorcery(card) {
                    "this spell"
                } else {
                    "it"
                }
            )
        }
        StaticCond::Saddled => "it's saddled".to_string(),
        StaticCond::LandEnteredThisTurn => {
            "you had a land enter the battlefield under your control this turn".to_string()
        }
        StaticCond::ArtifactEnteredThisTurn => {
            "an artifact entered the battlefield under your control this turn".to_string()
        }
        StaticCond::CardsDrawnThisTurn(n) => format!(
            "you've drawn {} or more cards this turn",
            count_words(Num::N(*n), "one")
        ),
        StaticCond::CardsInHand { n, at_most } => match (n, at_most) {
            (0, true) => "you have no cards in hand".to_string(),
            (n, true) => format!(
                "you have {} or fewer cards in hand",
                count_words(Num::N(*n), "one")
            ),
            (n, false) => format!(
                "you have {} or more cards in hand",
                count_words(Num::N(*n), "one")
            ),
        },
        StaticCond::Attacking => "it's attacking".to_string(),
        StaticCond::Enchanted => "it's enchanted".to_string(),
        StaticCond::Equipped => "it's equipped".to_string(),
        StaticCond::OpponentLifeAtMost(n) => format!("an opponent has {n} or less life"),
        StaticCond::YourLife { n, at_least } => format!(
            "you have {n} or {} life",
            if *at_least { "more" } else { "less" }
        ),
        StaticCond::GraveyardCards {
            filter,
            at_least,
            have,
        } => {
            let mut p = card_filter_phrase(filter, card);
            // Spell mastery: "instant and/or sorcery cards".
            if filter.or.is_some() || !filter.any_types.is_empty() {
                p = p.replace(" or ", " and/or ");
            }
            format!(
                "{} {} {p} cards in your graveyard",
                if *have { "you have" } else { "there are" },
                or_more(*at_least)
            )
        }
        StaticCond::OpponentGraveyard(n) => {
            format!("an opponent has {} cards in their graveyard", or_more(*n))
        }
        StaticCond::ManaValuesInGraveyard(n) => format!(
            "there are {} mana values among cards in your graveyard",
            or_more(*n)
        ),
        StaticCond::OpponentControls { filter, at_least } => {
            let mut f = filter.clone();
            f.who = None;
            if *at_least == 1 {
                format!(
                    "an opponent controls {}",
                    with_article(&filter_phrase(&f, card, false))
                )
            } else {
                format!(
                    "an opponent controls {} {}",
                    or_more(*at_least),
                    filter_phrase(&f, card, true)
                )
            }
        }
        // "its power is 5 or greater".
        StaticCond::Is(f)
            if f.power.is_some()
                && (Filter {
                    power: None,
                    ..f.clone()
                }) == Filter::default() =>
        {
            let (cmp, n) = f.power.expect("checked");
            format!("its power is {n}{}", cmp_words(cmp))
        }
        // "As long as this creature entered this turn".
        StaticCond::Is(f)
            if (Filter {
                status: mtg_core::card::StatusQuals::default(),
                ..f.clone()
            }) == Filter::default()
                && f.status
                    == (mtg_core::card::StatusQuals {
                        entered_this_turn: true,
                        ..Default::default()
                    }) =>
        {
            "it entered this turn".to_string()
        }
        StaticCond::Is(f) => format!("it's {}", is_predicate(f, card)),
        StaticCond::Modified => "it's modified".to_string(),
        StaticCond::AttackingAlone => "it's attacking alone".to_string(),
        StaticCond::Blocked => "it's blocked".to_string(),
        StaticCond::BeingBlocked => "at least one creature is blocking it".to_string(),
        // "As long as enchanted permanent is a creature" (an Aura that
        // enchants an artifact or creature names what it enchants).
        // "if enchanted creature's power is 4 or greater".
        StaticCond::AttachedIs(f)
            if f.power.is_some()
                && Filter {
                    power: None,
                    ..f.clone()
                } == Filter::default() =>
        {
            let (cmp, n) = f.power.expect("checked");
            let noun = if card.subtypes.iter().any(|s| s == "Equipment") {
                "equipped creature".to_string()
            } else {
                format!("enchanted {}", enchanted_noun(card))
            };
            format!("{noun}'s power is {n}{}", cmp_words(cmp))
        }
        StaticCond::AttachedIs(f) => format!(
            "{} is {}",
            if card.subtypes.iter().any(|s| s == "Equipment") {
                "equipped creature".to_string()
            } else {
                format!("enchanted {}", enchanted_noun(card))
            },
            is_predicate(f, card)
        ),
        StaticCond::DrawnThisTurn(n) => {
            format!("you've drawn {} cards this turn", or_more(*n))
        }
        StaticCond::CastThisTurn(f) => format!(
            "you've cast {} this turn",
            with_article(&format!("{} spell", card_filter_phrase(f, card)))
        ),
        StaticCond::DiscardedThisTurn => "you've discarded a card this turn".to_string(),
        StaticCond::MoreCardsInHand => "you have more cards in hand than each opponent".to_string(),
        StaticCond::OpponentPoison(1) => "an opponent is poisoned".to_string(),
        StaticCond::OpponentPoison(n) => {
            format!("an opponent has {} poison counters", or_more(*n))
        }
        StaticCond::OwnCardInExile => "you own a card in exile".to_string(),
        StaticCond::TotalPower(n) => {
            format!("creatures you control have total power {n} or greater")
        }
        StaticCond::DifferentPowers(n) => format!(
            "you control {} creatures with different powers",
            or_more(*n)
        ),
        StaticCond::During(t) => turn_time_text(*t).to_string(),
    }
}

/// "during your upkeep", "before attackers are declared" (CR 602.5b).
fn turn_time_text(t: mtg_core::card::TurnTime) -> &'static str {
    use mtg_core::card::TurnTime as T;
    match t {
        T::YourUpkeep => "during your upkeep",
        T::OpponentUpkeep => "during an opponent's upkeep",
        T::Combat => "during combat",
        T::BeforeAttackers => "before attackers are declared",
        T::YourTurnBeforeAttackers => "during your turn, before attackers are declared",
        T::DeclareBlockers => "during the declare blockers step",
        T::EndOfCombat => "during the end of combat step",
    }
}

/// What a permanent is, after "is": "untapped", "a Wall", "legendary".
fn is_predicate(f: &Filter, card: &Card) -> String {
    let bare = Filter {
        tapped: None,
        ..f.clone()
    };
    if bare == Filter::default() {
        return match f.tapped {
            Some(true) => "tapped".to_string(),
            _ => "untapped".to_string(),
        };
    }
    if !f.supertypes.is_empty()
        && (Filter {
            supertypes: Vec::new(),
            ..bare.clone()
        }) == Filter::default()
    {
        return f
            .supertypes
            .iter()
            .map(|s| format!("{s:?}").to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
    }
    target_predicate(f, card)
}

/// Whether a condition is about the source ("it's attacking"), printed
/// "this creature is ..." when the sentence's subject is something else.
fn about_source(c: &StaticCond) -> bool {
    matches!(
        c,
        StaticCond::Attacking
            | StaticCond::Enchanted
            | StaticCond::Equipped
            | StaticCond::Is(_)
            | StaticCond::Modified
            | StaticCond::AttackingAlone
            | StaticCond::Blocked
    )
}

/// An activated ability's restrictions as one sentence, in the order
/// written: "Activate only if you control an enchantment and only as a
/// sorcery." (CR 602.5).
fn restrictions_text(cost: &Cost, card: &Card) -> String {
    let mut items: Vec<String> = Vec::new();
    let default = [Restrict::Sorcery, Restrict::Once];
    let order: &[Restrict] = if cost.restrict.is_empty() {
        &default
    } else {
        &cost.restrict
    };
    for r in order {
        match r {
            // Loyalty abilities are sorcery-speed by rule (CR 606.3).
            Restrict::Sorcery if cost.sorcery && cost.loyalty.is_none() => {
                items.push("as a sorcery".to_string());
            }
            Restrict::Once if cost.per_turn == 1 => items.push("once each turn".to_string()),
            Restrict::OnceEver => items.push("once".to_string()),
            Restrict::Cond(i) => items.push(match &cost.conditions[*i] {
                StaticCond::DuringYourTurn => "during your turn".to_string(),
                StaticCond::During(t) => turn_time_text(*t).to_string(),
                c => {
                    let mut t = static_cond_text(c, card);
                    // "Activate only if this permanent is a creature": a
                    // type the card may lack is asked of the permanent.
                    let noun = match c {
                        StaticCond::Is(f) if !f.types.is_empty() => "permanent".to_string(),
                        _ => self_noun(card),
                    };
                    if about_source(c) {
                        t = t.replace("it's", &format!("this {noun} is"));
                    }
                    if *c == StaticCond::BeingBlocked {
                        t = t.replace("blocking it", &format!("blocking this {noun}"));
                    }
                    format!("if {t}")
                }
            }),
            _ => {}
        }
    }
    let mut out = String::new();
    if cost.restrict.is_empty() {
        // Written without an order: each its own sentence.
        for i in &items {
            let _ = write!(out, " Activate only {i}.");
        }
    } else if !items.is_empty() {
        let n = items.len();
        out.push_str(" Activate only ");
        for (k, i) in items.iter().enumerate() {
            if k > 0 {
                out.push_str(if n == 2 {
                    " and only "
                } else if k == n - 1 {
                    ", and only "
                } else {
                    ", only "
                });
            }
            out.push_str(i);
        }
        out.push('.');
    }
    if cost.per_turn > 1 {
        let n = match cost.per_turn {
            2 => "twice".to_string(),
            3 => "three times".to_string(),
            n => format!("{n} times"),
        };
        let _ = write!(out, " Activate no more than {n} each turn.");
    }
    out
}

/// A static ability's text with its condition: "As long as <cond>, <text>"
/// or "<text> as long as <cond>." ("During your turn, ..." for the turn).
fn conditional_static(body: &str, c: &StaticCond, first: bool, card: &Card) -> String {
    let mut cond = static_cond_text(c, card);
    // "as long as it's enchanted" about the subject; "as long as this
    // creature is enchanted" when the subject is something else.
    if about_source(c) && !body.starts_with(&capitalize(&this_ref(card))) {
        cond = cond.replace("it's", &format!("{} is", this_ref(card)));
        cond = cond.replace("it entered", &format!("{} entered", this_ref(card)));
    }
    if first {
        let this = format!("{} ", capitalize(&this_ref(card)));
        let rest = if starts_with_name(body, card) {
            body.to_string()
        } else if matches!(c, StaticCond::SourceCounters { .. }) && body.starts_with(&this) {
            // "As long as this creature has four or more oil counters on
            // it, it gets +2/+0."
            format!("it {}", &body[this.len()..])
        } else if about_source(c) && body.starts_with(&this) {
            // "As long as this creature is equipped, it gets +1/+1."
            cond = cond.replace("it's", &format!("{} is", this_ref(card)));
            cond = cond.replace("it entered", &format!("{} entered", this_ref(card)));
            // "As long as this creature's power is 5 or greater, ..."
            cond = cond.replace("its power", &format!("{}'s power", this_ref(card)));
            format!("it {}", &body[this.len()..])
        } else if let StaticCond::AttachedIs(_) = c
            && let Some(rest) = body
                .strip_prefix("Equipped creature ")
                .or_else(|| body.strip_prefix(&format!("Enchanted {} ", enchanted_noun(card))))
        {
            // "As long as equipped creature is legendary, it has menace."
            format!("it {rest}")
        } else {
            lowercase_first(body)
        };
        if *c == StaticCond::DuringYourTurn {
            format!("During your turn, {rest}")
        } else {
            format!("As long as {cond}, {rest}")
        }
    } else {
        let stem = body.strip_suffix('.').unwrap_or(body);
        format!("{stem} as long as {cond}.")
    }
}

// ---- abilities and cards ----

/// The text of one ability, as Oracle prints it.
#[allow(clippy::too_many_lines)] // one arm per ability kind
pub fn ability_text(card: &Card, a: &Ability, lookup: &dyn Lookup) -> String {
    let no_targets: &[Target] = &[];
    match a {
        Ability::Keyword(k) => keyword_line(card, *k),
        Ability::RestrictedMana { mana, restriction } => {
            restricted_mana_text(card, *mana, restriction)
        }
        Ability::XManaColor(c) => format!("Spend only {} mana on X.", color_name(*c)),
        Ability::AsEntersChoose { kind, joined } => {
            let what = choice_noun(kind);
            if *joined {
                format!("As it enters, choose {what}.")
            } else {
                format!("As {} enters, choose {what}.", this_ref(card))
            }
        }
        Ability::Mana(m) => {
            let syms: Vec<String> = m
                .iter()
                .map(|m| match m {
                    Mana::Colorless => "{C}".to_string(),
                    Mana::Colored(c) => format!("{{{}}}", c.letter()),
                    Mana::Any => "one mana of any color".to_string(),
                    Mana::Chosen => "one mana of the chosen color".to_string(),
                })
                .collect();
            format!("{{T}}: Add {}.", or_list(&syms))
        }
        Ability::SacMana(m) => {
            let what = match m {
                Mana::Colorless => "{C}".to_string(),
                Mana::Colored(c) => format!("{{{}}}", c.letter()),
                Mana::Any => "one mana of any color".to_string(),
                Mana::Chosen => "one mana of the chosen color".to_string(),
            };
            format!("{{T}}, Sacrifice {}: Add {what}.", this_ref(card))
        }
        Ability::Enchant(t) => match t.base() {
            Target::UpTo(_) | Target::Many { .. } => unreachable!("base"),
            Target::Spec(s) => format!("Enchant {}", target_phrase(*s).replace("target ", "")),
            Target::CreatureOfSubtype(sub) => format!("Enchant {sub} creature"),
            Target::Filter(f) | Target::Spell(f) | Target::GraveyardCard { filter: f, .. } => {
                format!("Enchant {}", filter_phrase(f, card, false))
            }
        },
        Ability::Exert(effects) => {
            let mut ctx = Ctx::new(card, lookup, &[]);
            ctx.current = Some(a);
            ctx.source_mentioned = true;
            format!(
                "You may exert {} as it attacks. When you do, {}",
                this_ref(card),
                lowercase_first(&sentences(effects, &mut ctx))
            )
        }
        Ability::Spell { targets, effects } => {
            let mut ctx = Ctx::new(card, lookup, targets);
            ctx.current = Some(a);
            sentences(effects, &mut ctx)
        }
        Ability::Modal(modes) | Ability::ModalBoth(modes) | Ability::ModalTwo(modes) => {
            let mut out = match a {
                Ability::ModalBoth(_) => "Choose one or both —",
                Ability::ModalTwo(_) => "Choose two —",
                _ => "Choose one —",
            }
            .to_string();
            for m in modes {
                let mut ctx = Ctx::new(card, lookup, &m.targets);
                ctx.current = Some(a);
                out.push_str("\n• ");
                push_mode_name(&mut out, m);
                out.push_str(&sentences(&m.effects, &mut ctx));
            }
            out
        }
        Ability::Activated {
            cost,
            targets,
            effects,
            equip,
            cycling,
        } => {
            if *equip {
                return format!("Equip {}", cost.mana);
            }
            if *cycling {
                if let Some(word) = typecycling_word(effects) {
                    return format!("{word}cycling {}", cost.mana);
                }
                return format!("Cycling {}", cost.mana);
            }
            let mut ctx = Ctx::new(card, lookup, targets);
            ctx.current = Some(a);
            let c = cost_text(cost, &mut ctx);
            let mut text = format!("{c}: {}", effects_after(effects, &mut ctx, false));
            // CR 702.142, 702.177, 702.193: the keyword prints before the
            // cost and its rules are reminder text.
            if let Some(k) = cost.keyword {
                text = format!("{} — {text}", k.name());
            }
            match cost.reduce.as_deref() {
                Some((by, crate::ast::ReduceBy::Each(e))) => {
                    let _ = write!(
                        text,
                        " This ability costs {by} less to activate {}.",
                        count_phrase(e, card, true)
                    );
                }
                Some((by, crate::ast::ReduceBy::If(c))) => {
                    let _ = write!(
                        text,
                        " This ability costs {by} less to activate if {}.",
                        static_cond_text(c, card)
                    );
                }
                None => {}
            }
            text.push_str(&restrictions_text(cost, card));
            text
        }
        Ability::Triggered {
            trigger,
            condition,
            targets,
            effects,
        } => {
            let mut ctx = Ctx::new(card, lookup, targets);
            ctx.current = Some(a);
            ctx.trigger = Some(trigger);
            let mut out = capitalize(&trigger_text(trigger, &mut ctx, false));
            match condition {
                Some(crate::ast::TrigCond::Core(c)) => {
                    out.push_str(", ");
                    out.push_str(&condition_text(*c, &mut ctx));
                }
                Some(crate::ast::TrigCond::Static(StaticCond::Saddled)) => {
                    out.push_str(" while saddled");
                }
                // "Whenever you cast a noncreature spell, if at least four
                // mana was spent to cast it, ...".
                Some(crate::ast::TrigCond::Static(StaticCond::ManaSpent {
                    kind: mtg_core::card::SpentKind::Total,
                    at_least,
                    that: true,
                })) => {
                    let _ = write!(
                        out,
                        ", if at least {} mana was spent to cast it",
                        count_words(Num::N(u32::from(*at_least)), "one")
                    );
                }
                Some(crate::ast::TrigCond::Static(c)) => {
                    out.push_str(", if ");
                    out.push_str(&static_cond_text(c, card));
                    // Oracle names the source again after a condition about
                    // something else ("..., if you control three or more
                    // artifacts, this creature gets +3/+3"), and "it" after
                    // one about the source ("if there are no arrowhead
                    // counters on this artifact, sacrifice it").
                    ctx.source_mentioned = matches!(
                        c,
                        StaticCond::SourceCounters { .. }
                            | StaticCond::ManaSpent { that: false, .. }
                    );
                    ctx.attached_mentioned = matches!(c, StaticCond::AttachedIs(_));
                }
                None => {}
            }
            out.push_str(", ");
            out.push_str(&effects_after(effects, &mut ctx, true));
            out
        }
        Ability::Replacement(r) => {
            let mut ctx = Ctx::new(card, lookup, no_targets);
            ctx.current = Some(a);
            replacement_text(r, &mut ctx, false)
        }
        Ability::Static {
            filter,
            mods,
            condition,
            cond_first,
        } => {
            let mut body = static_text(filter, mods, card);
            // "Threshold — Enchanted creature gets an additional +2/-2 as
            // long as ...": a conditional pump on what an earlier static
            // ability of this card already pumps.
            if condition.is_some()
                && card
                    .abilities
                    .iter()
                    .take_while(|b| !std::ptr::eq(*b, a))
                    .any(|b| {
                        matches!(b, Ability::Static { filter: f, condition: None, mods: m, .. }
                        if f == filter && m.iter().any(|x| matches!(x, Mod::ModifyPt { .. })))
                    })
            {
                body = body.replacen(" gets ", " gets an additional ", 1);
            }
            match condition {
                None => body,
                Some(c) => conditional_static(&body, c, *cond_first, card),
            }
        }
        Ability::BlockRule(r) => {
            let me = capitalize(&this_ref(card));
            format!("{me} {}.", block_rule_phrase(r, card, false))
        }
        Ability::Labeled { label, ability } => {
            format!("{label} — {}", ability_text(card, ability, lookup))
        }
        Ability::CostChange {
            filter,
            whose,
            generic,
        } => {
            let whose = match whose {
                Who::You => " you cast",
                Who::Opponents => " your opponents cast",
                Who::Any => "",
            };
            let more = if *generic > 0 { "more" } else { "less" };
            format!(
                "{} {whose} cost {{{}}} {more} to cast.",
                cost_spells_subject(filter, card),
                generic.unsigned_abs()
            )
            .replace("  ", " ")
        }
        Ability::CondCost { delta, cond } => format!(
            "This spell costs {{{}}} {} to cast if {}.",
            delta.unsigned_abs(),
            if *delta < 0 { "less" } else { "more" },
            if *cond == StaticCond::Bargained {
                "it's bargained".to_string()
            } else {
                static_cond_text(cond, card)
            }
        ),
        Ability::TargetCost { delta, filter } => format!(
            "This spell costs {{{}}} {} to cast if it targets {}.",
            delta.unsigned_abs(),
            if *delta < 0 { "less" } else { "more" },
            filter_a(filter, card)
        ),
        Ability::SelfCost { per, times } => {
            // "This spell costs {X} less to cast, where X is the greatest
            // power among creatures you control."
            if let Num::Count(i) = times
                && let Some(c) = card.counts.get(usize::from(*i))
                && c.style == crate::ast::CountStyle::Where
            {
                return format!(
                    "This spell costs {{X}} less to cast, where X is {}.",
                    count_phrase(&c.expr, card, false)
                );
            }
            let tail = match times {
                Num::Count(i) => card
                    .counts
                    .get(usize::from(*i))
                    .map(|c| format!(" {}", count_phrase(&c.expr, card, true)))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            format!("This spell costs {{{per}}} less to cast{tail}.")
        }
        Ability::Mechanic(m) => match m {
            crate::ast::Mechanic::Bushido(n) => format!("Bushido {n}"),
            crate::ast::Mechanic::Soulshift(n) => format!("Soulshift {n}"),
            crate::ast::Mechanic::Afflict(n) => format!("Afflict {n}"),
            crate::ast::Mechanic::Toxic(n) => format!("Toxic {n}"),
            crate::ast::Mechanic::Ward(c) => format!("Ward {c}"),
            crate::ast::Mechanic::Echo(c) => format!("Echo {c}"),
            crate::ast::Mechanic::Unearth(c) => format!("Unearth {c}"),
            crate::ast::Mechanic::Buyback(c) => format!("Buyback {c}"),
            crate::ast::Mechanic::Outlast(c) => format!("Outlast {c}"),
            crate::ast::Mechanic::Crew(n) => format!("Crew {n}"),
            crate::ast::Mechanic::Morph(c) => format!("Morph {c}"),
            crate::ast::Mechanic::Evoke(c) => format!("Evoke {c}"),
            crate::ast::Mechanic::Madness(c) => format!("Madness {c}"),
            crate::ast::Mechanic::Foretell(c) => format!("Foretell {c}"),
            crate::ast::Mechanic::Suspend(n, c) => format!("Suspend {n}\u{2014}{c}"),
            crate::ast::Mechanic::Entwine(c) => format!("Entwine {c}"),
            crate::ast::Mechanic::Modular(n) => format!("Modular {n}"),
            crate::ast::Mechanic::CumulativeUpkeep(c) => format!("Cumulative upkeep {c}"),
            crate::ast::Mechanic::Escape(c, n) => format!(
                "Escape\u{2014}{c}, Exile {} other cards from your graveyard.",
                count_words(Num::N(*n), "a")
            ),
            crate::ast::Mechanic::EscapesWith(n) => format!(
                "{} escapes with {} on it.",
                capitalize(&this_ref(card)),
                counters(Num::N(*n), CounterKind::PlusOnePlusOne)
            ),
            crate::ast::Mechanic::Bloodthirst(n) => format!("Bloodthirst {n}"),
            crate::ast::Mechanic::KickedWith(n, ks) => {
                let mut s = format!(
                    "If this {} was kicked, it enters with {} on it",
                    self_noun(card),
                    counters(Num::N(*n), CounterKind::PlusOnePlusOne)
                );
                if !ks.is_empty() {
                    let names: Vec<String> =
                        ks.iter().map(|k| keyword_name(*k).to_string()).collect();
                    let _ = write!(s, " and with {}", list(&names));
                }
                s.push('.');
                s
            }
            crate::ast::Mechanic::Megamorph(c) => format!("Megamorph {c}"),
            crate::ast::Mechanic::Disguise(c) => format!("Disguise {c}"),
            crate::ast::Mechanic::Ninjutsu(c) => format!("Ninjutsu {c}"),
            crate::ast::Mechanic::Scavenge(c) => format!("Scavenge {c}"),
            crate::ast::Mechanic::Dash(c) => format!("Dash {c}"),
            crate::ast::Mechanic::Embalm(c) => format!("Embalm {c}"),
            crate::ast::Mechanic::Eternalize(c) => format!("Eternalize {c}"),
            crate::ast::Mechanic::Backup(n) => format!("Backup {n}"),
            crate::ast::Mechanic::Offspring(c) => format!("Offspring {c}"),
            crate::ast::Mechanic::Bargain => "Bargain".to_string(),
            crate::ast::Mechanic::Saddle(n) => format!("Saddle {n}"),
        },
        Ability::EntersTappedUnless(c) => format!(
            "{} enters tapped unless {}.",
            capitalize(&this_ref(card)),
            static_cond_text(c, card)
        ),
        Ability::EntersWithCountersIf { kind, n, cond } => format!(
            "{} enters with {} on it if {}.",
            capitalize(&this_ref(card)),
            counters(Num::N(*n), *kind),
            static_cond_text(cond, card)
        ),
        Ability::AttackUnless {
            cond,
            attack,
            block,
        } => {
            let what = match (attack, block) {
                (true, true) => "attack or block",
                (true, false) => "attack",
                _ => "block",
            };
            // "unless an opponent has been dealt damage this turn".
            let cond = static_cond_text(cond, card).replace(" was dealt ", " has been dealt ");
            format!(
                "{} can't {what} unless {cond}.",
                capitalize(&this_ref(card))
            )
        }
        Ability::DeckAnyNumber => {
            format!("A deck can have any number of cards named {}.", card.name)
        }
        Ability::HandSize { whose, delta } => {
            let (subject, have) = if *whose == Who::Opponents {
                ("Each opponent's", "Each opponent has")
            } else {
                ("Your", "You have")
            };
            match delta {
                None => format!("{have} no maximum hand size."),
                Some(n) => format!(
                    "{subject} maximum hand size is {} by {}.",
                    if *n > 0 { "increased" } else { "reduced" },
                    count_words(Num::N(n.unsigned_abs()), "one")
                ),
            }
        }
        Ability::Protection(f) => {
            if *f == Filter::default() {
                "Protection from everything".to_string()
            } else if f.types.is_empty()
                && f.subtypes.is_empty()
                && f.any_types.is_empty()
                && let Some(cc) = f.color_count
            {
                format!(
                    "Protection from {}",
                    match cc {
                        mtg_core::card::ColorCount::Multicolored => "multicolored",
                        mtg_core::card::ColorCount::Monocolored => "monocolored",
                        mtg_core::card::ColorCount::Colorless => "colorless",
                    }
                )
            } else {
                format!("Protection from {}", filter_phrase(f, card, true))
            }
        }
        Ability::Affinity(f) => {
            let mut g = f.clone();
            g.who = None;
            format!("Affinity for {}", filter_phrase(&g, card, true))
        }
        Ability::EntersAsCopy(f) => {
            let mut s = format!(
                "You may have {} enter as a copy of any {} on the battlefield",
                this_ref(card),
                filter_phrase(f, card, false)
            );
            let x = except_text(card, None);
            if !x.is_empty() {
                let _ = write!(s, ", except {x}");
            }
            sentence(&s)
        }
    }
}

/// A named mode's name and dash: "Smash the Chest — ".
fn push_mode_name(out: &mut String, m: &crate::ast::Mode) {
    if let Some(n) = &m.name {
        out.push_str(n);
        out.push_str(" — ");
    }
}

/// The text of one mode, for naming the choice.
pub fn mode_text(card: &Card, m: &crate::ast::Mode, lookup: &dyn Lookup) -> String {
    let mut ctx = Ctx::new(card, lookup, &m.targets);
    sentences(&m.effects, &mut ctx)
}

/// The card's full Oracle text.
pub fn oracle_text(card: &Card, lookup: &dyn Lookup) -> String {
    let mut lines: Vec<String> = Vec::new();
    // "Bargain", then "This spell costs {3} less to cast if ..." come
    // first, before keywords.
    let bargain = |a: &Ability| matches!(a, Ability::Mechanic(crate::ast::Mechanic::Bargain));
    if card.abilities.iter().any(bargain) {
        lines.push("Bargain".to_string());
    }
    let cost_line = |a: &Ability| {
        matches!(a, Ability::TargetCost { .. } | Ability::CondCost { .. }) || bargain(a)
    };
    for a in card
        .abilities
        .iter()
        .filter(|a| cost_line(a) && !bargain(a))
    {
        lines.push(ability_text(card, a, lookup));
    }
    // Oracle prints kicker before keywords ("Kicker {2}{G}\nReach").
    if let Some(k) = &card.kicker {
        lines.push(format!("Kicker {k}"));
    }
    let alone_pair = card.keywords.contains(&Keyword::CantAttackAlone)
        && card.keywords.contains(&Keyword::CantBlockAlone);
    for k in &card.keywords {
        if alone_pair && *k == Keyword::CantBlockAlone || *k == Keyword::Rebound {
            continue;
        }
        if alone_pair && *k == Keyword::CantAttackAlone {
            lines.push(format!(
                "{} can't attack or block alone.",
                capitalize(&this_ref(card))
            ));
            continue;
        }
        lines.push(keyword_line(card, *k));
    }
    if let Some(cost) = &card.additional {
        let mut ctx = Ctx::new(card, lookup, &[]);
        let c = cost_text(cost, &mut ctx);
        lines.push(format!(
            "As an additional cost to cast this spell, {}.",
            lowercase_first(&c)
        ));
    }
    let mut i = 0;
    while i < card.abilities.len() {
        // Consecutive mana abilities print as one line.
        if let Ability::Mana(_) = &card.abilities[i] {
            let mut all = Vec::new();
            while let Some(Ability::Mana(m)) = card.abilities.get(i) {
                all.extend(m.iter().copied());
                i += 1;
            }
            lines.push(ability_text(card, &Ability::Mana(all), lookup));
            continue;
        }
        if cost_line(&card.abilities[i]) {
            i += 1;
            continue;
        }
        let text = ability_text(card, &card.abilities[i], lookup);
        // "This land enters tapped. As it enters, choose a color." is one
        // line.
        match (&card.abilities[i], lines.last_mut()) {
            (Ability::AsEntersChoose { joined: true, .. }, Some(last)) => {
                last.push(' ');
                last.push_str(&text);
            }
            _ => lines.push(text),
        }
        i += 1;
    }
    if let Some(fb) = &card.flashback {
        // "Flashback {4}{R}" for a mana cost; "Flashback—{1}{U}, Pay 3
        // life." when the cost has other parts.
        let mut ctx = Ctx::new(card, lookup, &[]);
        let mana_only = !fb.tap && fb.life == 0 && fb.discard == 0 && fb.sacrifice.is_none();
        if mana_only {
            lines.push(format!("Flashback {}", fb.mana));
        } else {
            lines.push(format!("Flashback—{}.", cost_text(fb, &mut ctx)));
        }
    }
    // Oracle prints rebound after the spell's effect.
    if card.keywords.contains(&Keyword::Rebound) {
        lines.push("Rebound".to_string());
    }
    lines.join("\n")
}

/// Oracle text as Scryfall prints it, reduced to what the printer can
/// produce: reminder text removed, keyword lines split one per line,
/// whitespace tidied. Apply to both sides before comparing.
pub fn normalize(text: &str) -> String {
    let mut out = Vec::new();
    for raw in text.lines() {
        let mut line = String::new();
        let mut depth = 0usize;
        for c in raw.chars() {
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                c if depth == 0 => line.push(c),
                _ => {}
            }
        }
        // Removed reminder text can leave "you get {E}{E} ." behind.
        let line = line
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace(" .", ".")
            .replace(" ,", ",");
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(", ").collect();
        // Keywords, or protection from a quality ("Flying, protection
        // from enchantments").
        let all_keywords = parts.iter().all(|p| {
            Keyword::ALL
                .iter()
                .any(|k| keyword_name(*k).eq_ignore_ascii_case(p))
                || (p.to_ascii_lowercase().starts_with("protection from ") && !p.contains(" and "))
        });
        if parts.len() > 1 && all_keywords {
            for p in parts {
                out.push(capitalize(p));
            }
        } else {
            out.push(line);
        }
    }
    out.join("\n")
}

/// "this spell" / "this creature" in "If this spell was kicked".
fn kicked_noun(card: &Card) -> String {
    if is_spell(card) {
        "spell".to_string()
    } else {
        self_noun(card)
    }
}

/// An instant or sorcery card, whose text calls itself "this spell".
fn is_instant_or_sorcery(card: &Card) -> bool {
    card.types.iter().any(|t| {
        matches!(
            t,
            mtg_core::card::CardType::Instant | mtg_core::card::CardType::Sorcery
        )
    })
}
