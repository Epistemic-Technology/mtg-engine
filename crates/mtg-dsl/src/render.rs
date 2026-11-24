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
    Ability, Affects, AttachedRef, Card, ChoiceKind, Cost, CostItem, DamageWording, Effect, Filter,
    Join, Kind, Mod, OfChosen, Recipient, Replacement, Replaces, Restrict, SacCost, SameRef,
    Sentence, StaticCond, Target, Trigger, TriggerObj,
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
        Num::XPlus(n) => format!("X plus {n}"),
        Num::XTimes(n) => times_x(n),
        Num::NMinusX(n) => format!("{} minus X", number_word(n)),
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
        Num::XPlus(n) => format!("X plus {n}"),
        Num::XTimes(n) => times_x(n),
        Num::NMinusX(n) => format!("{n} minus X"),
        Num::N(n) => n.to_string(),
    }
}

/// "three" (a number in words, digits past twenty).
fn number_word(n: u32) -> String {
    NUMBER_WORDS
        .get((n as usize).wrapping_sub(1))
        .map_or_else(|| n.to_string(), |w| (*w).to_string())
}

/// "twice X", "three times X".
fn times_x(n: u32) -> String {
    match n {
        2 => "twice X".to_string(),
        n => format!(
            "{} times X",
            NUMBER_WORDS
                .get(n as usize - 1)
                .map_or_else(|| n.to_string(), |w| (*w).to_string())
        ),
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
        | "Treefolk" | "Arcane" | "Spacecraft" | "Myr" => {
            return word.to_string();
        }
        "Elf" => return "Elves".to_string(),
        "Dwarf" => return "Dwarves".to_string(),
        "Wolf" => return "Wolves".to_string(),
        "Werewolf" => return "Werewolves".to_string(),
        "Sphinx" => return "Sphinxes".to_string(),
        "Hero" => return "Heroes".to_string(),
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

/// The creature type an equip ability is limited to ("Equip Knight {1}",
/// CR 702.6c), if any.
pub(crate) fn equip_subtype(targets: &[Target]) -> Option<&str> {
    match targets {
        [Target::Filter(f)] if f.subtypes.len() == 1 => Some(f.subtypes[0].as_str()),
        [Target::CreatureOfSubtype(s)] => Some(s.as_str()),
        _ => None,
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
            | Keyword::CantCrew
            | Keyword::UntapsDuringOthers
            | Keyword::MayNotUntap
            | Keyword::CantBeEquipped
            | Keyword::AssignsToughness
            | Keyword::CantTransform
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
    // "Enchanted creature can't attack, block, or transform." (Bound by
    // Moonsilver).
    let transform = ks.contains(&Keyword::CantTransform);
    if transform && ks.contains(&Keyword::CantAttack) && ks.contains(&Keyword::CantBlock) {
        parts.push("can't attack, block, or transform".to_string());
    } else if ks.contains(&Keyword::CantAttack)
        && ks.contains(&Keyword::CantBlock)
        && ks.contains(&Keyword::CantCrew)
    {
        parts.push("can't attack, block, or crew Vehicles".to_string());
    } else if ks.contains(&Keyword::CantAttack) && ks.contains(&Keyword::CantBlock) {
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
    if transform && !(ks.contains(&Keyword::CantAttack) && ks.contains(&Keyword::CantBlock)) {
        parts.push("can't transform".to_string());
    }
    for k in ks {
        if matches!(
            k,
            Keyword::MustAttack
                | Keyword::Unblockable
                | Keyword::DoesntUntap
                | Keyword::CantBecomeUntapped
                | Keyword::AssignsToughness
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
    if k == Keyword::UntapsDuringOthers {
        return format!(
            "Untap {} during each other player's untap step.",
            this_ref(card)
        );
    }
    if k == Keyword::MayNotUntap {
        return format!(
            "You may choose not to untap {} during your untap step.",
            this_ref(card)
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

/// Every card type, in the order rules text lists them.
pub(crate) const CARD_TYPES: [CardType; 9] = [
    CardType::Artifact,
    CardType::Battle,
    CardType::Creature,
    CardType::Enchantment,
    CardType::Instant,
    CardType::Kindred,
    CardType::Land,
    CardType::Planeswalker,
    CardType::Sorcery,
];

pub(crate) fn type_noun(t: CardType) -> &'static str {
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

/// Chapter numbers as printed (CR 714.2a): Roman numerals, "I, II".
fn chapter_numerals(ns: &[u8]) -> String {
    const ROMAN: [&str; 16] = [
        "", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII", "XIII",
        "XIV", "XV",
    ];
    ns.iter()
        .map(|n| ROMAN[usize::from(*n).min(15)])
        .collect::<Vec<_>>()
        .join(", ")
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
    // "a spell of the chosen color" (Diamond Mare).
    if let Some(rest) = what.strip_prefix("of the chosen ") {
        return format!("spell of the chosen {rest}");
    }
    match what.find(" with ") {
        Some(i) => format!("{} spell{}", &what[..i], &what[i..]),
        None if what.is_empty() => "spell".to_string(),
        None => format!("{what} spell"),
    }
}

/// Whether a spell filter's subject starts lowercase: "an artifact spell",
/// "a historic spell" (CR 700.6), but "a Giant spell".
fn lower_spell_subject(f: &Filter) -> bool {
    f.subtypes.is_empty()
        || f.subtypes
            .iter()
            .any(|s| s == crate::ast::HISTORIC || s == crate::ast::OUTLAW)
}

fn a_spell(f: &Filter, card: &Card) -> String {
    // "a spell that's both white and black" (CR 105.2).
    if f.status.all_colors
        && let Some(cs) = &f.colors
    {
        let mut g = f.clone();
        g.colors = None;
        g.status.all_colors = false;
        let names: Vec<&str> = cs.iter().map(|c| color_name(*c)).collect();
        let a = if g == Filter::default() {
            "a spell".to_string()
        } else {
            a_spell(&g, card)
        };
        return format!("{a} that's both {}", names.join(" and "));
    }
    // "a Lesson or Shrine spell": two creature or card types alone share
    // the article and the noun (`Lesson spell or Shrine spell` prints each
    // whole).
    if let Some(alt) = &f.or
        && !f.alt_article
        && alt.or.is_none()
    {
        let bare = |g: &Filter| {
            g.subtypes.len() == 1
                && Filter {
                    subtypes: Vec::new(),
                    or: None,
                    ..g.clone()
                } == Filter::default()
        };
        let mut first = f.clone();
        first.or = None;
        if bare(&first) && bare(alt) {
            return format!(
                "{} or {} spell",
                with_article(&first.subtypes[0]),
                alt.subtypes[0]
            );
        }
    }
    // "a Lesson, Saga, or Shrine spell" (Guru Pathik): three or more.
    if f.or.as_ref().is_some_and(|a| a.or.is_some()) && !f.alt_article {
        let mut names = Vec::new();
        let mut g = Some(f);
        while let Some(h) = g {
            let bare = h.subtypes.len() == 1
                && Filter {
                    subtypes: Vec::new(),
                    or: None,
                    ..h.clone()
                } == Filter::default();
            if !bare {
                names.clear();
                break;
            }
            names.push(h.subtypes[0].clone());
            g = h.or.as_deref();
        }
        if names.len() > 2 {
            let last = names.pop().expect("three");
            return format!("{}, or {last} spell", with_article(&names.join(", ")));
        }
    }
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
    // "an artifact spell", but "a Giant spell"; "a historic spell".
    let what = if lower_spell_subject(f) {
        lowercase_first(what)
    } else {
        what.to_string()
    };
    with_article(&spell_noun(&what))
}

/// "{T}: Add {C}. Spend this mana only to cast an instant or sorcery
/// spell." (CR 106.6).
fn restricted_mana_text(card: &Card, m: Mana, r: &crate::ast::ManaRestriction) -> String {
    let what = match m {
        Mana::Colorless => "{C}".to_string(),
        Mana::Colored(c) => format!("{{{}}}", c.letter()),
        Mana::Any => "one mana of any color".to_string(),
        Mana::Chosen => "one mana of the chosen color".to_string(),
    };
    format!("{{T}}: Add {what}. {}", spend_rule(card, r))
}

/// "instant and sorcery spells", "legendary spells", "Mount or Vehicle
/// spells": a restriction's spells in the plural, alternatives joined by
/// `join`.
fn plural_spells(f: &Filter, card: &Card, join: &str) -> String {
    let one = |g: &Filter| {
        let s = spells_subject(g, card);
        let s = s.strip_suffix(" spells").unwrap_or(&s).to_string();
        // "colorless Eldrazi spells", but "Dragon spells".
        if lower_spell_subject(g) || !g.subtypes.iter().any(|t| s.starts_with(t.as_str())) {
            lowercase_first(&s)
        } else {
            s
        }
    };
    if let Some(alt) = &f.or {
        let (b, a) = crate::format::factor_qualifiers(f, alt);
        return format!("{} {join} {} spells", one(&b), one(&a));
    }
    if f.any_types.len() == 2 {
        let types: Vec<&str> = f.any_types.iter().map(|t| type_noun(*t)).collect();
        let mut g = f.clone();
        g.any_types.clear();
        let rest = card_filter_phrase(&g, card);
        let rest = rest.trim();
        let types = format!("{} {join} {}", types[0], types[1]);
        return if rest.is_empty() {
            format!("{types} spells")
        } else {
            format!("{rest} {types} spells")
        };
    }
    spell_noun_plural(&one(f))
}

/// "spells with mana value 5 or greater": the noun before "with".
fn spell_noun_plural(what: &str) -> String {
    spell_noun(what).replacen("spell", "spells", 1)
}

/// "Spend this mana only to cast an instant or sorcery spell.", "Spend
/// this mana only to cast artifact spells or activate abilities of
/// artifacts." (CR 106.6).
#[allow(clippy::too_many_lines)] // one branch per wording
fn spend_rule(card: &Card, r: &crate::ast::ManaRestriction) -> String {
    use mtg_core::card::{SpendPlural, SpendWays, SpendWording};
    if r.ways.has(SpendWays::ON_COSTS) {
        return "Spend this mana only on costs that contain {X}.".to_string();
    }
    let plural = r.plural != SpendPlural::One;
    let join = match r.plural {
        SpendPlural::And => "and",
        SpendPlural::AndOr => "and/or",
        _ => "or",
    };
    let spells = r.spells.as_ref().map(|f| {
        let any = *f == Filter::default();
        let mut s = match (plural, any) {
            (true, true) => "spells".to_string(),
            (false, true) => "a spell".to_string(),
            (true, false) => plural_spells(f, card, join),
            (false, false) => a_spell(f, card),
        };
        if r.ways.has(SpendWays::FACE_DOWN) {
            s = match s.split_once(' ') {
                Some(("a" | "an", rest)) => format!("a face-down {rest}"),
                _ => format!("face-down {s}"),
            };
        }
        for (bit, words) in [
            (SpendWays::FROM_EXILE, " from exile"),
            (SpendWays::FROM_GRAVEYARD, " from your graveyard"),
            (
                SpendWays::NOT_FROM_HAND,
                " from anywhere other than your hand",
            ),
            (SpendWays::FLASHBACK, " with flashback from a graveyard"),
        ] {
            if r.ways.has(bit) {
                s.push_str(words);
            }
        }
        if let Some(x) = &r.x_spells {
            if **x == Filter::default() {
                let _ = write!(
                    s,
                    " or {} with {{X}} in {} mana cost{}",
                    if plural { "spells" } else { "a spell" },
                    if plural { "their" } else { "its" },
                    if plural { "s" } else { "" }
                );
            } else if plural {
                let _ = write!(
                    s,
                    " or {} with {{X}} in their mana costs",
                    plural_spells(x, card, join)
                );
            } else {
                let _ = write!(s, " or {} with {{X}} in its mana cost", a_spell(x, card));
            }
        }
        let s = format!("cast {s}");
        if r.foretell {
            format!("foretell a card from your hand or {s}")
        } else if r.disturb {
            format!("pay a disturb cost or {s}")
        } else {
            s
        }
    });
    // "an ability of an artifact source", "abilities of artifacts".
    let ability = match (&r.ability_source, plural, r.bare) {
        (Some(f), false, false) => format!(
            "an ability of {} source",
            with_article(&filter_phrase(f, card, false))
        ),
        (Some(f), false, true) => {
            format!(
                "an ability of {}",
                with_article(&filter_phrase(f, card, false))
            )
        }
        (Some(f), true, false) => {
            format!("abilities of {} sources", filter_phrase(f, card, false))
        }
        (Some(f), true, true) => format!("abilities of {}", filter_phrase(f, card, true)),
        (None, false, _) => "an ability".to_string(),
        (None, true, _) => "abilities".to_string(),
    };
    let other = r.ways.0
        & (SpendWays::ABILITY_KINDS | SpendWays::UNLOCK | SpendWays::FACE_UP | SpendWays::COST_C);
    let rule = if other != 0 {
        // The other ways, after the spells and abilities: "cast an
        // enchantment spell, unlock a door, or turn a permanent face up".
        let mut items: Vec<String> = spells.into_iter().collect();
        if r.abilities {
            items.push(format!("activate {ability}"));
        }
        items.extend(spend_way_phrases(r.ways, plural));
        let list = match items.as_slice() {
            [one] => one.clone(),
            [a, b] => {
                let joiner = if r.ways.has(SpendWays::OR_TO) {
                    "or to"
                } else if r.wording == SpendWording::CastAnd {
                    "and"
                } else {
                    "or"
                };
                format!("{a} {joiner} {b}")
            }
            [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
            [] => String::new(),
        };
        let tail = if r.ways.has(SpendWays::UNCOUNTERABLE) {
            ", and that spell can't be countered"
        } else {
            ""
        };
        format!("Spend this mana only to {list}{tail}.")
    } else {
        let rule = spend_rule_plain(r, spells, &ability);
        if r.ways.has(SpendWays::UNCOUNTERABLE) {
            format!(
                "{}, and that spell can't be countered.",
                rule.trim_end_matches('.')
            )
        } else {
            rule
        }
    };
    // "a creature spell of the chosen type", "a creature source of the
    // chosen type": the noun before the chosen type.
    rule.replace(" of the chosen type spell", " spell of the chosen type")
        .replace(" of the chosen type source", " source of the chosen type")
}

/// The phrases of a restriction's other ways (`SpendWays`), in printed
/// order.
fn spend_way_phrases(w: mtg_core::card::SpendWays, plural: bool) -> Vec<String> {
    use mtg_core::card::SpendWays;
    let mut out: Vec<String> = Vec::new();
    let pick = |one: &str, many: &str| if plural { many } else { one }.to_string();
    if w.has(SpendWays::POWER_UP) {
        out.push(pick(
            "activate a power-up ability",
            "activate power-up abilities",
        ));
    }
    if w.has(SpendWays::EQUIP) {
        out.push(pick(
            "activate an equip ability",
            "activate equip abilities",
        ));
    }
    if w.has(SpendWays::CLASS_LEVEL) {
        out.push("gain a Class level".to_string());
    }
    if w.has(SpendWays::UNLOCK) {
        out.push(pick("unlock a door", "unlock doors"));
    }
    if w.has(SpendWays::FACE_UP) {
        if w.has(SpendWays::FACE_UP_MORPH) {
            out.push("pay a mana cost to turn a manifested creature face up".to_string());
            out.push("pay a morph cost".to_string());
        } else if w.has(SpendWays::FACE_UP_CREATURES) {
            out.push(pick("turn a creature face up", "turn creatures face up"));
        } else {
            out.push(pick("turn a permanent face up", "turn permanents face up"));
        }
    }
    if w.has(SpendWays::COST_C) {
        out.push("pay a cost that contains {C}".to_string());
    }
    out
}

/// `spend_rule` without the other ways.
fn spend_rule_plain(
    r: &crate::ast::ManaRestriction,
    spells: Option<String>,
    ability: &str,
) -> String {
    use mtg_core::card::SpendWording;
    match (r.wording, spells) {
        (SpendWording::CantCast, Some(s)) => format!("This mana can't be spent to {s}."),
        (SpendWording::ActivateFirst, Some(s)) => {
            format!("Spend this mana only to activate {ability} or {s}.")
        }
        (SpendWording::CastFirstTo, Some(s)) if r.abilities => {
            format!("Spend this mana only to {s} or to activate {ability}.")
        }
        (SpendWording::CastAnd, Some(s)) if r.abilities => {
            format!("Spend this mana only to {s} and activate {ability}.")
        }
        (_, Some(s)) if r.abilities => {
            format!("Spend this mana only to {s} or activate {ability}.")
        }
        (_, Some(s)) => format!("Spend this mana only to {s}."),
        (_, None) => format!("Spend this mana only to activate {ability}."),
    }
}

/// "a" or "an".
fn article(word: &str) -> &'static str {
    // "an 8/8 red Lizard creature token", "an 11/11": said as "eight",
    // "eleven".
    if word.starts_with("11/") || word.starts_with("18/") {
        return "an";
    }
    match word.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u' | 'A' | 'E' | 'I' | 'O' | 'U' | '8') => "an",
        _ => "a",
    }
}

/// "a Knight or a Soldier", "an enchantment or an enchanted permanent":
/// two alternatives of a condition, each with its article.
fn each_with_article(f: &Filter, card: &Card) -> String {
    match &f.or {
        Some(alt) if alt.or.is_none() && f.alt_article => {
            let base = Filter {
                or: None,
                alt_article: false,
                ..f.clone()
            };
            format!(
                "{} or {}",
                with_article(&filter_phrase(&base, card, false)),
                with_article(&filter_phrase(alt, card, false))
            )
        }
        _ => with_article(&filter_phrase(f, card, false)),
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
/// Converge in a filter (`StatusQuals::spent_colors`): "less than or
/// equal to the number of colors of mana spent to cast this spell".
fn spent_at_most(card: &Card) -> String {
    let it = if is_instant_or_sorcery(card) {
        "this spell"
    } else {
        "this creature"
    };
    format!("less than or equal to the number of colors of mana spent to cast {it}")
}

fn cmp_words(c: mtg_core::card::Cmp) -> &'static str {
    match c {
        mtg_core::card::Cmp::AtMost => " or less",
        mtg_core::card::Cmp::AtLeast => " or greater",
        // Only filled in as a game is played (`ObjFilter::with_chosen`).
        mtg_core::card::Cmp::Exactly | mtg_core::card::Cmp::Odd | mtg_core::card::Cmp::Even => "",
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
    // "a permanent card" (CR 110.4a); "a nonland permanent card".
    if f.not_types.contains(&CardType::Instant)
        && f.not_types.contains(&CardType::Sorcery)
        && f.types.is_empty()
        && f.any_types.is_empty()
    {
        let mut g = f.clone();
        g.not_types
            .retain(|t| !matches!(t, CardType::Instant | CardType::Sorcery));
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
/// "second", "third", "4th".
/// "the fourth plan counter", "the seventh".
fn nth_word(n: u32) -> String {
    match n {
        4 => "fourth".to_string(),
        5 => "fifth".to_string(),
        6 => "sixth".to_string(),
        7 => "seventh".to_string(),
        8 => "eighth".to_string(),
        9 => "ninth".to_string(),
        10 => "tenth".to_string(),
        n => ordinal_word(n),
    }
}

fn ordinal_word(n: u32) -> String {
    match n {
        1 => "first".to_string(),
        2 => "second".to_string(),
        3 => "third".to_string(),
        n => format!("{n}th"),
    }
}

fn cost_spells_subject(f: &Filter, card: &Card) -> String {
    let second = |s: String, keep_case: bool| if keep_case { s } else { lowercase_first(&s) };
    if let Some(alt) = &f.or {
        let (b, a) = crate::format::factor_qualifiers(f, alt);
        // "Aura and Equipment spells you cast cost {1} less" (Danitha
        // Capashen, Paragon): two noncreature subtypes share "spells";
        // creature types don't ("Angel spells and Human spells").
        let lone_sub = |g: &Filter| match g.subtypes.as_slice() {
            [s] if matches!(s.as_str(), "Aura" | "Equipment" | "Vehicle")
                && Filter {
                    subtypes: Vec::new(),
                    ..g.clone()
                } == Filter::default() =>
            {
                Some(s.clone())
            }
            _ => None,
        };
        if let (Some(x), Some(y)) = (lone_sub(&b), lone_sub(&a)) {
            return format!("{x} and {y} spells");
        }
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
    // "Creature spells with flying you cast": the keyword quality follows
    // "spells".
    let subject = spells_subject(f, card);
    if !f.keywords.is_empty()
        && let Some(head) = subject.strip_suffix(" spells")
        && let Some((head, quality)) = head.split_once(" with ")
    {
        return format!("{head} spells with {quality}");
    }
    subject
}

/// What a permission to cast from the top of a library covers:
/// "creature spells", "Spider spells and noncreature spells", "spells
/// with mana value 4 or greater", "spells with flash or flying".
fn top_spells(f: &Filter, card: &Card) -> String {
    let one = |g: &Filter| {
        let s = cost_spells_subject(g, card);
        let s = if lower_spell_subject(g) {
            lowercase_first(&s)
        } else {
            s
        };
        // "with mana value 4 or greater spells": the qualities follow.
        match s
            .strip_prefix("with ")
            .and_then(|r| r.strip_suffix(" spells"))
        {
            Some(q) => format!("spells with {q}"),
            None => s,
        }
    };
    if let Some(alt) = &f.or {
        let (b, a) = crate::format::factor_qualifiers(f, alt);
        let (b, a) = (one(&b), one(&a));
        if let (Some(x), Some(y)) = (
            b.strip_prefix("spells with "),
            a.strip_prefix("spells with "),
        ) {
            return format!("spells with {x} or {y}");
        }
        return format!("{b} and {a}");
    }
    one(f)
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
    // `short_name` on a card that isn't legendary: it names itself in
    // full ("When Enduring Innocence dies", a card that may not stay a
    // creature).
    if card.kind != Kind::Token
        && (card.supertypes.contains(&Supertype::Legendary) || card.short_name.is_some())
    {
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
        // `short_name "Ajani"` for "Ajani Resolute".
        if let Some(s) = &card.short_name {
            return s;
        }
        card.name.split(", ").next().unwrap_or(&card.name)
    } else {
        card.short_name.as_deref().unwrap_or(&card.name)
    }
}

/// Whether text starts with the card's name for itself, which keeps its
/// capital after a comma.
fn starts_with_name(s: &str, card: &Card) -> bool {
    s.starts_with(&card.name) || s.starts_with(short_name(card))
}

/// What a permanent's text calls it as an object after naming it: "it",
/// or "him" or "her" for a character with a `pronoun` ("Morlun enters
/// with X +1/+1 counters on him").
fn it_obj(card: &Card) -> &'static str {
    match card.pronoun {
        Some(crate::ast::Pronoun::He) => "him",
        Some(crate::ast::Pronoun::She) => "her",
        Some(crate::ast::Pronoun::They) => "them",
        None => "it",
    }
}

/// What a character's text calls it as a subject: "it", "he", "she".
fn it_subj(card: &Card) -> &'static str {
    match card.pronoun {
        Some(crate::ast::Pronoun::He) => "he",
        Some(crate::ast::Pronoun::She) => "she",
        Some(crate::ast::Pronoun::They) => "they",
        None => "it",
    }
}

/// "Whenever X attacks or blocks" -> "Whenever X attack or block" for a
/// plural-named source X.
fn plural_subject_verbs(text: &str, name: &str) -> String {
    let key = format!("{name} ");
    let Some(i) = text.find(&key) else {
        return text.to_string();
    };
    let (head, rest) = text.split_at(i + key.len());
    let end = rest.find(',').unwrap_or(rest.len());
    let (pred, tail) = rest.split_at(end);
    let words: Vec<String> = pred
        .split(' ')
        .map(|w| match w {
            "dies" => "die".to_string(),
            "attacks" | "blocks" | "enters" | "deals" | "becomes" | "leaves" => {
                w[..w.len() - 1].to_string()
            }
            "is" => "are".to_string(),
            "has" => "have".to_string(),
            _ => w.to_string(),
        })
        .collect();
    format!("{head}{}{tail}", words.join(" "))
}

/// A character's "it's": "he's", "she's", "they're" ("If he's
/// attacking, untap him", The Incredible Hulk).
fn its_is(card: &Card) -> &'static str {
    match card.pronoun {
        Some(crate::ast::Pronoun::He) => "he's",
        Some(crate::ast::Pronoun::She) => "she's",
        Some(crate::ast::Pronoun::They) => "they're",
        None => "it's",
    }
}

/// A character's possessive: "its", "his", "her" ("draw a card if her
/// power is 4 or greater").
fn its_poss(card: &Card) -> &'static str {
    match card.pronoun {
        Some(crate::ast::Pronoun::He) => "his",
        Some(crate::ast::Pronoun::She) => "her",
        Some(crate::ast::Pronoun::They) => "their",
        None => "its",
    }
}

/// The noun a permanent uses for itself: "this creature", "this Aura".
pub fn self_noun(card: &Card) -> String {
    if card.kind == Kind::Token {
        return "token".to_string();
    }
    // "this emblem deals 4 damage to any target" (CR 114).
    if card.kind == Kind::Emblem {
        return "emblem".to_string();
    }
    // CR 702.176a: a creature with impending isn't always a creature
    // ("Whenever this permanent enters or attacks").
    if card
        .abilities
        .iter()
        .any(|a| matches!(a, Ability::Mechanic(crate::ast::Mechanic::Impending(..))))
    {
        return "permanent".to_string();
    }
    if card.types.contains(&CardType::Creature) {
        return "creature".to_string();
    }
    if card.types.contains(&CardType::Planeswalker) {
        return "planeswalker".to_string();
    }
    // "this Saga" (Urza's Saga is a land), "this Class", "this Case".
    if card.types.contains(&CardType::Enchantment)
        && let Some(s) = card
            .subtypes
            .iter()
            .find(|s| matches!(s.as_str(), "Saga" | "Class" | "Case"))
    {
        return s.clone();
    }
    if card.types.contains(&CardType::Land) {
        return "land".to_string();
    }
    if card.types.contains(&CardType::Artifact) {
        // "this Vehicle", "this Equipment" (Oracle's templating since
        // 2024).
        if card
            .subtypes
            .iter()
            .any(|s| s == "Vehicle" || s == "Spacecraft")
        {
            return card
                .subtypes
                .iter()
                .find(|s| *s == "Vehicle" || *s == "Spacecraft")
                .cloned()
                .unwrap_or_default();
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
        // "this Room" (Duskmourn's templating).
        if card.subtypes.iter().any(|s| s == "Room") {
            return "Room".to_string();
        }
        return "enchantment".to_string();
    }
    // "this Siege" (CR 310.3).
    if card.types.contains(&CardType::Battle) {
        if card.subtypes.iter().any(|s| s == "Siege") {
            return "Siege".to_string();
        }
        return "battle".to_string();
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

/// A historic target on the battlefield is a "historic permanent" (CR
/// 700.6): "another target historic permanent you control".
fn historic_permanent(f: &Filter, phrase: String) -> String {
    if f.subtypes == [crate::ast::HISTORIC] && f.types.is_empty() && f.any_types.is_empty() {
        phrase.replacen("historic", "historic permanent", 1)
    } else {
        phrase
    }
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
    /// Inside "If it's a creature, ...": the target the check calls "it"
    /// (Carbonize: "and if it would die this turn").
    it_target: Option<usize>,
    /// The trigger's condition named the enchanted creature's type: "untap
    /// that creature" (Howl of the Hunt), not "tap it" (Narcolepsy).
    attached_by_type: bool,
    /// The target last named "the chosen creature": its controller is
    /// "its controller" after it (Expel the Unworthy).
    chosen_said: Option<usize>,
    /// Inside a delayed trigger about the triggering object, which its
    /// effects call "that creature" ("destroy that creature at end of
    /// combat").
    delayed_that: bool,
    /// The sentence being rendered begins "until end of turn," (a delayed
    /// trigger in it prints without its own "this turn").
    eot_first: bool,
    /// The sentence being rendered lasts this long instead of the turn
    /// ("until your next turn", CR 611.2b).
    lasting: Option<String>,
    /// The ability being rendered, for "except it has this ability".
    current: Option<&'a Ability>,
    /// The trigger of the ability, for wording that depends on it.
    trigger: Option<&'a Trigger>,
    /// The last token created, and, inside an "instead" sentence, the
    /// token the sentence before created ("create four of those tokens
    /// instead").
    last_token: Option<String>,
    those_tokens: Option<String>,
    /// The last token was created tapped: "those tokens" are made the same
    /// way ("Create two tapped 2/2 ... tokens. ... instead create X of
    /// those tokens", The Final Days).
    last_token_tapped: bool,
    /// The controller was just told to discard one card ("draw a card,
    /// then discard a card"): a reflexive trigger after it is "When you
    /// discard a card this way".
    discarded_one: bool,
    /// What this is attached to has been named in the ability, so it is
    /// "it" as an object ("tap enchanted creature and remove all counters
    /// from it"; "if enchanted creature is untapped, tap it").
    attached_mentioned: bool,
    /// The attached object was named by a trigger condition ("if enchanted
    /// creature has toxic"): as the next subject it is "that creature".
    attached_by_cond: bool,
    /// An `amass` came before: `CreatedObject` is "the amassed Army".
    amassed: bool,
    /// Recipients of a pump already in the ability: another pump of the
    /// same one "gets an additional +4/+4".
    pumped: Vec<Recipient>,
    /// Recipients of a "-X/-X" pump already in the ability: a fixed pump
    /// of the same one after it "gets an additional -3/-3" (Faerie
    /// Fencing).
    pumped_x: Vec<Recipient>,
    /// A spell was just countered unless its controller paid: "If they
    /// do" follows.
    they_paid: bool,
    /// After "exchange life totals": "If you lost life this way".
    lost_life_did: bool,
    /// "That player may pay {4}": a following "If you do" prints "If the
    /// player does".
    player_did: bool,
    /// After "any player may ...": "If a player does" / "If no one does".
    any_player_did: bool,
    /// In a "When you cast this spell" trigger: "counter Brain Gorgers"
    /// names the spell.
    cast_this: bool,
    /// An "any player may ..." was printed in this ability.
    any_player_said: bool,
    /// Inside "If the player does, ...": that player is "they" (Chain
    /// Lightning's "they may copy this spell").
    player_they: bool,
    /// A sentence to print after a delayed trigger's timing ("Return that
    /// card ... at the beginning of the next end step. If it enters as a
    /// creature, ...").
    delayed_tail: Option<String>,
    /// The next sentence is "If you don't, ...": an optional choice before
    /// it prints its result "If you do, ..." ("You may choose a creature
    /// card from it. If you do, that player discards that card.").
    before_if_you_dont: bool,
    /// Rendering the effect of "[effect] unless [player] pays".
    in_unless: bool,
    /// Rendering what winning a clash does ("If you win, that creature
    /// gets an additional +2/+2").
    in_won: bool,
    /// Inside "If excess damage was dealt this way, ...": the amount is
    /// "that excess damage".
    in_excess: bool,
    /// Counts an earlier sentence of the ability already defined with
    /// ", where X is ...": a later X is the same X ("Draw X cards, where X
    /// is .... If ..., Balin deals X damage to each opponent.").
    x_defined: Vec<crate::ast::CountExpr>,
    /// Those the current sentence defines (several clauses of one
    /// sentence share one ", where X is ..." at its end).
    x_pending: Vec<crate::ast::CountExpr>,
    /// "each opponent" was just named as an object ("deals 2 damage to
    /// each opponent"): a following "each opponent discards" is "Those
    /// players each discard" (Skull Rend).
    opponents_object: bool,
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
            it_target: None,
            attached_by_type: false,
            chosen_said: None,
            delayed_that: false,
            eot_first: false,
            lasting: None,
            current: None,
            trigger: None,
            last_token: None,
            those_tokens: None,
            last_token_tapped: false,
            discarded_one: false,
            attached_mentioned: false,
            attached_by_cond: false,
            amassed: false,
            pumped: Vec::new(),
            pumped_x: Vec::new(),
            in_won: false,
            in_excess: false,
            they_paid: false,
            lost_life_did: false,
            player_did: false,
            any_player_did: false,
            cast_this: false,
            any_player_said: false,
            player_they: false,
            delayed_tail: None,
            before_if_you_dont: false,
            in_unless: false,
            x_defined: Vec::new(),
            x_pending: Vec::new(),
            opponents_object: false,
        }
    }

    /// The source as an object ("this creature", "Lightning Bolt", then
    /// "it", or "him" or "her" for a character with a `pronoun`).
    fn source(&mut self) -> String {
        if self.source_mentioned {
            return it_obj(self.card).to_string();
        }
        self.source_mentioned = true;
        self.source_name()
    }

    /// The source in subject position: "it", or "he" or "she" ("He fights
    /// up to one target creature an opponent controls").
    fn source_subj(&mut self) -> String {
        if self.source_mentioned {
            return it_subj(self.card).to_string();
        }
        self.source()
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
        if let Target::Many {
            min, max, total, ..
        } = &self.targets[i]
        {
            let count = match max {
                None => "any number of".to_string(),
                Some(m) if m == min => count_words(*m, "one"),
                // "up to X target cards", "up to three target creature
                // cards with total mana value 3 or less".
                Some(m) if *min == Num::N(0) => format!("up to {}", count_words(*m, "one")),
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
            if let Target::Spec(TargetSpec::Any) = self.targets[i].base() {
                return format!("{count} targets");
            }
            // "any number of other target nonland permanents you control".
            let mut noun = self.plural_target_noun(i);
            // "creature cards with total mana value 3 or less from your
            // graveyard".
            if let Some(n) = total {
                let with = format!(" with total mana value {} or less", digits(*n));
                noun = match noun.split_once(" from ") {
                    Some((a, b)) => format!("{a}{with} from {b}"),
                    None => format!("{noun}{with}"),
                };
            }
            return match noun.strip_prefix("other ") {
                Some(rest) => format!("{count} other target {rest}"),
                None => format!("{count} target {noun}"),
            };
        }
        let phrase = self.target_phrase_of(i);
        if matches!(self.targets[i], Target::UpTo(_)) {
            // "up to one other target permanent you control".
            match phrase.strip_prefix("another ") {
                Some(rest) => format!("up to one other {rest}"),
                None => format!("up to one {phrase}"),
            }
        } else {
            phrase
        }
    }

    /// "target spell that targets a creature".
    fn spell_target_phrase(&self, spell: &Filter) -> String {
        let mut base_filter = spell.clone();
        let targeting = base_filter.targeting.take();
        // "target instant or sorcery spell you control".
        let yours = base_filter.who == Some(Who::You) && !base_filter.dont_control;
        if yours {
            base_filter.who = None;
        }
        let base = card_filter_phrase(&base_filter, self.card);
        let mut out = format!("target {}", spell_noun(&base));
        if yours {
            out.push_str(" you control");
        }
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

    /// "each of " before a group of targets that each get counters ("put
    /// a +1/+1 counter on each of up to two target creatures", "... on
    /// each of them", "... on each of those creatures").
    fn each_of(&self, r: Recipient) -> &'static str {
        if matches!(
            r,
            Recipient::Targets(..)
                | Recipient::ThoseTargets(..)
                | Recipient::CreatedThem
                | Recipient::Those(_)
        ) || self.is_many(r)
        {
            "each of "
        } else {
            ""
        }
    }

    /// Whether a recipient is a variable target group (CR 601.2c).
    /// "each " after a variable group of targets ("any number of target
    /// players each discard a card").
    fn each_many(&self, r: Recipient) -> &'static str {
        if self.is_many(r) { "each " } else { "" }
    }

    fn is_many(&self, r: Recipient) -> bool {
        matches!(r, Recipient::Target(i) | Recipient::ThatTarget(i)
            if matches!(self.targets.get(i), Some(Target::Many { .. })))
    }

    fn target_phrase_of(&self, i: usize) -> String {
        match self.targets[i].base() {
            Target::UpTo(_) | Target::Many { .. } => unreachable!("base"),
            Target::Spec(s) => target_phrase(*s),
            Target::CreatureOfSubtype(sub) => format!("target {sub} creature"),
            Target::Filter(f) if f.relation == Relation::Other && !f.other_than_this => {
                let mut g = f.clone();
                crate::format::relation_any(&mut g);
                format!(
                    "another target {}",
                    historic_permanent(
                        &g,
                        controller_first(&g, filter_phrase(&g, self.card, false))
                    )
                )
            }
            // "target creature you control with power 2 or less".
            Target::Filter(f) => format!(
                "target {}",
                historic_permanent(f, controller_first(f, filter_phrase(f, self.card, false)))
            ),
            Target::Spell(f) => self.spell_target_phrase(f),
            Target::SpellOr(f) => format!("target spell or {}", filter_phrase(f, self.card, false)),
            Target::GraveyardCard {
                filter,
                who,
                graveyards,
            } => {
                let whose = match who {
                    _ if matches!(
                        filter.that_player,
                        Some(mtg_core::card::ThatPlayer::Target(_))
                    ) =>
                    {
                        if filter.their {
                            "their graveyard"
                        } else {
                            "target player's graveyard"
                        }
                    }
                    _ if filter.that_player.is_some() => "that player's graveyard",
                    Who::You => "your graveyard",
                    Who::Opponents => "an opponent's graveyard",
                    Who::Any if *graveyards => "graveyards",
                    Who::Any => "a graveyard",
                };
                let mut g = filter.clone();
                g.that_player = None;
                g.their = false;
                let tgt = if g.relation == Relation::Other {
                    crate::format::relation_any(&mut g);
                    "another target"
                } else {
                    "target"
                };
                format!("{tgt} {} from {whose}", cards_phrase(&g, self.card, false))
            }
        }
    }

    /// A recipient as the object of a verb.
    #[allow(clippy::too_many_lines)] // one arm per recipient
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
            Recipient::TheTarget(i) => {
                self.target_mentioned[i] = true;
                format!("the {}", self.target_noun[i])
            }
            // "put two +1/+1 counters on the other".
            Recipient::OtherTarget(i) => {
                self.target_mentioned[i] = true;
                "the other".to_string()
            }
            Recipient::ChosenTarget(i) => {
                self.target_mentioned[i] = true;
                self.chosen_said = Some(i);
                format!("the chosen {}", self.target_noun[i])
            }
            // "Exile the chosen creature, then its controller gains life
            // equal to its mana value."
            Recipient::ThatControllerOf(i) if self.chosen_said == Some(i) => {
                "its controller".to_string()
            }
            Recipient::TheTargetFull(i, chosen) => {
                let phrase = self.target_phrase_of(i);
                self.target_mentioned[i] = true;
                let phrase = phrase
                    .strip_prefix("target ")
                    .unwrap_or(&phrase)
                    .to_string();
                format!("the {}{phrase}", if chosen { "chosen " } else { "" })
            }
            Recipient::ChosenTargets(a, n) => format!("the chosen {}", self.group_noun(a, n)),
            Recipient::You => "you".to_string(),
            Recipient::EachOpponent => {
                self.opponents_object = true;
                "each opponent".to_string()
            }
            Recipient::EachOtherPlayer => "each other player".to_string(),
            Recipient::EachOfYourOpponents => "each of your opponents".to_string(),
            Recipient::AnOpponent => "an opponent".to_string(),
            Recipient::Source => self.source(),
            Recipient::SourceNamed => {
                self.source_mentioned = true;
                self.source_name()
            }
            Recipient::SourceIt => {
                self.source_mentioned = true;
                "it".to_string()
            }
            Recipient::SourceCard => {
                self.source_mentioned = true;
                "this card".to_string()
            }
            // "When enchanted creature is dealt damage, destroy it."
            Recipient::TriggeringObject if self.trigger_is_enchanted() => "it".to_string(),
            // "Whenever enchanted creature deals damage to a creature,
            // destroy the other creature."
            Recipient::TriggeringObject if self.trigger_other_of_enchanted() => {
                "the other creature".to_string()
            }
            Recipient::SameObject if self.delayed_that && self.trigger_other_of_enchanted() => {
                "the other creature".to_string()
            }
            // "At the beginning of the upkeep of enchanted land's
            // controller, destroy that land".
            Recipient::TriggeringObject
                if matches!(self.trigger, Some(Trigger::AttachedStep(_))) =>
            {
                format!("that {}", enchanted_noun(self.card))
            }
            // "Whenever a Vehicle you control attacks, that Vehicle gains
            // first strike".
            Recipient::TriggeringObject
                if matches!(self.trigger, Some(Trigger::Attacks(TriggerObj::Filter(f)))
                    if f.subtypes.iter().any(|s| s == "Vehicle")) =>
            {
                "that Vehicle".to_string()
            }
            // "Whenever this creature saddles a Mount or crews a Vehicle,
            // that Mount or Vehicle gains flying".
            Recipient::TriggeringObject
                if matches!(self.trigger, Some(Trigger::CrewsOrSaddles(_))) =>
            {
                "that Mount or Vehicle".to_string()
            }
            // "..., counter that spell or ability" (Shimmering Glasskite).
            Recipient::TriggeringObject
                if matches!(self.trigger, Some(Trigger::TargetedFirstTime)) =>
            {
                "that spell or ability".to_string()
            }
            Recipient::TriggeringObject | Recipient::SameThat | Recipient::Manifested => {
                "that creature".to_string()
            }
            Recipient::TheTriggeringObject => "the creature".to_string(),
            Recipient::SameToken | Recipient::CreatedThatToken => "that token".to_string(),
            Recipient::TriggeringPlayer | Recipient::ThatOwner(_) => "that player".to_string(),
            Recipient::TriggeringCard | Recipient::SameCard => "that card".to_string(),
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
            // "any target": "that player or that permanent's controller"
            // (Chain Lightning).
            Recipient::ThatControllerOf(i) if self.target_noun[i] == "permanent or player" => {
                "that player or that permanent's controller".to_string()
            }
            Recipient::ThatControllerOf(i) => format!("that {}'s controller", self.target_noun[i]),
            Recipient::TheControllerOf(i) => format!("the {}'s controller", self.target_noun[i]),
            Recipient::TheTriggeringController => "the creature's controller".to_string(),
            Recipient::OwnerOf(_) => "its owner".to_string(),
            Recipient::DefendingPlayer => "the player or planeswalker it's attacking".to_string(),
            Recipient::DefendingPlayerWord => "defending player".to_string(),
            Recipient::EachCreature => "each creature".to_string(),
            Recipient::EachPlayer => "each player".to_string(),
            // "Creatures you control get +2/+1 until end of turn. Untap
            // those creatures."
            Recipient::EachCreatureYouControl => {
                if self.groups_mentioned.insert(r) {
                    "each creature you control".to_string()
                } else {
                    "those creatures".to_string()
                }
            }
            Recipient::EachPermanentYouControl => {
                self.groups_mentioned.insert(r);
                "each permanent you control".to_string()
            }
            Recipient::SameObject if self.delayed_that => "that creature".to_string(),
            Recipient::CreatedTokens | Recipient::SameTokens | Recipient::CreatedThose => {
                "those tokens".to_string()
            }
            Recipient::CreatedThem | Recipient::SameThem | Recipient::TriggeringThey => {
                "them".to_string()
            }
            Recipient::Those(t) => crate::format::those_word(t).to_string(),
            Recipient::SameObject | Recipient::CreatedObject | Recipient::TriggeringIt => {
                "it".to_string()
            }
            Recipient::CreatedThat => "that creature".to_string(),
            Recipient::CreatedThatLand => "that land".to_string(),
            Recipient::Group(i) => {
                self.groups_mentioned.insert(r);
                // "each creature and planeswalker they control": Oracle
                // joins these two types with "and" after "each".
                let phrase = self.group_phrase(i, false);
                let phrase = match phrase.strip_prefix("creature or planeswalker") {
                    Some(rest) => format!("creature and planeswalker{rest}"),
                    None => phrase,
                };
                format!("each {phrase}")
            }
            // "enchanted creature's controller sacrifices it".
            Recipient::AttachedController => {
                self.attached_mentioned = true;
                format!(
                    "{}'s controller",
                    filter_phrase(&Filter::enchanted(), self.card, false)
                )
            }
            Recipient::DamagedThisWay => "a creature dealt damage this way".to_string(),
            Recipient::OpponentsWithout(i) => match self.card.groups.get(usize::from(i)) {
                Some(f) => format!(
                    "each opponent who doesn't control {}",
                    filter_a(f, self.card)
                ),
                None => "each opponent".to_string(),
            },
            Recipient::RandomOf(i) => match self.card.groups.get(usize::from(i)) {
                Some(f) => format!("{} chosen at random", filter_a(f, self.card)),
                None => "a permanent chosen at random".to_string(),
            },
            // "When this Aura enters, if enchanted creature is a Wolf or
            // Werewolf, untap that creature." (Howl of the Hunt).
            Recipient::Attached if self.attached_by_cond && self.attached_by_type => {
                self.attached_by_cond = false;
                "that creature".to_string()
            }
            Recipient::Attached if self.attached_mentioned => "it".to_string(),
            Recipient::Attached => {
                self.attached_mentioned = true;
                // "tap enchanted permanent" on an Aura whose static says
                // "Enchanted permanent" (Flood the Engine).
                let permanent = self.card.abilities.iter().any(
                    |a| matches!(a, Ability::Static { filter, .. } if filter.enchanted_permanent),
                );
                if permanent {
                    "enchanted permanent".to_string()
                } else {
                    filter_phrase(&Filter::enchanted(), self.card, false)
                }
            }
            Recipient::BlockingIt => "each creature blocking it".to_string(),
            Recipient::TargetAnd(t, i) => self.target_and(t, i),
            // "Enchanted creature and other creatures that share a creature
            // type with it".
            Recipient::AttachedAnd(i) => {
                let first = self.obj(Recipient::Attached);
                let rest = self.card.groups.get(usize::from(i)).map_or_else(
                    || "other creatures".to_string(),
                    |f| {
                        let mut g = f.clone();
                        g.same = None;
                        filter_phrase(&g, self.card, true)
                    },
                );
                format!("{first} and {rest} that share a creature type with it")
            }
            Recipient::YouAndTarget(a) => format!("you and {}", self.obj(Recipient::Target(a))),
            Recipient::SourceAndTarget(a) => {
                let first = self.obj(Recipient::Source);
                format!("{first} and {}", self.obj(Recipient::Target(a)))
            }
            Recipient::ExiledCard => "the exiled card".to_string(),
            // The granting card is the quoted ability's holder's name
            // (`quoted_as`): "Sacrifice Blazing Torch: Blazing Torch
            // deals 2 damage to any target."
            Recipient::Granter => self.card.name.clone(),
            Recipient::ExiledWith(i) => {
                // CR 607.2a: "all cards exiled with this artifact", "each
                // creature card exiled with this land".
                let f = self
                    .card
                    .groups
                    .get(usize::from(i))
                    .cloned()
                    .unwrap_or_default();
                let all = f.all;
                let g = Filter {
                    all: false,
                    all_word: false,
                    each: false,
                    ..f
                };
                let with = this_ref(self.card);
                if all {
                    format!(
                        "all {} exiled with {with}",
                        cards_phrase(&g, self.card, true)
                    )
                } else {
                    format!(
                        "each {} exiled with {with}",
                        cards_phrase(&g, self.card, false)
                    )
                }
            }
            Recipient::ThoseCards(..) | Recipient::SameCards => "those cards".to_string(),
            Recipient::SameExiledCards => "the exiled cards".to_string(),
            Recipient::ThoseTargets(a, n) => format!("those {}", self.group_noun(a, n)),
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
        let phrase = if matches!(
            self.targets[target].base(),
            Target::Spec(TargetSpec::CreatureCardInYourGraveyard) | Target::GraveyardCard { .. }
        ) {
            // "and all other cards with the same name as that card".
            format!("{} cards", card_filter_phrase(&rest, self.card).trim())
        } else {
            filter_phrase(&rest, self.card, plural_form)
        };
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
            Some(AttachedRef::TriggeringObject) => format!("{phrase} attached to that creature"),
            _ => phrase,
        }
    }

    /// A recipient as the subject of a sentence.
    /// A trigger of an Aura about the enchanted creature and another
    /// creature (blocking, or damage dealt to it): Oracle calls that one
    /// "the other creature".
    fn trigger_other_of_enchanted(&self) -> bool {
        // An Equipment's "equipped creature" keeps "that creature" (Wooden
        // Stake).
        if self.card.subtypes.iter().any(|s| s == "Equipment") {
            return false;
        }
        self.trigger.is_some_and(|t| {
            let t = match t {
                Trigger::When(inner) => inner,
                t => t,
            };
            matches!(
                t,
                Trigger::BlockPair { obj: TriggerObj::Filter(f), .. }
                    | Trigger::DealsDamageTo { obj: TriggerObj::Filter(f), .. }
                    | Trigger::DealsDamage {
                        obj: TriggerObj::Filter(f),
                        to: mtg_core::card::DamageTo::Creature,
                        ..
                    }
                    if f.enchanted
            )
        })
    }

    fn trigger_is_enchanted(&self) -> bool {
        fn enchanted(t: &Trigger) -> bool {
            let t = match t {
                Trigger::When(inner) => inner,
                // "When enchanted creature becomes tapped or is dealt
                // damage".
                Trigger::Or { first, .. } => return enchanted(first),
                t => t,
            };
            matches!(
                t,
                Trigger::Dies(TriggerObj::Filter(f))
                    | Trigger::DiesOrExiled(TriggerObj::Filter(f))
                    | Trigger::DealtDamageAtLeast(TriggerObj::Filter(f), _)
                    | Trigger::DealtDamage(TriggerObj::Filter(f))
                    | Trigger::DealtCombatDamage(TriggerObj::Filter(f))
                    | Trigger::Blocks(TriggerObj::Filter(f))
                    | Trigger::Attacks(TriggerObj::Filter(f))
                    | Trigger::BecomesTapped(TriggerObj::Filter(f))
                    if f.enchanted
            )
        }
        self.trigger.is_some_and(enchanted)
    }

    fn subj(&mut self, r: Recipient) -> String {
        match r {
            Recipient::TriggeringThey => "they".to_string(),
            // "Untap them. They gain haste until end of turn."; "Create
            // two 1/1 red Warrior creature tokens. They gain haste until
            // end of turn." (Salt Road Skirmish).
            Recipient::Those(crate::ast::Those::Them) | Recipient::CreatedTokens => {
                "they".to_string()
            }
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
                } else if self
                    .card
                    .groups
                    .get(usize::from(i))
                    .is_some_and(|f| f.all_word)
                {
                    format!("all {}", self.group_phrase(i, true))
                } else if self.each_group(r) {
                    format!("each {}", self.group_phrase(i, false))
                } else {
                    self.group_phrase(i, true)
                }
            }
            Recipient::Source => self.source_subj(),
            // "You and target opponent each draw a card."
            Recipient::YouAndTarget(a) => format!("you and {}", self.subj(Recipient::Target(a))),
            // "This creature and up to one other target creature each get
            // +3/+3 until end of turn."
            Recipient::SourceAndTarget(a) => {
                let first = self.source_subj();
                format!("{first} and {}", self.subj(Recipient::Target(a)))
            }
            // "Put a +1/+1 counter on Metalhead. He gains menace and haste
            // until end of turn."
            Recipient::SourceIt => {
                self.source_mentioned = true;
                it_subj(self.card).to_string()
            }
            // "When this Aura enters, if enchanted creature has toxic, that
            // creature gains hexproof" (Maze's Mantle).
            Recipient::Attached if self.attached_by_cond => {
                self.attached_by_cond = false;
                "that creature".to_string()
            }
            Recipient::EachOpponent => {
                self.opponents_object = false;
                "each opponent".to_string()
            }
            other => self.obj(other),
        }
    }

    /// A group in a combat relation to the source is a subject one at a
    /// time: "Each creature blocking this creature gets -1/-1".
    fn each_group(&self, r: Recipient) -> bool {
        matches!(r, Recipient::Group(i)
            if self.card.groups.get(usize::from(i)).is_some_and(|f| f.combat_with.is_some() || f.each))
    }

    /// An object, a group written `all` printed so ("sacrifice all
    /// non-Ogre creatures you control") where `each` prints "each".
    fn obj_written(&mut self, r: Recipient) -> String {
        match r {
            Recipient::Group(i) if self.card.groups.get(usize::from(i)).is_some_and(|f| f.all) => {
                self.groups_mentioned.insert(r);
                format!("all {}", self.group_phrase(i, true))
            }
            _ => self.obj(r),
        }
    }

    /// A group written `each single`, printed one at a time.
    fn single_group(&self, r: Recipient) -> bool {
        matches!(r, Recipient::Group(i)
            if self.card.groups.get(usize::from(i)).is_some_and(|f| f.each))
    }

    /// Objects of destruction and exile ("all creatures").
    fn obj_all(&mut self, r: Recipient) -> String {
        match r {
            Recipient::EachCreature => "all creatures".to_string(),
            Recipient::EachCreatureYouControl => "all creatures you control".to_string(),
            Recipient::EachPermanentYouControl => "all permanents you control".to_string(),
            // `each single`: "Destroy each creature with mana value 3 or
            // less."
            Recipient::Group(i) if self.single_group(r) => {
                format!("each {}", self.group_phrase(i, false))
            }
            // "Destroy all artifacts and creatures with mana value X or
            // less": Oracle joins a group's alternative types with "and".
            Recipient::Group(i)
                if self
                    .card
                    .groups
                    .get(usize::from(i))
                    .is_some_and(|f| f.any_types.len() == 2) =>
            {
                format!("all {}", self.group_phrase(i, true)).replacen(" or ", " and ", 1)
            }
            Recipient::Group(i) => format!("all {}", self.group_phrase(i, true)),
            other => self.obj(other),
        }
    }

    /// A list of recipients, merging fresh targets of one kind ("two target
    /// lands").
    /// Whether `rs` names several targets of one phrase not mentioned
    /// before ("up to two target creatures"), as `obj_list` would print
    /// them together.
    fn fresh_target_group(&self, rs: &[Recipient]) -> bool {
        match rs {
            [Recipient::Targets(a, n)] => *n > 1 && !self.target_mentioned[*a],
            // "deals five times X damage to each of up to X targets".
            [Recipient::Target(i)] => {
                !self.target_mentioned[*i] && matches!(self.targets[*i], Target::Many { .. })
            }
            [Recipient::Target(first), _, ..] => rs.iter().all(|r| {
                matches!(r, Recipient::Target(i)
                    if !self.target_mentioned[*i] && self.targets[*i] == self.targets[*first])
            }),
            _ => false,
        }
    }

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
            // "up to two other target creatures you control" (CR 115.3).
            let (other, noun) = match noun
                .strip_prefix("other ")
                .or_else(|| noun.strip_prefix("another "))
            {
                Some(rest) => ("other ", rest.to_string()),
                None => ("", noun),
            };
            return format!(
                "{up_to}{} {other}target {noun}",
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
    /// Targets `a..a + n` named together: "creatures you control" for
    /// targets alike, the plain noun ("those creatures") for targets of
    /// different kinds ("target creature you control and target creature
    /// you don't control").
    fn group_noun(&self, a: usize, n: usize) -> String {
        let alike = (a + 1..a + n).all(|i| self.targets.get(i) == self.targets.get(a));
        // "for each opponent, tap up to one target creature that player
        // controls. Put a stun counter on each of those creatures."
        let per_opponent = matches!(self.targets.get(a).map(Target::base),
            Some(Target::Filter(f)) if f.per_opponent);
        // "Choose two target creatures controlled by different players.
        // Return those creatures ...": the relation isn't repeated.
        let related = matches!(self.targets.get(a).map(Target::base),
            Some(Target::Filter(f)) if matches!(f.same, Some(SameRef::DifferentControllers
                | SameRef::SameController | SameRef::SameOpponent)));
        if alike && !per_opponent && !related {
            self.plural_target_noun(a)
        } else {
            pluralize(&self.target_noun[a])
        }
    }

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
            // "two target artifacts and/or enchantments": Oracle's plural
            // of a target of either type (but "attacking or blocking
            // creatures", one type with either quality).
            Target::Spec(s @ TargetSpec::AttackingOrBlockingCreature) => {
                pluralize(&target_phrase(*s).replace("target ", ""))
            }
            Target::Spec(s) => {
                // Each kind plural: "artifacts and/or enchantments".
                let phrase = target_phrase(*s).replace("target ", "");
                match phrase.split_once(" or ") {
                    Some((a, b)) if !a.contains(' ') => {
                        format!("{} and/or {}", pluralize(a), pluralize(b))
                    }
                    _ => pluralize(&phrase).replace(" or ", " and/or "),
                }
            }
            Target::CreatureOfSubtype(sub) => format!("{sub} creatures"),
            Target::Filter(f) => filter_phrase(f, self.card, true),
            Target::Spell(f) => format!("{} spells", filter_phrase(f, self.card, false)),
            Target::SpellOr(f) => format!("spells and/or {}", filter_phrase(f, self.card, true)),
            Target::GraveyardCard {
                filter,
                who,
                graveyards,
            } => {
                let whose = match who {
                    _ if matches!(
                        filter.that_player,
                        Some(mtg_core::card::ThatPlayer::Target(_))
                    ) =>
                    {
                        if filter.their {
                            "their graveyard"
                        } else {
                            "target player's graveyard"
                        }
                    }
                    _ if filter.that_player.is_some() => "that player's graveyard",
                    Who::You => "your graveyard",
                    Who::Opponents => "an opponent's graveyard",
                    Who::Any if *graveyards => "graveyards",
                    Who::Any => "a single graveyard",
                };
                let mut filter = filter.clone();
                filter.that_player = None;
                filter.their = false;
                let each = std::mem::take(&mut filter.each);
                let filter = &filter;
                if each {
                    let p = filter_phrase(filter, self.card, false);
                    // "... cards each with mana value 3 or less".
                    match p.split_once(" with ") {
                        Some((kind, quality)) => {
                            format!("{kind} cards each with {quality} from {whose}")
                        }
                        None => format!("{p} cards from {whose}"),
                    }
                } else {
                    // "one or two target permanent cards from your graveyard",
                    // as one is "target permanent card".
                    format!("{} from {whose}", cards_phrase(filter, self.card, true))
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
                // `each single`: "untap each other creature you control".
                Recipient::Group(i) if self.single_group(*r) => {
                    format!("each {}", self.group_phrase(*i, false))
                }
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
                | Recipient::ThoseTargets(..)
                | Recipient::ChosenTargets(..)
                | Recipient::TargetAnd(..)
                | Recipient::AttachedAnd(_)
                | Recipient::YouAndTarget(_)
                | Recipient::SourceAndTarget(_)
                | Recipient::CreatedTokens
                | Recipient::CreatedThem
                | Recipient::CreatedThose
                | Recipient::SameTokens
                | Recipient::SameCards
                | Recipient::SameExiledCards
                | Recipient::SameThem
                | Recipient::Those(_)
        )
    }

    fn verb(&self, r: Recipient, singular: &'static str, plural: &'static str) -> &'static str {
        if r == Recipient::You
            || r == Recipient::TriggeringThey
            || (Self::is_plural(r) && !self.each_group(r))
            || self.is_many(r)
        {
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
        Target::SpellOr(f) if f.types == [CardType::Creature] => "spell or creature".to_string(),
        Target::SpellOr(_) => "spell or permanent".to_string(),
        Target::GraveyardCard { .. } => "card".to_string(),
        Target::Spec(s) => match s {
            TargetSpec::Any | TargetSpec::AnyOther | TargetSpec::AnyDamaged => {
                "permanent or player".to_string()
            }
            TargetSpec::Creature
            | TargetSpec::TappedCreature
            | TargetSpec::CreatureYouControl
            | TargetSpec::OtherCreature
            | TargetSpec::NonlegendaryCreatureYouControl
            | TargetSpec::CreatureWithSubtype(_)
            | TargetSpec::AttackingOrBlockingCreature
            | TargetSpec::AttackingCreature
            | TargetSpec::BlockingCreature => "creature".to_string(),
            TargetSpec::PlayerOrPlaneswalker
            | TargetSpec::PlayerOrBattle
            | TargetSpec::Player
            | TargetSpec::Opponent
            | TargetSpec::OpponentOrPlaneswalker => "player".to_string(),
            TargetSpec::Land => "land".to_string(),
            TargetSpec::ArtifactCreatureOrLand
            | TargetSpec::ArtifactOrEnchantment
            // "that permanent's controller" (Unleash Shell).
            | TargetSpec::CreatureOrPlaneswalker
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
            TargetSpec::SpellOr(_) => "spell or permanent".to_string(),
        },
        Target::Filter(f) => {
            // "creature or planeswalker ... that's red or white": "that
            // permanent" (Flourishing Grapple).
            if f.types.contains(&CardType::Creature)
                && f.or
                    .as_ref()
                    .is_none_or(|a| a.types.is_empty() || a.types.contains(&CardType::Creature))
            {
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
        yours,
        single,
        instant_sorcery,
        listed,
    } = s
    {
        let kinds = match (activated, triggered) {
            _ if instant_sorcery => {
                "instant spell, sorcery spell, activated ability, or triggered ability"
            }
            _ if listed => "spell, activated ability, or triggered ability",
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
        // "target triggered ability you control from a creature source".
        let yours = if yours { " you control" } else { "" };
        let single = if single { " with a single target" } else { "" };
        return format!("target {kinds}{yours}{from}{single}");
    }
    match s {
        TargetSpec::OnStack { .. } => unreachable!("above"),
        TargetSpec::Any => "any target",
        TargetSpec::AnyOther => "any other target",
        TargetSpec::AnyDamaged => "any target that was dealt damage this turn",
        TargetSpec::Creature | TargetSpec::CreatureWithSubtype(_) => "target creature",
        TargetSpec::PlayerOrPlaneswalker => "target player or planeswalker",
        TargetSpec::PlayerOrBattle => "target player or battle",
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
        TargetSpec::SpellOr(_) => "target spell or permanent",
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
        // "Enchant player" (CR 303.4): a Curse.
        Some(Target::Spec(TargetSpec::Player)) => "player",
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

/// "This land enters tapped with two depletion counters on it.": one
/// sentence for an "enters tapped" followed by an "enters with counters".
fn enters_tapped_with(card: &Card, i: usize) -> Option<String> {
    let (
        Ability::Replacement(Replacement {
            event: Replaces::Enters,
            outcome: Outcome::EnterTapped,
            then: t1,
            ..
        }),
        Some(Ability::Replacement(Replacement {
            event: Replaces::Enters,
            outcome: Outcome::EnterWithCounters { kind, n },
            then: t2,
            ..
        })),
    ) = (&card.abilities[i], card.abilities.get(i + 1))
    else {
        return None;
    };
    (t1.is_empty() && t2.is_empty()).then(|| {
        format!(
            "{} enters tapped with {} on {}.",
            capitalize(&this_ref(card)),
            counters(*n, *kind),
            it_obj(card)
        )
    })
}

/// `enchanted_noun`, or the land type an Aura enchants ("enchanted
/// Forest" for "Enchant Forest").
fn enchanted_subtype_noun(card: &Card) -> String {
    let sub = card.abilities.iter().find_map(|a| match a {
        Ability::Enchant(Target::Filter(f)) if f.types.is_empty() => match f.subtypes.as_slice() {
            [s] => Some(s.clone()),
            _ => None,
        },
        _ => None,
    });
    sub.unwrap_or_else(|| enchanted_noun(card).to_string())
}

// ---- filters ----

/// The noun phrase of a filter without an article, singular ("creature you
/// control", "nonbasic land") or plural.
#[allow(clippy::too_many_lines)] // one block per filter part
fn filter_phrase(f: &Filter, card: &Card, plural_form: bool) -> String {
    // "two target creatures controlled by the same opponent": the
    // opponent is in the relation.
    if f.same == Some(SameRef::SameOpponent) && f.who.is_some() {
        let g = Filter {
            who: None,
            ..f.clone()
        };
        return filter_phrase(&g, card, plural_form);
    }
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
            // "the number of Equipment attached to him" (Whiplash).
            AttachedRef::Source => format!("{p} attached to {}", it_obj(card)),
            AttachedRef::Host => format!("{p} attached to it"),
            AttachedRef::Target(_) | AttachedRef::ThatTarget(_) | AttachedRef::TriggeringObject => {
                p
            }
        };
    }
    let Some(alt) = &f.or else {
        return filter_phrase_one(f, card, plural_form);
    };
    let mut base = f.clone();
    base.or = None;
    // "target attacking, blocking, or tapped creature" (Sonar Strike).
    if let Some(rest) = crate::format::attacking_blocking_or_tapped(&base, alt) {
        return format!(
            "attacking, blocking, or tapped {}",
            filter_phrase_one(&rest, card, plural_form)
        );
    }
    if let Some(rest) = crate::format::attacking_or_blocking(&base, alt) {
        // "each face-down creature that's attacking or blocking".
        if rest.attacking_late {
            return format!(
                "{} that's attacking or blocking",
                filter_phrase_one(&rest, card, plural_form)
            );
        }
        return format!(
            "attacking or blocking {}",
            filter_phrase_one(&rest, card, plural_form)
        );
    }
    // `attacking or tapped creature`: "an attacking or tapped creature"
    // (only creatures attack).
    if base
        == (Filter {
            attacking: true,
            ..Filter::default()
        })
        && alt.or.is_none()
        && !alt.attacking
    {
        return format!("attacking or {}", filter_phrase_one(alt, card, plural_form));
    }
    // "creature or planeswalker that's blue or red" (Terminal
    // Criticism): two or more colors both types share, after the types.
    if !plural_form
        && alt.or.is_none()
        && base.colors.as_ref().is_some_and(|cs| cs.len() > 1)
        && base.colors == alt.colors
        && !base.types.is_empty()
        && !alt.types.is_empty()
    {
        let mut b = base.clone();
        b.colors = None;
        let mut a = (**alt).clone();
        a.colors = None;
        // "creature or planeswalker an opponent controls that's red or
        // white" (Flourishing Grapple): one controller for both, printed
        // after the second type.
        if b.who.is_some() && b.who == a.who {
            b.who = None;
        }
        if (Filter {
            types: Vec::new(),
            ..b.clone()
        }) == Filter::default()
            && (Filter {
                types: Vec::new(),
                who: None,
                ..a.clone()
            }) == Filter::default()
        {
            let colors: Vec<String> = base
                .colors
                .iter()
                .flatten()
                .map(|c| color_name(*c).to_string())
                .collect();
            return format!(
                "{} or {} that's {}",
                filter_phrase_one(&b, card, false),
                filter_phrase_one(&a, card, false),
                or_list(&colors)
            );
        }
    }
    // "each other creature you control that's a token or a Rabbit"
    // (Finneas, Ace Archer): a token, or a creature of one subtype.
    if !plural_form
        && alt.or.is_none()
        && base.status.token == Some(true)
        && base.subtypes.is_empty()
        && alt.subtypes.len() == 1
        && alt.status.token.is_none()
    {
        let mut b = base.clone();
        b.status.token = None;
        let mut a = (**alt).clone();
        let sub = a.subtypes.remove(0);
        if a == b {
            return format!(
                "{} that's a token or {}",
                filter_phrase_one(&b, card, false),
                with_article(&sub)
            );
        }
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
    // "Skeletons you control and other Zombies you control" (Death
    // Baron): when only one side is "other", each names its controller.
    if plural_form
        && alt.or.is_none()
        && base.who == Some(Who::You)
        && alt.who == Some(Who::You)
        && base.relation == Relation::Any
        && alt.relation == Relation::Other
    {
        return format!(
            "{} and {}",
            filter_phrase_one(&base, card, true),
            filter_phrase_one(alt, card, true)
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
    // "an instant, sorcery, or Dragon card": types that are alternatives
    // of their own ("instant or sorcery") join the list.
    if !plural_form
        && sides
            .iter()
            .all(|s| !s.contains(' ') || s.split(" or ").all(|w| !w.contains(' ')))
        && sides.iter().any(|s| s.contains(" or "))
    {
        sides = sides
            .iter()
            .flat_map(|s| s.split(" or ").map(str::to_string).collect::<Vec<_>>())
            .collect();
    }
    // In the plural the alternatives join with "and": "Ninjas and Rogues
    // you control", "Bears, Spiders, and Wolves"; `andor` prints
    // "and/or" ("up to two target Mounts and/or Vehicles").
    // `andlist`: "each Pest, Bat, Insect, Snake, and Spider".
    let conj = match (plural_form, f.and_or) {
        (true, true) => "and/or",
        (true, false) => "and",
        (false, _) if f.and_list => "and",
        (false, _) => "or",
    };
    if sides.len() == 2 {
        format!("{} {conj} {}", sides[0], sides[1])
    } else {
        let last = sides.pop().expect("three or more");
        format!("{}, {conj} {last}", sides.join(", "))
    }
}

#[allow(clippy::too_many_lines)] // one clause per qualifier
fn filter_phrase_one(f: &Filter, card: &Card, plural_form: bool) -> String {
    // "a spell that's both white and black" (CR 105.2).
    if f.status.all_colors
        && let Some(cs) = &f.colors
    {
        let mut g = f.clone();
        g.colors = None;
        g.status.all_colors = false;
        let names: Vec<&str> = cs.iter().map(|c| color_name(*c)).collect();
        return format!(
            "{} {} both {}",
            filter_phrase_one(&g, card, plural_form),
            if plural_form { "that are" } else { "that's" },
            names.join(" and ")
        );
    }
    if f.enchanted {
        if is_equipment(card) {
            return "equipped creature".to_string();
        }
        if f.enchanted_permanent {
            return "enchanted permanent".to_string();
        }
        return format!("enchanted {}", enchanted_subtype_noun(card));
    }
    if f.types.is_empty() && f.not_types.contains(&CardType::Planeswalker) {
        let mut s = "artifacts, creatures, enchantments, and lands".to_string();
        if f.who == Some(Who::You) {
            s.push_str(" you control");
        }
        return s;
    }
    // "a commander that's a creature or planeswalker" (CR 903.3).
    if f.status.commander == Some(false) && !f.any_types.is_empty() && f.types.is_empty() {
        let names: Vec<String> = f
            .any_types
            .iter()
            .map(|t| type_noun(*t).to_string())
            .collect();
        return format!(
            "commander that's {} {}",
            article(&names[0]),
            or_list(&names)
        );
    }
    let mut words: Vec<String> = Vec::new();
    if f.relation == Relation::Other && !f.other_than_this {
        words.push("other".to_string());
    }
    if f.status.enchanted == Some(true) && !f.enchanted_late {
        words.push("enchanted".to_string());
    }
    if f.status.equipped && !f.equipped_late {
        words.push("equipped".to_string());
    }
    if let Some(t) = f.tapped {
        words.push(if t { "tapped" } else { "untapped" }.to_string());
    }
    if f.status.nonattacking {
        words.push("nonattacking".to_string());
    }
    // "a nontoken modified creature".
    let nontoken_first = f.status.modified && f.status.token == Some(false);
    if nontoken_first {
        words.push("nontoken".to_string());
    }
    if f.status.modified {
        words.push("modified".to_string());
    }
    if f.status.face_down {
        words.push("face-down".to_string());
    }
    if f.status.transformed {
        words.push("transformed".to_string());
    }
    if f.status.suspected == Some(true) {
        words.push("suspected".to_string());
    }
    // CR 903.3: "commander creatures you own", "a commander".
    if f.status.commander.is_some() && !(f.types.is_empty() && f.any_types.is_empty()) {
        words.push("commander".to_string());
    }
    if let Some(b) = f.status.blocked {
        words.push(if b { "blocked" } else { "unblocked" }.to_string());
    }
    if f.attacking && !f.attacking_late {
        words.push("attacking".to_string());
    }
    if f.blocking {
        words.push("blocking".to_string());
    }
    if f.status.equipped && f.equipped_late {
        words.push("equipped".to_string());
    }
    if let Some(cc) = f.color_count
        && cc != mtg_core::card::ColorCount::Colored
    {
        words.push(format!("{cc:?}").to_lowercase());
    }
    if let Some(colors) = &f.colors {
        let list = or_list(
            &colors
                .iter()
                .map(|c| color_name(*c).to_string())
                .collect::<Vec<_>>(),
        );
        // `andor white or blue creature`: "each white and/or blue
        // creature" (Ember Gale).
        words.push(
            if f.and_or && f.any_types.is_empty() && f.subtypes.is_empty() {
                list.replace(" or ", " and/or ")
            } else {
                list
            },
        );
    }
    for s in &f.supertypes {
        words.push(format!("{s:?}").to_lowercase());
    }
    // "noncreature, nonland card", "nonartifact, nonblack creature":
    // several negations are listed, supertypes first ("nonlegendary,
    // nonland card"), types before colors; "nontoken" after the types
    // ("nonland, nontoken permanent", "nontoken, non-Angel creature").
    let mut negs: Vec<String> = f
        .not_supertypes
        .iter()
        .map(|s| {
            match s {
                Supertype::Basic => "nonbasic",
                Supertype::Legendary => "nonlegendary",
                Supertype::Snow => "nonsnow",
                Supertype::World => "nonworld",
            }
            .to_string()
        })
        .collect();
    // "non-Faerie, nonland permanent" (Nurturing Pixie).
    if f.subtype_neg_first {
        negs.extend(f.not_subtypes.iter().map(|s| format!("non-{s}")));
    }
    negs.extend(f.not_types.iter().map(|t| format!("non{}", type_noun(*t))));
    if f.status.token == Some(false) && !nontoken_first {
        negs.push("nontoken".to_string());
    }
    negs.extend(
        f.not_colors
            .iter()
            .map(|c| format!("non{}", color_name(*c))),
    );
    if !f.subtype_neg_first {
        negs.extend(f.not_subtypes.iter().map(|s| format!("non-{s}")));
    }
    if !negs.is_empty() {
        words.push(negs.join(", "));
    }
    if f.attacking && f.attacking_late {
        words.push("attacking".to_string());
    }
    let mut noun: Vec<String> = Vec::new();
    for s in &f.subtypes {
        noun.push(s.clone());
    }
    for t in &f.types {
        noun.push(type_noun(*t).to_string());
    }
    let any_types = !f.any_types.is_empty();
    // "a token", "artifact token" (CR 111.1).
    if f.status.token == Some(true) && !any_types {
        noun.push("token".to_string());
    }
    if noun.is_empty() && !any_types {
        noun.push(
            if f.status.commander.is_some() {
                "commander"
            } else {
                "permanent"
            }
            .to_string(),
        );
    }
    if plural_form && !noun.is_empty() {
        let last = noun.pop().expect("nonempty");
        // "four Eldrazi Spawn" (Hand of Emrakul); alone, "Spawns".
        if last == "Spawn" && noun.last().is_some_and(|w| w == "Eldrazi") {
            noun.push(last);
        } else {
            noun.push(pluralize(&last));
        }
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
        // `andor`: "two target artifacts and/or enchantments".
        if plural_form && f.and_or {
            noun.push(or_list(&names).replace(" or ", " and/or "));
        } else if f.and_list {
            // `andlist`: "artifacts and creatures you control" (Thorin
            // Oakenshield).
            noun.push(or_list(&names).replace(" or ", " and "));
        } else {
            noun.push(or_list(&names));
        }
    }
    words.extend(noun);
    let mut s = words.join(" ");
    // "creatures you control with defender": Oracle names your control
    // before the qualities (but "creatures with flying your opponents
    // control").
    if f.status.commander == Some(true) || f.status.you_own {
        s.push_str(" you own");
    }
    let you_first = f.who == Some(Who::You) && f.or.is_none() && !f.named_first && !f.control_late;
    if you_first {
        s.push_str(" you control");
    }
    // "with toughness greater than its power" ("their power" in the
    // plural).
    if f.status.toughness_over_power {
        let its = if plural_form { "their" } else { "its" };
        let _ = write!(s, " with toughness greater than {its} power");
    }
    for (name, v) in [("power", f.power), ("toughness", f.toughness)] {
        if let Some((cmp, n)) = v {
            if f.status.pt_either {
                // "with power or toughness 1 or less".
                if name == "power" {
                    let _ = write!(s, " with power or toughness {n}{}", cmp_words(cmp));
                }
            } else if f.status.spent_colors && name == "power" {
                let _ = write!(s, " with power {}", spent_at_most(card));
            } else {
                let _ = write!(s, " with {name} {n}{}", cmp_words(cmp));
            }
        }
    }
    if let Some((cmp, n)) = f.mana_value {
        if f.status.spent_colors {
            let _ = write!(s, " with mana value {}", spent_at_most(card));
        } else if f.status.mana_value_x {
            let _ = write!(s, " with mana value X{}", cmp_words(cmp));
        } else {
            let _ = write!(s, " with mana value {n}{}", cmp_words(cmp));
        }
    }
    if let Some(b) = f.status.bound {
        let _ = write!(s, " with {}", bound_phrase(b, card));
    }
    if let Some((cmp, n)) = f.status.pt_total {
        let _ = write!(s, " with total power and toughness {n}{}", cmp_words(cmp));
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
    // `permanent you control late of chosen type`: "each permanent of the
    // chosen type you control" (Pack's Disdain), the choice before the
    // controller.
    let chosen_first = f.control_late && f.who.is_some();
    if let Some(c) = f.chosen
        && chosen_first
    {
        s.push_str(of_chosen(c));
    }
    match f.who {
        Some(Who::You) if !you_first => s.push_str(" you control"),
        Some(Who::Opponents) if f.dont_control => s.push_str(" you don't control"),
        Some(Who::Opponents) if f.your_opponents => s.push_str(" your opponents control"),
        Some(Who::Opponents) if f.they_control => s.push_str(" they control"),
        Some(Who::Opponents) if f.per_opponent => s.push_str(" that player controls"),
        Some(Who::Opponents) if f.defending => s.push_str(" defending player controls"),
        Some(Who::Opponents) => s.push_str(" an opponent controls"),
        Some(Who::You | Who::Any) | None => {}
    }
    match f.that_player {
        // "for each tapped artifact they control" after "target player".
        Some(_) if f.they_control => s.push_str(" they control"),
        Some(mtg_core::card::ThatPlayer::Target(_)) if f.that_opponent => {
            s.push_str(" target opponent controls");
        }
        Some(mtg_core::card::ThatPlayer::Target(_)) => s.push_str(" target player controls"),
        Some(mtg_core::card::ThatPlayer::TargetOrController {
            planeswalker: true, ..
        }) => s.push_str(" that player or that planeswalker's controller controls"),
        Some(_) => s.push_str(" that player controls"),
        None => {}
    }
    if f.enchanted_player {
        s.push_str(" enchanted player controls");
    }
    if f.other_than_this {
        let _ = write!(s, " other than {}", this_ref(card));
    }
    if f.not_attached {
        let mut host = Filter::enchanted();
        host.types.clone_from(&f.types);
        let _ = write!(s, " other than {}", filter_phrase(&host, card, false));
    }
    if f.no_abilities {
        s.push_str(" with no abilities");
    }
    // "each other creature you control with toxic" (CR 702.164).
    if f.status.toxic {
        s.push_str(" with toxic");
    }
    // "target creature with a morph ability" (CR 702.37).
    if f.status.morph {
        s.push_str(" with a morph ability");
    }
    // "target Aura attached to a creature".
    match f.status.on {
        // "all permanents attached to creatures" (End Hostilities).
        Some(mtg_core::card::OnHost::Creature) if plural_form => {
            s.push_str(" attached to creatures");
        }
        Some(mtg_core::card::OnHost::Creature) => s.push_str(" attached to a creature"),
        Some(mtg_core::card::OnHost::Land) => s.push_str(" attached to a land"),
        None => {}
    }
    // "creature you control with a +1/+1 counter on it" (after the
    // controller, as Oracle prints it).
    if let Some(mtg_core::card::CounterQual::AtLeast(k, n)) = f.status.counter {
        let _ = write!(
            s,
            " with {} or more {} counters on {}",
            count_words(Num::N(n), "one"),
            counter_name(k),
            if plural_form { "them" } else { "it" }
        );
    } else if f.status.counter == Some(mtg_core::card::CounterQual::NoCounters) {
        s.push_str(if plural_form {
            " with no counters on them"
        } else {
            " with no counters on it"
        });
    } else if let Some(q) = f.status.counter {
        let kind = match q {
            mtg_core::card::CounterQual::AtLeast(..) | mtg_core::card::CounterQual::NoCounters => {
                unreachable!("above")
            }
            mtg_core::card::CounterQual::Any => String::new(),
            mtg_core::card::CounterQual::Kind(k) => format!("{} ", counter_name(k)),
        };
        if plural_form {
            let _ = write!(s, " with {kind}counters on them");
        } else {
            let a = if article(&kind) == "an" { "an" } else { "a" };
            let _ = write!(s, " with {a} {kind}counter on it");
        }
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
    if f.status.enchanted == Some(true) && f.enchanted_late {
        that(&mut s, " that's enchanted", " that are enchanted");
    }
    if let Some(r) = f.combat_with {
        let this = this_ref(card);
        let _ = match r {
            crate::ast::CombatRef::BlockingThis => write!(s, " blocking {this}"),
            crate::ast::CombatRef::BlockingIt => write!(s, " blocking it"),
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
    if f.status.attacked_this_turn {
        s.push_str(" that attacked this turn");
    }
    // "a creature dealt damage by this creature this turn".
    if f.status.damaged_by_source {
        let _ = write!(s, " dealt damage by {} this turn", this_ref(card));
    }
    if let Some(c) = f.chosen
        && !chosen_first
    {
        s.push_str(of_chosen(c));
    }
    if !f.named_first {
        s.push_str(&same_phrase(f, card, plural_form));
    }
    // "target creature that's one or more colors" (Reach of Shadows).
    if f.color_count == Some(mtg_core::card::ColorCount::Colored) {
        s.push_str(if plural_form {
            " that are one or more colors"
        } else {
            " that's one or more colors"
        });
    }
    s
}

/// "of the chosen type", "of that color": a filter's chosen color or type.
fn of_chosen(c: OfChosen) -> &'static str {
    match c {
        OfChosen::ChosenColor => " of the chosen color",
        OfChosen::ChosenType => " of the chosen type",
        OfChosen::ThatType => " of that type",
        OfChosen::ThatColor => " of that color",
        OfChosen::ColorOfChoice => " of the color of your choice",
        OfChosen::CreatureTypeOfChoice => " of the creature type of your choice",
        OfChosen::ChosenQuality => " with mana value of the chosen quality",
    }
}

/// "power less than or equal to the number of creatures you control",
/// "mana value X or less, where X is the number of Faeries you control"
/// (`count where`), "toughness less than this creature's power": a
/// characteristic compared with a count (`StatusQuals::bound`).
fn bound_phrase(b: mtg_core::card::StatBound, card: &Card) -> String {
    bound_words(b, card, "")
}

/// `bound_phrase` with a verb after the characteristic ("its toughness is
/// less than or equal to ...").
fn bound_words(b: mtg_core::card::StatBound, card: &Card, verb: &str) -> String {
    let stat = format!("{}{verb}", stat_name(b.stat));
    if b.n == Num::X {
        return format!("{stat} X or less");
    }
    // The parser takes only a count or X here.
    let Some(c) = (match b.n {
        Num::Count(i) => card.counts.get(usize::from(i)),
        _ => None,
    }) else {
        return stat;
    };
    let phrase = match c.expr {
        // "with power less than this creature's power".
        crate::ast::CountExpr::SourcePower => format!("{}'s power", this_ref(card)),
        crate::ast::CountExpr::SourceToughness => format!("{}'s toughness", this_ref(card)),
        _ => count_phrase(&c.expr, card, false),
    };
    if b.cmp == mtg_core::card::Cmp::Exactly {
        // "with mana value equal to the number of charge counters on this
        // artifact" (Engineered Explosives).
        return format!("{stat} equal to {phrase}");
    }
    match (c.style, b.strict) {
        (crate::ast::CountStyle::Where, false) => {
            format!("{stat} X or less, where X is {phrase}")
        }
        (_, false) => format!("{stat} less than or equal to {phrase}"),
        (_, true) => format!("{stat} less than {phrase}"),
    }
}

fn stat_name(s: mtg_core::card::Stat) -> &'static str {
    match s {
        mtg_core::card::Stat::Power => "power",
        mtg_core::card::Stat::Toughness => "toughness",
        mtg_core::card::Stat::ManaValue => "mana value",
    }
}

/// " named ~", " not named ~" (CR 201.4b: the card's own name), and the
/// relations to a target outside a recipient that names it (" with the
/// same name as it").
fn same_phrase(f: &Filter, card: &Card, plural_form: bool) -> String {
    // "a creature named Keimi" (a token's name).
    if let Some(n) = &f.named_card {
        return format!(" named {n}");
    }
    match f.same {
        None => String::new(),
        Some(SameRef::NamedThis) => format!(" named {}", card.name),
        Some(SameRef::NotNamedThis) => format!(" not named {}", card.name),
        Some(SameRef::SameNameAs(_)) => " with the same name as it".to_string(),
        Some(SameRef::SharesColorWith(_)) => format!(
            " that {} a color with it",
            if plural_form { "share" } else { "shares" }
        ),
        Some(SameRef::LesserPower) => " with lesser power".to_string(),
        Some(SameRef::ManaValueAtMostPower) => format!(
            " with mana value less than or equal to {}'s power",
            this_ref(card)
        ),
        Some(SameRef::ShareCardType) => format!(
            " that {} a card type",
            if plural_form { "share" } else { "shares" }
        ),
        Some(SameRef::ShareCreatureType) => format!(
            " that {} a creature type",
            if plural_form { "share" } else { "shares" }
        ),
        Some(SameRef::DifferentControllers) => " controlled by different players".to_string(),
        Some(SameRef::SameController) => " controlled by the same player".to_string(),
        Some(SameRef::SameOpponent) => " controlled by the same opponent".to_string(),
        Some(SameRef::WithSameController) => " with the same controller".to_string(),
    }
}

/// "card named ~", "cards named ~", "creature card": a filter on cards
/// (in a library or graveyard) as a noun phrase, the name after the noun.
fn cards_phrase(f: &Filter, card: &Card, plural_form: bool) -> String {
    // `land card or Elf card`: "land card or Elf card".
    if let Some(alt) = &f.or
        && f.alt_card
        && !plural_form
    {
        let base = Filter {
            or: None,
            alt_card: false,
            ..f.clone()
        };
        return format!(
            "{} or {}",
            cards_phrase(&base, card, false),
            cards_phrase(alt, card, false)
        );
    }
    let mut g = f.clone();
    g.same = None;
    // "creature card with mana value 3 or less": the mana value after the
    // noun.
    let mut mv = String::new();
    if g.or.is_none()
        && let Some((cmp, n)) = g.mana_value.take()
    {
        mv = if g.status.spent_colors {
            format!(" with mana value {}", spent_at_most(card))
        } else if g.status.mana_value_x {
            format!(" with mana value X{}", cmp_words(cmp))
        } else {
            format!(" with mana value {n}{}", cmp_words(cmp))
        };
        g.status.mana_value_x = false;
        g.status.spent_colors = false;
    }
    // "creature card with power 2 or less" (Slumbering Walker).
    if g.or.is_none()
        && mv.is_empty()
        && let Some((cmp, n)) = g.power.take()
    {
        mv = format!(" with power {n}{}", cmp_words(cmp));
    }
    // "creature card with toughness 2 or less" (Recruiter of the Guard).
    if g.or.is_none()
        && mv.is_empty()
        && let Some((cmp, n)) = g.toughness.take()
    {
        mv = format!(" with toughness {n}{}", cmp_words(cmp));
    }
    // "creature card with mana value less than or equal to this
    // creature's power".
    if g.or.is_none()
        && mv.is_empty()
        && let Some(b) = g.status.bound.take()
    {
        mv = format!(" with {}", bound_phrase(b, card));
    }
    // "a creature card with no abilities" (Fang-Druid Summoner).
    if g.or.is_none() && mv.is_empty() && g.no_abilities {
        g.no_abilities = false;
        mv = " with no abilities".to_string();
    }
    let p = if g == Filter::default() {
        String::new()
    } else {
        format!("{} ", card_filter_phrase(&g, card))
    };
    // `andor`: "up to two target instant and/or sorcery cards".
    let p = if plural_form && f.and_or {
        p.replacen(" or ", " and/or ", 1)
    } else {
        p
    };
    // "an instant, sorcery, or Faerie card" (Free the Fae): three
    // alternatives as a list.
    let p = if p.matches(" or ").count() == 2 && !p.contains(',') {
        let i = p.find(" or ").expect("counted");
        format!("{},{}", &p[..i], &p[i + 3..]).replacen(" or ", ", or ", 1)
    } else {
        p
    };
    let noun = if plural_form { "cards" } else { "card" };
    // "a card with flash": only qualities after the noun.
    if let Some(after) = p.trim().strip_prefix("with ") {
        return format!(
            "{noun} with {after}{mv}{}",
            same_phrase(f, card, plural_form)
        );
    }
    // "creature card with infect": a keyword after the noun.
    if g.or.is_none()
        && !g.keywords.is_empty()
        && let Some((kind, quality)) = p.trim_end().split_once(" with ")
    {
        return format!(
            "{kind} {noun} with {quality}{mv}{}",
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
        ChoiceKind::Named(names) => or_list(names),
        ChoiceKind::Parity => "odd or even".to_string(),
        ChoiceKind::CardName { nonland: false } => "a card name".to_string(),
        ChoiceKind::CardName { nonland: true } => "a nonland card name".to_string(),
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
    // `Mount creature or a Plains card`: "a Mount creature card or a
    // Plains card", each alternative its own noun.
    if let Some(alt) = &f.or
        && alt.or.is_none()
        && f.alt_article
    {
        let base = Filter {
            or: None,
            alt_article: false,
            ..f.clone()
        };
        return format!(
            "{} or {}",
            card_filter_a(&base, card),
            card_filter_a(alt, card)
        );
    }
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
        && let Some((before, name)) = p.split_once("permanent named ")
    {
        // Grandeur: "Discard another card named Skoa, Embermage".
        with_article(&format!("{before}card named {name}"))
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
/// "If it's your turn, you may sacrifice a Mountain rather than pay this
/// spell's mana cost.", "You may pay {1} and return a basic land you
/// control to its owner's hand rather than pay this spell's mana cost."
fn alt_cost_line(card: &Card, a: &crate::ast::AltCost) -> String {
    let mut parts = Vec::new();
    if a.others_gain > 0 {
        parts.push(format!(
            "have each other player gain {} life",
            a.others_gain
        ));
    }
    let mana = a.cost.mana.to_string();
    if !mana.is_empty() {
        parts.push(format!("pay {mana}"));
    }
    if let Some(SacCost::Filter(f)) = &a.cost.sacrifice {
        let what = if a.count > 1 {
            format!(
                "{} {}",
                count_words(Num::N(a.count), "a"),
                filter_phrase(f, card, true)
            )
        } else {
            filter_a(f, card)
        };
        parts.push(match a.cost.chosen {
            mtg_core::card::ChosenCost::Return => {
                format!("return {what} to its owner's hand")
            }
            mtg_core::card::ChosenCost::Tap => format!("tap {what}"),
            mtg_core::card::ChosenCost::Sacrifice => format!("sacrifice {what}"),
        });
    }
    if let Some(g) = &a.cost.exile_cards {
        let kind = g
            .filter
            .as_ref()
            .map_or(String::new(), |f| card_filter_phrase(f, card) + " ");
        parts.push(if g.top {
            // "exile the top three black cards of your graveyard".
            format!(
                "exile the top {} {kind}{} of your graveyard",
                count_words(Num::N(g.n), "one"),
                if g.n == 1 { "card" } else { "cards" }
            )
        } else if g.n == 1 {
            format!(
                "exile {} from your graveyard",
                with_article(&format!("{kind}card"))
            )
        } else {
            format!(
                "exile {} {kind}cards from your graveyard",
                count_words(Num::N(g.n), "a")
            )
        });
    }
    if a.cost.life > 0 {
        parts.push(format!("pay {} life", a.cost.life));
    }
    // "exile a blue card from your hand" (a pitch spell).
    if a.cost.discard_exile {
        parts.push(hand_exile_text(&a.cost, card));
    }
    if a.cost.reveal_hand {
        parts.push("reveal your hand".to_string());
    }
    let you_may = match a.cost.conditions.first() {
        Some(c) if !a.cond_late => format!("If {}, you may", static_cond_text(c, card)),
        _ => "You may".to_string(),
    };
    // "You may pay {B} rather than pay this spell's mana cost if there are
    // thirteen or more creatures on the battlefield." (Blasphemous Edict).
    if a.cond_late
        && let Some(c) = a.cost.conditions.first()
    {
        return format!(
            "You may {} rather than pay this spell's mana cost if {}.",
            parts.join(" and "),
            static_cond_text(c, card)
        );
    }
    // "If you've cast another white spell this turn, you may cast this
    // spell without paying its mana cost."
    if parts.is_empty() {
        return format!("{you_may} cast this spell without paying its mana cost.");
    }
    // "If you control a Forest, rather than pay this spell's mana cost, you
    // may have each other player gain 6 life." (the older wording).
    if a.others_gain > 0
        && let Some(c) = a.cost.conditions.first()
    {
        return format!(
            "If {}, rather than pay this spell's mana cost, you may {}.",
            static_cond_text(c, card),
            parts.join(" and ")
        );
    }
    format!(
        "{you_may} {} rather than pay this spell's mana cost.",
        parts.join(" and ")
    )
}

/// "exile a blue card from your hand", "exile two white cards from your
/// hand" (`Cost::discard_exile`).
fn hand_exile_text(cost: &crate::ast::Cost, card: &Card) -> String {
    let kind = cost
        .discard_filter
        .as_ref()
        .map_or(String::new(), |f| card_filter_phrase(f, card) + " ");
    let n = Num::N(cost.discard.max(1));
    let what = if cost.discard <= 1 {
        with_article(&format!("{kind}card"))
    } else {
        format!("{} {kind}cards", count_words(n, "a"))
    };
    format!("exile {what} from your hand")
}

fn filter_a(f: &Filter, card: &Card) -> String {
    // "a black or artifact creature" (Soldevi Adnate): a colored creature
    // or an artifact creature, sharing the noun.
    if let Some(alt) = &f.or
        && alt.or.is_none()
        && f.types == [CardType::Creature]
        && f.colors.as_ref().is_some_and(|c| c.len() == 1)
        && (Filter {
            colors: None,
            or: None,
            ..f.clone()
        }) == (Filter {
            types: vec![CardType::Creature],
            ..Filter::default()
        })
        && (**alt)
            == (Filter {
                types: vec![CardType::Artifact, CardType::Creature],
                ..Filter::default()
            })
    {
        let color = f.colors.as_ref().map_or("", |c| color_name(c[0]));
        return format!("a {color} or artifact creature");
    }
    // "an artifact or another creature", "another creature or an
    // artifact": one side is other, each with its own article.
    if let Some(alt) = &f.or
        && f.relation != alt.relation
        && alt.or.is_none()
    {
        let (b, a) = crate::format::factor_qualifiers(f, alt);
        return format!("{} or {}", filter_a(&b, card), filter_a(&a, card));
    }
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

/// A spell's additional cost, in a sentence: "sacrifice a creature",
/// "pay {3}{B}".
fn additional_text(cost: &Cost, ctx: &mut Ctx) -> String {
    let c = cost_text(cost, ctx);
    let mana = cost.mana.to_string();
    if !mana.is_empty() && c.starts_with(&mana) {
        return format!("pay {c}");
    }
    lowercase_first(&c)
}

/// A ward cost's alternatives (CR 702.21a), the first capitalized:
/// "Pay life equal to Raubahn's power", "Get five poison counters", "You
/// get two poison counters", "Discard a card or pay {2}".
fn ward_alts_text(alts: &[crate::ast::WardAlt], ctx: &mut Ctx) -> String {
    let mut parts = Vec::new();
    for (i, a) in alts.iter().enumerate() {
        let text = if let Some(n) = a.life {
            match n {
                Num::Count(idx) => {
                    let phrase = match ctx.card.counts.get(usize::from(idx)).map(|c| &c.expr) {
                        Some(crate::ast::CountExpr::SourcePower) | None => {
                            format!("{}'s power", this_ref(ctx.card))
                        }
                        Some(expr) => count_phrase(expr, ctx.card, false),
                    };
                    format!("pay life equal to {phrase}")
                }
                n => format!("pay {} life", digits(n)),
            }
        } else if a.poison > 0 {
            format!(
                "{}get {} poison counters",
                if a.you { "you " } else { "" },
                count_words(Num::N(a.poison), "a")
            )
        } else if i > 0
            && a.cost
                == (Cost {
                    mana: a.cost.mana.clone(),
                    order: a.cost.order.clone(),
                    ..Cost::default()
                })
        {
            // "Discard a card or pay {2}".
            format!("pay {}", a.cost.mana)
        } else {
            let t = cost_text(&a.cost, ctx);
            if i > 0 { lowercase_first(&t) } else { t }
        };
        parts.push(text);
    }
    capitalize(&parts.join(" or "))
}

#[allow(clippy::too_many_lines)] // one arm per cost part
fn cost_text(cost: &Cost, ctx: &mut Ctx) -> String {
    if let Some(n) = cost.loyalty {
        // A zero loyalty cost prints "0:" (Gideon Jura's "0: Until end of
        // turn, ...").
        return match n.cmp(&0) {
            std::cmp::Ordering::Equal => "0".to_string(),
            std::cmp::Ordering::Greater => format!("+{n}"),
            std::cmp::Ordering::Less => format!("−{}", -n),
        };
    }
    let default_order = [
        CostItem::Mana,
        CostItem::Tap,
        CostItem::Untap,
        CostItem::Sacrifice,
        CostItem::Discard,
        CostItem::Life,
        CostItem::ExileCards,
        CostItem::Blight,
        CostItem::Evidence,
        CostItem::Forage,
        CostItem::Process,
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
                if cost.waterbend {
                    parts.push(format!("Waterbend {mana}"));
                } else if !mana.is_empty() {
                    parts.push(mana);
                }
            }
            CostItem::Tap => {
                if cost.tap {
                    parts.push("{T}".to_string());
                }
            }
            CostItem::Untap => {
                if cost.untap {
                    parts.push("{Q}".to_string());
                }
            }
            CostItem::Sacrifice => match &cost.sacrifice {
                // "you may sacrifice any number of Spirits" (CR 601.2b).
                Some(SacCost::Filter(f)) if cost.sacrifice_any > 0 => parts.push(format!(
                    "{} any number of {}",
                    if cost.sacrifice_any == 2 {
                        "you may sacrifice"
                    } else {
                        "Sacrifice"
                    },
                    filter_phrase(f, ctx.card, true)
                )),
                Some(SacCost::This) => parts.push(format!("Sacrifice {}", ctx.source())),
                Some(SacCost::Granter) => parts.push(format!("Sacrifice {}", ctx.card.name)),
                Some(SacCost::Filter(f)) => {
                    // "Tap two untapped artifacts and/or creatures you
                    // control".
                    let what = if cost.count > 1 {
                        let p = filter_phrase(f, ctx.card, true);
                        let p = match p.rfind(" or ") {
                            Some(i) => format!("{} and/or {}", &p[..i], &p[i + 4..]),
                            None => p,
                        };
                        format!("{} {p}", count_words(Num::N(cost.count), "a"))
                    } else if f.relation == Relation::This && *f == Filter::this() {
                        // "Return this enchantment to its owner's hand".
                        ctx.source()
                    } else if f.attached_to == Some(AttachedRef::Source) {
                        // "Sacrifice an Aura attached to this creature"
                        // (Faunsbane Troll): the source not yet named.
                        let this = this_ref(ctx.card);
                        filter_a(f, ctx.card).replacen(
                            " attached to it",
                            &format!(" attached to {this}"),
                            1,
                        )
                    } else {
                        filter_a(f, ctx.card)
                    };
                    parts.push(match cost.chosen {
                        mtg_core::card::ChosenCost::Sacrifice => format!("Sacrifice {what}"),
                        // "Return two lands you control to their owner's
                        // hand".
                        mtg_core::card::ChosenCost::Return if cost.count > 1 => {
                            format!("Return {what} to their owner's hand")
                        }
                        mtg_core::card::ChosenCost::Return => {
                            format!("Return {what} to its owner's hand")
                        }
                        mtg_core::card::ChosenCost::Tap => format!("Tap {what}"),
                    });
                }
                None => {}
            },
            CostItem::RemoveCounters if cost.remove_from_creature => {
                if let Some((k, n)) = cost.remove_counters {
                    let what = counters(Num::N(n), k);
                    parts.push(format!("Remove {what} from a creature you control"));
                }
            }
            CostItem::RemoveCounters => {
                if let Some((k, n)) = cost.remove_counters {
                    let what = counters(if cost.remove_x { Num::X } else { Num::N(n) }, k);
                    let src = ctx.source();
                    parts.push(format!("Remove {what} from {src}"));
                }
            }
            CostItem::PutCounters => {
                if let Some((k, n)) = cost.put_counters {
                    let what = counters(Num::N(n), k);
                    let src = ctx.source();
                    parts.push(format!("Put {what} on {src}"));
                }
            }
            CostItem::FromGraveyard => {}
            CostItem::ExileCards => {
                if let Some(g) = &cost.exile_cards {
                    let n = if g.x { Num::X } else { Num::N(g.n) };
                    let kind = g
                        .filter
                        .as_ref()
                        .map_or(String::new(), |f| card_filter_phrase(f, ctx.card) + " ");
                    if g.top {
                        // "Exile the top card of your graveyard", "Exile
                        // the top creature card of your graveyard" (the
                        // most recent, CR 404.2).
                        let what = if n == Num::N(1) {
                            format!("{kind}card")
                        } else {
                            format!("{} {kind}cards", count_words(n, "one"))
                        };
                        parts.push(format!("Exile the top {what} of your graveyard"));
                        continue;
                    }
                    let what = if n == Num::N(1) {
                        with_article(&format!("{kind}card"))
                    } else {
                        format!("{} {kind}cards", count_words(n, "a"))
                    };
                    if cost.exile_this_joined {
                        // "Exile two cards from your graveyard and this
                        // creature".
                        let src = ctx.source();
                        parts.push(format!("Exile {what} from your graveyard and {src}"));
                        continue;
                    }
                    parts.push(format!("Exile {what} from your graveyard"));
                }
            }
            CostItem::Exile => {
                if cost.exile_self {
                    parts.push("Exile this card from your graveyard".to_string());
                } else if cost.exile_hand {
                    parts.push("Exile this card from your hand".to_string());
                } else if cost.exile_this {
                    parts.push(format!("Exile {}", ctx.source()));
                }
            }
            CostItem::Discard if cost.discard_exile => {
                parts.push(capitalize(&hand_exile_text(cost, ctx.card)));
            }
            CostItem::Discard => {
                if cost.discard_self {
                    parts.push("Discard this card".to_string());
                } else if cost.discard_x {
                    // "Discard X creature cards".
                    let kind = cost
                        .discard_filter
                        .as_ref()
                        .map_or(String::new(), |f| card_filter_phrase(f, ctx.card) + " ");
                    parts.push(format!("Discard X {kind}cards"));
                } else if let Some(f) = &cost.discard_filter {
                    parts.push(format!("Discard {}", card_filter_a(f, ctx.card)));
                } else if cost.discard > 0 {
                    parts.push(format!(
                        "Discard {} {}{}",
                        count_words(Num::N(cost.discard), "a"),
                        plural(Num::N(cost.discard), "card"),
                        if cost.discard_random {
                            " at random"
                        } else {
                            ""
                        }
                    ));
                }
            }
            CostItem::Life => {
                if cost.life_x {
                    parts.push("Pay X life".to_string());
                } else if cost.life > 0 {
                    parts.push(format!("Pay {} life", cost.life));
                }
            }
            // "Exert Basri", "Exert this creature" (CR 701.43a).
            CostItem::Exert => parts.push(format!("Exert {}", ctx.source())),
            CostItem::DiscardHand => parts.push("Discard your hand".to_string()),
            CostItem::RevealHand => parts.push("Reveal your hand".to_string()),
            // CR 702.57a: "Reveal this card from your hand" ("this
            // creature" on a creature card).
            CostItem::RevealThis => parts.push(format!(
                "Reveal this {} from your hand",
                if ctx.card.types.contains(&mtg_core::card::CardType::Creature) {
                    "creature"
                } else {
                    "card"
                }
            )),
            CostItem::Energy => parts.push(format!("Pay {}", energy(cost.energy))),
            // CR 400.3: to the owner's graveyard.
            CostItem::Process => match cost.process {
                0 => {}
                1 => parts.push(
                    "Put a card an opponent owns from exile into that player's graveyard"
                        .to_string(),
                ),
                n => parts.push(format!(
                    "Put {} cards your opponents own from exile into their owners' graveyards",
                    count_words(Num::N(n), "a")
                )),
            },
            CostItem::Blight => {
                if cost.blight > 0 && cost.blight_put {
                    let what = counters(Num::N(cost.blight), CounterKind::MinusOneMinusOne);
                    parts.push(format!("Put {what} on a creature you control"));
                } else if cost.blight > 0 {
                    parts.push(format!("Blight {}", cost.blight));
                }
            }
            // CR 701.69a: "behold a Kithkin and exile it", "you may behold
            // a Dragon", "Behold three Elementals".
            CostItem::Behold => {
                if let Some(b) = &cost.behold
                    && b.reveal
                    && b.x
                {
                    // "Reveal X red cards from your hand".
                    let what = card_filter_phrase(&b.filter, ctx.card);
                    parts.push(format!("Reveal X {what} cards from your hand"));
                } else if let Some(b) = &cost.behold
                    && b.reveal
                {
                    let what = card_filter_a(&b.filter, ctx.card);
                    let may = if b.optional {
                        "you may reveal"
                    } else {
                        "Reveal"
                    };
                    parts.push(format!("{may} {what} from your hand"));
                } else if let Some(b) = &cost.behold {
                    let what = if b.n == 1 {
                        filter_a(&b.filter, ctx.card)
                    } else {
                        format!(
                            "{} {}",
                            count_words(Num::N(b.n), "one"),
                            filter_phrase(&b.filter, ctx.card, true)
                        )
                    };
                    let may = if b.optional {
                        "you may behold"
                    } else {
                        "Behold"
                    };
                    let exile = if b.exile { " and exile it" } else { "" };
                    parts.push(format!("{may} {what}{exile}"));
                }
            }
            CostItem::Evidence => {
                if cost.evidence > 0 {
                    parts.push(format!("Collect evidence {}", cost.evidence));
                }
            }
            CostItem::Forage => {
                if cost.forage {
                    parts.push("Forage".to_string());
                }
            }
        }
    }
    // "Remove three quest counters from this enchantment and sacrifice
    // it" (Ior Ruin Expedition).
    if let [first, last] = parts.as_slice()
        && first.starts_with("Remove ")
        && last == "Sacrifice it"
    {
        return format!("{first} and sacrifice it");
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

/// "its", "your", "target creature's".
fn possessive(s: &str) -> String {
    match s {
        "it" => "its".to_string(),
        "you" => "your".to_string(),
        "they" => "their".to_string(),
        s => format!("{s}'s"),
    }
}

fn counter_name(k: CounterKind) -> &'static str {
    k.name()
}

fn counters(n: Num, k: CounterKind) -> String {
    // "Remove a counter from this creature", "Remove two counters from
    // it": any kind.
    if k == CounterKind::Any {
        return format!("{} {}", count_words(n, "a"), plural(n, "counter"));
    }
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
    list(&merge_protections(
        ks.iter()
            .filter(|k| !is_restriction(**k))
            .map(|k| keyword_name(*k).to_string())
            .collect(),
    ))
}

/// "protection from green and from blue": consecutive protections in a
/// list share one "protection".
fn merge_protections(items: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(items.len());
    for it in items {
        if let Some(q) = it.strip_prefix("protection from ")
            && let Some(last) = out.last_mut()
            && last.starts_with("protection from ")
        {
            let _ = write!(last, " and from {q}");
            continue;
        }
        out.push(it);
    }
    out
}

/// A token's quoted restrictions that Oracle prints as one: "This token
/// can't attack or block alone." (Toby, Beastie Befriender), "This token
/// can't block or be blocked by non-Spirit creatures." (Foggy Swamp
/// Spirit Keeper).
fn merge_token_restrictions(quoted: &mut Vec<String>) {
    let mut i = 0;
    while i + 1 < quoted.len() {
        let merged = match (quoted[i].as_str(), quoted[i + 1].as_str()) {
            ("This token can't attack alone.", "This token can't block alone.") => {
                Some("This token can't attack or block alone.".to_string())
            }
            (first, second) => match (
                first.strip_prefix("This token can't block "),
                second.strip_prefix("This token can't be blocked by "),
            ) {
                (Some(what), Some(by)) if what == by => {
                    Some(format!("This token can't block or be blocked by {what}"))
                }
                _ => None,
            },
        };
        if let Some(m) = merged {
            quoted[i] = m;
            quoted.remove(i + 1);
        }
        i += 1;
    }
}

/// A token's description; a legendary one is named first: "Galactus, a
/// legendary 16/16 black Elder Alien creature token with flying,
/// trample, and \"Whenever Galactus attacks, destroy target land.\""
/// (The Coming of Galactus).
fn token_description(name: &str, count: Num, tapped: bool, it_has: bool, ctx: &Ctx) -> String {
    let s = token_description_plain(name, count, tapped, it_has, ctx);
    let Some(token) = ctx.lookup.get(name) else {
        return s;
    };
    if !token.supertypes.contains(&Supertype::Legendary) || count != Num::N(1) || tapped {
        return s;
    }
    let shown = token.display_name.as_deref().unwrap_or(&token.name);
    let named = format!(" named {shown}");
    if !s.contains(&named) {
        return s;
    }
    let rest = s.replacen(&named, "", 1);
    let rest = rest
        .strip_prefix("a ")
        .or_else(|| rest.strip_prefix("an "))
        .unwrap_or(&rest)
        .replace("this token", shown)
        .replace("This token", shown);
    format!("{shown}, a legendary {rest}")
}

#[allow(clippy::too_many_lines)] // one part of the description after another
fn token_description_plain(
    name: &str,
    count: Num,
    tapped: bool,
    it_has: bool,
    ctx: &Ctx,
) -> String {
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
    // Role tokens (CR 111.10j–r) print by name too: "a Wicked Role token".
    if token.subtypes.iter().any(|s| s == "Role") {
        let role = format!(
            "{} Role",
            token.display_name.as_deref().unwrap_or(&token.name)
        );
        return format!(
            "{} {tap}{role} {}",
            count_words(count, if tapped { "a" } else { article(&role) }),
            plural(count, "token")
        );
    }
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
    // An Equipment token is named too: "a colorless Equipment artifact
    // token named Sturdy Shield with \"Equipped creature gets +1/+2\" and
    // equip {2}".
    let equipment = !token.types.contains(&CardType::Creature) && is_equipment(token);
    // So is a noncreature token with no subtype to be named by: "a
    // colorless artifact token named Munitions with ..." (Weapons
    // Manufacturing).
    let unnamed_type = token.subtypes.is_empty() && !token.types.contains(&CardType::Creature);
    let named = ((token.types.contains(&CardType::Creature) || equipment || unnamed_type)
        && shown != token.subtypes.join(" "))
    .then(|| format!(" named {shown}"));
    let mut equip_last: Vec<String> = Vec::new();
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
            // "a 3/2 colorless Vehicle artifact token with crew 1".
            Ability::Mechanic(crate::ast::Mechanic::Crew(n)) => bare.push(format!("crew {n}")),
            // "a 2/2 red Soldier creature token with firebending 1".
            Ability::Mechanic(crate::ast::Mechanic::Firebending(Num::N(n))) => {
                bare.push(format!("firebending {n}"));
            }
            Ability::Keyword(k) if !is_restriction(*k) => {
                bare.push(keyword_name(*k).to_string());
            }
            Ability::Activated { equip: true, .. } if equipment => {
                equip_last.push(lowercase_first(&ability_text(token, a, ctx.lookup)));
            }
            // Its abilities print without their final period.
            _ if equipment => {
                let t = ability_text(token, a, ctx.lookup);
                quoted.push(t.strip_suffix('.').unwrap_or(&t).to_string());
            }
            _ => quoted.push(ability_text(token, a, ctx.lookup)),
        }
    }
    merge_token_restrictions(&mut quoted);
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
    // An Equipment token with several quoted abilities puts the list's
    // commas inside the quotation marks: "with \"Equipped creature gets
    // +2/+0,\" \"Whenever equipped creature attacks, ...,\" and equip {2}"
    // (Icingdeath, Frost Tyrant).
    if equipment && bare.is_empty() && quoted.len() >= 2 && !equip_last.is_empty() && !it_has {
        let q: Vec<String> = quoted.iter().map(|q| format!("\"{q},\"")).collect();
        let _ = write!(s, " with {} and {}", q.join(" "), equip_last.join(" and "));
        return s;
    }
    let parts: Vec<String> = bare
        .into_iter()
        .chain(quoted.into_iter().map(|q| format!("\"{q}\"")))
        .chain(equip_last)
        .collect();
    if it_has && !parts.is_empty() {
        // "create a 1/1 colorless Eldrazi Scion creature token. It has
        // \"Sacrifice this token: Add {C}.\"", "... tokens. They have ...".
        let they = if count == Num::N(1) {
            "It has"
        } else {
            "They have"
        };
        let _ = write!(s, ". {they} {}", list(&parts));
        return s;
    }
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
    if x.not_legendary {
        parts.push("it isn't legendary".to_string());
    }
    // "it's a Vehicle artifact in addition to its other types".
    if !x.subtypes.is_empty() || !x.types.is_empty() {
        // "it's a 1/1 red Balloon creature in addition to its other
        // colors and types".
        let mut words: Vec<String> = Vec::new();
        if let Some((p, t)) = x.pt {
            words.push(format!("{p}/{t}"));
        }
        words.extend(x.colors.iter().map(|c| color_name(*c).to_string()));
        words.extend(x.subtypes.iter().cloned());
        words.extend(x.types.iter().map(|t| type_noun(*t).to_string()));
        let other = if x.colors.is_empty() {
            "types"
        } else {
            "colors and types"
        };
        parts.push(format!(
            "it's {} in addition to its other {other}",
            with_article(&words.join(" "))
        ));
    }
    // "it has haste and \"At the beginning of the end step, sacrifice
    // this token.\"": a spell's copy is a token, which its abilities
    // name.
    let token;
    let about = if is_spell(card) {
        let mut t = card.clone();
        t.kind = Kind::Token;
        t.types
            .retain(|t| !matches!(t, CardType::Instant | CardType::Sorcery));
        token = t;
        &token
    } else {
        card
    };
    let mut has = Vec::new();
    if !x.keywords.is_empty() {
        has.push(keywords_list(&x.keywords));
    }
    for a in &x.abilities {
        if current == Some(a) {
            has.push("this ability".to_string());
        } else {
            has.push(format!("\"{}\"", ability_text(about, a, &())));
        }
    }
    if !has.is_empty() {
        parts.push(format!("it has {}", list(&has)));
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
        | Effect::HandToLibrary { count, .. }
        | Effect::AddCounters { count, .. }
        | Effect::DistributeCounters { count, .. }
        | Effect::AddManaTimes { count, .. }
        | Effect::AddManaCombination { count, .. }
        | Effect::EmpowerJace { count }
        | Effect::Amass { count, .. }
        | Effect::Endure { count, .. }
        | Effect::Dig { count, .. }
        | Effect::ExilePlayable { count, .. }
        | Effect::ExileTop { count, .. }
        | Effect::Earthbend { count, .. }
        | Effect::Search { count, .. }
        | Effect::InvestigateCount { count, .. } => vec![*count],
        Effect::CreateToken { count, sized, .. } => std::iter::once(*count).chain(*sized).collect(),
        Effect::CopySpells { times, .. } => vec![*times],
        Effect::AddCounterList { counters, .. } => counters.iter().map(|(_, n)| *n).collect(),
        Effect::CounterUnlessPays { x, .. } => vec![*x],
        Effect::Scry(n)
        | Effect::Surveil(n)
        | Effect::LookAtTop { count: n, .. }
        | Effect::DigSplit { count: n, .. }
        | Effect::ScryEach { count: n, .. } => {
            vec![*n]
        }
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
    // "half the number of artifacts you control, rounded down"; "for
    // every seven cards in your graveyard".
    if let CountExpr::Divided { by, up, inner } = c {
        // "mills half their library, rounded down".
        if *by == 2
            && !each
            && let CountExpr::TargetLibrary(_) = **inner
        {
            return format!(
                "half their library, rounded {}",
                if *up { "up" } else { "down" }
            );
        }
        let s = count_phrase(inner, card, false);
        if *by == 2 && !each {
            return format!("half {s}, rounded {}", if *up { "up" } else { "down" });
        }
        let many = s.strip_prefix("the number of ").unwrap_or(&s);
        return format!("for every {} {many}", count_words(Num::N(*by), "one"));
    }
    let (one, many) = match c {
        // "the greatest power among creatures you control".
        CountExpr::GreatestPower(f) => {
            return format!("the greatest power among {}", filter_phrase(f, card, true));
        }
        // "for each creature it devoured", "the number of Goblins it
        // devoured" (CR 702.82c).
        CountExpr::Devoured(f) => (
            format!("{} it devoured", filter_phrase(f, card, false)),
            format!("{} it devoured", filter_phrase(f, card, true)),
        ),
        CountExpr::DifferentlyNamed(f) => {
            return format!(
                "the number of differently named {}",
                filter_phrase(f, card, true)
            );
        }
        CountExpr::GreatestPowerInGraveyard(f) => {
            return format!(
                "the greatest power among {} in your graveyard",
                cards_phrase(f, card, true)
            );
        }
        // "for each creature that died under your control this turn".
        CountExpr::DiedThisTurn(f) if f.who == Some(Who::You) => {
            let g = Filter {
                who: None,
                ..f.clone()
            };
            (
                format!(
                    "{} that died under your control this turn",
                    filter_phrase(&g, card, false)
                ),
                format!(
                    "{} that died under your control this turn",
                    filter_phrase(&g, card, true)
                ),
            )
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
        // "the number of permanents you control that are creatures and/or
        // Vehicles" (Collision Course).
        CountExpr::Permanents(f) if f.that_are && f.who == Some(Who::You) => {
            let mut g = Filter {
                that_are: false,
                who: None,
                ..f.clone()
            };
            let mut alt = &mut g.or;
            while let Some(a) = alt {
                a.who = None;
                alt = &mut a.or;
            }
            (
                format!(
                    "permanent you control that's {}",
                    with_article(&filter_phrase(&g, card, false))
                ),
                format!(
                    "permanents you control that are {}",
                    filter_phrase(&g, card, true)
                ),
            )
        }
        CountExpr::Permanents(f) => {
            // "for each other attacking creature": attacking and blocking
            // creatures are on the battlefield without saying so.
            let place = if f.who.is_none()
                && f.that_player.is_none()
                && !f.attacking
                && !f.blocking
                && f.attached_to.is_none()
            {
                " on the battlefield"
            } else {
                ""
            };
            // "for each creature you control with defender": Oracle names
            // the controller before the qualities.
            (
                format!(
                    "{}{place}",
                    controller_first(f, filter_phrase(f, card, false))
                ),
                format!(
                    "{}{place}",
                    controller_first(f, filter_phrase(f, card, true))
                ),
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
        // "draw cards equal to the difference" (after "if you have fewer
        // than ten cards in hand").
        CountExpr::Difference { .. } => return "the difference".to_string(),
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
        CountExpr::Speed => return "your speed".to_string(),
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
        CountExpr::Times(..) | CountExpr::Divided { .. } => unreachable!("unwrapped above"),
        // "for each enchantment destroyed this way".
        // Parley: "For each nonland card revealed this way".
        // "for each creature card milled this way" (Gorging Vulture).
        CountExpr::ThisWay {
            verb: mtg_core::card::ThisWay::Milled,
            filter: Some(f),
        } => (
            format!("{} milled this way", cards_phrase(f, card, false)),
            format!("{} milled this way", cards_phrase(f, card, true)),
        ),
        CountExpr::ThisWay {
            verb: mtg_core::card::ThisWay::Revealed,
            filter: Some(f),
        } => (
            format!("{} revealed this way", cards_phrase(f, card, false)),
            format!("{} revealed this way", cards_phrase(f, card, true)),
        ),
        CountExpr::ThisWay { verb, filter } => {
            // "the number of cards shuffled into your library this way".
            let verb = match verb {
                mtg_core::card::ThisWay::Shuffled => "shuffled into your library",
                // "for each permanent returned to its owner's hand this
                // way" (Wanderwine Farewell): permanents, any player's.
                mtg_core::card::ThisWay::PutInHand
                    if filter
                        .as_ref()
                        .is_some_and(|f| filter_phrase(f, card, false) == "permanent") =>
                {
                    "returned to its owner's hand"
                }
                v => crate::ast::this_way_word(*v),
            };
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
        CountExpr::Votes { word, .. } => (format!("{word} vote"), format!("{word} votes")),
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
        CountExpr::OtherInstantSorceryCast { before } => {
            let b = if *before { "before it " } else { "" };
            (
                format!("other instant and sorcery spell you've cast {b}this turn"),
                format!("other instant and sorcery spells you've cast {b}this turn"),
            )
        }
        CountExpr::SpellsCastThisTurn { other } => {
            let o = if *other { "other " } else { "" };
            (
                format!("{o}spell you've cast this turn"),
                format!("{o}spells you've cast this turn"),
            )
        }
        CountExpr::SourceToughness => return "its toughness".to_string(),
        CountExpr::CardManaValue(w) => {
            return match w {
                crate::ast::CardWord::ThatCard => "that card's mana value",
                crate::ast::CardWord::TheCard => "the card's mana value",
                crate::ast::CardWord::Its => "its mana value",
            }
            .to_string();
        }
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
        CountExpr::ExiledWith(f) => {
            let f = f.clone().unwrap_or_default();
            let with = this_ref(card);
            (
                format!("{} exiled with {with}", cards_phrase(&f, card, false)),
                format!("{} exiled with {with}", cards_phrase(&f, card, true)),
            )
        }
        CountExpr::CardsInExile { filter, graveyard } => {
            let f = filter.clone().unwrap_or_default();
            let tail = if *graveyard {
                " and in your graveyard"
            } else {
                ""
            };
            let (mut one, mut many) = (cards_phrase(&f, card, false), cards_phrase(&f, card, true));
            // "instant and sorcery cards you own in exile".
            if !f.any_types.is_empty() {
                one = one.replace(" or ", " and ");
                many = many.replace(" or ", " and ");
            }
            let one = format!("{one} you own in exile{tail}");
            let many = format!("{many} you own in exile{tail}");
            // "the total number of instant and sorcery cards you own in
            // exile and in your graveyard".
            if *graveyard && !each {
                return format!("the total number of {many}");
            }
            (one, many)
        }
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
        // "for each {E} paid this way".
        CountExpr::EnergyPaid => (
            "{E} paid this way".to_string(),
            "{E} paid this way".to_string(),
        ),
        // "for each time it was kicked" (CR 702.33d).
        CountExpr::TimesKicked => {
            let it = if is_instant_or_sorcery(card) {
                "this spell"
            } else {
                "it"
            };
            (
                format!("time {it} was kicked"),
                format!("times {it} was kicked"),
            )
        }
        CountExpr::ManaSpent { that } => {
            let it = if *that {
                "that spell"
            } else if is_instant_or_sorcery(card) {
                "this spell"
            } else {
                "it"
            };
            return format!("the amount of mana spent to cast {it}");
        }
        CountExpr::SpentColors(_) | CountExpr::SpentTreasure => {
            let it = match c {
                CountExpr::SpentColors(crate::ast::SpentOf::That) => "that spell",
                CountExpr::SpentColors(crate::ast::SpentOf::Spell) => "this spell",
                CountExpr::SpentColors(crate::ast::SpentOf::Creature) => "this creature",
                _ if is_instant_or_sorcery(card) => "this spell",
                _ => "it",
            };
            if matches!(c, CountExpr::SpentColors(_)) {
                (
                    format!("color of mana spent to cast {it}"),
                    format!("colors of mana spent to cast {it}"),
                )
            } else if it == "this spell" {
                // "for each mana from a Treasure that was spent to cast
                // this spell" (Spoils of the Hunt).
                (
                    format!("mana from a Treasure that was spent to cast {it}"),
                    format!("mana from Treasures that were spent to cast {it}"),
                )
            } else {
                (
                    format!("mana from a Treasure spent to cast {it}"),
                    format!("mana from Treasures spent to cast {it}"),
                )
            }
        }
        // "for each artifact and/or creature card in your graveyard".
        // "for each other creature and/or artifact you control".
        CountExpr::AndOr(inner) => {
            let p = count_phrase(inner, card, each);
            let sep = if p.contains(" and ") { " and " } else { " or " };
            return p.replacen(sep, " and/or ", 1);
        }
        CountExpr::TriggerStat { stat, that } => {
            let whose = if *that { "that creature's" } else { "its" };
            return format!("{whose} {}", stat_word(*stat));
        }
        CountExpr::AttachedStat(stat) => return format!("its {}", stat_word(*stat)),
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
        // "for each Cave you control and each Cave card in your
        // graveyard" (Gargantuan Leech).
        CountExpr::Sum(a, b) if each => {
            let first = count_phrase(a, card, true);
            let second = count_phrase(b, card, true);
            return match second.strip_prefix("for each ") {
                Some(rest) if first.starts_with("for each ") => format!("{first} and each {rest}"),
                _ => format!("{first} plus {second}"),
            };
        }
        CountExpr::Sum(a, b) => {
            return format!(
                "{} plus {}",
                count_phrase(a, card, false),
                count_phrase(b, card, false)
            );
        }
        CountExpr::LifeLostThisTurn => return "the life you've lost this turn".to_string(),
        // "the number of +1/+1 counters on this creature", "for each
        // scream counter on this Aura", "the number of age counters on it".
        // "for each +1/+1 counter on it" of the triggering object.
        CountExpr::TriggerCounters(mtg_core::card::CounterKind::Any) => {
            ("counter on it".to_string(), "counters on it".to_string())
        }
        CountExpr::TriggerCounters(kind) => (
            format!("{} counter on it", counter_name(*kind)),
            format!("{} counters on it", counter_name(*kind)),
        ),
        // "for each of those creatures".
        CountExpr::Those(t) => {
            let w = crate::format::those_word(*t);
            return if each {
                format!("for each of {w}")
            } else {
                format!("the number of {w}")
            };
        }
        CountExpr::SourceCounters { kind, it } => {
            let on = if *it {
                "it".to_string()
            } else {
                this_ref(card)
            };
            (
                format!("{} counter on {on}", counter_name(*kind)),
                format!("{} counters on {on}", counter_name(*kind)),
            )
        }
        // CR 700.5.
        CountExpr::Devotion { colors, chroma } => {
            let names: Vec<&str> = colors.iter().map(|c| color_name(*c)).collect();
            let names = names.join(" and ");
            if !*chroma {
                return format!("your devotion to {names}");
            }
            (
                format!("{names} mana symbol in the mana costs of permanents you control"),
                format!("{names} mana symbols in the mana costs of permanents you control"),
            )
        }
        CountExpr::HostGraveyard(f) => {
            let f = f.clone().unwrap_or_default();
            (
                format!(
                    "{} in its controller's graveyard",
                    cards_phrase(&f, card, false)
                ),
                format!(
                    "{} in its controller's graveyard",
                    cards_phrase(&f, card, true)
                ),
            )
        }
        CountExpr::CardTypesInGraveyards => (
            "card type among cards in all graveyards".to_string(),
            "card types among cards in all graveyards".to_string(),
        ),
        CountExpr::DamageThisWay { excess, to_that } => {
            return if *to_that {
                "the amount of excess damage dealt to that creature this way".to_string()
            } else if *excess {
                "the excess damage dealt this way".to_string()
            } else {
                "the damage dealt this way".to_string()
            };
        }
        CountExpr::LifeGainedThisTurn { ve } => {
            let you = if *ve { "you've" } else { "you" };
            return format!("the amount of life {you} gained this turn");
        }
        CountExpr::CardTypesInGraveyard => (
            "card type among cards in your graveyard".to_string(),
            "card types among cards in your graveyard".to_string(),
        ),
        CountExpr::SourceAllCounters { it } => {
            let on = if *it {
                "it".to_string()
            } else {
                this_ref(card)
            };
            (format!("counter on {on}"), format!("counters on {on}"))
        }
        CountExpr::SacrificedThisTurn => (
            "permanent you've sacrificed this turn".to_string(),
            "permanents you've sacrificed this turn".to_string(),
        ),
        CountExpr::TwoToX => return "2ˣ".to_string(),
        CountExpr::AttackedThisTurn => (
            "creature that attacked this turn".to_string(),
            "creatures that attacked this turn".to_string(),
        ),
        CountExpr::OpponentsLostLife => (
            "opponent who lost life this turn".to_string(),
            "opponents who lost life this turn".to_string(),
        ),
        CountExpr::DiscardedThisTurn => (
            "card you've discarded this turn".to_string(),
            "cards you've discarded this turn".to_string(),
        ),

        CountExpr::CreaturesEnteredThisTurn => (
            "creature that entered the battlefield under your control this turn".to_string(),
            "creatures that entered the battlefield under your control this turn".to_string(),
        ),
        CountExpr::OpponentsMoreCards => (
            "opponent who has more cards in hand than you".to_string(),
            "opponents who have more cards in hand than you".to_string(),
        ),
        CountExpr::Total(stat, f) => {
            return format!(
                "the total {} of {}",
                stat_word(*stat),
                controller_first(f, filter_phrase(f, card, true))
            );
        }
        CountExpr::TargetLibrary(_) => (
            "card in that player's library".to_string(),
            "cards in that player's library".to_string(),
        ),
        CountExpr::GreatestManaValueOf(f) => {
            return format!(
                "the greatest mana value among {}",
                filter_phrase(f, card, true)
            );
        }
        CountExpr::GreatestManaValueInGraveyard(f) => {
            let cards = f
                .as_ref()
                .map_or_else(|| "cards".to_string(), |f| cards_phrase(f, card, true));
            return format!("the greatest mana value among {cards} in your graveyard");
        }
        // "2 or the greatest power among Dinosaurs you control, whichever
        // is greater".
        CountExpr::AtLeast(n, inner) => {
            return format!(
                "{n} or {}, whichever is greater",
                count_phrase(inner, card, false)
            );
        }
        CountExpr::ThatSpellColors => {
            return if each {
                "for each of that spell's colors".to_string()
            } else {
                "the number of colors that spell is".to_string()
            };
        }
        CountExpr::SourceDamage => {
            return "the amount of damage dealt to it this turn".to_string();
        }
        CountExpr::TargetColors(_) => {
            return if each {
                "for each of its colors".to_string()
            } else {
                "the number of its colors".to_string()
            };
        }
        CountExpr::CreaturesLeftThisTurn => (
            "creature that left the battlefield under your control this turn".to_string(),
            "creatures that left the battlefield under your control this turn".to_string(),
        ),
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
    // "equal to twice the number of Mountains returned this way" (sweep,
    // Barrel Down Sokenzan); "for each" prints the multiplier itself.
    if !each && let CountExpr::Times(n, inner) = c {
        let x = times_x(*n);
        let times = x.strip_suffix(" X").unwrap_or(&x);
        return format!("{times} {}", count_phrase_in(inner, ctx, false));
    }
    let phrase = count_phrase(c, ctx.card, each);
    match c {
        CountExpr::DamageThisWay {
            excess: true,
            to_that: false,
        } if ctx.in_excess => "that excess damage".to_string(),
        CountExpr::TargetStat { n, that: true, .. }
        | CountExpr::TargetManaValue { n, that: true, .. }
            if ctx
                .target_noun
                .get(*n)
                .is_some_and(|t| t == "card" || t == "permanent" || t == "spell") =>
        {
            phrase.replacen(
                "that creature's",
                &format!("that {}'s", ctx.target_noun[*n]),
                1,
            )
        }
        // "its power" only once the source is named in the sentence;
        // otherwise "where X is this creature's power", "... Captain
        // America's toughness".
        CountExpr::SourcePower | CountExpr::SourceToughness
            if !each && !ctx.source_mentioned && !ctx.in_delayed =>
        {
            phrase.replacen("its", &format!("{}'s", ctx.source_name()), 1)
        }
        _ => phrase,
    }
}

#[allow(clippy::too_many_lines)] // one arm per count style
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
            // "put a number of +1/+1 counters equal to that creature's
            // power on this creature".
            CountStyle::EqualBefore if matches!(e, Effect::AddCounters { .. }) => {
                let phrase = count_phrase_in(&count.expr, ctx, false);
                c.text = c.text.replacen("put X ", "put a number of ", 1).replacen(
                    " counters on ",
                    &format!(" counters equal to {phrase} on "),
                    1,
                );
            }
            CountStyle::EqualBefore => {
                let mut phrase = count_phrase_in(&count.expr, ctx, false);
                // "deals damage equal to that spell's power to its
                // controller" (Essence Backlash): one target named once,
                // then "its".
                if let crate::ast::CountExpr::TargetStat {
                    n: i,
                    toughness: false,
                    ..
                } = &count.expr
                    && let Effect::DealDamage { to, .. } = e
                    && to.as_slice() == [Recipient::ThatControllerOf(*i)]
                {
                    let noun = ctx.target_noun[*i].clone();
                    let controller = format!(" to that {noun}'s controller");
                    if c.text.contains(&controller) {
                        c.text = c.text.replacen(&controller, " to its controller", 1);
                        phrase = format!("that {noun}'s power");
                    }
                }
                c.text = c
                    .text
                    .replacen("X damage", &format!("damage equal to {phrase}"), 1);
            }
            // "Look at the top X cards of your library, where X is ...,
            // then put them back in any order."
            CountStyle::Where if matches!(e, Effect::LookAtTop { only: false, .. }) => {
                let phrase = count_phrase_in(&count.expr, ctx, false);
                c.text =
                    c.text
                        .replacen(", then put", &format!(", where X is {phrase}, then put"), 1);
            }
            // "look at the top X cards of your library, where X is ...
            // . Put two of those cards into your hand ...".
            CountStyle::Where
                if matches!(
                    e,
                    Effect::Dig { .. } | Effect::ExilePlayable { .. } | Effect::ExileTop { .. }
                ) =>
            {
                let phrase = count_phrase_in(&count.expr, ctx, false);
                c.text = c.text.replacen(
                    " cards of your library",
                    &format!(" cards of your library, where X is {phrase}"),
                    1,
                );
            }
            // "Search your library for up to X basic land cards, where X
            // is your speed. Put those cards onto the battlefield tapped,
            // then shuffle."
            CountStyle::Where
                if matches!(e, Effect::Search { .. })
                    && matches!(count.expr, crate::ast::CountExpr::Speed)
                    && c.text.contains(" cards, put them ") =>
            {
                let phrase = count_phrase_in(&count.expr, ctx, false);
                c.text = c.text.replacen(
                    " cards, put them ",
                    &format!(" cards, where X is {phrase}. Put those cards "),
                    1,
                );
            }
            // "... for up to X basic land cards, where X is the number of
            // Shrines you control. Put those cards onto the battlefield
            // tapped, then shuffle.", "... Reveal those cards, put them
            // into your hand, then shuffle."
            CountStyle::Where
                if matches!(e, Effect::Search { those: true, .. })
                    && (c.text.contains(" cards, put them ")
                        || c.text.contains(" cards, reveal them, put them ")) =>
            {
                let phrase = count_phrase_in(&count.expr, ctx, false);
                c.text = if c.text.contains(" cards, reveal them, put them ") {
                    c.text.replacen(
                        " cards, reveal them, put them ",
                        &format!(" cards, where X is {phrase}. Reveal those cards, put them "),
                        1,
                    )
                } else {
                    c.text.replacen(
                        " cards, put them ",
                        &format!(" cards, where X is {phrase}. Put those cards "),
                        1,
                    )
                };
            }
            // "... for up to X basic land cards, where X
            // is the number of lands you control, put them onto the
            // battlefield tapped, then shuffle."
            CountStyle::Where
                if matches!(e, Effect::Search { .. }) && c.text.contains(" cards, ") =>
            {
                let phrase = count_phrase_in(&count.expr, ctx, false);
                c.text = c
                    .text
                    .replacen(" cards, ", &format!(" cards, where X is {phrase}, "), 1);
            }
            // "Target player draws 2ˣ cards."
            CountStyle::Where if count.expr == crate::ast::CountExpr::TwoToX => {
                c.text = c.text.replacen("X ", "2ˣ ", 1);
            }
            CountStyle::Where if ctx.x_defined.contains(&count.expr) => {}
            CountStyle::Where => {
                // "amass Goblins X, where X is this creature's power"
                // (Rhovanion Rampager): "its" would read as the Army's.
                let mentioned = ctx.source_mentioned;
                if matches!(e, Effect::Amass { .. }) {
                    ctx.source_mentioned = false;
                }
                // "tokens with \"This token can't block,\" where X is ...":
                // the comma goes inside the closing quote.
                let comma = if c.tail.is_empty() && c.text.ends_with(".\"") {
                    c.text.truncate(c.text.len() - 2);
                    c.text.push_str(",\"");
                    ""
                } else {
                    ","
                };
                let _ = write!(
                    c.tail,
                    "{comma} where X is {}",
                    count_phrase_in(&count.expr, ctx, false)
                );
                ctx.source_mentioned = mentioned;
                ctx.x_pending.push(count.expr.clone());
            }
            // "Add an amount of {G} equal to this creature's power."
            CountStyle::Equal if matches!(e, Effect::AddManaTimes { .. }) => {
                c.text = c.text.replacen("add ", "add an amount of ", 1);
                let what = match &count.expr {
                    crate::ast::CountExpr::SourcePower => {
                        format!("{}'s power", this_ref(ctx.card))
                    }
                    other => count_phrase(other, ctx.card, false),
                };
                let _ = write!(c.tail, " equal to {what}");
            }
            // "Target opponent mills half their library, rounded down."
            CountStyle::Equal
                if matches!(
                    &count.expr,
                    crate::ast::CountExpr::Divided { by: 2, inner, .. }
                        if matches!(**inner, crate::ast::CountExpr::TargetLibrary(_))
                ) =>
            {
                let phrase = count_phrase_in(&count.expr, ctx, false);
                c.text = c.text.replacen("X cards", &phrase, 1);
            }
            CountStyle::Equal => {
                // "Create a number of 1/1 red Goblin creature tokens equal
                // to ...", "put a number of +1/+1 counters on it equal to
                // ...".
                c.text = if matches!(e, Effect::CreateToken { .. } | Effect::AddCounters { .. }) {
                    c.text.replacen("X ", "a number of ", 1)
                } else {
                    c.text.replacen("X ", "", 1)
                };
                let mut phrase = count_phrase_in(&count.expr, ctx, false);
                // "up to one target player mills cards equal to this
                // creature's power" (Veteran Ice Climber): another
                // player's action names the source again.
                if matches!(
                    e,
                    Effect::Mill {
                        who: Some(Recipient::Target(_)),
                        ..
                    }
                ) && matches!(
                    count.expr,
                    crate::ast::CountExpr::SourcePower | crate::ast::CountExpr::SourceToughness
                ) {
                    phrase = phrase.replacen("its", &format!("{}'s", ctx.source_name()), 1);
                }
                // "then its controller gains life equal to its mana value"
                // (Expel the Unworthy).
                if let crate::ast::CountExpr::TargetManaValue {
                    n: i, that: true, ..
                } = &count.expr
                    && let Effect::GainLife {
                        who: Recipient::ThatControllerOf(w),
                        ..
                    } = e
                    && w == i
                    && c.subject
                        .as_ref()
                        .is_some_and(|(_, s)| s == "its controller")
                {
                    phrase = "its mana value".to_string();
                }
                // "deals damage to that creature's controller equal to the
                // creature's power" (Boros Fury-Shield): named once already.
                if let crate::ast::CountExpr::TargetStat {
                    n: i, that: true, ..
                } = &count.expr
                    && let Effect::DealDamage { to, .. } = e
                    && to.as_slice() == [Recipient::ThatControllerOf(*i)]
                {
                    let noun = ctx.target_noun[*i].clone();
                    if c.text.contains(&format!(" to that {noun}'s controller")) {
                        phrase =
                            phrase.replacen(&format!("that {noun}'s"), &format!("the {noun}'s"), 1);
                    }
                }
                let _ = write!(c.tail, " equal to {phrase}");
            }
            // "then draw that many cards"; `count that many plus 1
            // discarded this way`: "then draw that many cards plus one".
            CountStyle::ThatMany => {
                c.text = c.text.replacen("X ", "that many ", 1);
                if let crate::ast::CountExpr::Plus { n, .. } = &count.expr {
                    let _ = write!(c.text, " plus {}", count_words(Num::N(*n), "one"));
                }
            }
            // "unless its controller pays {2} for each Wizard on the
            // battlefield", "{2} plus an additional {1} for each Faerie you
            // control".
            CountStyle::Each if matches!(e, Effect::CounterUnlessPays { .. }) => {
                let (pays, inner) = match &count.expr {
                    crate::ast::CountExpr::Times(n, inner) => (format!("{{{n}}}"), &**inner),
                    // "{2} plus an additional {2} for each card named Rune
                    // Snag in each graveyard".
                    crate::ast::CountExpr::Plus { n, inner, .. } => match &**inner {
                        crate::ast::CountExpr::Times(m, inner) => {
                            (format!("{{{n}}} plus an additional {{{m}}}"), &**inner)
                        }
                        inner => (format!("{{{n}}} plus an additional {{1}}"), inner),
                    },
                    other => ("{1}".to_string(), other),
                };
                c.text = c.text.replacen("{X}", &pays, 1);
                let _ = write!(c.tail, " {}", count_phrase(inner, ctx.card, true));
            }
            // "For each land you control, create a Treasure token."
            CountStyle::ForEach => {
                c.text = c
                    .text
                    .replacen("X ", "a ", 1)
                    .replacen("a cards", "a card", 1);
                if matches!(e, Effect::CreateToken { .. }) {
                    c.text = c.text.replacen(" tokens", " token", 1);
                }
                // "For each nonland card revealed this way, put a +1/+1
                // counter on this creature." (parley).
                if matches!(e, Effect::AddCounters { .. }) {
                    c.text = c.text.replacen(" counters ", " counter ", 1);
                }
                let lead = count_phrase(&count.expr, ctx.card, true);
                // "For each ..., you create ...": the subject after the
                // count.
                c.text = match c.subject.take() {
                    Some((_, s)) => format!("{lead}, {s} {}", c.text),
                    None => format!("{lead}, {}", c.text),
                };
            }
            CountStyle::Each => {
                // "gain 1 life for each", "deals 1 damage ... for each".
                let per = match &count.expr {
                    crate::ast::CountExpr::Times(n, _) => *n,
                    _ => 1,
                };
                // "put two +1/+1 counters on this creature for each ...",
                // "put a +1/+1 counter on ... for each ...".
                if matches!(e, Effect::AddCounters { .. }) {
                    c.text =
                        c.text
                            .replacen("X ", &format!("{} ", count_words(Num::N(per), "a")), 1);
                    if per == 1 {
                        c.text = c.text.replacen(" counters ", " counter ", 1);
                    }
                }
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
                let _ = write!(c.tail, " {}", count_phrase(&count.expr, ctx.card, true));
            }
        }
    }
    // CR 707.10c: "You may choose new targets for the copy."
    if let Effect::CopySpells { and_may: true, .. } = e {
        // "copy it and you may choose new targets for the copy".
        c.tail
            .push_str(" and you may choose new targets for the copy");
    } else if let Effect::CopySpells {
        times,
        new_targets: true,
        ..
    } = e
    {
        let what = if *times == Num::N(1) {
            "copy"
        } else {
            "copies"
        };
        let _ = write!(c.tail, ". You may choose new targets for the {what}");
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
        // "Target opponent chooses a creature they control."
        Effect::PlayerChooses { who, filter } => {
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{} {} they control",
                    ctx.verb(*who, "chooses", "choose"),
                    with_article(&filter_phrase(filter, ctx.card, false))
                ),
            )
        }
        // "Target creature can't be the target of spells or abilities
        // your opponents control this turn."
        Effect::CantBeTargeted { what, ban } => {
            let s = ctx.subj(*what);
            Clause::with_subject(
                *what,
                s,
                format!(
                    "can't be the target of {} this turn",
                    match ban {
                        mtg_core::card::TargetBan::Opponents => {
                            "spells or abilities your opponents control"
                        }
                        mtg_core::card::TargetBan::Spells => "spells",
                    }
                ),
            )
        }
        // "That player sacrifices one of them of their choice."
        Effect::PickTarget {
            first,
            by_controller,
            act,
            wording,
        } => {
            use mtg_core::card::{PickAct, PickWording};
            let those = format!("one of those {}", pluralize(&ctx.target_noun[*first]));
            for i in [*first, first + 1] {
                if let Some(m) = ctx.target_mentioned.get_mut(i) {
                    *m = true;
                }
            }
            Clause::imperative(match (*by_controller, *act, *wording) {
                (false, PickAct::Exile, _) => format!("exile {those}"),
                (false, PickAct::Sacrifice, _) => format!("sacrifice {those}"),
                (false, PickAct::Choose, _) => format!("choose {those}"),
                (true, PickAct::Sacrifice, PickWording::OfTheirChoice) => {
                    "that player sacrifices one of them of their choice".to_string()
                }
                (true, PickAct::Sacrifice, PickWording::TheirController) => {
                    "their controller chooses and sacrifices one of them".to_string()
                }
                (true, PickAct::Sacrifice, _) => {
                    format!("that player chooses and sacrifices {those}")
                }
                (true, PickAct::Exile, _) => format!("that player chooses and exiles {those}"),
                (true, PickAct::Choose, _) => format!("that player chooses {those}"),
            })
        }
        // "it deals 4 damage instead": to what the sentence before named.
        Effect::DealDamage { amount, to, by } if to.is_empty() => {
            // "Addendum — If you cast this spell during your main phase,
            // it deals 5 damage instead."
            let s = if *by == Recipient::SourceIt {
                ctx.subj(*by)
            } else {
                ctx.source_subj()
            };
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
            } else if let [Recipient::ThoseTargets(a, 1)] = to.as_slice()
                && ctx.is_many(Recipient::Target(*a))
            {
                // "X target attacking creatures become blocked. Choking
                // Vines deals 1 damage to each of those creatures."
                format!("each of those {}", pluralize(&ctx.target_noun[*a]))
            } else {
                // "deals 1 damage to each of up to two target creatures":
                // the same amount to each of several targets of one phrase;
                // "deals 1 damage to each of one or two targets" (a
                // variable group, CR 601.2c).
                let many = matches!(to.as_slice(), [r @ Recipient::Target(i)]
                    if ctx.is_many(*r) && !ctx.target_mentioned[*i]);
                let each = if many || ctx.fresh_target_group(to) {
                    "each of "
                } else {
                    ""
                };
                format!("{each}{}", ctx.obj_list(to, false))
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
            // "Target creature gets +3/+3 until end of turn. It gets an
            // additional +2/+2 until end of turn for each card named ...":
            // a scaled second pump of the same recipient.
            let same = |r: Recipient| match r {
                Recipient::ThatTarget(n) => Recipient::Target(n),
                r => r,
            };
            let additional = if ctx.pumped.contains(&same(*to)) {
                "an additional "
            } else {
                ""
            };
            let (mods, tail) = match &count {
                Some(c) if c.style == CountStyle::Each => (
                    format!("{additional}{}", pt_mod(*power, *toughness)),
                    format!(" {}", count_phrase(&c.expr, ctx.card, true)),
                ),
                Some(c) => (
                    format!(
                        "{}/{}",
                        x_sign(*power, *toughness),
                        x_sign(*toughness, *power)
                    ),
                    {
                        // Another creature getting +X/+X: the source is
                        // named, "where X is Captain America's toughness".
                        let mention = ctx.source_mentioned;
                        if !matches!(to, Recipient::Source | Recipient::SourceNamed) {
                            ctx.source_mentioned = false;
                        }
                        let phrase = count_phrase_in(&c.expr, ctx, false);
                        ctx.source_mentioned = mention;
                        format!(", where X is {phrase}")
                    },
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
            // "Up to two target creatures each get +X/+X".
            let each = if matches!(to, Recipient::Targets(..)) || ctx.is_many(*to) {
                "each "
            } else {
                ""
            };
            if count.is_none() {
                ctx.pumped_x.push(same(*to));
            }
            let mut c = Clause::with_subject(
                *to,
                s,
                format!("{each}{} {mods}", ctx.verb(*to, "gets", "get")),
            )
            .until_eot();
            c.tail = tail;
            c
        }
        Effect::Pump {
            power,
            toughness,
            to,
        } => {
            let mut s = ctx.subj(*to);
            // "Up to two target creatures each get +1/+0".
            let each = if matches!(to, Recipient::Targets(..) | Recipient::SourceAndTarget(_))
                || ctx.is_many(*to)
            {
                "each "
            } else {
                ""
            };
            // "Target creature gets +1/+1 until end of turn. That creature
            // gets an additional +4/+4 until end of turn unless any player
            // pays {2}": a punisher's second pump of the same recipient.
            let same = |r: Recipient| match r {
                Recipient::ThatTarget(n) => Recipient::Target(n),
                r => r,
            };
            let additional = if ctx.pumped_x.contains(&same(*to)) {
                "an additional "
            } else if ctx.pumped.contains(&same(*to)) {
                if ctx.in_unless || ctx.in_won {
                    // "Creatures you control get +0/+1 ... They get an
                    // additional +0/+2".
                    if *to == Recipient::EachCreatureYouControl {
                        s = "they".to_string();
                    }
                    "an additional "
                } else {
                    ""
                }
            } else if ctx.pumped.contains(&Recipient::EachCreatureYouControl)
                && narrower_creatures_you_control(*to, ctx.card)
            {
                // "Creatures you control get +1/+1 until end of turn. If
                // this spell was kicked, Zombie creatures you control get
                // an additional +2/+2 until end of turn." (Strength of
                // Night): some of the creatures just pumped.
                ctx.pumped.push(same(*to));
                "an additional "
            } else {
                ctx.pumped.push(same(*to));
                ""
            };
            Clause::with_subject(
                *to,
                s,
                format!(
                    "{each}{} {additional}{}",
                    ctx.verb(*to, "gets", "get"),
                    pt_mod(*power, *toughness)
                ),
            )
            .until_eot()
        }
        Effect::Destroy {
            to,
            no_regen,
            joined,
        } => {
            let objs = ctx.obj_list(to, true);
            let mut text = format!("destroy {objs}");
            if *no_regen {
                let pronoun = if to.len() > 1 || to.iter().any(|r| Ctx::is_plural(*r)) {
                    "They"
                } else {
                    "It"
                };
                if *joined {
                    let _ = write!(text, " and {} can't be regenerated", pronoun.to_lowercase());
                } else {
                    let _ = write!(text, ". {pronoun} can't be regenerated");
                }
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
        Effect::Proliferate { times } => Clause::imperative(match times {
            Some(n) => format!("proliferate {} times", digits(*n)),
            None => "proliferate".to_string(),
        }),
        Effect::RingTempts => Clause::imperative("the Ring tempts you".to_string()),
        Effect::BecomeMonarch => Clause::imperative("you become the monarch".to_string()),
        Effect::TakeInitiative => Clause::imperative("you take the initiative".to_string()),
        Effect::Venture { undercity: false } => {
            Clause::imperative("venture into the dungeon".to_string())
        }
        Effect::Venture { undercity: true } => {
            Clause::imperative("venture into Undercity".to_string())
        }
        Effect::Manifest => Clause::imperative("manifest the top card of your library".to_string()),
        Effect::ManifestDread => Clause::imperative("manifest dread".to_string()),
        Effect::Blight(n) => Clause::imperative(format!("blight {n}")),
        Effect::CollectEvidence(n) => Clause::imperative(format!("collect evidence {n}")),
        Effect::Forage => Clause::imperative("forage".to_string()),
        Effect::Adapt(n) => Clause::imperative(format!("adapt {n}")),
        Effect::Bolster(n) => Clause::imperative(format!("bolster {}", digits(*n))),
        Effect::Monstrosity(n) => Clause::imperative(format!("monstrosity {}", digits(*n))),
        Effect::Suspect { what, on: true } => {
            let o = ctx.obj(*what);
            Clause::imperative(format!("suspect {o}"))
        }
        Effect::Suspect { what, on: false } => {
            // "All suspected creatures are no longer suspected."
            let s = if matches!(what, Recipient::Group(_)) {
                ctx.obj_all(*what)
            } else {
                ctx.subj(*what)
            };
            Clause::with_subject(
                *what,
                s,
                format!("{} no longer suspected", ctx.verb(*what, "is", "are")),
            )
        }
        Effect::BlightBy { who, n } => {
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!("{} {n}", ctx.verb(*who, "blights", "blight")),
            )
        }
        Effect::Firebend(n) => Clause::imperative(format!(
            "add {} {{R}}. Until end of combat, you don't lose this mana as steps and phases end",
            digits(*n)
        )),
        Effect::Earthbend { count, .. } => {
            Clause::imperative(format!("earthbend {}", digits(*count)))
        }
        Effect::Support(to) => Clause::imperative(format!("support {}", to.len())),
        Effect::Populate => Clause::imperative("populate".to_string()),
        Effect::EndTurn => Clause::imperative("end the turn".to_string()),
        Effect::Recruit { you: false } => Clause::imperative("recruit".to_string()),
        Effect::CloakTop => Clause::imperative("cloak the top card of your library".to_string()),
        Effect::GraveyardToBattlefield { filter, tapped } => Clause::imperative(format!(
            "return all {} from your graveyard to the battlefield{}",
            cards_phrase(filter, ctx.card, true),
            if *tapped { " tapped" } else { "" }
        )),
        Effect::ExchangeLife { with } => {
            // "If you lost life this way, draw that many cards." (Mister
            // Negative).
            ctx.lost_life_did = true;
            Clause::imperative(format!("exchange life totals with {}", ctx.obj(*with)))
        }
        Effect::GiveControl { of, to } => {
            let s = ctx.subj(*to);
            let v = ctx.verb(*to, "gains", "gain");
            // "that player gains control of Kain" (Kain, Traitorous
            // Dragoon): a legendary card names itself again.
            let o = if *of == Recipient::Source
                && ctx.card.supertypes.contains(&Supertype::Legendary)
            {
                ctx.source_mentioned = true;
                short_name(ctx.card).to_string()
            } else {
                ctx.obj(*of)
            };
            // "If they do, you draw a card." (Stiltzkin).
            ctx.they_paid = true;
            Clause::with_subject(*to, s, format!("{v} control of {o}"))
        }
        Effect::ExchangeControl { what } => {
            let o = if what.len() == 1 {
                ctx.obj_all(what[0])
            } else {
                ctx.obj_list(what, true)
            };
            Clause::imperative(format!("exchange control of {o}"))
        }
        Effect::Airbend { what } => {
            let o = if what.len() == 1 {
                ctx.obj_all(what[0])
            } else {
                ctx.obj_list(what, true)
            };
            Clause::imperative(format!("airbend {o}"))
        }
        Effect::Recruit { you: true } => {
            Clause::with_subject(Recipient::You, "you".to_string(), "recruit".to_string())
        }
        Effect::Incubate(n) => Clause::imperative(format!("incubate {}", digits(*n))),
        Effect::Discover(n) => Clause::imperative(format!("discover {n}")),
        Effect::Soulbond { .. } => Clause::imperative("pair them".to_string()),
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
        Effect::BecomePrepared { what } => {
            let s = ctx.subj(*what);
            Clause::with_subject(*what, s, "becomes prepared".to_string())
        }
        Effect::BecomeSaddledTarget { what } => {
            let s = ctx.subj(*what);
            Clause::with_subject(*what, s, "becomes saddled".to_string()).until_eot()
        }
        Effect::ExileTop { count, who } => {
            let one = *count == Num::N(1);
            let whose = if who.is_some() { "their" } else { "your" };
            let what = if one {
                format!("the top card of {whose} library")
            } else {
                format!(
                    "the top {} cards of {whose} library",
                    count_words(*count, "one")
                )
            };
            match who {
                None => Clause::imperative(format!("exile {what}")),
                Some(w) => {
                    let s = ctx.subj(*w);
                    Clause::with_subject(
                        *w,
                        s,
                        format!("{} {what}", ctx.verb(*w, "exiles", "exile")),
                    )
                }
            }
        }
        Effect::ExilePlayable {
            count,
            until,
            first,
            it,
            choose,
            from,
            possessive,
            this_way,
        } => {
            use mtg_core::card::PlayUntil;
            let one = *count == Num::N(1);
            // "Exile the top three cards of target opponent's library."
            let owner = from
                .filter(|_| *possessive)
                .map(|w| format!("{}'s", ctx.obj(w)));
            let whose = match (&owner, from) {
                (Some(o), _) => o.as_str(),
                (None, Some(_)) => "their",
                (None, None) => "your",
            };
            let exile = if from.is_some() && !*possessive {
                "exiles"
            } else {
                "exile"
            };
            let head = if one {
                format!("{exile} the top card of {whose} library")
            } else if *count == Num::ThatMuch {
                // "exile that many cards from the top of your library".
                format!("{exile} that many cards from the top of {whose} library")
            } else {
                format!(
                    "{exile} the top {} cards of {whose} library",
                    count_words(*count, "one")
                )
            };
            let head = match from {
                Some(w) if !*possessive => format!("{} {head}", ctx.subj(*w)),
                _ => head,
            };
            // "Choose one of them. Until the end of your next turn, you may
            // play that card."
            let head = if *choose {
                format!("{head}. Choose one of them")
            } else {
                head
            };
            let what = match (one || *choose, *it) {
                _ if *this_way => "cards exiled this way",
                (true, true) => "it",
                (true, false) => "that card",
                // "You may play them until the end of your next turn."
                (false, true) => "them",
                (false, false) => "those cards",
            };
            let body = if *until == PlayUntil::Forever {
                // "You may play them." (no duration).
                format!("You may play {}", if one { "it" } else { "them" })
            } else if *until == PlayUntil::WhileYouControlSource {
                format!(
                    "You may play {what} for as long as you control {}",
                    ctx.source_name()
                )
            } else if *first {
                let when = match until {
                    PlayUntil::ThisTurn => "Until end of turn",
                    PlayUntil::EndOfYourNextTurn => "Until the end of your next turn",
                    PlayUntil::YourNextEndStep
                    | PlayUntil::Forever
                    | PlayUntil::WhileYouControlSource => "Until your next end step",
                };
                format!("{when}, you may play {what}")
            } else {
                let when = match until {
                    PlayUntil::ThisTurn => "this turn",
                    PlayUntil::EndOfYourNextTurn => "until the end of your next turn",
                    PlayUntil::YourNextEndStep
                    | PlayUntil::Forever
                    | PlayUntil::WhileYouControlSource => "until your next end step",
                };
                format!("You may play {what} {when}")
            };
            Clause::imperative(format!("{head}. {body}"))
        }
        Effect::RevealTop { who, look, of } => match (who, of) {
            (None, _) => Clause::imperative(format!(
                "{} the top card of your library",
                if *look { "look at" } else { "reveal" }
            )),
            (Some(w), true) => {
                let o = ctx.obj(*w);
                Clause::imperative(format!(
                    "{} the top card of {o}'s library",
                    if *look { "look at" } else { "reveal" }
                ))
            }
            (Some(w), false) => {
                let subject = ctx.subj(*w);
                let v = if *look {
                    ctx.verb(*w, "looks at", "look at")
                } else {
                    ctx.verb(*w, "reveals", "reveal")
                };
                let whose = if *w == Recipient::You {
                    "your"
                } else {
                    "their"
                };
                Clause::with_subject(*w, subject, format!("{v} the top card of {whose} library"))
            }
        },
        Effect::MoveCounters {
            from,
            to,
            kind,
            count,
        } => match (kind, count) {
            (None, None) if matches!(from, Recipient::Source | Recipient::TriggeringObject) => {
                let o = ctx.obj(*to);
                // `put those counters on up to 1 target ...`: "put those
                // counters on up to one target artifact or creature you
                // control", the triggering object's (Donatello).
                let its = if *from == Recipient::TriggeringObject && *to != Recipient::Source {
                    "those"
                } else {
                    "its"
                };
                Clause::imperative(format!("put {its} counters on {o}"))
            }
            (Some(kind), None)
                if matches!(from, Recipient::Source | Recipient::TriggeringObject) =>
            {
                let o = ctx.obj(*to);
                Clause::imperative(format!("put its {} counters on {o}", counter_name(*kind)))
            }
            // "move any number of +1/+1 counters from this creature onto
            // another target creature" (Scrounging Bandar).
            (Some(kind), Some(mtg_core::card::MOVE_ANY_NUMBER)) => {
                let src = ctx.obj(*from);
                let dst = ctx.obj(*to);
                Clause::imperative(format!(
                    "move any number of {} counters from {src} onto {dst}",
                    counter_name(*kind)
                ))
            }
            (Some(kind), Some(n)) => {
                let what = counters(Num::N(*n), *kind);
                let src = ctx.obj(*from);
                let dst = ctx.obj(*to);
                Clause::imperative(format!("move {what} from {src} onto {dst}"))
            }
            _ => {
                let src = ctx.obj(*from);
                let dst = ctx.obj(*to);
                Clause::imperative(format!("move all counters from {src} onto {dst}"))
            }
        },
        Effect::Shuffle {
            who, have: true, ..
        } => Clause::imperative(format!("have {} shuffle", ctx.obj(*who))),
        Effect::Shuffle { who, library, .. } => {
            let subject = ctx.subj(*who);
            let v = ctx.verb(*who, "shuffles", "shuffle");
            if *library {
                let whose = if *who == Recipient::You {
                    "your"
                } else {
                    "their"
                };
                Clause::with_subject(*who, subject, format!("{v} {whose} library"))
            } else {
                Clause::with_subject(*who, subject, v.to_string())
            }
        }
        Effect::ExileUntilCard { who, filter } => {
            let pred = card_filter_a(filter, ctx.card);
            match who {
                None => Clause::imperative(format!(
                    "exile cards from the top of your library until you exile {pred}"
                )),
                Some(w) => {
                    let subject = ctx.subj(*w);
                    let v = ctx.verb(*w, "exiles", "exile");
                    Clause::with_subject(
                        *w,
                        subject,
                        format!("{v} cards from the top of their library until they exile {pred}"),
                    )
                }
            }
        }
        Effect::MayCastThisFromExile => Clause::imperative(
            "you may cast this card for as long as it remains exiled".to_string(),
        ),
        Effect::MayPlayCard {
            until,
            cast,
            free,
            first,
            it,
            from_exile,
        } => {
            use mtg_core::card::PlayUntil;
            let what = match (*it, *from_exile) {
                (true, false) => "it",
                (true, true) => "it from exile",
                (false, false) => "that card",
                (false, true) => "that card from exile",
            };
            let verb = if *cast { "cast" } else { "play" };
            let free = if *free {
                " without paying its mana cost"
            } else {
                ""
            };
            if *first {
                let when = match until {
                    PlayUntil::ThisTurn => "Until end of turn",
                    PlayUntil::EndOfYourNextTurn => "Until the end of your next turn",
                    PlayUntil::YourNextEndStep
                    | PlayUntil::Forever
                    | PlayUntil::WhileYouControlSource => "Until your next end step",
                };
                Clause::imperative(format!("{when}, you may {verb} {what}{free}"))
            } else {
                let when = match until {
                    PlayUntil::ThisTurn => " this turn",
                    PlayUntil::EndOfYourNextTurn => " until the end of your next turn",
                    PlayUntil::YourNextEndStep => " until your next end step",
                    PlayUntil::Forever => "",
                    PlayUntil::WhileYouControlSource => {
                        return Clause::imperative(format!(
                            "you may {verb} {what}{free} for as long as you control {}",
                            ctx.source_name()
                        ));
                    }
                };
                Clause::imperative(format!("you may {verb} {what}{free}{when}"))
            }
        }
        // "You may cast target Zombie creature card from your graveyard
        // this turn."
        Effect::MayPlayTarget {
            what,
            until,
            cast,
            adventure,
        } => {
            use mtg_core::card::PlayUntil;
            let o = ctx.obj(*what);
            // "you may cast it from your graveyard as an Adventure until
            // the end of your next turn" (Mosswood Dreadknight).
            let o = if *adventure {
                let o = if o == "this card" {
                    "it".to_string()
                } else {
                    o
                };
                format!("{o} from your graveyard as an Adventure")
            } else {
                o
            };
            let verb = if *cast { "cast" } else { "play" };
            let when = match until {
                PlayUntil::ThisTurn => " this turn",
                PlayUntil::EndOfYourNextTurn => " until the end of your next turn",
                PlayUntil::YourNextEndStep => " until your next end step",
                PlayUntil::Forever | PlayUntil::WhileYouControlSource => "",
            };
            Clause::imperative(format!("you may {verb} {o}{when}"))
        }
        Effect::RevealUntil { who, filter } => {
            let pred = card_filter_a(filter, ctx.card);
            match who {
                None => Clause::imperative(format!(
                    "reveal cards from the top of your library until you reveal {pred}"
                )),
                Some(w) => {
                    let subject = ctx.subj(*w);
                    let v = ctx.verb(*w, "reveals", "reveal");
                    let (whose, they) = if *w == Recipient::You {
                        ("your", "you")
                    } else {
                        ("their", "they")
                    };
                    Clause::with_subject(
                        *w,
                        subject,
                        format!(
                            "{v} cards from the top of {whose} library until {they} reveal {pred}"
                        ),
                    )
                }
            }
        }
        Effect::RevealIt { that } => Clause::imperative(
            if *that {
                "reveal that card"
            } else {
                "reveal it"
            }
            .to_string(),
        ),
        Effect::IfYouDont { cant, then, words } => {
            // "If you don't draw a card this way, put a +1/+1 counter on
            // this creature." (Trade Route Envoy): the source named again.
            if *words != crate::ast::DontWords::Plain {
                ctx.source_mentioned = false;
            }
            let body = lowercase_first(&sentences(then, ctx));
            let verb = match words {
                crate::ast::DontWords::Plain if *cant => "can't",
                crate::ast::DontWords::Plain => "don't",
                crate::ast::DontWords::Draw => "don't draw a card this way",
                crate::ast::DontWords::PutHand => "don't put a card into your hand this way",
                crate::ast::DontWords::DidntPutHand => "didn't put a card into your hand this way",
            };
            Clause::imperative(format!("if you {verb}, {}", body.trim_end_matches('.')))
        }
        Effect::IfYouDontPut { to, then, it } => {
            let body = lowercase_first(&sentences(then, ctx));
            let dest = match to {
                mtg_core::card::CardDest::Hand => "into your hand",
                mtg_core::card::CardDest::Graveyard => "into your graveyard",
                mtg_core::card::CardDest::Battlefield { .. } => "onto the battlefield",
                mtg_core::card::CardDest::LibraryBottom => "on the bottom of your library",
            };
            let what = if *it { "it" } else { "the card" };
            Clause::imperative(format!(
                "if you don't put {what} {dest}, {}",
                body.trim_end_matches('.')
            ))
        }
        Effect::DrawReveal => Clause::imperative("draw a card and reveal it".to_string()),
        Effect::DiscardIt => Clause::imperative("discard it".to_string()),
        Effect::IfCard {
            filter,
            negated,
            card,
            that,
            shares_type,
            revealed,
            then,
        } => {
            let body = sentences(then, ctx);
            let body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            let pred = if *card {
                card_filter_a(filter, ctx.card)
            } else {
                target_predicate(filter, ctx.card)
            };
            let cond = match (that, negated) {
                _ if *revealed => format!("{pred} is revealed this way"),
                _ if *shares_type => {
                    format!("it shares a creature type with {}", this_ref(ctx.card))
                }
                (false, false) => format!("it's {pred}"),
                (false, true) => format!("it isn't {pred}"),
                (true, false) => format!("that card is {pred}"),
                (true, true) => format!("that card isn't {pred}"),
            };
            Clause::imperative(format!("if {cond}, {}", body.trim_end_matches('.')))
        }
        Effect::PutCard {
            what,
            to,
            who,
            that_player,
            random,
            rest,
        } => {
            use crate::ast::CardWords;
            use mtg_core::card::CardDest;
            let words = match what {
                CardWords::It => "it",
                CardWords::ThatCard => "that card",
                CardWords::AllRevealed => "all cards revealed this way",
                CardWords::Those => "those cards",
                CardWords::TheRevealed => "the revealed cards",
                CardWords::OtherRevealed => "all other cards revealed this way",
            };
            let whose = match who {
                Some(player) if *player != Recipient::You => "their",
                _ => "your",
            };
            let many = !matches!(what, CardWords::It | CardWords::ThatCard);
            let dest = match to {
                CardDest::Hand => format!("into {whose} hand"),
                CardDest::Graveyard if *that_player => "into that player's graveyard".to_string(),
                CardDest::Graveyard => format!("into {whose} graveyard"),
                CardDest::Battlefield { tapped: false } => "onto the battlefield".to_string(),
                CardDest::Battlefield { tapped: true } => "onto the battlefield tapped".to_string(),
                CardDest::LibraryBottom if *random => {
                    format!("on the bottom of {whose} library in a random order")
                }
                CardDest::LibraryBottom if many => {
                    format!("on the bottom of {whose} library in any order")
                }
                CardDest::LibraryBottom => format!("on the bottom of {whose} library"),
            };
            // "Put that card into your hand and the rest on the bottom of
            // your library in a random order."
            let dest = match rest {
                Some((to, random)) => {
                    let r = match to {
                        CardDest::Hand => format!("into {whose} hand"),
                        CardDest::Graveyard => format!("into {whose} graveyard"),
                        CardDest::Battlefield { .. } => "onto the battlefield".to_string(),
                        CardDest::LibraryBottom if *random => {
                            format!("on the bottom of {whose} library in a random order")
                        }
                        CardDest::LibraryBottom => {
                            format!("on the bottom of {whose} library in any order")
                        }
                    };
                    format!("{dest} and the rest {r}")
                }
                None => dest,
            };
            match who {
                None => Clause::imperative(format!("put {words} {dest}")),
                Some(player) => {
                    let subject = ctx.subj(*player);
                    let verb = ctx.verb(*player, "puts", "put");
                    Clause::with_subject(*player, subject, format!("{verb} {words} {dest}"))
                }
            }
        }
        Effect::DigSplit {
            count,
            of,
            reveal,
            opponent,
            piles,
            rest,
            then,
        } => {
            use mtg_core::card::SplitDest;
            let n = count_words(*count, "one");
            let own = matches!(of, None | Some(Recipient::You));
            let whose = if own {
                "your".to_string()
            } else {
                format!("{}'s", ctx.obj(of.unwrap_or(Recipient::You)))
            };
            let head = if *reveal {
                format!("reveal the top {n} cards of {whose} library")
            } else {
                format!("look at the top {n} cards of {whose} library")
            };
            let lib = if own { "your library" } else { "that library" };
            let dest = |d: SplitDest| match d {
                SplitDest::Hand => "into your hand".to_string(),
                SplitDest::Graveyard if own => "into your graveyard".to_string(),
                SplitDest::Graveyard => "into that player's graveyard".to_string(),
                SplitDest::Top => format!("on top of {lib}"),
                SplitDest::Bottom => format!("on the bottom of {lib}"),
                SplitDest::Exile => "into exile".to_string(),
            };
            // "exile one of them", "put one of them into your graveyard".
            let one = |d: SplitDest, them: &str| match d {
                SplitDest::Exile => format!("exile one of {them}"),
                d => format!("put one of {them} {}", dest(d)),
            };
            let text = match (piles.as_slice(), rest, *opponent) {
                // "An opponent chooses one of them. Put that card into your
                // graveyard and the rest into your hand."
                ([d], Some(r), true) => format!(
                    "{head}. An opponent chooses one of them. Put that card {} and the rest {}",
                    dest(*d),
                    dest(*r)
                ),
                // "Put one back and the rest into that player's graveyard."
                ([SplitDest::Top], Some(r), false) => {
                    format!("{head}. Put one back and the rest {}", dest(*r))
                }
                ([d], None, false) if *then => format!("{head}, then {}", one(*d, "them")),
                ([d], None, false) => {
                    let s = one(*d, "them");
                    format!("{head}. {}{}", s[..1].to_uppercase(), &s[1..])
                }
                // "Put one of those cards into your hand, one on top of your
                // library, and one on the bottom of your library."
                (ds, None, false) if ds.len() >= 2 => {
                    let mut parts: Vec<String> = ds.iter().map(|d| dest(*d)).collect();
                    let last = parts.pop().unwrap_or_default();
                    let mut s = format!("{head}. Put one of those cards {}", parts[0]);
                    for p in &parts[1..] {
                        let _ = write!(s, ", one {p}");
                    }
                    if ds.len() == 2 {
                        let _ = write!(s, " and one {last}");
                    } else {
                        let _ = write!(s, ", and one {last}");
                    }
                    s
                }
                _ => format!("{head}. (unsupported split)"),
            };
            Clause::imperative(text)
        }
        Effect::LookAtTop {
            count,
            of,
            by,
            only,
        } => {
            let n = count_words(*count, "one");
            let whose = match of {
                None | Some(Recipient::You) => "your".to_string(),
                Some(w) if by.is_some_and(|b| b == *w) => "their".to_string(),
                Some(w) => format!("{}'s", ctx.obj(*w)),
            };
            match by {
                None | Some(Recipient::You) => {
                    let back = if *only {
                        ""
                    } else {
                        ", then put them back in any order"
                    };
                    Clause::imperative(format!(
                        "look at the top {n} cards of {whose} library{back}"
                    ))
                }
                Some(b) => {
                    let subject = ctx.subj(*b);
                    let v = ctx.verb(*b, "looks at", "look at");
                    let back = if *only {
                        String::new()
                    } else {
                        format!(
                            ", then {} them back in any order",
                            ctx.verb(*b, "puts", "put")
                        )
                    };
                    Clause::with_subject(
                        *b,
                        subject,
                        format!("{v} the top {n} cards of {whose} library{back}"),
                    )
                }
            }
        }
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
            top,
            onto,
            that_card,
            short,
            reword,
            rest_revealed,
        } => {
            let n = count_words(*count, "one");
            let head = if *reveal_top {
                format!("reveal the top {n} cards of your library")
            } else {
                format!("look at the top {n} cards of your library")
            };
            let two = *count == Num::N(2);
            let rest = |word: &str| {
                // "the rest of the revealed cards" (Kruphix's Insight).
                let word = if *rest_revealed && word == "the rest" {
                    "the rest of the revealed cards"
                } else {
                    word
                };
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
                    // "creature and/or land cards", "creature cards with
                    // mana value 3 or less".
                    let plural = cards_phrase(f, ctx.card, true);
                    let kind = match plural.split_once(" cards") {
                        Some((a, b)) => format!("{} cards{b}", a.replace(" or ", " and/or ")),
                        None => plural,
                    };
                    if *take == crate::ast::ANY_NUMBER {
                        format!("any number of {kind}")
                    } else {
                        format!(
                            "{}{} {kind}",
                            if *optional { "up to " } else { "" },
                            count_words(Num::N(*take), "one")
                        )
                    }
                }
            });
            let it = if several && *that_card {
                "them"
            } else if several {
                "the revealed cards"
            } else if *that_card {
                "that card"
            } else {
                "it"
            };
            // "You may put one of those cards back on top of your library.
            // Put the rest into your graveyard."
            if *top {
                let w = what
                    .clone()
                    .unwrap_or_else(|| "one of those cards".to_string());
                let may = if *optional { "You may put" } else { "Put" };
                // `reword`: "on top of your library" (Voda Sea Scavenger).
                let back = if *reword { "" } else { "back " };
                return Clause::imperative(format!(
                    "{head}. {may} {w} {back}on top of your library. Put {}",
                    rest("the rest")
                ));
            }
            // "You may put a creature card from among them onto the
            // battlefield. Put the rest on the bottom of your library in a
            // random order."; "Put up to two land cards from among them
            // onto the battlefield tapped. ..."; `reword` prints "You may
            // put up to two ...".
            if let Some(tapped) = onto {
                let w = what.unwrap_or_else(|| "one of them".to_string());
                let field = if *tapped {
                    "onto the battlefield tapped"
                } else {
                    "onto the battlefield"
                };
                let put = if *optional && (*take == 1 || *take == crate::ast::ANY_NUMBER || *reword)
                {
                    "You may put"
                } else {
                    "Put"
                };
                // "Put a land card from among them onto the battlefield
                // and the rest into your graveyard." (one sentence when
                // not optional).
                let join = if *optional { ". Put" } else { " and" };
                return Clause::imperative(format!(
                    "{head}. {put} {w} from among them {field}{join} {}",
                    rest("the rest")
                ));
            }
            if filter.is_none() && *take != 1 {
                let k = count_words(Num::N(*take), "one");
                // "Put up to two of them into your hand and the rest into
                // your graveyard."
                if *optional {
                    return Clause::imperative(format!(
                        "{head}. Put up to {k} of them into your hand and {}",
                        rest("the rest")
                    ));
                }
                // "Put two of them into your hand and the other into your
                // graveyard." (Dark Bargain): one card left over.
                if *count == Num::N(*take + 1) {
                    return Clause::imperative(format!(
                        "{head}. Put {k} of them into your hand and {}",
                        rest("the other")
                    ));
                }
                // "Put two of those cards into your hand and the rest into
                // your graveyard."
                let those = if *reword { "them" } else { "those cards" };
                return Clause::imperative(format!(
                    "{head}. Put {k} of {those} into your hand and {}",
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
                // `reword`: "... into your hand. Then put the rest on the
                // bottom of your library in any order." (Ancient Stirrings).
                (Some(w), false, true) if *revealing => format!(
                    "You may reveal {w} from among them and put {it} into your hand. {} {}",
                    if *reword { "Then put" } else { "Put" },
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
                // "Put one into your hand and the other into your
                // graveyard."
                (None, _, _) if *short => format!(
                    "Put one into your hand and {}",
                    rest(if two { "the other" } else { "the rest" })
                ),
                (None, _, _) => format!(
                    "Put one of {} into your hand and {}",
                    if *reword { "those cards" } else { "them" },
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
        Effect::Modes(modes) | Effect::ModesTwo(modes) | Effect::ModesOf(_, modes) => {
            let mut out = match e {
                Effect::ModesTwo(_) => "choose two —".to_string(),
                Effect::ModesOf(h, _) => format!("{} —", mode_head_text(*h)),
                _ => "choose one —".to_string(),
            };
            for m in modes {
                let mut inner = Ctx::new(ctx.card, ctx.lookup, &m.targets);
                inner.current = ctx.current;
                inner.trigger = ctx.trigger;
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
            // After "Until end of turn," the rule is quoted: "gains
            // \"This creature can't be blocked by more than one
            // creature.\"" (Full Steam Ahead).
            if let Ability::BlockRule(r) = ability.as_ref()
                && !ctx.eot_first
            {
                return Clause::with_subject(*to, s, block_rule_phrase(r, ctx.card, true));
            }
            // "gains toxic 1 until end of turn": a keyword ability, bare.
            let q = granted_keyword(ability, ctx.card).unwrap_or_else(|| {
                // "Kellan ... gains \"Whenever Kellan deals combat damage
                // ...\"": a legendary card granting itself an ability
                // names itself in it.
                if *to == Recipient::Source && ctx.card.supertypes.contains(&Supertype::Legendary) {
                    let mut holder = ctx.card.blank();
                    holder.name.clone_from(&ctx.card.name);
                    holder.short_name.clone_from(&ctx.card.short_name);
                    holder.supertypes.clone_from(&ctx.card.supertypes);
                    holder.types = vec![CardType::Creature];
                    holder.groups.clone_from(&ctx.card.groups);
                    holder.counts.clone_from(&ctx.card.counts);
                    format!("\"{}\"", ability_text(&holder, ability, ctx.lookup))
                } else {
                    quoted(ability, ctx.card, CardType::Creature, ctx.lookup)
                }
            });
            Clause::with_subject(*to, s, format!("{} {q}", ctx.verb(*to, "gains", "gain")))
                .until_eot()
        }
        Effect::ReturnChosen { count, filter, who } => {
            let what = if *count == Num::N(1) {
                filter_a(filter, ctx.card)
            } else {
                format!(
                    "{} {}",
                    count_words(*count, "a"),
                    filter_phrase(filter, ctx.card, true)
                )
            };
            let owner = if *count == Num::N(1) {
                "its owner's hand"
            } else {
                "their owners' hands"
            };
            if let Some(who) = who {
                let s = ctx.subj(*who);
                return Clause::with_subject(
                    *who,
                    s,
                    format!("{} {what} to {owner}", ctx.verb(*who, "returns", "return")),
                );
            }
            Clause::imperative(format!("return {what} to its owner's hand"))
        }
        Effect::BlocksIfAble { blocker, attacker } => {
            let s = ctx.subj(*blocker);
            let a = ctx.obj(*attacker);
            Clause::with_subject(*blocker, s, format!("blocks {a} this turn if able"))
        }
        Effect::CantBlockThis { blocker, attacker } => {
            let s = ctx.subj(*blocker);
            let a = ctx.obj(*attacker);
            Clause::with_subject(*blocker, s, format!("can't block {a} this turn"))
        }
        Effect::CantBeRegenerated { to } => {
            let s = ctx.subj(*to);
            Clause::with_subject(*to, s, "can't be regenerated this turn".to_string())
        }
        // "Creatures don't untap during target player's next untap step."
        Effect::SkipNext {
            who,
            what: mtg_core::card::SkipWhat::CreatureUntap,
        } => {
            let whose = if *who == Recipient::You {
                "your".to_string()
            } else {
                format!("{}'s", ctx.obj(*who))
            };
            Clause::imperative(format!(
                "creatures don't untap during {whose} next untap step"
            ))
        }
        Effect::SkipNext { who, what } => {
            let s = ctx.subj(*who);
            let whose = if *who == Recipient::You {
                "your"
            } else {
                "their"
            };
            let w = match what {
                mtg_core::card::SkipWhat::DrawStep => "draw step",
                mtg_core::card::SkipWhat::CombatPhase => "combat phase",
                mtg_core::card::SkipWhat::CombatPhaseThisTurn => "combat phase this turn",
                mtg_core::card::SkipWhat::CreatureUntap => "untap step",
            };
            let v = ctx.verb(*who, "skips", "skip");
            Clause::with_subject(*who, s, format!("{v} {whose} next {w}"))
        }
        Effect::AssignsNoCombatDamage { to } => {
            let s = ctx.subj(*to);
            Clause::with_subject(*to, s, "assigns no combat damage this turn".to_string())
        }
        Effect::ExtraTurns { who, n } => {
            let turns = if *n == 1 {
                "an extra turn".to_string()
            } else {
                format!("{} extra turns", count_words(Num::N(u32::from(*n)), "one"))
            };
            if *who == Recipient::You {
                Clause::imperative(format!("take {turns} after this one"))
            } else {
                let s = ctx.subj(*who);
                let v = ctx.verb(*who, "takes", "take");
                Clause::with_subject(*who, s, format!("{v} {turns} after this one"))
            }
        }
        // CR 500.8.
        Effect::AdditionalPhases {
            combats,
            main,
            leading,
            main_phase,
        } => {
            let there = if *combats == 1 {
                "there is an additional combat phase".to_string()
            } else {
                format!(
                    "there are {} additional combat phases",
                    count_words(Num::N(u32::from(*combats)), "one")
                )
            };
            let then = if *main {
                " followed by an additional main phase"
            } else {
                ""
            };
            if *leading {
                let m = if *main_phase { "main " } else { "" };
                Clause::imperative(format!("after this {m}phase, {there}{then}"))
            } else {
                Clause::imperative(format!("{there} after this phase{then}"))
            }
        }
        Effect::SkipNextUntap { to, steps } => {
            let s = match *to {
                // "Tap up to two target creatures. Those creatures don't
                // untap ...", named again without who controls them.
                Recipient::Targets(a, _) if ctx.target_mentioned[a] => {
                    let n = ctx.plural_target_noun(a);
                    let n = [" you don't control", " an opponent controls"]
                        .iter()
                        .find_map(|c| n.strip_suffix(c))
                        .unwrap_or(&n);
                    format!("those {n}")
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
            // "It doesn't untap during your next untap step": the source,
            // or a target you control.
            let yours = match *to {
                Recipient::Source => true,
                Recipient::Target(n) | Recipient::ThatTarget(n) => {
                    match ctx.targets.get(n).map(Target::base) {
                        Some(Target::Filter(f)) => f.who == Some(Who::You) && f.or.is_none(),
                        Some(Target::Spec(s)) => matches!(
                            s,
                            TargetSpec::CreatureYouControl
                                | TargetSpec::NonlegendaryCreatureYouControl
                        ),
                        _ => false,
                    }
                }
                _ => false,
            };
            let text = if yours && !plural {
                "doesn't untap during your next untap step"
            } else if plural && *steps {
                "don't untap during their controllers' next untap steps"
            } else if plural {
                "don't untap during their controller's next untap step"
            } else {
                "doesn't untap during its controller's next untap step"
            };
            Clause::with_subject(*to, s, text.to_string())
        }
        Effect::ProtectionOr { to, filter } => {
            let s = ctx.subj(*to);
            let p = lowercase_first(&protection_text(filter, ctx.card));
            Clause::with_subject(
                *to,
                s,
                format!(
                    "{} {p} or from the color of your choice",
                    ctx.verb(*to, "gains", "gain")
                ),
            )
            .until_eot()
        }
        Effect::CreatureTypes { to, all } => {
            let s = ctx.subj(*to);
            let verb = if *all {
                ctx.verb(*to, "gains", "gain")
            } else {
                ctx.verb(*to, "loses", "lose")
            };
            Clause::with_subject(*to, s, format!("{verb} all creature types")).until_eot()
        }
        Effect::GrantFlashback {
            to,
            that_card,
            harmonize: true,
        } => {
            // "Target instant or sorcery card in your graveyard gains
            // harmonize until end of turn. Its harmonize cost is equal to
            // its mana cost."
            let s = ctx
                .subj(*to)
                .replacen(" from your graveyard", " in your graveyard", 1);
            let its = if *that_card { "That card's" } else { "Its" };
            Clause::with_subject(
                *to,
                s,
                format!(
                    "{} harmonize until end of turn. {its} harmonize cost is equal to its mana cost",
                    ctx.verb(*to, "gains", "gain")
                ),
            )
        }
        Effect::GrantFlashback { to, that_card, .. } => {
            // "Target instant or sorcery card in your graveyard gains
            // flashback until end of turn. The flashback cost is equal to
            // its mana cost."
            let s = ctx
                .subj(*to)
                .replacen(" from your graveyard", " in your graveyard", 1);
            let its = if *that_card { "that card's" } else { "its" };
            Clause::with_subject(
                *to,
                s,
                format!(
                    "{} flashback until end of turn. The flashback cost is equal to {its} mana cost",
                    ctx.verb(*to, "gains", "gain")
                ),
            )
        }
        Effect::SwitchPt { to } => {
            let o = ctx.obj(*to);
            Clause::imperative(format!("switch {o}'s power and toughness")).until_eot()
        }
        Effect::PutFromHand {
            filter,
            tapped,
            also,
            attacking,
        } => {
            let one = |f: &Filter| {
                if *f == Filter::default() {
                    "a card".to_string()
                } else {
                    with_article(&format!("{} card", card_filter_phrase(f, ctx.card)))
                }
            };
            // "a creature card and/or a land card".
            let what = match also {
                Some(g) => format!("{} and/or {}", one(filter), one(g)),
                None => one(filter),
            };
            Clause::imperative(format!(
                "put {what} from your hand onto the battlefield{}",
                match (*tapped, *attacking) {
                    (true, true) => " tapped and attacking",
                    (true, false) => " tapped",
                    _ => "",
                }
            ))
        }
        Effect::Explore { what, twice, times } => {
            let s = ctx.subj(*what);
            let text = if *twice {
                "explores, then it explores again".to_string()
            } else if let Some(n) = times {
                format!("explores {} times", digits(*n))
            } else {
                "explores".to_string()
            };
            Clause::with_subject(*what, s, text)
        }
        Effect::PhaseOut { what } => {
            let s = ctx.subj(*what);
            Clause::with_subject(*what, s, "phases out".to_string())
        }
        Effect::Endure { what, count } => {
            let s = ctx.subj(*what);
            Clause::with_subject(*what, s, format!("endures {}", digits(*count)))
        }
        Effect::Amass { kind, count } => {
            ctx.amassed = true;
            Clause::imperative(format!("amass {kind} {}", digits(*count)))
        }
        Effect::EmpowerJace { count } => {
            Clause::imperative(format!("empower Jace {}", digits(*count)))
        }
        Effect::CreatedAttack | Effect::ClearEcho => Clause::imperative(String::new()),
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
        Effect::PayEnergy(mtg_core::card::PAY_ANY_ENERGY) => {
            Clause::imperative("pay any amount of {E}".to_string())
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
                    "{}{} {} life",
                    ctx.each_many(*who),
                    ctx.verb(*who, "loses", "lose"),
                    digits(*amount)
                ),
            )
        }
        // CR 104.3e.
        Effect::LoseGame { who } => {
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!("{} the game", ctx.verb(*who, "loses", "lose")),
            )
        }
        // CR 107.1a: "loses half their life, rounded up".
        Effect::LoseHalfLife { who } => {
            let s = ctx.subj(*who);
            let their = if *who == Recipient::You {
                "your"
            } else {
                "their"
            };
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{}{} half {their} life, rounded up",
                    ctx.each_many(*who),
                    ctx.verb(*who, "loses", "lose")
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
                    // "Two target players each draw a card" (Gleaming
                    // Splendor).
                    let each = if matches!(w, Recipient::Targets(..) | Recipient::YouAndTarget(_)) {
                        "each "
                    } else {
                        ctx.each_many(*w)
                    };
                    Clause::with_subject(
                        *w,
                        s,
                        format!("{each}{} {what}", ctx.verb(*w, "draws", "draw")),
                    )
                }
            }
        }
        Effect::Mill { count, who } => {
            let what = format!("{} {}", count_words(*count, "a"), plural(*count, "card"));
            match who {
                None => Clause::imperative(format!("mill {what}")),
                Some(w) => {
                    let s = ctx.subj(*w);
                    let each = ctx.each_many(*w);
                    Clause::with_subject(
                        *w,
                        s,
                        format!("{each}{} {what}", ctx.verb(*w, "mills", "mill")),
                    )
                }
            }
        }
        Effect::AddCounters { kind, count, to } => {
            let c = counters(*count, *kind);
            let each = ctx.each_of(*to);
            let o = ctx.obj(*to);
            Clause::imperative(format!("put {c} on {each}{o}"))
        }
        // "This ability can't cause the total number of +1/+0 counters on
        // this creature to be greater than four." (the Clockwork cap).
        Effect::AddCountersCapped {
            kind,
            count,
            to,
            cap,
        } => {
            let c = counters(*count, *kind);
            let o = ctx.obj(*to);
            let all = counters(Num::N(2), *kind);
            let all = all.trim_start_matches("two ");
            Clause::imperative(format!(
                "put up to {c} on {o}. This ability can't cause the total number of {all} on {o} to be greater than {}",
                count_words(Num::N(*cap), "one")
            ))
        }
        Effect::AddCounterList { counters: cs, to } => {
            let c: Vec<String> = cs.iter().map(|(k, n)| counters(*n, *k)).collect();
            let each = ctx.each_of(*to);
            let o = ctx.obj(*to);
            Clause::imperative(format!("put {} on {each}{o}", list(&c)))
        }
        Effect::RemoveCounters { kind, count, to } => {
            let c = counters(*count, *kind);
            let o = ctx.obj(*to);
            Clause::imperative(format!("remove {c} from {o}"))
        }
        // CR 601.2d.
        Effect::DealDamageDivided { amount, to } => {
            let s = ctx.source_subj();
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
        Effect::RemoveCounterChosen { to, up_to } if *up_to > 0 => {
            let o = ctx.obj(*to);
            Clause::imperative(format!(
                "remove up to {} counters from {o}",
                count_words(Num::N(u32::from(*up_to)), "one")
            ))
        }
        Effect::RemoveCounterChosen { to, .. } => {
            let o = ctx.obj(*to);
            Clause::imperative(format!("remove a counter from {o}"))
        }
        Effect::RemoveAllCounters { to } => {
            let o = ctx.obj_written(*to);
            Clause::imperative(format!("remove all counters from {o}"))
        }
        Effect::ReturnToHand { what, owner_hand } => {
            let yours = what.iter().all(|w| {
                matches!(
                    w,
                    Recipient::Target(i) if matches!(ctx.targets[*i].base(),
                        Target::Spec(TargetSpec::CreatureCardInYourGraveyard)
                        | Target::GraveyardCard { who: Who::You, .. })
                        // "Return target creature you own to your hand."
                        // (Saving Grasp).
                        || matches!(ctx.targets[*i].base(),
                            Target::Filter(f) if f.status.you_own)
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
            // "target nonland permanent and all other permanents with the
            // same name as that permanent ... to their owners' hands".
            let target_and = matches!(what.as_slice(), [Recipient::TargetAnd(..)]);
            // "Return X target creatures to their owners' hands".
            let plural = what.len() > 1
                || all
                || target_and
                || ctx.is_many(what[0])
                || matches!(
                    what[0],
                    Recipient::Targets(..)
                        | Recipient::ThoseTargets(..)
                        | Recipient::ChosenTargets(..)
                );
            // "Return target creature card and all other cards with the
            // same name as that card from your graveyard to your hand."
            if let [Recipient::TargetAnd(t, _)] = what.as_slice()
                && let Target::Spec(TargetSpec::CreatureCardInYourGraveyard)
                | Target::GraveyardCard { who: Who::You, .. } = ctx.targets[*t].base()
            {
                let o = ctx.obj(what[0]).replacen(" from your graveyard", "", 1);
                return Clause::imperative(format!("return {o} from your graveyard to your hand"));
            }
            let o = if all {
                ctx.obj_all(what[0])
            } else if what.len() == 1 {
                ctx.obj(what[0])
            } else {
                ctx.obj_list(what, false)
            };
            // "Return up to one target instant card and up to one target
            // sorcery card from your graveyard to your hand": one graveyard
            // for the list.
            let o = if yours && what.len() > 1 {
                format!(
                    "{} from your graveyard",
                    o.replace(" from your graveyard", "")
                )
            } else {
                o
            };
            // Activated from the graveyard: "Return this card from your
            // graveyard to your hand."
            let from_graveyard =
                what.as_slice() == [Recipient::Source] && works_from_graveyard(ctx.current);
            if from_graveyard {
                return Clause::imperative(
                    "return this card from your graveyard to your hand".to_string(),
                );
            }
            // "Return two target cards from an opponent's graveyard to
            // their hand."
            let theirs = what.iter().all(|w| {
                matches!(
                    w,
                    Recipient::Target(i) if matches!(ctx.targets[*i].base(),
                        Target::GraveyardCard { who: Who::Opponents, .. })
                )
            });
            let hand = match (yours, plural) {
                (true, _) => "your hand",
                _ if theirs => "their hand",
                (false, false) => "its owner's hand",
                // "Return all attacking creatures to their owner's hand."
                (false, true) if *owner_hand => "their owner's hand",
                (false, true) => "their owners' hands",
            };
            Clause::imperative(format!("return {o} to {hand}"))
        }
        Effect::ReturnToBattlefield {
            what,
            counters: c,
            tapped,
            under_you,
            attached,
            put,
            attacking,
            additional,
            more,
            if_creature,
            transformed,
            front_up,
        } => {
            let what_ref = what;
            // `that card` prints "that card", `it` (inside a delayed
            // trigger) "it".
            let o = match what {
                Recipient::SameCard | Recipient::CreatedObject => "that card".to_string(),
                other => ctx.obj(*other),
            };
            // A card from your own graveyard is yours: Oracle drops "under
            // its owner's control".
            let from_yours = |i: usize| {
                matches!(
                    ctx.targets[i].base(),
                    Target::Spec(TargetSpec::CreatureCardInYourGraveyard)
                        | Target::GraveyardCard { who: Who::You, .. }
                )
            };
            // "return up to two target creature cards ... from your
            // graveyard to the battlefield" too.
            let yours = match what {
                Recipient::Target(i) | Recipient::ThatTarget(i) | Recipient::ChosenTarget(i) => {
                    from_yours(*i)
                }
                Recipient::Targets(first, count) => (*first..*first + *count).all(from_yours),
                // "Whenever a creature is put into your graveyard from the
                // battlefield, ... return that card to the battlefield"
                // (Angelic Renewal): the card is yours.
                Recipient::SameCard | Recipient::TriggeringCard => matches!(
                    ctx.current,
                    Some(Ability::Triggered {
                        trigger: Trigger::PutIntoGraveyardFromBattlefield(_)
                            | Trigger::PutIntoGraveyard(_),
                        ..
                    })
                ),
                _ => false,
            };
            // A card from another player's graveyard isn't "returned":
            // "put target creature card from a graveyard onto the
            // battlefield under your control".
            let others = *under_you
                && matches!(what, Recipient::Target(i) if matches!(
                    ctx.targets[*i].base(),
                    Target::GraveyardCard { who: Who::Any | Who::Opponents, .. }
                ) || matches!(
                    ctx.targets[*i].base(),
                    Target::GraveyardCard { filter, .. } if filter.that_player.is_some()
                ));
            // On the battlefield the returned creature card is "that
            // creature" ("That creature explores, then it explores again").
            if let Recipient::Target(i) = what {
                let creature = match ctx.targets[*i].base() {
                    Target::Spec(TargetSpec::CreatureCardInYourGraveyard) => true,
                    Target::GraveyardCard { filter, .. } => filter.types == [CardType::Creature],
                    _ => false,
                };
                if creature {
                    ctx.target_noun[*i] = "creature".to_string();
                } else if matches!(
                    ctx.targets[*i].base(),
                    Target::GraveyardCard { .. }
                        | Target::Spec(TargetSpec::CreatureCardInYourGraveyard)
                ) {
                    // "Return target artifact card ... to the battlefield.
                    // If it's a Vehicle, ..." (Tune Up): a permanent now.
                    ctx.target_noun[*i] = "permanent".to_string();
                } else if ctx.target_noun[*i] == "card" {
                    // "Exile target creature you control, then return it to
                    // the battlefield ... If that creature is a Bird, ..."
                    // (Splash Portal): back on the battlefield.
                    ctx.target_noun[*i] = target_noun(&ctx.targets[*i]);
                }
            }
            let tapped = match (*tapped, *attacking, *transformed) {
                (true, true, _) => " tapped and attacking",
                // CR 712.14a: "return it to the battlefield tapped and
                // transformed under its owner's control".
                (true, false, true) => " tapped and transformed",
                (true, false, false) => " tapped",
                (false, _, true) => " transformed",
                _ => "",
            };
            // Activated from the graveyard: "Return this card from your
            // graveyard to the battlefield tapped."
            let from_graveyard = *what == Recipient::Source && works_from_graveyard(ctx.current);
            let mut text = if *front_up {
                // A transforming card's back face returning itself (CR
                // 712.14): "return it to the battlefield (front face up)".
                format!("return {o} to the battlefield{tapped} (front face up)")
            } else if *attacking && (others || (*put && *under_you)) {
                // "put target creature card from defending player's
                // graveyard onto the battlefield under your control tapped
                // and attacking".
                format!("put {o} onto the battlefield under your control{tapped}")
            } else if from_graveyard {
                format!("return this card from your graveyard to the battlefield{tapped}")
            } else if others || (*put && *under_you) {
                format!("put {o} onto the battlefield{tapped} under your control")
            } else if *put && yours {
                format!("put {o} onto the battlefield{tapped}")
            } else if *put {
                format!("put {o} onto the battlefield{tapped} under its owner's control")
            } else if *under_you {
                format!("return {o} to the battlefield{tapped} under your control")
            } else if yours
                || (*what == Recipient::Source
                    && ctx.card.pronoun == Some(crate::ast::Pronoun::They))
            {
                // "return them to the battlefield" (Tom, Bert, and William
                // return themselves, their owner's ability).
                format!("return {o} to the battlefield{tapped}")
            } else if matches!(what, Recipient::ThoseCards(..) | Recipient::SameCards) {
                // "return those cards to the battlefield under their
                // owner's control" (Oracle's singular "owner's"); "exile
                // two target creatures and/or lands you control, then
                // return them ..." after N targets of one kind (Gone
                // Fishing; "up to two" keeps "those cards", Displace).
                let o = if ctx.targets.len() > 1
                    && !matches!(ctx.targets[0], Target::UpTo(_))
                    && ctx.targets.iter().all(|t| *t == ctx.targets[0])
                {
                    "them".to_string()
                } else {
                    o
                };
                format!("return {o} to the battlefield{tapped} under their owner's control")
            } else if matches!(what, Recipient::Targets(..) | Recipient::SameExiledCards) {
                // "exile up to one target artifact you control and up to
                // one target creature you control. Then return them to the
                // battlefield under their owners' control."
                format!("return {o} to the battlefield{tapped} under their owners' control")
            } else {
                // "Exile Nicol Bolas, then return him to the battlefield
                // transformed under his owner's control."
                let its = match (what, ctx.card.pronoun) {
                    (Recipient::Source, Some(crate::ast::Pronoun::He)) => "his",
                    (Recipient::Source, Some(crate::ast::Pronoun::She)) => "her",
                    _ => "its",
                };
                format!("return {o} to the battlefield{tapped} under {its} owner's control")
            };
            if let Some((kind, n)) = c {
                let mut what = counters(Num::N(*n), *kind);
                // "with a hexproof counter and an indestructible counter".
                if let Some((k, m)) = more {
                    let _ = write!(what, " and {}", counters(Num::N(*m), *k));
                }
                // "with two additional +1/+1 counters on it" (Rakdos
                // Joins Up).
                // "with an additional +1/+1 counter on it" (Prison Break).
                let what = if *additional {
                    match what.split_once(' ') {
                        Some(("a", rest)) => format!("an additional {rest}"),
                        Some((count, rest)) => format!("{count} additional {rest}"),
                        None => what,
                    }
                } else {
                    what
                };
                if *if_creature {
                    // A sentence of its own, after a delayed return's
                    // timing (Teferi's Time Twist).
                    let tail = format!("If it enters as a creature, it enters with {what} on it");
                    if ctx.in_delayed {
                        ctx.delayed_tail = Some(tail);
                    } else {
                        let _ = write!(text, ". {tail}");
                    }
                    return Clause::imperative(text);
                }
                // "with a +1/+1 counter on her" (Gallia, returning itself).
                let it = if *what_ref == Recipient::Source {
                    it_obj(ctx.card)
                } else {
                    "it"
                };
                let _ = write!(text, " with {what} on {it}");
            }
            if let Some(a) = attached {
                let _ = write!(text, " attached to {}", ctx.obj(*a));
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
        // CR 610.3: "exile target creature an opponent controls until this
        // enchantment leaves the battlefield".
        Effect::ExileUntil { what } => {
            let o = ctx.obj_written(*what);
            if let Recipient::Target(i) = what {
                ctx.target_noun[*i] = "card".to_string();
            }
            Clause::imperative(format!(
                "exile {o} until {} leaves the battlefield",
                this_ref(ctx.card)
            ))
        }
        Effect::ExileAllGraveyards => Clause::imperative("exile all graveyards".to_string()),
        Effect::ExileGraveyard { who, filter } => {
            let o = ctx.obj(*who);
            let whose = match o.as_str() {
                "you" => "your".to_string(),
                "it" | "them" => "their".to_string(),
                // "Exile any number of target players' graveyards."
                // (Thraben Charm).
                o if o.ends_with("players") => format!("{o}'"),
                o => format!("{o}'s"),
            };
            let yards = if whose.ends_with("players'") {
                "graveyards"
            } else {
                "graveyard"
            };
            match filter {
                // "Exile all creature cards from target player's graveyard."
                Some(f) => Clause::imperative(format!(
                    "exile all {} from {whose} {yards}",
                    cards_phrase(f, ctx.card, true)
                )),
                None => Clause::imperative(format!("exile {whose} {yards}")),
            }
        }
        Effect::Sacrifice { what } => {
            let o = ctx.obj_written(*what);
            Clause::imperative(format!("sacrifice {o}"))
        }
        Effect::SacrificeBy { who, what } => {
            let s = ctx.subj(*who);
            let o = ctx.obj(*what);
            let v = ctx.verb(*who, "sacrifices", "sacrifice");
            Clause::with_subject(*who, s, format!("{v} {o}"))
        }
        Effect::ExtraLandPlays(1) => {
            Clause::imperative("you may play an additional land this turn".to_string())
        }
        Effect::ExtraLandPlays(n) => Clause::imperative(format!(
            "you may play up to {} this turn",
            additional_lands(*n)
        )),
        Effect::NextSpellUncounterable => {
            Clause::imperative("the next spell you cast this turn can't be countered".to_string())
        }
        Effect::NextSpellFree(f) => Clause::imperative(format!(
            "the next {} you cast this turn can be cast without paying its mana cost",
            a_spell(f, ctx.card)
                .trim_start_matches("a ")
                .trim_start_matches("an ")
        )),
        Effect::NextSpellLess(n) => Clause::imperative(format!(
            "the next spell you cast this turn costs {{{n}}} less to cast"
        )),
        Effect::TapOrUntap { to } => {
            let o = ctx.obj(*to);
            Clause::imperative(format!("tap or untap {o}"))
        }
        // "Search its controller's graveyard, hand, and library for all
        // cards with the same name as that spell and exile them. Then that
        // player shuffles."
        Effect::ExileSameName { what } => {
            let o = match what {
                // The target's own noun, though it has been exiled since
                // ("that creature", not "that card").
                Recipient::Target(i) if *i < ctx.targets.len() => {
                    ctx.target_mentioned[*i] = true;
                    format!("that {}", target_noun(&ctx.targets[*i]))
                }
                other => ctx.obj(*other),
            };
            Clause::imperative(format!(
                "search its controller's graveyard, hand, and library for all cards with the same name as {o} and exile them. Then that player shuffles"
            ))
        }
        Effect::FaceDownSpellsLess(n) => Clause::imperative(format!(
            "face-down spells you cast this turn cost {{{n}}} less to cast"
        )),
        Effect::TurnFaceUp { what } => {
            let o = ctx.obj(*what);
            Clause::imperative(format!("turn {o} face up"))
        }
        Effect::TurnFaceDown { what } => {
            let o = ctx.obj(*what);
            Clause::imperative(format!("turn {o} face down"))
        }
        Effect::LookAtFaceDown { what } => {
            let o = ctx.obj(*what);
            Clause::imperative(format!("look at {o}"))
        }
        Effect::LookAtPlayer { who } => {
            let o = ctx.obj(*who);
            let whose = match o.as_str() {
                "you" => "your".to_string(),
                "it" | "them" => "their".to_string(),
                o => format!("{o}'s"),
            };
            Clause::imperative(format!(
                "look at {whose} hand, the top card of that player's library, and any face-down creatures they control"
            ))
        }
        Effect::Goad { what } => {
            let o = ctx.obj(*what);
            Clause::imperative(format!("goad {o}"))
        }
        // CR 702.84a's replacement, as its own sentence (Whip of Erebos).
        Effect::ExileOnLeave { what } => {
            let o = ctx.obj(*what);
            // "If that creature would leave the battlefield, exile it
            // instead ..." (Gruesome Encore).
            let it = if o.starts_with("those ") || o == "them" {
                "them"
            } else {
                "it"
            };
            Clause::imperative(format!(
                "if {o} would leave the battlefield, exile {it} instead of putting {it} anywhere else"
            ))
        }
        Effect::Detain { what } => {
            let o = ctx.obj(*what);
            Clause::imperative(format!("detain {o}"))
        }
        Effect::Tap { to } => {
            let o = ctx.obj_list_tap(to);
            Clause::imperative(format!("tap {o}"))
        }
        Effect::Untap { to } => {
            let o = ctx.obj_list_tap(to);
            Clause::imperative(format!("untap {o}"))
        }
        Effect::RemoveFromCombat { what } => {
            let o = ctx.obj(*what);
            Clause::imperative(format!("remove {o} from combat"))
        }
        Effect::BecomeBlocked { what } => {
            let s = ctx.subj(*what);
            let v = ctx.verb(*what, "becomes", "become");
            Clause::with_subject(*what, s, format!("{v} blocked"))
        }
        Effect::Grant {
            keywords,
            to,
            each,
            no_also,
        } => {
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
            // "That creature doesn't untap during its controller's untap
            // step for as long as you control this creature".
            if keywords.as_slice() == [Keyword::DoesntUntap] {
                let v = ctx.verb(
                    *to,
                    "doesn't untap during its controller's untap step",
                    "don't untap during their controllers' untap steps",
                );
                let mut c = Clause::with_subject(*to, s, v.to_string());
                c.suffix = Some("this turn");
                return c;
            }
            // "Its activated abilities can't be activated for as long as it
            // remains tapped" (CR 602.5).
            if keywords.len() == 1 && is_activation_bar(keywords[0]) {
                let mut c =
                    Clause::imperative(format!("{} {}", possessive(&s), &keywords[0].name()[4..]));
                c.suffix = Some("this turn");
                return c;
            }
            if keywords.iter().all(|k| is_restriction(*k)) {
                let mut c = Clause::with_subject(*to, s, restriction_phrase(keywords));
                c.suffix = Some("this turn");
                return c;
            }
            // "Creatures you control get +2/+1 until end of turn. White
            // creatures you control also gain first strike ...": some of
            // the creatures just pumped.
            let also = if !*no_also
                && ctx.pumped.contains(&Recipient::EachCreatureYouControl)
                && narrower_creatures_you_control(*to, ctx.card)
            {
                "also "
            } else {
                ""
            };
            Clause::with_subject(
                *to,
                s,
                format!(
                    "{also}{}{} {}",
                    if *each { "each " } else { "" },
                    ctx.verb(*to, "gains", "gain"),
                    keywords_list(keywords)
                ),
            )
            .until_eot()
        }
        Effect::GainControl { of, forever } => {
            // "Gain control of all artifacts your opponents control" (`all
            // <filter>`) against "each permanent with a corruption counter".
            let o = match of {
                Recipient::Group(i)
                    if ctx.card.groups.get(usize::from(*i)).is_some_and(|f| f.all) =>
                {
                    ctx.obj_all(*of)
                }
                _ => ctx.obj(*of),
            };
            let c = Clause::imperative(format!("gain control of {o}"));
            if *forever { c } else { c.until_eot() }
        }
        Effect::Attach { what, to } if *what != Recipient::Source => {
            let w = ctx.obj(*what);
            // "Attach it to U.S.Agent": after "it" is the created token,
            // the source is named again.
            let o = if *what == Recipient::CreatedObject && *to == Recipient::Source {
                ctx.source_name()
            } else {
                ctx.obj(*to)
            };
            Clause::imperative(format!("attach {w} to {o}"))
        }
        // "Unattach all Equipment from target creature" (the group's filter
        // names what they're attached to).
        Effect::Unattach { what } => {
            let w = ctx.obj(*what).replacen(" attached to ", " from ", 1);
            let w = w
                .strip_prefix("each ")
                .map_or_else(|| w.clone(), |r| format!("all {r}"));
            Clause::imperative(format!("unattach {w}"))
        }
        Effect::Attach {
            to: Recipient::CreatedObject,
            ..
        } if ctx.amassed => {
            Clause::imperative(format!("attach {} to the amassed Army", this_ref(ctx.card)))
        }
        Effect::Attach { to, .. } => {
            let o = ctx.obj(*to);
            // After creating a token, "it" is the token: Oracle names the
            // source again ("attach this Equipment to it"); likewise after
            // manifesting dread ("... to that creature").
            let me = if matches!(*to, Recipient::CreatedObject | Recipient::Manifested)
                && !is_spell(ctx.card)
            {
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
            attacking,
            who,
            it_has,
            attached,
            sized,
        } => {
            if *tapped == ctx.last_token_tapped
                && !*attacking
                && who.is_none()
                && attached.is_none()
                && ctx.those_tokens.as_deref() == Some(token.as_str())
            {
                return Clause::imperative(format!(
                    "create {} of those tokens",
                    count_words(*count, "one")
                ));
            }
            ctx.last_token = Some(token.clone());
            ctx.last_token_tapped = *tapped && !*attacking;
            let mut what = token_description(token, *count, *tapped && !*attacking, *it_has, ctx);
            // "an X/X green Elemental creature token" (the definition's
            // power and toughness stand for X).
            if sized.is_some()
                && let Some(t) = ctx.lookup.get(token)
                && let Some((p, q)) = t.pt
            {
                let pt = format!("{p}/{q} ");
                if let Some(i) = what.find(&pt) {
                    what.replace_range(i..i + pt.len(), "X/X ");
                    // "a X/X" → "an X/X".
                    if what[..i].ends_with("a ") && !what[..i].ends_with("an ") {
                        what.insert(i - 1, 'n');
                    }
                }
            }
            // CR 508.4: "a ... token that's tapped and attacking", "two ...
            // tokens that are tapped and attacking".
            if *attacking {
                let _ = write!(
                    what,
                    " {} tapped and attacking",
                    if *count == Num::N(1) {
                        "that's"
                    } else {
                        "that are"
                    }
                );
            }
            // "Create a Wicked Role token attached to up to one target
            // creature you control", "... attached to it".
            if let Some(a) = attached {
                let o = ctx.obj(*a);
                // One for each object: "For each Rat you control, create a
                // Wicked Role token attached to that Rat", "For each of
                // those creatures, create a Monster Role token attached to
                // it".
                let each = if let Some(rest) = o.strip_prefix("each ") {
                    let head = [
                        " you control",
                        " your opponents control",
                        " an opponent controls",
                    ]
                    .iter()
                    .find_map(|c| rest.split_once(c).map(|(h, _)| h))
                    .unwrap_or(rest);
                    let noun = head.rsplit(' ').next().unwrap_or(head);
                    Some((rest.to_string(), format!("that {noun}")))
                } else if matches!(o.as_str(), "them" | "those creatures") {
                    Some(("of those creatures".to_string(), "it".to_string()))
                } else {
                    None
                };
                if let Some((each, that)) = each {
                    let _ = write!(what, " attached to {that}");
                    return Clause::imperative(format!("for each {each}, create {what}"));
                }
                let _ = write!(what, " attached to {o}");
            }
            match who {
                None => Clause::imperative(format!("create {what}")),
                Some(w) => {
                    let s = ctx.subj(*w);
                    let v = ctx.verb(*w, "creates", "create");
                    Clause::with_subject(*w, s, format!("{v} {what}"))
                }
            }
        }
        // "Choose two target creatures controlled by different players."
        Effect::ChooseTarget(r @ Recipient::Targets(a, 2))
            if ctx.targets.get(*a) == ctx.targets.get(a + 1) =>
        {
            Clause::imperative(format!("choose {}", ctx.obj(*r)))
        }
        Effect::ChooseTarget(Recipient::Targets(a, 2)) => {
            let first = ctx.obj(Recipient::Target(*a));
            let second = ctx.obj(Recipient::Target(a + 1));
            Clause::imperative(format!("choose {first} and {second}"))
        }
        // "choose target artifact card in your graveyard" (Daretti,
        // Rocketeer Engineer): "in", where a move says "from".
        Effect::ChooseTarget(r) => Clause::imperative(format!(
            "choose {}",
            ctx.obj(*r)
                .replacen(" from your graveyard", " in your graveyard", 1)
        )),
        Effect::FlipCoin => Clause::imperative("flip a coin".to_string()),
        Effect::Clash => Clause::imperative("clash with an opponent".to_string()),
        Effect::IfWonClash(inner) => {
            // A spell names itself again: "If you win, Lash Out deals 3
            // damage to that creature's controller."; a permanent is "this
            // creature" again ("put a +1/+1 counter on this creature").
            ctx.source_mentioned = false;
            ctx.in_won = true;
            let body = sentences(inner, ctx);
            ctx.in_won = false;
            let body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            Clause::imperative(format!("if you win, {}", body.trim_end_matches('.')))
        }
        Effect::Vote(words) => Clause::imperative(format!(
            "starting with you, each player votes for {}",
            or_list(words)
        )),
        Effect::VoteReturn(f) => Clause::imperative(format!(
            "starting with you, each player votes for {} in your graveyard. Return each card with the most votes or tied for most votes to your hand",
            card_filter_a(f, ctx.card)
        )),
        Effect::IfWonFlip(inner) => {
            // A spell names itself again: "If you win the flip, return
            // Molten Birth to its owner's hand."
            if is_spell(ctx.card) {
                ctx.source_mentioned = false;
            }
            let body = sentences(inner, ctx);
            let body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            Clause::imperative(format!(
                "if you win the flip, {}",
                body.trim_end_matches('.')
            ))
        }
        Effect::RollDie { sides, rows } => {
            // "Roll a d20." then a paragraph per row of the results table
            // (CR 706.3a): "1—9 | ...", "20 | ...", "20+ | ...".
            let mut text = format!("roll a d{sides}");
            let mut first_token: Option<String> = None;
            for (i, row) in rows.iter().enumerate() {
                let range = match row.hi {
                    Some(hi) if hi == row.lo => hi.to_string(),
                    Some(hi) => format!("{}\u{2014}{hi}", row.lo),
                    None => format!("{}+", row.lo),
                };
                // Each row names the source in full again ("This artifact
                // deals 5 damage to that creature."), and a later row
                // making the creature token the first row made prints
                // "Create two of those tokens.".
                ctx.source_mentioned = false;
                ctx.those_tokens.clone_from(&first_token);
                ctx.last_token = None;
                let body = sentences(&row.effects, ctx);
                if i == 0 {
                    first_token = ctx.last_token.clone().filter(|t| {
                        token_description(t, Num::N(1), false, false, ctx).contains('/')
                    });
                }
                let body = body.strip_suffix('.').unwrap_or(&body);
                let _ = write!(text, ".\n{range} | {body}");
            }
            ctx.those_tokens = None;
            Clause::imperative(text)
        }
        Effect::CreateTokenCopy { of, except, count } => {
            let o = ctx.obj(*of);
            // "create a token that's a copy of", "create two tokens that are
            // copies of", "Create X tokens that are copies of".
            let copies = if *count == Num::N(1) {
                "a token that's a copy of".to_string()
            } else {
                format!("{} tokens that are copies of", count_words(*count, "one"))
            };
            let noun = |rest: &str| {
                if rest.contains("creature") {
                    "creature"
                } else {
                    "permanent"
                }
            };
            // "For each nontoken creature you control, create a token
            // that's a copy of that creature" (Multiversal Incursion);
            // "For each of X target permanents, create X tokens that are
            // copies of that permanent." (Doppelgang).
            let many = ctx.is_many(*of);
            let mut text = match o.strip_prefix("each ") {
                Some(rest) => {
                    format!("for each {rest}, create {copies} that {}", noun(rest))
                }
                None if many => format!("for each of {o}, create {copies} that {}", noun(&o)),
                None => format!("create {copies} {o}"),
            };
            if *except {
                let _ = write!(text, ", except {}", except_text(ctx.card, ctx.current));
            }
            Clause::imperative(text)
        }
        // CR 707.12: "Copy it. You may cast the copy without paying its
        // mana cost."
        Effect::CastCopy {
            what,
            free,
            wording,
        } => {
            let o = ctx.obj(*what);
            let join = if *wording == crate::ast::CopyCastWording::Then {
                ", then you"
            } else {
                ". You"
            };
            let free = if *free {
                " without paying its mana cost"
            } else {
                ""
            };
            Clause::imperative(format!("copy {o}{join} may cast the copy{free}"))
        }
        Effect::CopySpells {
            what,
            times,
            except,
            ..
        } => {
            let o = match what {
                Recipient::TriggeringObject => "that spell".to_string(),
                // "copy this spell" (a creature spell's own trigger).
                Recipient::Source => {
                    ctx.source_mentioned = true;
                    "this spell".to_string()
                }
                r => ctx.obj(*r),
            };
            let mut text = format!("copy {o}");
            match times {
                Num::N(1) | Num::Count(_) => {}
                Num::N(2) => text.push_str(" twice"),
                n => {
                    let _ = write!(text, " {} times", count_words(*n, "one"));
                }
            }
            if *except {
                // "except the copy isn't legendary".
                let x = except_text(ctx.card, ctx.current)
                    .replace("it isn't", "the copy isn't")
                    .replace("it's", "the copy is");
                let _ = write!(text, ", except {x}");
            }
            Clause::imperative(text)
        }
        Effect::BecomeCopy {
            of,
            except,
            who,
            until,
        } => {
            let source = matches!(
                who,
                Recipient::Source | Recipient::SourceNamed | Recipient::SourceIt
            );
            let s = if source {
                ctx.source_subj()
            } else {
                ctx.subj(*who)
            };
            let mut o = ctx.obj(*of);
            // "Target artifact you control becomes a copy of a second
            // target artifact you control" (Shuri): the same kind of target
            // twice.
            if let (Recipient::Target(w), Recipient::Target(f)) = (who, of)
                && w < f
                && ctx.targets.get(*w) == ctx.targets.get(*f)
                && o.starts_with("target ")
            {
                o = format!("a second {o}");
            }
            let mut text = format!("becomes a copy of {o}");
            match until {
                mtg_core::card::CopyUntil::Permanent => {}
                // "Until end of turn, it becomes a copy of ..." (Fleeting
                // Reflection): the duration printed first.
                mtg_core::card::CopyUntil::EndOfTurn if ctx.eot_first => {}
                mtg_core::card::CopyUntil::EndOfTurn => text.push_str(" until end of turn"),
                mtg_core::card::CopyUntil::YourNextTurn => text.push_str(" until your next turn"),
            }
            if *except {
                let _ = write!(text, ", except {}", except_text(ctx.card, ctx.current));
            }
            Clause::with_subject(if source { Recipient::Source } else { *who }, s, text)
        }
        // Where a countered spell goes is added in `clause`, after a count.
        // "When you cast this spell, any player may pay 5 life. If a player
        // does, counter Dash Hopes."
        Effect::Counter {
            to: Recipient::Source,
            ..
        } if ctx.cast_this && ctx.any_player_said => {
            Clause::imperative(format!("counter {}", short_name(ctx.card)))
        }
        Effect::Counter { to, .. } => {
            let o = ctx.obj(*to);
            Clause::imperative(format!("counter {o}"))
        }
        Effect::ChangeTarget { what } => {
            let o = ctx.obj(*what);
            Clause::imperative(format!("change the target of {o}"))
        }
        Effect::CounterUnlessPays { to, mana, .. } => {
            ctx.they_paid = true;
            let o = ctx.obj(*to);
            Clause::imperative(format!("counter {o} unless its controller pays {mana}"))
        }
        Effect::Scry(n) => Clause::imperative(format!("scry {}", digits(*n))),
        Effect::ScryEach { who, count, may } => {
            let s = ctx.subj(*who);
            let verb = if *may {
                "may scry".to_string()
            } else {
                ctx.verb(*who, "scries", "scry").to_string()
            };
            Clause::with_subject(*who, s, format!("{verb} {}", digits(*count)))
        }
        Effect::Investigates(w) => {
            let s = ctx.subj(*w);
            Clause::with_subject(
                *w,
                s,
                ctx.verb(*w, "investigates", "investigate").to_string(),
            )
        }
        // The count's "for each ..." follows (`clause`).
        Effect::InvestigateCount { once, .. } => Clause::imperative(if *once {
            "investigate once".to_string()
        } else {
            "investigate".to_string()
        }),
        Effect::Investigate(n) => Clause::imperative(match n {
            1 => "investigate".to_string(),
            2 => "investigate twice".to_string(),
            n => format!("investigate {} times", count_words(Num::N(*n), "one")),
        }),
        Effect::PutInLibrary {
            what,
            place,
            wording,
        } => {
            use crate::ast::{LibraryPlace as P, LibraryWording as W};
            // A card from your own graveyard goes to "your library"; one
            // from an opponent's, to "their library".
            let graveyard_of = match *what {
                Recipient::Target(i) | Recipient::Targets(i, _) => {
                    match ctx.targets.get(i).map(Target::base) {
                        Some(
                            Target::GraveyardCard { who: Who::You, .. }
                            | Target::Spec(TargetSpec::CreatureCardInYourGraveyard),
                        ) => Some("your"),
                        Some(Target::GraveyardCard {
                            who: Who::Opponents,
                            ..
                        }) => Some("their"),
                        Some(Target::GraveyardCard { filter, .. })
                            if matches!(
                                filter.that_player,
                                Some(mtg_core::card::ThatPlayer::Target(_))
                            ) =>
                        {
                            Some("their")
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            let ordinal = |n: u8| match n {
                2 => "second".to_string(),
                3 => "third".to_string(),
                n => format!("{n}th"),
            };
            // A graveyard ability's own card: "put this card from your
            // graveyard on top of your library".
            if *wording == W::Put && *what == Recipient::Source && works_from_graveyard(ctx.current)
            {
                return Clause::imperative(match place {
                    P::Top => {
                        "put this card from your graveyard on top of your library".to_string()
                    }
                    P::Shuffle => {
                        "shuffle this card from your graveyard into your library".to_string()
                    }
                    _ => "put this card from your graveyard on the bottom of your library"
                        .to_string(),
                });
            }
            // "Target player shuffles up to three target cards from their
            // graveyard into their library."
            if let W::By(who) = wording {
                let s = ctx.subj(*who);
                let v = ctx.verb(*who, "shuffles", "shuffle");
                let o = ctx.obj(*what);
                let whose = graveyard_of.unwrap_or("their");
                return Clause::with_subject(*who, s, format!("{v} {o} into {whose} library"));
            }
            if *wording == W::Put {
                let o = ctx.obj(*what);
                let whose = graveyard_of.unwrap_or("its owner's");
                // "Put any number of target artifact cards from target
                // player's graveyard on top of their library in any order."
                let any_order = match *what {
                    Recipient::Target(i) => {
                        matches!(ctx.targets.get(i), Some(Target::Many { .. }))
                    }
                    _ => false,
                };
                // (From your own graveyard Oracle leaves the order out:
                // Gravepurge.)
                let order = if any_order && whose == "their" {
                    " in any order"
                } else {
                    ""
                };
                return Clause::imperative(match place {
                    P::Top => format!("put {o} on top of {whose} library{order}"),
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
        Effect::SacrificeChosen {
            who,
            count,
            filter,
            and,
            choice_first,
        } => {
            let s = ctx.subj(*who);
            let some = |count: Num, filter: &Filter| {
                if count == Num::N(1) {
                    with_article(&filter_phrase(filter, ctx.card, false))
                } else {
                    format!(
                        "{} {}",
                        count_words(count, "a"),
                        filter_phrase(filter, ctx.card, true)
                    )
                }
            };
            // "a creature of their choice other than enchanted creature".
            if *choice_first && *who != Recipient::You && filter.not_attached {
                let g = Filter {
                    not_attached: false,
                    ..filter.clone()
                };
                let what = some(*count, &g);
                return Clause::with_subject(
                    *who,
                    s,
                    format!(
                        "{} {what} of their choice other than enchanted {}",
                        ctx.verb(*who, "sacrifices", "sacrifice"),
                        enchanted_noun(ctx.card)
                    ),
                );
            }
            // "a creature of their choice with flying".
            if *choice_first && *who != Recipient::You {
                let mut g = filter.clone();
                let ks = std::mem::take(&mut g.keywords);
                let what = some(*count, &g);
                return Clause::with_subject(
                    *who,
                    s,
                    format!(
                        "{} {what} of their choice with {}",
                        ctx.verb(*who, "sacrifices", "sacrifice"),
                        keywords_list(&ks)
                    ),
                );
            }
            let mut what = some(*count, filter);
            // "an artifact and a land of their choice".
            if let Some(b) = and {
                what = format!("{what} and {}", some(b.0, &b.1));
            }
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
        Effect::CastFree {
            what,
            filter,
            max,
            exile,
        } => {
            // CR 608.2g: "You may cast a noncreature spell from your hand
            // without paying its mana cost."
            #[allow(clippy::single_match_else)] // the hand's long phrase
            let what = match what {
                // "a card exiled with Krang & Shredder" (CR 607.2a).
                Some(Recipient::ExiledWith(i)) => {
                    let f = ctx
                        .card
                        .groups
                        .get(usize::from(*i))
                        .cloned()
                        .unwrap_or_default();
                    with_article(&format!(
                        "{} exiled with {}",
                        cards_phrase(&f, ctx.card, false),
                        this_ref(ctx.card)
                    ))
                }
                Some(r) => ctx.obj(*r),
                None => {
                    let noun = match filter {
                        Some(f) => format!("{} spell", card_filter_phrase(f, ctx.card).trim()),
                        None => "spell".to_string(),
                    };
                    // "a spell with mana value 4 or less from your hand",
                    // but "a spell from your hand with mana value less
                    // than or equal to that damage".
                    match max {
                        None => format!("{} from your hand", with_article(&noun)),
                        Some(Num::ThatMuch) => format!(
                            "{} from your hand with mana value less than or equal to that damage",
                            with_article(&noun)
                        ),
                        Some(Num::Count(i)) => format!(
                            "{} from your hand with mana value less than or equal to {}",
                            with_article(&noun),
                            count_phrase_in(&ctx.card.counts[usize::from(*i)].expr, ctx, false)
                        ),
                        Some(n) => format!(
                            "{} with mana value {} or less from your hand",
                            with_article(&noun),
                            digits(*n)
                        ),
                    }
                }
            };
            let tail = if *exile {
                ". If that spell would be put into your graveyard, exile it instead"
            } else {
                ""
            };
            Clause::imperative(format!(
                "you may cast {what} without paying its mana cost{tail}"
            ))
        }
        // "It's an enchantment." / "They're an artifact." (the Enduring
        // cycle, Tom, Bert, and William).
        Effect::IsNow { types, .. } => {
            let words: Vec<String> = types.iter().map(|t| type_noun(*t).to_string()).collect();
            let they = ctx.card.pronoun == Some(crate::ast::Pronoun::They);
            Clause::imperative(format!(
                "{} {} {}",
                if they { "they're" } else { "it's" },
                article(&words[0]),
                words.join(" ")
            ))
        }
        Effect::LoseAllAbilities { to, indefinite } => {
            let s = ctx.subj(*to);
            let v = ctx.verb(*to, "loses", "lose");
            let c = Clause::with_subject(*to, s, format!("{v} all abilities"));
            if *indefinite { c } else { c.until_eot() }
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
        Effect::RevealRandom { who } => {
            let s = ctx.subj(*who);
            let (verb, their) = if *who == Recipient::You {
                ("reveal", "your")
            } else {
                (ctx.verb(*who, "reveals", "reveal"), "their")
            };
            Clause::with_subject(
                *who,
                s,
                format!("{verb} a card at random from {their} hand"),
            )
        }
        Effect::RevealHand { who } => {
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!("{} their hand", ctx.verb(*who, "reveals", "reveal")),
            )
        }
        Effect::DiscardRandom {
            count,
            who,
            another,
        } => {
            let what = if *another {
                "another card".to_string()
            } else if *count == Num::N(1) {
                "a card".to_string()
            } else {
                format!("{} cards", count_words(*count, "a"))
            };
            let Some(who) = who else {
                return Clause::imperative(format!("discard {what} at random"));
            };
            // "Skull Rend deals 2 damage to each opponent. Those players
            // each discard two cards at random."
            if *who == Recipient::EachOpponent && ctx.opponents_object {
                ctx.opponents_object = false;
                return Clause::with_subject(
                    *who,
                    "those players".to_string(),
                    format!("each discard {what} at random"),
                );
            }
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!("{} {what} at random", ctx.verb(*who, "discards", "discard")),
            )
        }
        Effect::ExileChosen {
            who,
            count,
            filter,
            graveyard,
        } => {
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
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{} {what} they control{}",
                    ctx.verb(*who, "exiles", "exile"),
                    if *graveyard {
                        " and their graveyard"
                    } else {
                        ""
                    }
                ),
            )
        }
        Effect::ExileFromHand {
            count,
            who,
            graveyard,
        } => {
            let s = ctx.subj(*who);
            let what = format!("{} {}", count_words(*count, "a"), plural(*count, "card"));
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{} {what} from their {}",
                    ctx.verb(*who, "exiles", "exile"),
                    if *graveyard { "graveyard" } else { "hand" }
                ),
            )
        }
        Effect::DrawDiscardDrawn { draw, discard } => Clause::imperative(format!(
            "draw {} {}, then discard {} of them",
            count_words(*draw, "a"),
            plural(*draw, "card"),
            count_words(*discard, "one")
        )),
        Effect::ExileFromGraveyardOrHand { who, filter } => {
            let what = if *filter == Filter::default() {
                "a card".to_string()
            } else {
                with_article(&format!("{} card", card_filter_phrase(filter, ctx.card)))
            };
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{} their hand. You choose {what} from that player's graveyard or hand and exile it",
                    ctx.verb(*who, "reveals", "reveal")
                ),
            )
        }
        Effect::DiscardChosen {
            who,
            filter,
            exile: Some(joined),
            until,
            and_choose,
            look,
            may,
            ..
        } => {
            let what = match filter {
                None => "a card".to_string(),
                Some(f) => with_article(&format!("{} card", card_filter_phrase(f, ctx.card))),
            };
            // CR 610.3: "until this creature leaves the battlefield".
            let until = if *until {
                format!(" until {} leaves the battlefield", this_ref(ctx.card))
            } else {
                String::new()
            };
            if *look {
                // "Look at target opponent's hand. You may exile a nonland
                // card from it until this creature leaves the battlefield."
                let o = ctx.obj(*who);
                let whose = match o.as_str() {
                    "you" => "your".to_string(),
                    "it" | "them" => "their".to_string(),
                    o => format!("{o}'s"),
                };
                let may = if *may { "You may exile" } else { "Exile" };
                return Clause::imperative(format!(
                    "look at {whose} hand. {may} {what} from it{until}"
                ));
            }
            let s = ctx.subj(*who);
            if *and_choose {
                return Clause::with_subject(
                    *who,
                    s,
                    format!(
                        "{} their hand and you choose {what} from it. Exile that card{until}",
                        ctx.verb(*who, "reveals", "reveal")
                    ),
                );
            }
            let exile = if *joined {
                format!(" and exile that card{until}")
            } else {
                format!(". Exile that card{until}")
            };
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{} their hand. You choose {what} from it{exile}",
                    ctx.verb(*who, "reveals", "reveal")
                ),
            )
        }
        // "Target player reveals three cards from their hand and you
        // choose one of them. That player discards that card."
        // (Blackmail); "... reveals X cards from their hand, where X is
        // .... You choose one of those cards. ..." (Thieving Sprite); "...
        // You choose two of them. That player discards those cards."
        // (Noggin Whack).
        Effect::DiscardChosen {
            who,
            reveal: Some((n, take)),
            exile: None,
            filter: None,
            may: false,
            ..
        } => {
            let s = ctx.subj(*who);
            let verb = ctx.verb(*who, "reveals", "reveal");
            let cards = if *n == Num::N(1) { "card" } else { "cards" };
            let (lead, first) = match n {
                Num::Count(i) => {
                    let phrase = ctx
                        .card
                        .counts
                        .get(usize::from(*i))
                        .map(|c| count_phrase_in(&c.expr, ctx, false))
                        .unwrap_or_default();
                    (
                        format!("{verb} X cards from their hand, where X is {phrase}"),
                        ". You choose one of those cards".to_string(),
                    )
                }
                _ => (
                    format!("{verb} {} {cards} from their hand", count_words(*n, "a")),
                    if *take == 1 {
                        " and you choose one of them".to_string()
                    } else {
                        format!(
                            ". You choose {} of them",
                            count_words(Num::N(u32::from(*take)), "one")
                        )
                    },
                ),
            };
            let then = if *take == 1 {
                "That player discards that card"
            } else {
                "That player discards those cards"
            };
            Clause::with_subject(*who, s, format!("{lead}{first}. {then}"))
        }
        Effect::DiscardChosen {
            who,
            filter,
            may,
            look,
            and_choose,
            top,
            ..
        } => {
            let what = match filter {
                None => "a card".to_string(),
                Some(f) => {
                    let p = card_filter_phrase(f, ctx.card);
                    // "a card of that color".
                    if p.trim_start().starts_with("of ") {
                        format!("a card {}", p.trim())
                    } else {
                        with_article(&format!("{p} card"))
                    }
                }
            };
            // "You choose a nonland card from it with mana value 3 or
            // less." (Inquisition of Kozilek): the qualifier after "it".
            let (what, after) = match what.split_once(" with mana value ") {
                Some((w, mv)) => match mv.strip_suffix(" card") {
                    Some(mv) => (format!("{w} card"), format!(" with mana value {mv}")),
                    None => (w.to_string(), format!(" with mana value {mv}")),
                },
                None => (what, String::new()),
            };
            let choose = match (*look, *may) {
                // "Look at target player's hand and choose a card from it."
                (true, false) => format!(" and choose {what} from it{after}."),
                (_, true) => format!(". You may choose {what} from it{after}."),
                (false, false) => format!(". You choose {what} from it{after}."),
            };
            if *look {
                let o = ctx.obj(*who);
                let whose = match o.as_str() {
                    "you" => "your".to_string(),
                    "it" | "them" => "their".to_string(),
                    o => format!("{o}'s"),
                };
                let then = if *top {
                    "Put that card on top of that player's library"
                } else {
                    "That player discards that card"
                };
                return Clause::imperative(format!("look at {whose} hand{choose} {then}"));
            }
            let s = ctx.subj(*who);
            // "Target player reveals their hand and you choose a card of
            // that color from it. That player discards that card." (Addle)
            if *and_choose {
                return Clause::with_subject(
                    *who,
                    s,
                    format!(
                        "{} their hand and you choose {what} from it{after}. That player discards that card",
                        ctx.verb(*who, "reveals", "reveal")
                    ),
                );
            }
            let then = if *may && ctx.before_if_you_dont {
                "If you do, that player discards"
            } else if s.eq_ignore_ascii_case("that player") {
                // "That player reveals their hand. You choose a nonland
                // card from it. They discard that card." (Stinging
                // Vitriol): the subject isn't named a third time.
                "They discard"
            } else {
                "That player discards"
            };
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{} their hand{choose} {then} that card",
                    ctx.verb(*who, "reveals", "reveal")
                ),
            )
        }
        Effect::IfExcessDamage(inner, to_that) => {
            // "If excess damage was dealt this way, amass Goblins X, where
            // X is that excess damage." (Bolg of the North).
            let was = ctx.in_excess;
            ctx.in_excess = true;
            let body = sentences(inner, ctx);
            ctx.in_excess = was;
            let body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            // "to that creature", "to that permanent": the damaged target's
            // noun.
            let that = if *to_that {
                format!(
                    "to that {} ",
                    ctx.target_noun.last().map_or("permanent", String::as_str)
                )
            } else {
                String::new()
            };
            Clause::imperative(format!(
                "if excess damage was dealt {that}this way, {}",
                body.trim_end_matches('.')
            ))
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
            among_those,
            this_way,
            returns,
        } => {
            use mtg_core::card::ChooseAct;
            // After another permanent is sacrificed, "it" would be that
            // one: the source is named again ("you may sacrifice an
            // artifact. When you do, this creature deals 3 damage",
            // Cornered Crook).
            if *act == ChooseAct::Sacrifice {
                ctx.source_mentioned = false;
            }
            let one = *count == Num::N(1) && !*up_to;
            let n = if *count == Num::N(crate::ast::ANY_NUMBER) {
                "any number of".to_string()
            } else if *up_to {
                format!("up to {}", count_words(*count, "one"))
            } else {
                count_words(*count, "a")
            };
            let permanents = || {
                // "sacrifice a token or a land" (West Wind Avatar).
                if one && filter.alt_article {
                    each_with_article(filter, ctx.card)
                } else if one {
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
                ChooseAct::Tap | ChooseAct::Enlist => format!("tap {}", permanents()),
                ChooseAct::Choose => format!("choose {}", permanents()),
                ChooseAct::Untap => format!("untap {}", permanents()),
                ChooseAct::Transform => format!("transform {}", permanents()),
                // "sacrifice any number of artifacts, enchantments, and/or
                // tokens" (Malevolent Witchkite).
                ChooseAct::Sacrifice => {
                    let p = permanents();
                    let parts: Vec<&str> = p.split(" and/or ").collect();
                    if parts.len() > 2 {
                        let (last, rest) = parts.split_last().expect("three or more");
                        format!("sacrifice {}, and/or {last}", rest.join(", "))
                    } else {
                        format!("sacrifice {p}")
                    }
                }
                ChooseAct::Bounce => format!(
                    "return {} to {} owner's hand",
                    permanents(),
                    if one { "its" } else { "their" }
                ),
                ChooseAct::Discard => format!("discard {}", cards()),
                ChooseAct::RevealFromHand => format!("reveal {} in your hand", cards()),
                ChooseAct::HandToBottom => format!(
                    "put {} from your hand on the bottom of your library",
                    cards()
                ),
                ChooseAct::ExileFromGraveyard => {
                    format!("exile {} from your graveyard", cards())
                }
                // CR 404.2: "exile the top creature card of your graveyard".
                ChooseAct::ExileTopOfGraveyard => {
                    let what = if one {
                        cards_phrase(filter, ctx.card, false)
                    } else {
                        format!(
                            "{} {}",
                            count_words(*count, "one"),
                            cards_phrase(filter, ctx.card, true)
                        )
                    };
                    format!("exile the top {what} of your graveyard")
                }
                // "exile a creature you control other than this creature".
                ChooseAct::Exile if filter.relation == mtg_core::card::Relation::Other => {
                    let mut rest = filter.clone();
                    rest.relation = mtg_core::card::Relation::Any;
                    let what = if one {
                        filter_a(&rest, ctx.card)
                    } else {
                        format!("{n} {}", filter_phrase(&rest, ctx.card, true))
                    };
                    format!("exile {what} other than {}", ctx.source_name())
                }
                ChooseAct::Exile => format!("exile {}", permanents()),
                ChooseAct::ExileWithThisSpell => {
                    format!("exile this spell and {} from your graveyard", cards())
                }
                ChooseAct::GraveyardToTop => {
                    format!("put {} from your graveyard on top of your library", cards())
                }
                // CR 400.3: to the owner's graveyard. "a card an opponent
                // owns from exile into that player's graveyard"; "two cards
                // your opponents own from exile into their owners'
                // graveyards".
                ChooseAct::ExileToGraveyard => {
                    let mut rest = filter.clone();
                    rest.who = None;
                    if one {
                        format!(
                            "put {} an opponent owns from exile into that player's graveyard",
                            with_article(&cards_phrase(&rest, ctx.card, false))
                        )
                    } else {
                        format!(
                            "put {n} {} your opponents own from exile into their owners' graveyards",
                            cards_phrase(&rest, ctx.card, true)
                        )
                    }
                }
                ChooseAct::ReturnFromGraveyard => {
                    format!("return {} from your graveyard to your hand", cards())
                }
                ChooseAct::AttachToTarget(i) => {
                    let host = ctx.obj(Recipient::Target(usize::from(*i)));
                    format!("attach {} to {host}", permanents())
                }
                ChooseAct::ReturnRandomFromGraveyard => {
                    format!(
                        "return {} at random from your graveyard to your hand",
                        cards()
                    )
                }
                ChooseAct::MilledToHand if *returns => {
                    format!("return {} from among them to your hand", cards())
                }
                ChooseAct::GraveyardToBattlefield { tapped } => format!(
                    "return {} from your graveyard to the battlefield{}",
                    cards(),
                    if *tapped { " tapped" } else { "" }
                ),
                ChooseAct::MilledToHand | ChooseAct::MilledToBattlefield => format!(
                    "put {} from among {} {}",
                    cards(),
                    if *among_them {
                        "them"
                    } else if *among_those {
                        "those cards"
                    } else if *this_way {
                        "the cards milled this way"
                    } else {
                        "the milled cards"
                    },
                    if *act == ChooseAct::MilledToHand {
                        "into your hand"
                    } else {
                        "onto the battlefield"
                    }
                ),
            })
        }
        Effect::OneOf(options) => {
            // "sacrifice an artifact or discard a card"; "create a Food
            // token or a Treasure token" (the verb once).
            // "Destroy all enchantments or all nonwhite enchantments";
            // with a subject, each option names it: "Lava Storm deals 2
            // damage to each attacking creature or Lava Storm deals 2
            // damage to each blocking creature".
            let mut first: Option<(Recipient, String)> = None;
            let texts: Vec<String> = options
                .iter()
                .map(|o| {
                    let c = clause(o, ctx);
                    match c.subject {
                        Some((r, s)) if !s.is_empty() => {
                            // The same subject is named again, not "it".
                            let s = match &first {
                                Some((fr, fs)) if *fr == r => fs.clone(),
                                _ => s,
                            };
                            first.get_or_insert((r, s.clone()));
                            format!("{s} {}", c.text)
                        }
                        _ => c.text,
                    }
                })
                .collect();
            let verb = texts[0].split(' ').next().unwrap_or_default().to_string();
            let same = texts
                .iter()
                .all(|t| t.split(' ').next() == Some(verb.as_str()));
            let mut out = texts[0].clone();
            for t in &texts[1..] {
                out.push_str(" or ");
                if same && (verb == "create" || verb == "destroy") {
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
                    or_list(&ks)
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
        Effect::Meld { partner, into } => Clause::imperative(format!(
            "if you both own and control {} and a creature named {partner}, exile them, then meld them into {into}",
            this_ref(ctx.card)
        )),
        Effect::Flip { what } => {
            let o = ctx.obj(*what);
            Clause::imperative(format!("flip {o}"))
        }
        Effect::Search {
            filter,
            count,
            up_to,
            to,
            reveal,
            that_card,
            or_card,
            who,
            may,
            graveyard,
            name,
            old,
            ..
        } => {
            if *graveyard || name.is_some() {
                return search_named_text(e, ctx);
            }
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
                // "instead search your library for any card" after a
                // search for a particular card.
                let other_search = matches!(ctx.current, Some(Ability::Spell { effects, .. })
                    if effects.iter().flat_map(|s| &s.effects).any(|e| matches!(e,
                        Effect::Search { filter, .. } if filter != &Filter::default())));
                if one && other_search {
                    "any card".to_string()
                } else if one {
                    "a card".to_string()
                } else {
                    format!(
                        "{}{} cards",
                        if *up_to { "up to " } else { "" },
                        count_words(*count, "a")
                    )
                }
            } else if one && filter.or.is_none() {
                // "a Rebel permanent card with mana value 3 or less".
                with_article(&cards_phrase(filter, ctx.card, false))
            } else if one {
                with_article(&format!("{} card", filter_phrase(filter, ctx.card, false)))
            } else if filter.or.is_none() && filter.mana_value.is_some() {
                // "up to two creature cards with mana value 1 or less": the
                // mana value after the noun.
                format!(
                    "{}{} {}",
                    if *up_to { "up to " } else { "" },
                    count_words(*count, "a"),
                    cards_phrase(filter, ctx.card, true)
                )
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
            // "Its controller may search their library for ...".
            let (verb, library, hand) = match who {
                Some(w) if *w != Recipient::You => (
                    if *may { "may search" } else { "searches" },
                    "their library",
                    "their hand",
                ),
                _ => (
                    if *may { "may search" } else { "search" },
                    "your library",
                    "your hand",
                ),
            };
            // The older wording: "Search your library for a Plains, ...
            // card and put that card onto the battlefield. Then shuffle."
            if *old && who.is_none() && !*may && !*reveal {
                let place = match to {
                    mtg_core::card::SearchTo::Hand => format!("into {hand}"),
                    mtg_core::card::SearchTo::Battlefield { tapped } => format!(
                        "onto the battlefield{}",
                        if *tapped { " tapped" } else { "" }
                    ),
                    _ => "(unsupported)".to_string(),
                };
                return Clause::imperative(format!(
                    "{verb} {library} for {what} and put {pron} {place}. Then shuffle"
                ));
            }
            let mut text = format!("{verb} {library} for {what}, ");
            // "Target player searches ..., puts it onto the battlefield
            // tapped, then shuffles."
            let third = !*may && who.is_some_and(|w| w != Recipient::You);
            let (put, reveal_v, shuffle) = if third {
                ("puts", "reveals", "shuffles")
            } else {
                ("put", "reveal", "shuffle")
            };
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
                let _ = write!(text, "{reveal_v} {r}, ");
            }
            match to {
                mtg_core::card::SearchTo::BattlefieldAndHand => {
                    let _ = write!(
                        text,
                        "{put} one onto the battlefield tapped and the other into {hand}, then {shuffle}",
                    );
                }
                mtg_core::card::SearchTo::Hand => {
                    let _ = write!(text, "{put} {pron} into {hand}, then {shuffle}");
                }
                mtg_core::card::SearchTo::Battlefield { tapped } => {
                    let _ = write!(
                        text,
                        "{put} {pron} onto the battlefield{}, then {shuffle}",
                        if *tapped { " tapped" } else { "" }
                    );
                }
                mtg_core::card::SearchTo::TopOfLibrary => {
                    let _ = write!(text, "then shuffle and put {it} on top");
                }
                // "put that card into your graveyard" (Vile Entomber).
                mtg_core::card::SearchTo::Graveyard => {
                    let gy = hand.replace("hand", "graveyard");
                    let _ = write!(text, "{put} {pron} into {gy}, then {shuffle}");
                }
                mtg_core::card::SearchTo::HandOrGraveyard => {
                    let gy = hand.replace("hand", "hand or graveyard");
                    let _ = write!(text, "{put} {pron} into {gy}, then {shuffle}");
                }
                // "exile them, then shuffle" (Foresight).
                mtg_core::card::SearchTo::Exile => {
                    let v = if third { "exiles" } else { "exile" };
                    let _ = write!(text, "{v} {pron}, then {shuffle}");
                }
            }
            match who {
                Some(w) if *w != Recipient::You => {
                    let s = ctx.subj(*w);
                    Clause::with_subject(*w, s, text)
                }
                _ if *may => Clause::with_subject(Recipient::You, "you".to_string(), text),
                _ => Clause::imperative(text),
            }
        }
        Effect::AddMana(m) if m == &[Mana::Chosen] => {
            Clause::imperative("add one mana of the chosen color".to_string())
        }
        Effect::AddMana(m) => {
            Clause::imperative(format!("add {}", mana_symbols(m, "one mana of any color")))
        }
        Effect::AddManaCommander { rider } => Clause::imperative(format!(
            "add one mana of any color in your commander's color identity{}",
            match rider {
                1 =>
                    ". If you spend this mana to cast your commander, it enters with a number \
                      of additional +1/+1 counters on it equal to the number of times it's been \
                      cast from the command zone this game",
                2 =>
                    ". When that mana is spent to cast a creature spell that shares a creature \
                      type with your commander, scry 1",
                _ => "",
            }
        )),
        Effect::AddManaTwoColors(n) => {
            let n = count_words(Num::N(u32::from(*n)), "one");
            Clause::imperative(format!(
                "add {n} mana of any one color and {n} mana of any other color"
            ))
        }
        Effect::AddManaLandCould {
            whose,
            basic,
            any_type,
        } => Clause::imperative(format!(
            "add one mana of any {} that a {}land {} could produce",
            if *any_type { "type" } else { "color" },
            if *basic { "basic " } else { "" },
            match whose {
                Who::Opponents => "an opponent controls",
                _ => "you control",
            }
        )),
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
        // "Add two mana of the chosen color." (Sunken Citadel)
        Effect::AddManaTimes {
            mana: Mana::Chosen,
            count: n @ Num::N(_),
        } => Clause::imperative(format!(
            "add {} mana of the chosen color",
            count_words(*n, "one")
        )),
        // "Add six {G}." (Castle Garenbrig), "Add seven {R}."
        Effect::AddManaTimes {
            mana: m @ (Mana::Colored(_) | Mana::Colorless),
            count: n @ Num::N(_),
        } => Clause::imperative(format!(
            "add {} {}",
            count_words(*n, "one"),
            mana_symbols(&[*m], "")
        )),
        // "add that much {R}" (Raphael, after "is dealt damage").
        Effect::AddManaTimes {
            mana,
            count: Num::ThatMuch,
        } => Clause::imperative(format!(
            "add that much {}",
            mana_symbols(&[*mana], "mana of any one color")
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
            let n = count_words(*count, "one");
            Clause::imperative(if *different {
                format!("add {n} mana of different colors")
            } else if colors.count() == 5 {
                format!("add {n} mana in any combination of colors")
            } else {
                let mut syms: Vec<String> = colors
                    .iter()
                    .map(|c| format!("{{{}}}", c.letter()))
                    .collect();
                // A pair prints in the color wheel's order, which wraps
                // around: "{G} and/or {W}", "{R} and/or {W}", "{G} and/or
                // {U}" (Saltcrusted Steppe).
                if let [a, b] = syms.as_slice()
                    && matches!(
                        (a.as_str(), b.as_str()),
                        ("{W}", "{G}" | "{R}") | ("{U}", "{G}")
                    )
                {
                    syms.swap(0, 1);
                }
                format!(
                    "add {n} mana in any combination of {}",
                    syms.join(" and/or ")
                )
            })
        }
        Effect::Surveil(n) => Clause::imperative(format!("surveil {}", digits(*n))),
        Effect::Learn => Clause::imperative("learn".to_string()),
        Effect::DoublePt { to, toughness, of } => {
            let pt = if *toughness {
                "power and toughness"
            } else {
                "power"
            };
            // "Until end of turn, double its power and toughness and it
            // gains trample." (World War Hulk): the duration printed first.
            let eot = if ctx.eot_first {
                ""
            } else {
                " until end of turn"
            };
            if *of {
                let o = ctx.obj(*to);
                Clause::imperative(format!("double the {pt} of {o}{eot}"))
            } else {
                let s = ctx.subj(*to);
                Clause::imperative(format!("double {} {pt}{eot}", possessive(&s)))
            }
        }
        Effect::DoubleCounters { kind, to } => {
            let what = kind.map_or("each kind of counter".to_string(), |k| {
                format!("{} counters", counter_name(k))
            });
            let o = ctx.obj(*to);
            // "on each of those creatures" (Biogenic Upgrade).
            let o = if o.starts_with("those ") {
                format!("each of {o}")
            } else {
                o
            };
            Clause::imperative(format!("double the number of {what} on {o}"))
        }
        Effect::DoubleLife { who } => {
            let s = ctx.subj(*who);
            Clause::imperative(format!("double {} life total", possessive(&s)))
        }
        Effect::Discard { count, who } => {
            ctx.discarded_one = who.is_none() && *count == Num::N(1);
            let what = format!("{} {}", count_words(*count, "a"), plural(*count, "card"));
            match who {
                None => Clause::imperative(format!("discard {what}")),
                Some(w) => {
                    let s = ctx.subj(*w);
                    let each = ctx.each_many(*w);
                    Clause::with_subject(
                        *w,
                        s,
                        format!("{each}{} {what}", ctx.verb(*w, "discards", "discard")),
                    )
                }
            }
        }
        Effect::HandToLibrary {
            count,
            who,
            shuffle: true,
            ..
        } => {
            let what = format!("{} {}", count_words(*count, "a"), plural(*count, "card"));
            let whose = if who.is_none_or(|w| w == Recipient::You) {
                "your"
            } else {
                "their"
            };
            let rest = format!("{what} from {whose} hand into {whose} library");
            match who {
                None => Clause::imperative(format!("shuffle {rest}")),
                Some(w) => {
                    let s = ctx.subj(*w);
                    Clause::with_subject(
                        *w,
                        s,
                        format!("{} {rest}", ctx.verb(*w, "shuffles", "shuffle")),
                    )
                }
            }
        }
        Effect::ShuffleZone {
            who,
            of,
            hand,
            both,
            filter,
            cards,
        } => {
            let zone = if *both {
                "hand and graveyard"
            } else if *hand {
                "hand"
            } else {
                "graveyard"
            };
            // "Shuffle all creature cards from target player's graveyard
            // into that player's library".
            let (from, into) = match (who, of) {
                (None, Some(o)) if *o != Recipient::You => {
                    let o = ctx.obj(*o);
                    (format!("{o}'s {zone}"), "that player's library".to_string())
                }
                (Some(w), _) if *w != Recipient::You => {
                    (format!("their {zone}"), "their library".to_string())
                }
                _ => (format!("your {zone}"), "your library".to_string()),
            };
            let what = match filter {
                Some(f) => format!("all {} from {from}", cards_phrase(f, ctx.card, true)),
                None if *cards => format!("the cards from {from}"),
                None => from,
            };
            let rest = format!("{what} into {into}");
            match who {
                None => Clause::imperative(format!("shuffle {rest}")),
                Some(w) => {
                    let s = ctx.subj(*w);
                    Clause::with_subject(
                        *w,
                        s,
                        format!("{} {rest}", ctx.verb(*w, "shuffles", "shuffle")),
                    )
                }
            }
        }
        Effect::HandToLibrary {
            count, who, bottom, ..
        } => {
            let what = format!("{} {}", count_words(*count, "a"), plural(*count, "card"));
            let you = who.is_none_or(|w| w == Recipient::You);
            let (hand, library) = if you {
                ("your hand", "your library")
            } else {
                ("their hand", "their library")
            };
            let place = if *bottom {
                format!("on the bottom of {library}")
            } else {
                format!("on top of {library}")
            };
            // CR 401.4: two or more put on top go in the order their
            // owner chooses.
            let order = if !*bottom && *count != Num::N(1) {
                " in any order"
            } else {
                ""
            };
            let rest = format!("{what} from {hand} {place}{order}");
            match who {
                None => Clause::imperative(format!("put {rest}")),
                Some(w) => {
                    let s = ctx.subj(*w);
                    Clause::with_subject(*w, s, format!("{} {rest}", ctx.verb(*w, "puts", "put")))
                }
            }
        }
        // "Then those creatures fight each other." (CR 701.14a).
        Effect::Fight { a, b } if a == b => {
            let s = ctx.subj(*a);
            Clause::with_subject(*a, s, "fight each other".to_string())
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
            // `target creature bites that target 1`: the newer "deals
            // damage equal to its power to itself" (Wisecrack).
            let o = match (source, to) {
                (Recipient::Target(a), Recipient::ThatTarget(b)) if a == b => "itself".to_string(),
                _ => ctx.obj(*to),
            };
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
            pt,
            base,
            colors,
            keywords,
            indefinite,
            loses_abilities,
            in_addition,
            is_word,
        } => {
            let s = ctx.subj(*to);
            // "It's a Spirit in addition to its other types", "That
            // creature is a Hero ...", "It has flying and is an Angel ...",
            // "It's a Skeleton ... and has no abilities".
            if *is_word {
                let noun = with_article(&subtypes.join(" "));
                let rest = "in addition to its other types";
                let text = if keywords.is_empty() {
                    format!("is {noun} {rest}")
                } else {
                    format!("has {} and is {noun} {rest}", keywords_list(keywords))
                };
                let text = if *loses_abilities {
                    format!("{text} and has no abilities")
                } else {
                    text
                };
                let (s, text) = match text.strip_prefix("is ") {
                    Some(r) if s == "it" => ("it's".to_string(), r.to_string()),
                    _ => (s, text),
                };
                return Clause::with_subject(*to, s, text);
            }
            // "loses all abilities and has base power and toughness 0/1".
            if let (Some((p, q)), true, true) = (pt, base, *loses_abilities)
                && types.is_empty()
                && subtypes.is_empty()
                && colors.is_empty()
                && keywords.is_empty()
                && !*in_addition
            {
                let text = format!(
                    "{} all abilities and {} base power and toughness {p}/{q}",
                    ctx.verb(*to, "loses", "lose"),
                    ctx.verb(*to, "has", "have")
                );
                let clause = Clause::with_subject(*to, s, text);
                return if *indefinite {
                    clause
                } else {
                    clause.until_eot()
                };
            }
            // "up to two target creatures each ... become blue Illusions".
            let plural = ctx.verb(*to, "becomes", "become") == "become";
            let mut words = Vec::new();
            if let (Some((p, q)), false) = (pt, base) {
                words.push(format!("{p}/{q}"));
            }
            // "a 3/4 red and green Dinosaur creature".
            let color_words: Vec<String> = colors
                .iter()
                .map(|c| format!("{c:?}").to_lowercase())
                .collect();
            if !color_words.is_empty() {
                words.push(list(&color_words));
            }
            let every = subtypes
                .iter()
                .any(|s| s == crate::ast::EVERY_CREATURE_TYPE);
            words.extend(
                subtypes
                    .iter()
                    .filter(|s| *s != crate::ast::EVERY_CREATURE_TYPE)
                    .cloned(),
            );
            words.extend(types.iter().map(|t| type_noun(*t).to_string()));
            let mut text = if plural && *in_addition {
                if let Some(last) = words.pop() {
                    words.push(pluralize(&last));
                }
                format!("become {}", words.join(" "))
            } else {
                format!(
                    "{} {} {}",
                    ctx.verb(*to, "becomes", "become"),
                    article(&words[0]),
                    words.join(" ")
                )
            };
            if *loses_abilities {
                text = format!(
                    "{} all abilities and {text}",
                    ctx.verb(*to, "loses", "lose")
                );
            }
            // "a Shapeshifter artifact creature with base power and
            // toughness 5/5", "a 2/2 Gargoyle creature with flying".
            // "a Dragon with base power and toughness 4/4, flying, and
            // haste" (Bogardan Dragonheart): one list.
            if let (Some((p, q)), true) = (pt, base)
                && !keywords.is_empty()
            {
                let mut items = vec![format!("base power and toughness {p}/{q}")];
                items.extend(
                    keywords
                        .iter()
                        .filter(|k| !is_restriction(**k))
                        .map(|k| keyword_name(*k).to_string()),
                );
                let _ = write!(text, " with {}", list(&items));
            } else {
                if let (Some((p, q)), true) = (pt, base) {
                    let _ = write!(text, " with base power and toughness {p}/{q}");
                }
                if !keywords.is_empty() {
                    let _ = write!(text, " with {}", keywords_list(keywords));
                }
            }
            // "a 3/3 creature with vigilance and all creature types".
            if every {
                text.push_str(if keywords.is_empty() {
                    " with all creature types"
                } else {
                    " and all creature types"
                });
            }
            if *in_addition {
                let their = if plural { "their" } else { "its" };
                let what = if colors.is_empty() {
                    "types"
                } else {
                    "colors and types"
                };
                let _ = write!(text, " in addition to {their} other {what}");
            }
            let mut clause = Clause::with_subject(*to, s, text);
            if !*indefinite {
                clause = clause.until_eot();
            }
            // "Target snow land becomes a 2/2 creature until end of turn.
            // It's still a land."
            let land_target = matches!(*to, Recipient::Target(i) if matches!(
                ctx.targets[i].base(),
                Target::Filter(f) if f.types == [CardType::Land]
            ) || (types.contains(&CardType::Creature)
                && matches!(ctx.targets[i].base(), Target::Spec(TargetSpec::Land))));
            if (ctx.card.types.contains(&CardType::Land) || land_target)
                && !types.contains(&CardType::Land)
            {
                // "Put seven +1/+1 counters on target noncreature land you
                // control. It becomes a 0/0 Fungus creature with haste.
                // It's still a land." (Tendril of the Mycotyrant): after a
                // pronoun, its own sentence.
                let pronoun = clause.subject.as_ref().is_some_and(|(_, t)| t == "it");
                if (*indefinite || (ctx.eot_first && keywords.is_empty()))
                    && land_target
                    && !pronoun
                    && !ctx.card.types.contains(&CardType::Land)
                {
                    // "target land you control becomes a 3/3 Elemental
                    // creature with haste that's still a land" (Brave the
                    // Wilds), "Until end of turn, target land becomes a 3/3
                    // creature that's still a land" (Lifespark Spellbomb):
                    // with no duration after it.
                    clause.text.push_str(" that's still a land");
                } else {
                    clause.tail.push_str(". It's still a land");
                }
            }
            clause
        }
        Effect::BecomeColor {
            to,
            colors,
            indefinite,
        } => {
            let s = ctx.subj(*to);
            let c: Vec<String> = colors
                .iter()
                .map(|c| format!("{c:?}").to_lowercase())
                .collect();
            let what = if c.is_empty() {
                "colorless".to_string()
            } else if c.len() == 5 {
                // Scrapbasket: "becomes all colors" (CR 105.2).
                "all colors".to_string()
            } else {
                list(&c)
            };
            let c = Clause::with_subject(
                *to,
                s,
                format!("{} {what}", ctx.verb(*to, "becomes", "become")),
            );
            if *indefinite { c } else { c.until_eot() }
        }
        Effect::OfChoice {
            to,
            kind,
            protection,
            in_addition,
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
                ChoiceKind::Named(names) => or_list(names),
                ChoiceKind::Parity => "odd or even".to_string(),
                ChoiceKind::CardName { .. } => "the card name of your choice".to_string(),
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
            let more = if *in_addition {
                " in addition to its other types"
            } else {
                ""
            };
            Clause::with_subject(*to, s, format!("{verb} {what}{more}")).until_eot()
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
        Effect::BecomeChosen { to, color } => {
            let (s, verb) = match to {
                Recipient::Group(_)
                | Recipient::EachCreature
                | Recipient::EachCreatureYouControl
                | Recipient::EachPermanentYouControl => (ctx.obj(*to), "becomes"),
                _ => (ctx.subj(*to), ctx.verb(*to, "becomes", "become")),
            };
            let what = if *color { "color" } else { "type" };
            Clause::with_subject(*to, s, format!("{verb} that {what}")).until_eot()
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
            // "When you do, General Traag deals 4 damage".
            let body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            // "Draw a card, then discard a card. When you discard a card
            // this way, ...".
            let when = if ctx.discarded_one {
                "when you discard a card this way"
            } else {
                "when you do"
            };
            Clause::imperative(format!("{when}, {}", body.trim_end_matches('.')))
        }
        // "If no one does, target player mills six cards." (after "Any
        // player may ...").
        Effect::Otherwise(inner) if ctx.any_player_did => {
            ctx.any_player_did = false;
            let body = sentences(inner, ctx);
            let body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            Clause::imperative(format!("if no one does, {}", body.trim_end_matches('.')))
        }
        Effect::Otherwise(inner) => {
            // "Otherwise, create one of those tokens.": the token the
            // sentence before created.
            let saved = ctx.those_tokens.take();
            ctx.those_tokens = ctx
                .last_token
                .clone()
                .filter(|t| token_description(t, Num::N(1), false, false, ctx).contains('/'));
            let body = sentences(inner, ctx);
            ctx.those_tokens = saved;
            let body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            Clause::imperative(format!("otherwise, {}", body.trim_end_matches('.')))
        }
        Effect::IfYouDo(inner) => {
            // "Counter target spell unless its controller pays {2}. If they
            // do, ...".
            let who = if std::mem::take(&mut ctx.lost_life_did) {
                "you lost life this way"
            } else if std::mem::take(&mut ctx.any_player_did) {
                "a player does"
            } else if std::mem::take(&mut ctx.player_did) {
                ctx.player_they = true;
                "the player does"
            } else if std::mem::take(&mut ctx.they_paid) {
                "they do"
            } else {
                "you do"
            };
            // "If you do, Heated Argument also deals 2 damage to that
            // creature's controller": a spell's second damage, to the
            // controller of the creature it damaged, names it again.
            let also = is_spell(ctx.card)
                && matches!(
                    inner.first().and_then(|s| s.effects.first()),
                    Some(Effect::DealDamage { to, by: Recipient::Source, .. })
                        if matches!(to.as_slice(), [Recipient::ThatControllerOf(_)])
                );
            if also {
                ctx.source_mentioned = false;
            }
            let body = sentences(inner, ctx);
            ctx.player_they = false;
            // "If you do, Oros deals 3 damage ...".
            let mut body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            let name = format!("{} deals ", short_name(ctx.card));
            if also && body.starts_with(&name) {
                body.insert_str(name.len() - "deals ".len(), "also ");
            }
            Clause::imperative(format!("if {who}, {}", body.trim_end_matches('.')))
        }
        Effect::Waterbend(mana) => Clause::imperative(format!("waterbend {mana}")),
        Effect::Pay { mana, life } => Clause::imperative(if *life > 0 && !mana.is_empty() {
            format!("pay {mana} and {life} life")
        } else if *life > 0 {
            format!("pay {life} life")
        } else {
            format!("pay {mana}")
        }),
        Effect::UnlessPays {
            payer,
            mana,
            life,
            discard,
            waterbend,
            discard_random,
            effect,
            ..
        } => {
            ctx.in_unless = true;
            let mut c = clause(effect, ctx);
            ctx.in_unless = false;
            // "unless that player discards a card".
            if *discard > 0 {
                let what = if *discard == 1 {
                    "a card".to_string()
                } else {
                    format!("{} cards", count_words(Num::N(*discard), "a"))
                };
                let who = match (payer, &c.subject) {
                    (Recipient::You, _) => "you discard".to_string(),
                    (p, Some((s, _))) if p == s => "they discard".to_string(),
                    (p, _) => format!("{} discards", ctx.obj(*p)),
                };
                let random = if *discard_random { " at random" } else { "" };
                let _ = write!(c.tail, " unless {who} {what}{random}");
                return c;
            }
            let amount = if *life > 0 && *mana != mtg_core::mana::ManaCost::default() {
                format!("{mana} or {life} life")
            } else if *life > 0 {
                format!("{life} life")
            } else if *waterbend {
                // Not an Oracle form outside ward ("Ward—Waterbend {4}"),
                // which prints from the keyword.
                format!("waterbend {mana}")
            } else {
                mana.to_string()
            };
            let who = match (payer, &c.subject) {
                (Recipient::You, _) => "you pay".to_string(),
                (Recipient::EachPlayer, _) => "any player pays".to_string(),
                (p, Some((s, _))) if p == s => "they pay".to_string(),
                (p, _) => {
                    let w = ctx.obj(*p);
                    format!("{w} pays")
                }
            };
            let _ = write!(c.tail, " unless {who} {amount}");
            c
        }
        // "unless they sacrifice a nonland permanent of their choice or
        // discard a card", "unless that player sacrifices a creature of
        // their choice".
        Effect::UnlessSacrifices {
            payer,
            filter,
            or_discard,
            effect,
        } => {
            let mut c = clause(effect, ctx);
            let what = filter_a(filter, ctx.card);
            let (sac, disc, who) = match (payer, &c.subject) {
                (Recipient::EachOpponent | Recipient::EachPlayer, _) => {
                    ("sacrifices", "discards", "that player")
                }
                (p, Some((s, _))) if p == s => ("sacrifice", "discard", "they"),
                _ => ("sacrifices", "discards", "that player"),
            };
            let or = if *or_discard {
                format!(" or {disc} a card")
            } else {
                String::new()
            };
            let _ = write!(c.tail, " unless {who} {sac} {what} of their choice{or}");
            c
        }
        // A ward cost's effect (no syntax of its own; the keyword prints
        // "Ward—...").
        Effect::UnlessPaysCost { alts, effect, .. } => {
            let mut c = clause(effect, ctx);
            let what = ward_alts_text(alts, ctx);
            let _ = write!(c.tail, " unless that player pays: {what}");
            c
        }
        // "sacrifice it unless you return an artifact you control to its
        // owner's hand".
        Effect::UnlessYou { act, effect } => {
            let mut c = clause(effect, ctx);
            let a = clause(act, ctx);
            let _ = write!(c.tail, " unless you {}", a.text);
            c
        }
        Effect::IfCondition(c, inner, crate::ast::IfWording::After) => {
            let cond = static_cond_text(c, ctx.card);
            let body = sentences(inner, ctx);
            let body = body.trim_end_matches('.');
            // "copy it if you gained life this turn. You may choose new
            // targets for the copy."
            if let Some(i) = body.find(". You may choose new targets for the cop") {
                let (head, rest) = body.split_at(i);
                return Clause::imperative(format!("{head} if {cond}{rest}"));
            }
            Clause::imperative(format!("{body} if {cond}"))
        }
        // "It also gets +1/+0 until end of turn if you control a Mouse."
        Effect::IfCondition(c, inner, crate::ast::IfWording::AlsoAfter) => {
            let cond = static_cond_text(c, ctx.card);
            let mut body = sentences(inner, ctx).trim_end_matches('.').to_string();
            let that = body
                .strip_prefix("That ")
                .and_then(|r| r.split_once(' '))
                .map(|(noun, _)| format!("That {noun} "))
                .unwrap_or_default();
            let at = ["It ", "They ", that.as_str()]
                .into_iter()
                .find(|s| !s.is_empty() && body.starts_with(s))
                .map_or(0, str::len);
            body.insert_str(at, "also ");
            Clause::imperative(format!("{} if {cond}", lowercase_first(&body)))
        }
        Effect::IfCondition(c, inner, crate::ast::IfWording::Unless) => {
            let mut cond = static_cond_text(c, ctx.card);
            // "If you do, discard a card unless this creature entered this
            // turn." (Moon-Circuit Hacker): the source named in full.
            if cond == "it entered this turn" {
                cond = format!("{} entered this turn", this_ref(ctx.card));
            }
            let body = sentences(inner, ctx);
            Clause::imperative(format!(
                "{} unless {cond}",
                lowercase_first(body.trim_end_matches('.'))
            ))
        }
        Effect::IfCondition(c, inner, also) => {
            let also = *also == crate::ast::IfWording::Also;
            // "If the gift was promised, Blooming Blast also deals ...": a
            // gift names the spell again (CR 702.174).
            let kicked = *c == StaticCond::Kicked && !has_gift(ctx.card);
            // "If this spell was kicked, those creatures also gain trample".
            let cond = if kicked {
                format!("this {} was kicked", kicked_noun(ctx.card))
            } else if matches!(c, StaticCond::Suspected(_))
                && !is_spell(ctx.card)
                && !ctx.source_mentioned
            {
                // "If this creature is suspected, put a +1/+1 counter on
                // it.": the source named in the condition, "it" after.
                ctx.source_mentioned = true;
                static_cond_text(c, ctx.card).replacen(
                    "it's",
                    &format!("this {} is", self_noun(ctx.card)),
                    1,
                )
            } else if matches!(c, StaticCond::Is(_) | StaticCond::Isnt(_))
                && !is_spell(ctx.card)
                && !ctx.source_mentioned
                && static_cond_text(c, ctx.card).starts_with("it")
            {
                // "If Kellan is a Scout, it becomes ...", "If this
                // enchantment isn't a creature, it becomes ..." (Answered
                // Prayers): the source named in the condition, "it" after.
                ctx.source_mentioned = true;
                let t = static_cond_text(c, ctx.card);
                let who = this_ref(ctx.card);
                match t.strip_prefix("it's ") {
                    Some(rest) => format!("{who} is {rest}"),
                    None => format!("{who}{}", &t["it".len()..]),
                }
            } else if matches!(c, StaticCond::SourceCounters { at_least: true, n, .. } if *n > 1)
                && ctx.source_mentioned
                && !is_spell(ctx.card)
            {
                // "put a quest counter on this enchantment. Then if it has
                // four or more quest counters on it" (Waterbender
                // Ascension).
                static_cond_text(c, ctx.card).replacen(&this_ref(ctx.card), "it", 1)
            } else if matches!(c, StaticCond::AttachedIs(_))
                && matches!(
                    ctx.trigger,
                    Some(Trigger::Attacks(TriggerObj::Filter(f))) if f.enchanted
                )
            {
                // "Whenever enchanted creature attacks, put a +1/+1
                // counter on it. Then if it has three or more +1/+1
                // counters on it" (Ordeal of Nylea): the attacker,
                // named by the trigger.
                let t = static_cond_text(c, ctx.card);
                let noun = format!("enchanted {} ", enchanted_noun(ctx.card));
                match t.strip_prefix(&noun) {
                    Some(rest) => match rest.strip_prefix("is ") {
                        Some(r) => format!("it's {r}"),
                        None => format!("it {rest}"),
                    },
                    None => t,
                }
            } else if ctx.card.pronoun.is_some()
                && ctx.source_mentioned
                && !is_spell(ctx.card)
                && matches!(
                    c,
                    StaticCond::Is(_)
                        | StaticCond::Attacking
                        | StaticCond::AttackingAlone
                        | StaticCond::Blocked
                        | StaticCond::Modified
                )
            {
                // "put a +1/+1 counter on him. If he's attacking, untap
                // him" (The Incredible Hulk).
                static_cond_text(c, ctx.card).replacen("it's", its_is(ctx.card), 1)
            } else {
                static_cond_text(c, ctx.card)
            };
            // "Adamant — If at least three red mana was spent to cast this
            // spell, Searing Barrage deals 3 damage ...": a spell names
            // itself again after a condition ("If you control an
            // Equipment, Misthios's Fury also deals ..."); not after
            // teamwork ("it also deals 2 damage").
            if is_spell(ctx.card) && *c != StaticCond::Teamwork && !kicked {
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
            if also {
                let this = format!("{} ", this_ref(ctx.card));
                // "Misthios's Fury also deals 2 damage".
                let name = format!("{} ", ctx.card.name);
                let short = format!("{} ", short_name(ctx.card));
                // "that creature also gains trample".
                let that = body
                    .strip_prefix("that ")
                    .and_then(|r| r.split_once(' '))
                    .map(|(noun, _)| format!("that {noun} "))
                    .unwrap_or_default();
                let subject = [
                    "it ",
                    "they ",
                    "those creatures ",
                    this.as_str(),
                    name.as_str(),
                    short.as_str(),
                    that.as_str(),
                ]
                .into_iter()
                .find(|s| !s.is_empty() && body.starts_with(s))
                .map_or(0, str::len);
                body.insert_str(subject, "also ");
            }
            Clause::imperative(format!("if {cond}, {}", body.trim_end_matches('.')))
        }
        Effect::IfTarget(n, f, inner, check) => {
            use crate::ast::TargetTense;
            let noun = ctx.target_noun[*n].clone();
            if !matches!(
                check.tense,
                TargetTense::Was | TargetTense::Had | TargetTense::ThatWas
            ) && !check.after
            {
                ctx.target_mentioned[*n] = true;
                // "If that creature is an enchantment, Starfall deals 3
                // damage ...": a spell names itself again.
                if is_spell(ctx.card) {
                    ctx.source_mentioned = false;
                }
            }
            let it_before = ctx.it_target;
            if check.tense == TargetTense::Is && !check.after {
                ctx.it_target = Some(*n);
            }
            let body = sentences(inner, ctx);
            ctx.it_target = it_before;
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
                // "If that creature is a Bird, Frog, Otter, or Rat" (Splash
                // Portal): three or more creature types.
                Some(alt)
                    if alt.or.is_some() && {
                        let mut ok = true;
                        let mut g = Some(f);
                        while let Some(h) = g {
                            ok &= h.subtypes.len() == 1
                                && h.types.iter().all(|t| *t == CardType::Creature)
                                && (Filter {
                                    subtypes: Vec::new(),
                                    types: Vec::new(),
                                    or: None,
                                    ..h.clone()
                                }) == Filter::default();
                            g = h.or.as_deref();
                        }
                        ok
                    } =>
                {
                    let mut names = Vec::new();
                    let mut g = Some(f);
                    while let Some(h) = g {
                        names.push(h.subtypes[0].clone());
                        g = h.or.as_deref();
                    }
                    let last = names.pop().expect("three");
                    format!("{}, or {last}", with_article(&names.join(", ")))
                }
                _ => target_predicate(f, ctx.card),
            };
            // "If it's a Zombie card": a card in a graveyard.
            let graveyard_card = matches!(
                ctx.targets.get(*n).map(Target::base),
                Some(
                    Target::GraveyardCard { .. }
                        | Target::Spec(TargetSpec::CreatureCardInYourGraveyard)
                )
            );
            if graveyard_card && noun == "card" && f.types.is_empty() && !f.subtypes.is_empty() {
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
            // "If that creature's power is 0 or less" (Depressurize).
            let power_only = f.power.filter(|_| {
                (Filter {
                    power: None,
                    types: Vec::new(),
                    ..f.clone()
                }) == Filter::default()
                    && f.types.iter().all(|t| *t == CardType::Creature)
            });
            // "If its mana value was 3 or less", "If it had mana value 3
            // or less" (CR 608.2h: as it last existed).
            let mana_value_only = f.mana_value.filter(|_| {
                (Filter {
                    mana_value: None,
                    ..f.clone()
                }) == Filter::default()
            });
            // Converge: "if its mana value is less than or equal to the
            // number of colors of mana spent to cast this spell".
            let spent_only = f.status.spent_colors
                && (Filter {
                    mana_value: None,
                    power: None,
                    types: Vec::new(),
                    status: mtg_core::card::StatusQuals::default(),
                    ..f.clone()
                }) == Filter::default();
            // "if its toughness is less than or equal to the number of
            // cards in your graveyard" (Ghastly Demise).
            let bound_only = f.status.bound.filter(|_| {
                (Filter {
                    status: mtg_core::card::StatusQuals::default(),
                    ..f.clone()
                }) == Filter::default()
            });
            let cond = match (check.tense, has) {
                // "If the creature you control entered this turn" (Malamet
                // Battle Glyph).
                (TargetTense::TheFullIs, _) => {
                    let who = ctx.obj(Recipient::TheTargetFull(*n, false));
                    let entered_only = (Filter {
                        status: mtg_core::card::StatusQuals::default(),
                        types: Vec::new(),
                        ..f.clone()
                    }) == Filter::default()
                        && f.status
                            == mtg_core::card::StatusQuals {
                                entered_this_turn: true,
                                ..mtg_core::card::StatusQuals::default()
                            };
                    if entered_only {
                        format!("{who} entered this turn")
                    } else {
                        format!("{who} is {pred}")
                    }
                }
                (TargetTense::Is, _) if let Some(b) = bound_only => {
                    format!("its {}", bound_words(b, ctx.card, " is"))
                }
                _ if spent_only => format!(
                    "its {} is {}",
                    if f.power.is_some() {
                        "power"
                    } else {
                        "mana value"
                    },
                    spent_at_most(ctx.card)
                ),
                _ if damaged_only => "it was dealt damage this turn".to_string(),
                (TargetTense::Was, _) if let Some((cmp, n)) = mana_value_only => {
                    format!("its mana value was {n}{}", cmp_words(cmp))
                }
                (TargetTense::Had, _) if let Some((cmp, n)) = mana_value_only => {
                    format!("it had mana value {n}{}", cmp_words(cmp))
                }
                // "Counter target spell if its mana value is 2 or less."
                // (Prohibit).
                (TargetTense::Is, _) if let Some((cmp, n)) = mana_value_only => {
                    format!("its mana value is {n}{}", cmp_words(cmp))
                }
                // "If that spell's mana value was 2 or less" (Sound the
                // Trumpets).
                // "If that permanent's mana value was 3 or less" after it
                // was exiled (Vindictive Triumph): as it last existed.
                (TargetTense::ThatWas, _) if let Some((cmp, mv)) = mana_value_only => {
                    let was = target_noun(&ctx.targets[*n]);
                    format!("that {was}'s mana value was {mv}{}", cmp_words(cmp))
                }
                // "If that creature had power 2 or less" (Driftgloom
                // Coyote).
                (TargetTense::Had, _) if let Some((cmp, p)) = power_only => {
                    // As it last existed: the noun it was targeted by,
                    // not "card" after it was exiled.
                    let was = target_noun(&ctx.targets[*n]);
                    format!("that {was} had power {p}{}", cmp_words(cmp))
                }
                (TargetTense::Had, _) => format!("that {noun} had {pred}"),
                (TargetTense::Is, _) if let Some((cmp, n)) = power_only => {
                    format!("its power is {n}{}", cmp_words(cmp))
                }
                (TargetTense::ThatIs, _) if let Some((cmp, n)) = power_only => {
                    format!("that {noun}'s power is {n}{}", cmp_words(cmp))
                }
                (TargetTense::Was, _) if *f == Filter::controlled_by_you() => {
                    format!("you controlled that {noun}")
                }
                // "If you controlled it, investigate." (Hotshot
                // Investigators).
                (TargetTense::Is, _) if *f == Filter::controlled_by_you() => {
                    "you controlled it".to_string()
                }
                // "If an opponent protects it, remove a defense counter
                // from it." (Portent Tracker).
                (TargetTense::Is, _)
                    if let Some(who) = f.status.protector
                        && *f == Filter::protected_by(who) =>
                {
                    if who == Who::You {
                        "you protect it".to_string()
                    } else {
                        "an opponent protects it".to_string()
                    }
                }
                (TargetTense::ThatWas, _) if *f == Filter::controlled_by_you() => {
                    "you controlled that permanent".to_string()
                }
                // "If it was a creature card" (Carrion Locust): an exiled
                // graveyard card, as it last existed.
                (TargetTense::Was, None) if graveyard_card => {
                    format!("it was {}", card_filter_a(f, ctx.card))
                }
                // "If it was attacking" (Desculpting Blast), "If it was
                // tapped" (Brackish Blunder): a state as it last existed.
                (TargetTense::Was, None) if let Some(state) = status_only(f) => {
                    format!("it was {state}")
                }
                // "If it was a token, draw a card." (Sunpearl Kirin).
                (TargetTense::Was, None)
                    if *f
                        == (Filter {
                            status: mtg_core::card::StatusQuals {
                                token: Some(true),
                                ..Default::default()
                            },
                            ..Filter::default()
                        }) =>
                {
                    "it was a token".to_string()
                }
                // "If it was a legendary spell, the Ring tempts you."
                // (Glorious Gale): a countered spell, as it last existed.
                (TargetTense::Was, None)
                    if noun == "spell" && !pred.starts_with("a ") && !pred.starts_with("an ") =>
                {
                    format!("it was {}", with_article(&format!("{pred} spell")))
                }
                (TargetTense::Was | TargetTense::ThatWas, _) => format!("that {noun} was {pred}"),
                // "If an opponent controls that creature".
                (TargetTense::ThatIs, _)
                    if *f
                        == (Filter {
                            who: Some(Who::Opponents),
                            ..Filter::default()
                        }) =>
                {
                    format!("an opponent controls that {noun}")
                }
                // "If you control that creature, draw two cards." (Dreamdew
                // Entrancer).
                (TargetTense::ThatIs, _) if *f == Filter::controlled_by_you() => {
                    format!("you control that {noun}")
                }
                (TargetTense::Is, Some(k)) => format!("it has {k}"),
                (TargetTense::ThatIs, Some(k)) => format!("that {noun} has {k}"),
                // "If it isn't a creature", "If it's not a creature".
                (TargetTense::Is, None) if let Some(w) = check.not => {
                    let types: Vec<String> = f
                        .not_types
                        .iter()
                        .map(|t| with_article(type_noun(*t)))
                        .collect();
                    let verb = match w {
                        crate::ast::NotWording::Isnt => "it isn't",
                        crate::ast::NotWording::IsNot => "it's not",
                    };
                    format!("{verb} {}", or_list(&types))
                }
                // "destroy target creature if it's tapped" (Deathbringer
                // Liege): a state as it resolves.
                (TargetTense::Is, None) if let Some(state) = status_only(f) => {
                    format!("it's {state}")
                }
                (TargetTense::Is, None) => format!("it's {pred}"),
                (TargetTense::ThatIs, None) => format!("that {noun} is {pred}"),
            };
            if check.after {
                ctx.target_mentioned[*n] = true;
                let body = body.trim_end_matches('.');
                return Clause::imperative(format!("{body} if {cond}"));
            }
            Clause::imperative(format!("if {cond}, {}", body.trim_end_matches('.')))
        }
        Effect::Instead {
            cond,
            wording,
            label,
            effects,
        } => {
            use crate::ast::{InsteadCond, InsteadWording};
            // Teamwork reads like kicker: "it deals 4 damage to that
            // creature instead".
            let kicked = match cond {
                InsteadCond::Kicked => !has_gift(ctx.card),
                InsteadCond::Static(
                    StaticCond::Teamwork
                    | StaticCond::AdditionalCostPaid
                    | StaticCond::EvidenceCollected,
                ) => true,
                _ => false,
            };
            let cond = match cond {
                InsteadCond::Kicked => kicked_text(ctx.card),
                InsteadCond::Static(c) => static_cond_text(c, ctx.card),
                InsteadCond::ThisWay(c) => this_way_text(c, ctx.card),
                InsteadCond::Target { n, that, filter } => {
                    ctx.target_mentioned[*n] = true;
                    // "If that creature is another Hero".
                    let what = if filter.relation == Relation::Other {
                        let mut g = filter.clone();
                        crate::format::relation_any(&mut g);
                        let p = target_predicate(&g, ctx.card);
                        let bare = p
                            .strip_prefix("a ")
                            .or_else(|| p.strip_prefix("an "))
                            .unwrap_or(&p);
                        format!("another {bare}")
                    } else if ctx.target_noun[*n] == "permanent or player"
                        && filter.types == [CardType::Creature]
                    {
                        // "If it's a green creature" (Elvish Healer): any
                        // target, so the type is named.
                        filter_a(filter, ctx.card)
                    } else {
                        target_predicate(filter, ctx.card)
                    };
                    // "If that creature has toxic" (Hexgold Slash).
                    match (what.strip_prefix("a permanent with "), *that) {
                        (Some(k), true) => format!("that {} has {k}", ctx.target_noun[*n]),
                        (Some(k), false) => format!("it has {k}"),
                        (None, true) => format!("that {} is {what}", ctx.target_noun[*n]),
                        (None, false) => format!("it's {what}"),
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
            let mut body = body.trim_end_matches('.');
            // "Prevent the next 1 damage that would be dealt to any target
            // this turn. If it's a green creature, prevent the next 2
            // damage instead." (Elvish Healer): the recipient is the
            // sentence before's.
            if matches!(wording, InsteadWording::Trail)
                && let Some(head) = body.strip_suffix(" that would be dealt to it this turn")
                && head.starts_with("prevent the next ")
            {
                body = head;
            }
            let text = match wording {
                InsteadWording::Lead => format!("if {cond}, instead {body}"),
                InsteadWording::Trail => format!("if {cond}, {body} instead"),
                InsteadWording::After => format!("{body} instead if {cond}"),
                InsteadWording::AfterBare => format!(
                    "{} instead if {cond}",
                    body.strip_suffix(" until end of turn").unwrap_or(body)
                ),
            };
            Clause::imperative(match label {
                Some(l) => format!("{l} \u{2014} {}", capitalize(&text)),
                None => text,
            })
        }
        Effect::EachPutsFromGraveyard { who, filter } => {
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{} {} card from their graveyard onto the battlefield",
                    ctx.verb(*who, "puts", "put"),
                    filter_a(filter, ctx.card)
                ),
            )
        }
        Effect::ForEachCouldnt { opponent, inner } => {
            let body = sentences(inner, ctx);
            Clause::imperative(format!(
                "for each {} who can't, {}",
                if *opponent { "opponent" } else { "player" },
                lowercase_first(body.trim_end_matches('.'))
            ))
        }
        Effect::SpendRider {
            inner,
            spells,
            then,
        } => {
            let body = sentences(inner, ctx);
            let rider = sentences(then, ctx);
            Clause::imperative(format!(
                "{}. When you spend this mana to cast {}, {}",
                lowercase_first(body.trim_end_matches('.')),
                a_spell(spells, ctx.card),
                lowercase_first(rider.trim_end_matches('.'))
            ))
        }
        Effect::SpendOnly { inner, restriction } => {
            let body = sentences(inner, ctx);
            let rule = spend_rule(ctx.card, restriction);
            Clause::imperative(format!(
                "{}. {}",
                lowercase_first(body.trim_end_matches('.')),
                rule.trim_end_matches('.')
            ))
        }
        Effect::KeepMana(inner) => {
            let body = sentences(inner, ctx);
            Clause::imperative(format!(
                "{}. Until end of turn, you don't lose this mana as steps and phases end",
                lowercase_first(body.trim_end_matches('.'))
            ))
        }
        Effect::IfKicked(inner) => {
            let body = sentences(inner, ctx);
            let body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            // "If this spell was kicked, they get +1/+1 until end of turn."
            let body = match body.strip_prefix("those creatures ") {
                Some(rest) => format!("they {rest}"),
                None => body,
            };
            Clause::imperative(format!(
                "if {}, {}",
                kicked_text(ctx.card),
                body.trim_end_matches('.')
            ))
        }
        Effect::SetBasePtVar { to, n } => {
            let s = ctx.subj(*to);
            let each = if matches!(to, Recipient::Targets(..)) {
                "each "
            } else {
                ""
            };
            let v = digits(*n);
            Clause::with_subject(
                *to,
                s,
                format!(
                    "{each}{} base power and toughness {v}/{v}",
                    ctx.verb(*to, "has", "have")
                ),
            )
            .until_eot()
        }
        Effect::SetBasePower { to, n, becomes } => {
            let s = ctx.subj(*to);
            if *becomes {
                let v = match n {
                    Num::Count(i) => ctx
                        .card
                        .counts
                        .get(usize::from(*i))
                        .map_or_else(|| digits(*n), |c| count_phrase(&c.expr, ctx.card, false)),
                    n => digits(*n),
                };
                return Clause::with_subject(
                    *to,
                    format!("{} base power", possessive(&s)),
                    format!("becomes equal to {v}"),
                )
                .until_eot();
            }
            Clause::with_subject(
                *to,
                s,
                format!("{} base power {}", ctx.verb(*to, "has", "have"), digits(*n)),
            )
            .until_eot()
        }
        Effect::SetBasePt {
            to,
            power,
            toughness,
        } => {
            let s = ctx.subj(*to);
            // "up to two target creatures each have base power and
            // toughness 3/3" (Phantasmal Form).
            let each = if matches!(to, Recipient::Targets(..)) {
                "each "
            } else {
                ""
            };
            Clause::with_subject(
                *to,
                s,
                format!(
                    "{each}{} base power and toughness {power}/{toughness}",
                    ctx.verb(*to, "has", "have")
                ),
            )
            .until_eot()
        }
        // "This creature gets +2/+2 until end of turn unless a player has
        // this creature deal 4 damage to them."
        Effect::UnlessAnyPlayer { act, effect } => {
            let mut c = clause(effect, ctx);
            ctx.source_mentioned = false;
            let a = clause(act, ctx);
            let text = a.text.trim_end_matches('.');
            let body = match (&a.subject, text.strip_prefix("deals ")) {
                (Some((_, who)), Some(dmg)) if dmg.ends_with(" damage to you") => format!(
                    "has {who} deal {} damage to them",
                    dmg.trim_end_matches(" damage to you")
                ),
                _ => text.to_string(),
            };
            let _ = write!(c.tail, " unless a player {body}");
            c
        }
        Effect::AnyPlayerMay { opponents, inner } => {
            let body = sentences(inner, ctx);
            let body = if starts_with_name(&body, ctx.card) {
                body
            } else {
                lowercase_first(&body)
            };
            let body = body.trim_end_matches('.');
            let body = body.strip_prefix("you ").unwrap_or(body);
            // The player acts as themselves: "exile a card from their
            // graveyard", "sacrifice a creature of their choice", "have it
            // deal 5 damage to them".
            let body = body.replace("your graveyard", "their graveyard");
            let body = if body.starts_with("sacrifice ") {
                format!("{body} of their choice")
            } else if let Some((who, dmg)) = body.split_once(" deals ")
                && let Some(n) = dmg.strip_suffix(" damage to you")
            {
                format!("have {who} deal {n} damage to them")
            } else {
                body
            };
            ctx.any_player_did = true;
            ctx.any_player_said = true;
            // "If a player does, sacrifice this creature.": the source is
            // named again after "have it deal 5 damage to them".
            ctx.source_mentioned = false;
            let who = if *opponents {
                "any opponent"
            } else {
                "any player"
            };
            Clause::imperative(format!("{who} may {body}"))
        }
        Effect::PlayerMay { who, inner } => {
            let you = *who == Recipient::You;
            // "If the player does, they may copy this spell and may choose
            // a new target for that copy." (Chain Lightning): the player
            // of the "may" before.
            let again = ctx.player_they;
            let who = if again {
                "they".to_string()
            } else {
                ctx.obj(*who)
            };
            if let [one] = inner.as_slice()
                && let [
                    Effect::CopySpells {
                        what: Recipient::Source,
                        times: Num::N(1),
                        new_targets: true,
                        except: false,
                        ..
                    },
                ] = one.effects.as_slice()
            {
                let target = if ctx.targets.len() == 1 {
                    "a new target"
                } else {
                    "new targets"
                };
                return Clause::imperative(format!(
                    "{who} may copy this spell and may choose {target} for that copy"
                ));
            }
            let body = lowercase_first(&sentences(inner, ctx));
            let body = body.strip_prefix("you ").unwrap_or(&body);
            // The player acts on their own cards ("Each player may discard
            // their hand").
            let body = if you {
                body.to_string()
            } else {
                body.replace("your hand", "their hand")
                    .replace("your library", "their library")
                    .replace("your graveyard", "their graveyard")
            };
            ctx.player_did = true;
            Clause::imperative(format!("{who} may {}", body.trim_end_matches('.')))
        }
        Effect::YouMay(inner) => {
            if let [one] = inner.as_slice()
                && let [
                    Effect::BecomeCopy {
                        of,
                        except,
                        who: Recipient::Source | Recipient::SourceNamed | Recipient::SourceIt,
                        until: mtg_core::card::CopyUntil::Permanent,
                    },
                ] = one.effects.as_slice()
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
            // "You may discard a card. When you do, ...".
            ctx.discarded_one = false;
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
                    ("fights", "fight"),
                    ("has", "have"),
                ];
                let mut text = body.trim_end_matches('.').to_string();
                // "you may have this creature's base power and toughness
                // become 4/2 until end of turn".
                if let Some((who, rest)) = text.split_once(" has base power and toughness ") {
                    return Clause::imperative(format!(
                        "you may have {who}'s base power and toughness become {rest}"
                    ));
                }
                // "You may have it become no longer suspected." (Deadly
                // Complication).
                if let Some(who) = text.strip_suffix(" is no longer suspected") {
                    return Clause::imperative(format!(
                        "you may have {who} become no longer suspected"
                    ));
                }
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
            it,
            ..
        } => {
            // "When target creature dies this turn, ...": a target first
            // named by the delayed trigger itself.
            let named = match about {
                Recipient::Target(i) if *this_turn && !ctx.target_mentioned[*i] => {
                    let what = ctx.obj(*about);
                    let event = crate::format::this_turn_trigger(trigger)
                        .strip_prefix("when that creature ")
                        .unwrap_or("dies this turn");
                    Some(format!("when {what} {event}"))
                }
                _ => None,
            };
            // "Target player draws a card at the beginning of the next
            // turn's upkeep": a target player first named by the delayed
            // trigger's own effect (`that player` in it).
            let player_first = match about {
                Recipient::Target(i)
                    if !ctx.target_mentioned[*i] && ctx.target_noun[*i] == "player" =>
                {
                    Some(ctx.obj(*about))
                }
                _ => None,
            };
            let was = (ctx.in_delayed, ctx.delayed_that);
            ctx.in_delayed = true;
            ctx.delayed_that = *about == Recipient::TriggeringObject;
            let body = sentences(effects, ctx);
            (ctx.in_delayed, ctx.delayed_that) = was;
            let body = match player_first {
                Some(who) if body.starts_with("That player ") => {
                    format!("{}{}", capitalize(&who), &body["That player".len()..])
                }
                _ => body,
            };
            let body = body.trim_end_matches('.').to_string();
            let t = if let Some(named) = named {
                named
            } else if *it {
                crate::format::this_turn_trigger(trigger).replacen("that creature", "it", 1)
            } else if *this_turn && !crate::format::about_that_creature(trigger) {
                // "Until end of turn, whenever ...": the duration is
                // printed first instead.
                let t = trigger_text(trigger, ctx, true);
                // About a target: the trigger's own object is "that
                // creature" ("Whenever a creature dealt damage by that
                // creature dies this turn", Touch of Moonglove).
                let t = if matches!(about, Recipient::Target(_)) {
                    t.replace(&this_ref(ctx.card), "that creature")
                        .replace(" that creature this turn dies", " that creature dies")
                } else {
                    t
                };
                if ctx.eot_first {
                    t
                } else {
                    format!("{t} this turn")
                }
            } else if *this_turn {
                crate::format::this_turn_trigger(trigger).to_string()
            } else {
                trigger_text(trigger, ctx, true)
            };
            let tail = ctx
                .delayed_tail
                .take()
                .map_or_else(String::new, |s| format!(". {s}"));
            if *trigger_first {
                Clause::imperative(format!("{t}, {}{tail}", lowercase_first(&body)))
            } else {
                Clause::imperative(format!("{} {t}{tail}", lowercase_first(&body)))
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
        Effect::Rule(RuleKind::GraveyardUntargetable) => Clause::imperative(
            "cards in graveyards can't be the targets of spells or abilities this turn".to_string(),
        ),
        Effect::DamageUnpreventable => {
            Clause::imperative("the damage can't be prevented".to_string())
        }
        Effect::ExcessToController => Clause::imperative(
            "excess damage is dealt to that creature's controller instead".to_string(),
        ),
        Effect::PlayerRule { who, kind } => {
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!(
                    "can't {} this turn",
                    crate::format::player_rule_words(*kind)
                ),
            )
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
        (Some(f), false) => each_with_article(f, card),
    };
    match &c.wording {
        // "If a permanent you controlled or a token was destroyed this
        // way": control as it was, in the past tense.
        ThisWayWording::Was => format!(
            "{} was {verb} this way",
            what.replace(" you controlled", " you control")
                .replace(" you control", " you controlled")
        ),
        ThisWayWording::Is => format!("{what} is {verb} this way"),
        // "If at least one creature card was exiled this way".
        ThisWayWording::AtLeastOne => {
            let what = match (&c.filter, c.card) {
                (None, _) => "card".to_string(),
                (Some(f), true) => cards_phrase(f, card, false),
                (Some(f), false) => filter_phrase(f, card, false),
            };
            format!("at least one {what} was {verb} this way")
        }
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
                // "If the sacrificed creature was suspected" (Agency
                // Coroner), "was tapped": a state alone.
                (Some(f), false) if status_only(f).is_some() => {
                    status_only(f).expect("checked").to_string()
                }
                (Some(f), false)
                    if f.status.suspected == Some(true)
                        && (Filter {
                            status: mtg_core::card::StatusQuals::default(),
                            ..f.clone()
                        }) == Filter::default()
                        && (mtg_core::card::StatusQuals {
                            suspected: None,
                            ..f.status
                        }) == mtg_core::card::StatusQuals::default() =>
                {
                    "suspected".to_string()
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

/// A target check that names only a state ("attacking", "tapped",
/// "untapped", "blocking"): the adjective, or `None`.
fn status_only(f: &Filter) -> Option<&'static str> {
    let mut bare = Filter {
        tapped: None,
        attacking: false,
        blocking: false,
        ..f.clone()
    };
    // "If it's suspected, exile it." (Agrus Kos).
    let suspected = bare.status.suspected == Some(true);
    bare.status.suspected = None;
    if bare != Filter::default() {
        return None;
    }
    if suspected {
        return (f.tapped.is_none() && !f.attacking && !f.blocking).then_some("suspected");
    }
    match (f.tapped, f.attacking, f.blocking) {
        (Some(true), false, false) => Some("tapped"),
        (Some(false), false, false) => Some("untapped"),
        (None, true, false) => Some("attacking"),
        (None, false, true) => Some("blocking"),
        _ => None,
    }
}

fn target_predicate(f: &Filter, card: &Card) -> String {
    // "If that creature is attacking" (Sweep Away).
    if f.types.iter().all(|t| *t == CardType::Creature)
        && (f.attacking || f.blocking)
        && (Filter {
            types: Vec::new(),
            attacking: false,
            blocking: false,
            ..f.clone()
        }) == Filter::default()
    {
        return match (f.attacking, f.blocking) {
            (true, true) => "attacking or blocking",
            (true, false) => "attacking",
            _ => "blocking",
        }
        .to_string();
    }
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

#[allow(clippy::too_many_lines)] // one list of words that keep their case
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
                | "Reveal"
                | "Exile"
                | "Draw"
                | "This"
                | "That"
                | "The"
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
                | "Face-down"
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
                | "Historic"
                | "Snow"
                | "Nonlegendary"
        )
    {
        return s.to_string();
    }
    // "Aura and Equipment spells": a subtype keeps its capital.
    if matches!(s.split(' ').next(), Some("Aura" | "Equipment" | "Vehicle")) {
        return s.to_string();
    }
    // "Birds, Frogs, Otters, and Rats you control get +1/+1": a list of
    // creature types in the plural.
    if let Some(first) = s.split(' ').next()
        && first.ends_with("s,")
        && first.chars().next().is_some_and(char::is_uppercase)
    {
        return s.to_string();
    }
    // "Raccoons you control get +1/+1": a creature type in the plural.
    let mut words = s.split(' ');
    if let (Some(first), Some("you"), Some("control")) = (words.next(), words.next(), words.next())
        && first.chars().next().is_some_and(char::is_uppercase)
        // "Mice you control get +1/+0" (Flowerfoot Swordmaster).
        && (first.ends_with('s') || first == "Mice")
        && !matches!(
            first,
            "Creatures"
                | "Permanents"
                | "Artifacts"
                | "Lands"
                | "Enchantments"
                | "Planeswalkers"
                | "Battles"
                | "Tokens"
                | "Spells"
                | "Cards"
                | "Others"
                // "outlaws you control" (At Knifepoint): no creature type.
                | "Outlaws"
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

/// A clause's duration after its text; a quoted ability before it drops
/// its own period ("gain \"{T}: This creature deals 2 damage to any
/// target\" until end of turn").
fn push_suffix(out: &mut String, suffix: &str) {
    // No duration (`Lasting::Forever`): "It gains \"...\"."
    if suffix.is_empty() {
        return;
    }
    if out.ends_with(".\"") {
        out.truncate(out.len() - 2);
        out.push('"');
    }
    out.push(' ');
    out.push_str(suffix);
}

/// One sentence (no final period, lowercase start).
#[allow(clippy::too_many_lines)]
fn sentence_text(s: &Sentence, ctx: &mut Ctx) -> String {
    let defined = std::mem::take(&mut ctx.x_pending);
    ctx.x_defined.extend(defined);
    ctx.groups_mentioned
        .retain(|r| !matches!(r, Recipient::Group(_)));
    let eot = std::mem::replace(&mut ctx.eot_first, s.eot_first);
    let lasting = s.lasting.map(|l| lasting_text(l, ctx));
    let outer_lasting = std::mem::replace(&mut ctx.lasting, lasting.clone());
    let clauses: Vec<Clause> = s
        .effects
        .iter()
        .enumerate()
        .map(|(i, e)| {
            // "you lose 1 life and this creature endures 1": after another
            // clause Oracle names the source again (Sinkhole Surveyor).
            if i > 0
                && matches!(
                    e,
                    Effect::Endure {
                        what: Recipient::Source,
                        ..
                    }
                )
            {
                ctx.source_mentioned = false;
            }
            clause(e, ctx)
        })
        .collect();
    ctx.eot_first = eot;
    ctx.lasting = outer_lasting;
    // Two pumps of different recipients each keep their duration
    // ("target creature an opponent controls gets -2/-2 until end of turn
    // and target creature you control gets +2/+2 until end of turn").
    let pumps_apart = clauses
        .iter()
        .filter(|c| c.text.starts_with("get"))
        .map(|c| c.subject.as_ref().map(|(r, _)| *r))
        .collect::<Vec<_>>()
        .windows(2)
        .any(|w| w[0] != w[1]);
    // "this creature has base power and toughness 4/4 until end of turn
    // and gains flying until end of turn" (the Mimics): a base power and
    // toughness keeps its own duration, unless the duration comes first
    // ("Until end of turn, target creature you control has base power and
    // toughness 4/4 and gains flying").
    let base_pt_apart = s
        .effects
        .iter()
        .any(|e| matches!(e, Effect::SetBasePt { .. }));
    let shared_suffix = clauses.len() > 1
        && !pumps_apart
        && !base_pt_apart
        && clauses[0].suffix.is_some()
        && clauses.iter().all(|c| c.suffix == clauses[0].suffix);
    // CR 611.2b: a duration other than "until end of turn" in its place.
    let sfx = |x: &'static str| -> String {
        match &lasting {
            // "can't block this combat" (CR 500.5a).
            Some(_)
                if x == "this turn" && s.lasting == Some(mtg_core::card::Lasting::EndOfCombat) =>
            {
                "this combat".to_string()
            }
            Some(t) if x == EOT || x == "this turn" => t.clone(),
            _ => x.to_string(),
        }
    };
    let mut out = String::new();
    if s.then_first {
        out.push_str("then ");
    }
    if s.per_opponent {
        out.push_str("for each opponent, ");
    }
    if s.eot_first {
        match &lasting {
            Some(t) => {
                out.push_str(t);
                out.push_str(", ");
            }
            None => out.push_str("until end of turn, "),
        }
    }
    let mut prev_subject: Option<Recipient> = None;
    let mut suffix_done = false;
    for (i, c) in clauses.iter().enumerate() {
        if i > 0 {
            // "put it into your hand, shuffle, then discard a card at
            // random" (Curse-Marred Demon): one "then", the last.
            if s.joins.get(i - 1) == Some(&Join::Then)
                && let Some(head) = out.strip_suffix(", then shuffle")
            {
                out = format!("{head}, shuffle");
            }
            out.push_str(match s.joins.get(i - 1) {
                Some(Join::Then) => ", then ",
                Some(Join::Comma) => ", ",
                // "discards two cards, mills a card, and loses 1 life".
                // "each opponent loses 1 life, you gain 1 life, and you
                // draw a card" (Intimidation Campaign).
                Some(Join::And | Join::AndRepeat) if s.joins[..i - 1].contains(&Join::Comma) => {
                    ", and "
                }
                // "it can't be regenerated this turn, and if it would die
                // this turn, exile it instead" (Carbonize).
                Some(Join::And) if c.subject.is_none() && c.text.starts_with("if ") => ", and ",
                // "deals 5 damage to that creature, and the other can't
                // block this turn" (Trial of Agony): a second subject.
                Some(Join::And) if matches!(c.subject, Some((Recipient::OtherTarget(_), _))) => {
                    ", and "
                }
                _ => " and ",
            });
        }
        let repeat = i > 0 && s.joins.get(i - 1) == Some(&Join::AndRepeat);
        let mut same_subject = false;
        // "you draw that many cards, create that many tapped Treasure
        // tokens, then lose that much life" (Kain, Traitorous Dragoon): a
        // list's subject isn't repeated after subjectless clauses.
        let list_subject = i > 1
            && s.joins.get(i - 1) == Some(&Join::Then)
            && s.joins[..i - 1].iter().all(|j| *j == Join::Comma)
            && clauses[1..i].iter().all(|d| d.subject.is_none())
            && clauses[0].subject.as_ref().map(|(r, _)| *r) == c.subject.as_ref().map(|(r, _)| *r);
        match &c.subject {
            Some((r, _)) if list_subject => {
                same_subject = true;
                prev_subject = Some(*r);
            }
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
        let text = if same_subject && c.text.starts_with("each ") {
            // "each mill a card and lose 1 life".
            &c.text["each ".len()..]
        } else if same_subject
            && c.text.starts_with("deals ")
            && clauses[i - 1].text.starts_with("deals ")
        {
            &c.text["deals ".len()..]
        } else if i > 0
            && s.joins.get(i - 1) == Some(&Join::And)
            && matches!(s.effects[i], Effect::AddCounters { .. })
            && matches!(s.effects[i - 1], Effect::AddCounters { .. })
            && c.text.starts_with("put ")
        {
            // "put a +1/+1 counter on target creature you control and a
            // plan counter on this enchantment" (`, and` repeats "put").
            &c.text["put ".len()..]
        } else if i > 0
            && c.subject.is_none()
            && clauses[i - 1].subject.is_none()
            && matches!(s.joins.get(i - 1), Some(Join::And))
            && c.text.starts_with("create a")
            && clauses[i - 1].text.starts_with("create a")
            && c.tail.is_empty()
            && clauses[i - 1].tail.is_empty()
        {
            // "Create a 1/1 white Human creature token and a Food token."
            // (Have for Dinner).
            &c.text["create ".len()..]
        } else if same_subject
            && c.text.starts_with("gains \"")
            && clauses[i - 1].text.starts_with("gains ")
        {
            // "gains deathtouch and \"When this creature dies, ...\"".
            &c.text["gains ".len()..]
        } else {
            &c.text
        };
        out.push_str(text);
        // "creatures you control get +1/+0 and gain haste until end of
        // turn": the same subject's clauses share one duration.
        let next_shares = !base_pt_apart
            && clauses.get(i + 1).is_some_and(|d| {
                s.joins.get(i) == Some(&Join::And)
                    && d.suffix == c.suffix
                    && d.subject.as_ref().map(|(r, _)| *r) == prev_subject
                    && c.subject.is_some()
            });
        if let Some(suffix) = c.suffix
            && !shared_suffix
            && !s.eot_first
            && !next_shares
        {
            push_suffix(&mut out, &sfx(suffix));
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
            push_suffix(&mut out, &sfx(c.suffix.expect("shared")));
            suffix_done = true;
        }
        if !shared_where {
            out.push_str(&c.tail);
        }
    }
    if shared_suffix && !s.eot_first && !suffix_done {
        push_suffix(&mut out, &sfx(clauses[0].suffix.expect("shared")));
    }
    // "blocks it this combat if able", "can't block this combat" (CR
    // 500.5a): the one-turn wording printed inside a clause.
    if s.lasting == Some(mtg_core::card::Lasting::EndOfCombat) {
        out = out
            .replace("until end of turn", "until end of combat")
            .replace("this turn", "this combat");
    }
    // "It loses all abilities for as long as ...": no clause printed a
    // duration of its own.
    if let Some(t) = &lasting
        && !s.eot_first
        && !out.contains(t.as_str())
        && !(s.lasting == Some(mtg_core::card::Lasting::EndOfCombat) && out.contains("this combat"))
    {
        push_suffix(&mut out, t);
    }
    out
}

/// A sentence's duration (CR 611.2b): "until your next turn", "for as
/// long as this creature remains on the battlefield".
fn lasting_text(l: mtg_core::card::Lasting, ctx: &Ctx) -> String {
    use mtg_core::card::Lasting;
    match l {
        Lasting::YourNextTurn => "until your next turn".to_string(),
        Lasting::SourceOnBattlefield => {
            format!(
                "for as long as {} remains on the battlefield",
                ctx.source_name()
            )
        }
        Lasting::WhileTapped => "for as long as it remains tapped".to_string(),
        Lasting::YouControlSource => format!("for as long as you control {}", ctx.source_name()),
        Lasting::SourceTapped => format!("for as long as {} remains tapped", ctx.source_name()),
        Lasting::UntilCastFromExile => "until this card is cast from exile".to_string(),
        // No duration: "That token gains haste."
        Lasting::Forever => String::new(),
        Lasting::EndOfCombat => "until end of combat".to_string(),
    }
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
        let saved = ctx.before_if_you_dont;
        ctx.before_if_you_dont = ss.get(i + 1).is_some_and(|n| {
            matches!(
                n.effects.first(),
                Some(Effect::IfYouDont { cant: false, .. })
            )
        });
        out.push_str(&sentence(&sentence_text(s, ctx)));
        ctx.before_if_you_dont = saved;
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

/// Two events of one subject, the words they share printed once: "When
/// enchanted creature becomes tapped or is dealt damage".
pub(crate) fn or_shared(a: &str, b: &str, whenever: bool) -> String {
    let (wa, wb): (Vec<&str>, Vec<&str>) = (
        a.split(' ').skip(1).collect(),
        b.split(' ').skip(1).collect(),
    );
    let k = wa.iter().zip(&wb).take_while(|(x, y)| x == y).count();
    format!(
        "{} {} or {}",
        if whenever { "whenever" } else { "when" },
        wa.join(" "),
        wb[k..].join(" ")
    )
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
        // "When you next cast an instant or sorcery spell" (the delayed
        // trigger adds "this turn").
        Trigger::Next(inner) => {
            trigger_text(inner, ctx, next).replacen("whenever you cast", "when you next cast", 1)
        }
        // CR 603.2c.
        Trigger::Batch(inner) => match &**inner {
            Trigger::PutIntoGraveyard(f) => format!(
                "whenever one or more {} cards are put into your graveyard from anywhere",
                card_filter_phrase(f, ctx.card)
            ),
            Trigger::BecomesTapped(TriggerObj::Filter(f)) => format!(
                "whenever one or more {} become tapped",
                controller_first(f, filter_phrase(f, ctx.card, true))
            ),
            Trigger::DiscardsBatch { .. } => {
                "whenever one or more players discard one or more cards".to_string()
            }
            Trigger::YouTap(f) => format!(
                "whenever you tap one or more {}",
                controller_first(f, filter_phrase(f, ctx.card, true))
            ),
            Trigger::LeavesWithoutDying(TriggerObj::Filter(f)) => format!(
                "whenever one or more {} leave the battlefield without dying",
                controller_first(f, filter_phrase(f, ctx.card, true))
            ),
            t => unreachable!("no batched form of {t:?}"),
        },
        // "At the beginning of the end step on your next turn".
        Trigger::OnNextTurn(inner) => match &**inner {
            Trigger::BeginningOfStep { step, .. } => format!(
                "at the beginning of the {} on your next turn",
                step_name(*step, Who::You).trim_start_matches("your ")
            ),
            t => unreachable!("no next-turn form of {t:?}"),
        },
        Trigger::Each(inner) => match &**inner {
            Trigger::LeavesGraveyard(f) => format!(
                "whenever {} leaves your graveyard",
                with_article(&if *f == Filter::default() {
                    "card".to_string()
                } else {
                    format!("{} card", card_filter_phrase(f, ctx.card))
                })
            ),
            Trigger::YouPutCounters { obj, kind } => format!(
                "whenever you put {} on {}",
                with_article(&format!(
                    "{}counter",
                    kind.map_or(String::new(), |k| format!("{} ", counter_name(k)))
                )),
                trigger_subject(obj, ctx)
            ),
            t => unreachable!("no per-object form of {t:?}"),
        },
        Trigger::State(c) => format!("when {}", static_cond_text(c, ctx.card)),
        Trigger::Either(a, b) => {
            let a = trigger_text(a, ctx, next);
            let b = trigger_text(b, ctx, next);
            // "When this creature enters and whenever it attacks": the
            // second names it again as "it".
            let named = this_ref(ctx.card);
            let b = match b.strip_prefix(&format!("whenever {named} ")) {
                Some(rest) if named.starts_with("this ") && a.contains(&named) => {
                    format!("whenever it {rest}")
                }
                _ => b,
            };
            format!("{a} and {b}")
        }
        // "When this creature enters or dies", "Whenever this creature
        // enters or attacks": the subject printed once.
        // "When this creature enters or the creature it haunts dies".
        Trigger::Or { first, second, .. } if **second == Trigger::HauntedDies => {
            let a = trigger_text(first, ctx, next);
            ctx.source_mentioned = true;
            format!("{a} or the creature it haunts dies")
        }
        Trigger::Or {
            first,
            second,
            whenever,
        } => {
            let a = trigger_text(first, ctx, next);
            let b = trigger_text(second, ctx, next);
            // "Whenever a Detective you control enters or is turned face
            // up", "When enchanted creature becomes tapped or is dealt
            // damage": the words both events share printed once.
            if !matches!(**first, Trigger::Enters(crate::ast::TriggerObj::This)) {
                return or_shared(&a, &b, *whenever);
            }
            // "Whenever this creature enters or transforms into ~": a
            // legendary one too.
            let named = format!("{} ", this_ref(ctx.card));
            let subject = if **second == Trigger::Transforms {
                format!("this {} ", self_noun(ctx.card))
            } else {
                named.clone()
            };
            let a = a.split_once(&named).map_or(a.as_str(), |(_, v)| v);
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
        // CR 603.2: the spell itself; "cast or cycle" names the card.
        Trigger::Sacrifices(f) => {
            format!("whenever you sacrifice {}", trigger_filter_a(f, ctx.card))
        }
        Trigger::CastThis { or_cycle: false } => {
            ctx.source_mentioned = true;
            ctx.cast_this = true;
            "when you cast this spell".to_string()
        }
        Trigger::CastThis { or_cycle: true } => {
            ctx.source_mentioned = true;
            format!("when you cast or cycle {}", ctx.card.name)
        }
        Trigger::CastsNth { whose, nth } => {
            format!("whenever {}", crate::format::spell_ordinal(*nth, *whose))
        }
        Trigger::Draws { whose } => {
            format!("whenever {} a card", crate::format::draws_words(*whose))
        }
        // CR 701.9: the card is discarded from the hand, "this card".
        Trigger::DiscardsThis => {
            ctx.source_mentioned = true;
            "when you discard this card".to_string()
        }
        Trigger::Discards { whose, filter: f } => format!(
            "whenever {} {}",
            crate::format::discards_words(*whose),
            if *f == Filter::default() {
                "a card".to_string()
            } else {
                with_article(&format!("{} card", card_filter_phrase(f, ctx.card)))
            }
        ),
        Trigger::LeavesGraveyard(f) => format!(
            "whenever one or more {} leave your graveyard",
            if *f == Filter::default() {
                "cards".to_string()
            } else if f.and_or {
                // "artifact and/or creature cards" (Dredger's Insight).
                format!(
                    "{} cards",
                    card_filter_phrase(f, ctx.card).replace(" or ", " and/or ")
                )
            } else {
                format!("{} cards", card_filter_phrase(f, ctx.card))
            }
        ),
        Trigger::PutIntoGraveyard(f) => format!(
            "whenever {} is put into your graveyard from anywhere",
            with_article(&format!("{} card", card_filter_phrase(f, ctx.card)))
        ),
        Trigger::EntersFromGraveyard(f) => format!(
            "whenever {} enters from your graveyard",
            filter_a(f, ctx.card)
        ),
        Trigger::PutIntoGraveyardFromBattlefield(f) => format!(
            "whenever {} is put into your graveyard from the battlefield",
            filter_a(f, ctx.card)
        ),
        Trigger::Expends(n) => format!("whenever you expend {n}"),
        Trigger::CollectsEvidence => "whenever you collect evidence".to_string(),
        Trigger::Forages => "whenever you forage".to_string(),
        Trigger::CreaturesDealCombatDamage(f) => format!(
            "whenever one or more {} deal combat damage to a player",
            controller_first(f, filter_phrase(f, ctx.card, true))
        ),
        Trigger::CreaturesDealDamage(f) => format!(
            "whenever one or more {} deal damage to a player",
            controller_first(f, filter_phrase(f, ctx.card, true))
        ),
        Trigger::NoncombatDamageToOpponent { yours, batch } => match (yours, batch) {
            (true, _) => {
                "whenever a source you control deals noncombat damage to an opponent".to_string()
            }
            (false, false) => "whenever an opponent is dealt noncombat damage".to_string(),
            (false, true) => {
                "whenever one or more opponents are dealt noncombat damage".to_string()
            }
        },
        Trigger::CommitsCrime { your_turn } => format!(
            "whenever you commit a crime{}",
            if *your_turn { " during your turn" } else { "" }
        ),
        Trigger::YouTurnFaceUp => "whenever you turn a permanent face up".to_string(),
        Trigger::OpponentGainsControl => {
            "whenever an opponent gains control of a permanent from you".to_string()
        }
        Trigger::DiscardsBatch { whose } => format!(
            "whenever {} one or more cards",
            crate::format::discards_words(*whose)
        ),
        Trigger::SacrificesBatch(f) => format!(
            "whenever you sacrifice one or more {}",
            filter_phrase(f, ctx.card, true)
        ),
        Trigger::AttacksWithFilter { filter: f, player } => format!(
            "whenever you attack{} with one or more {}",
            if *player { " a player" } else { "" },
            controller_first(f, filter_phrase(f, ctx.card, true))
        ),
        Trigger::GainsOrLosesLife { your_turn } => format!(
            "whenever you gain or lose life{}",
            if *your_turn { " during your turn" } else { "" }
        ),
        Trigger::ScriesOrSurveils => "whenever you scry or surveil".to_string(),
        Trigger::Scries => "whenever you scry".to_string(),
        Trigger::Explores(o) => format!("whenever {} explores", trigger_subject(o, ctx)),
        Trigger::CrewsOrSaddles(o) => format!(
            "whenever {} saddles a Mount or crews a Vehicle",
            trigger_subject(o, ctx)
        ),
        // "..., then this creature gets +X/+X": Oracle names it again.
        Trigger::BecomesSaddled(TriggerObj::This) => format!(
            "whenever {} becomes saddled for the first time each turn",
            this_ref(ctx.card)
        ),
        Trigger::BecomesSaddled(o) => format!(
            "whenever {} becomes saddled for the first time each turn",
            trigger_subject(o, ctx)
        ),
        Trigger::YouTap(f) => format!("whenever you tap {}", filter_a(f, ctx.card)),
        Trigger::Activates { whose, kind } => {
            format!("whenever {}", crate::format::activates_words(*whose, *kind))
        }
        // CR 701.27: the face this ability is on.
        Trigger::Transforms => {
            ctx.source_mentioned = true;
            // "this creature" even for a legendary one (Ulrich).
            format!(
                "whenever this {} transforms into {}",
                self_noun(ctx.card),
                ctx.card.name
            )
        }
        Trigger::EntersBatch(f) => format!(
            "whenever one or more {} enter",
            controller_first(f, filter_phrase(f, ctx.card, true))
        ),
        Trigger::LeavesBatch { filter: f, dies } => format!(
            "whenever one or more {} {}",
            controller_first(f, filter_phrase(f, ctx.card, true)),
            if *dies {
                "die"
            } else {
                "leave the battlefield"
            }
        ),
        Trigger::DiesOrExiled(o) => {
            let dies = trigger_text(&Trigger::Dies(o.clone()), ctx, next);
            format!("{dies} or is put into exile")
        }
        Trigger::Dies(o @ TriggerObj::ThisOr(_)) => {
            format!("whenever {} dies", trigger_subject(o, ctx))
        }
        Trigger::Enters(TriggerObj::This) => format!("when {} enters", this(ctx)),
        Trigger::Enters(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!("whenever {} enters", trigger_subject(o, ctx))
        }
        // "When this Vehicle dies" (CR 700.4: put into a graveyard from
        // the battlefield).
        Trigger::Dies(TriggerObj::This) => {
            if ctx.card.types.contains(&CardType::Creature)
                || ctx.card.subtypes.iter().any(|s| s == "Vehicle")
            {
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
            // "Whenever an Angel you control dies" on an enchantment: nor
            // a subtype the card lacks.
            let can_be_self = f.types.iter().all(|t| ctx.card.types.contains(t))
                && f.subtypes.iter().all(|s| ctx.card.subtypes.contains(s));
            // "Whenever an enchantment you control is put into a graveyard
            // from the battlefield": only creatures die (CR 700.4).
            // "Whenever a Clue you control is put into a graveyard ...": a
            // noncreature subtype, no Vehicle, names no creature either.
            let noncreature_subtype = f.types.is_empty()
                && !f.subtypes.is_empty()
                && f.subtypes.iter().all(|s| {
                    s != "Vehicle" && mtg_core::card::NONCREATURE_SUBTYPES.contains(&s.as_str())
                });
            // "When enchanted land dies" on an Aura that makes the land a
            // creature (Wind Zendikon).
            let animated = f.enchanted
                && ctx.card.abilities.iter().any(|a| {
                    matches!(a, Ability::Static { filter, mods, .. }
                        if filter.enchanted
                            && mods.iter().any(|m| matches!(m,
                                Mod::AddTypes(ts) if ts.contains(&CardType::Creature))))
                });
            let dies = if ((!f.types.is_empty() && !f.types.contains(&CardType::Creature))
                || noncreature_subtype)
                && !animated
            {
                "is put into a graveyard from the battlefield"
            } else {
                "dies"
            };
            // "Whenever a creature dealt damage by this creature this turn
            // dies": not the source.
            // "Whenever a creature an opponent controls dies": nor one
            // its controller's opponent controls.
            if f.relation == Relation::Other
                || f.enchanted
                || !can_be_self
                || f.status.damaged_by_source
                || f.who == Some(Who::Opponents)
                // "Whenever a nontoken modified creature you control dies":
                // Oracle names no "this creature or another" for a
                // qualifier the source may lack.
                || f.status.modified
                // "Whenever a creature you control with a +1/+1 counter on
                // it dies" (Meltstrider Eulogist): likewise.
                || f.status.counter.is_some()
                // "Whenever a nonland creature you control dies" (Beifong's
                // Bounty Hunters).
                || f.not_types.contains(&CardType::Land)
                // `whenever a creature dies`: the older wording.
                || f.article
            {
                format!("whenever {} {dies}", trigger_filter_a(f, ctx.card))
            } else {
                let t = this(ctx);
                format!(
                    "whenever {t} or another {} {dies}",
                    filter_phrase(f, ctx.card, false)
                )
            }
        }
        Trigger::EveryStep(Step::BeginningOfCombat) => {
            "at the beginning of each combat".to_string()
        }
        Trigger::EachPlayersEndStep => "at the beginning of each player's end step".to_string(),
        // "Whenever another creature or artifact you control is put into a
        // graveyard from the battlefield" (Forgehammer Centurion), "When
        // enchanted artifact is put into a graveyard" (Viridian Harvest).
        Trigger::PutInGraveyard {
            obj,
            from_battlefield,
        } => {
            let t = trigger_text(&Trigger::Dies(obj.clone()), ctx, next);
            let base = t
                .strip_suffix(" is put into a graveyard from the battlefield")
                .or_else(|| t.strip_suffix(" dies"))
                .unwrap_or(&t);
            format!(
                "{base} is put into a graveyard{}",
                if *from_battlefield {
                    " from the battlefield"
                } else {
                    ""
                }
            )
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
        // "At the beginning of enchanted player's upkeep" (a Curse).
        Trigger::AttachedStep(step) if enchanted_noun(ctx.card) == "player" => format!(
            "at the beginning of enchanted player's {}",
            step_name(*step, Who::You).trim_start_matches("your ")
        ),
        Trigger::AttachedStep(step) => format!(
            "at the beginning of the {} of enchanted {}'s controller",
            step_name(*step, Who::You).trim_start_matches("your "),
            enchanted_noun(ctx.card)
        ),
        Trigger::BeginningOfStep { step, whose } => {
            if next && *step == Step::Upkeep && *whose == Who::Any {
                "at the beginning of the next turn's upkeep".to_string()
            } else if next && *whose == Who::You {
                // "At the beginning of your next upkeep".
                format!(
                    "at the beginning of your next {}",
                    step_name(*step, Who::You).trim_start_matches("your ")
                )
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
        Trigger::AttacksFirstTime(TriggerObj::This) => format!(
            "whenever {} attacks for the first time each turn",
            this(ctx)
        ),
        Trigger::AttacksFirstTime(o) => format!(
            "whenever {} attacks for the first time each turn",
            trigger_subject(o, ctx)
        ),
        Trigger::Attacks(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!("whenever {} attacks", trigger_subject(o, ctx))
        }
        Trigger::AttacksWith {
            this: false,
            min: 1,
        } => "whenever you attack".to_string(),
        Trigger::OneOrMoreAttack(f) => format!(
            "whenever one or more {} attack",
            controller_first(f, filter_phrase(f, ctx.card, true))
        ),
        Trigger::CreaturesYouControlAttack => {
            "whenever one or more creatures you control attack".to_string()
        }
        // The card names itself: it triggers from the graveyard, where it
        // isn't "this creature" (CR 113.6).
        Trigger::ThisToGraveyard => {
            // "..., shuffle it into its owner's library."
            ctx.source_mentioned = true;
            format!(
                "when {} is put into a graveyard from anywhere",
                short_name(ctx.card)
            )
        }
        Trigger::OpponentCreaturesAttack(n) => format!(
            "whenever {} or more creatures your opponents control attack",
            count_words(Num::N(u32::from(*n)), "one")
        ),
        Trigger::PlayerAttacksWith { whose, min } => format!(
            "whenever {} attacks with {} or more creatures",
            if *whose == Who::Opponents {
                "an opponent"
            } else {
                "a player"
            },
            count_words(Num::N(u32::from(*min)), "one")
        ),
        Trigger::AttacksWith { this: false, min } => format!(
            "whenever you attack with {} or more creatures",
            count_words(Num::N(u32::from(*min)), "one")
        ),
        // "Whenever Sokka and at least one other creature attack".
        Trigger::AttacksWith { this: true, min } => format!(
            "whenever {} and at least {} other {} attack",
            this(ctx),
            count_words(Num::N(u32::from(min.saturating_sub(1))), "one"),
            if *min == 2 { "creature" } else { "creatures" }
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
        Trigger::LeavesWithoutDying(o) => format!(
            "whenever {} leaves the battlefield without dying",
            trigger_subject(o, ctx)
        ),
        // "Whenever another nonland permanent you control is returned to
        // its owner's hand".
        Trigger::ReturnedToHand(o) => format!(
            "whenever {} is returned to its owner's hand",
            trigger_subject(o, ctx)
        ),
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
            any,
        } => {
            let who = if *any {
                // "by one or more blue and/or black creatures" (CR
                // 509.3c).
                let p = filter_phrase(other, ctx.card, true);
                let p = if other.colors.as_ref().is_some_and(|c| c.len() == 2) {
                    p.replacen(" or ", " and/or ", 1)
                } else {
                    p
                };
                format!("one or more {p}")
            } else {
                // "by a creature that has been dealt damage this turn"
                // (Giant Shark).
                filter_a(other, ctx.card).replace(
                    " that was dealt damage this turn",
                    " that has been dealt damage this turn",
                )
            };
            let s = format!(
                "whenever {} {} {who}",
                trigger_subject(obj, ctx),
                crate::format::block_pair_verb(*blocks, *blocked),
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
        Trigger::DealsCombatDamageToPlayerOrBattle(TriggerObj::This) => {
            format!(
                "whenever {} deals combat damage to a player or battle",
                this(ctx)
            )
        }
        Trigger::DealsCombatDamageToPlayerOrBattle(o) => {
            format!(
                "whenever {} deals combat damage to a player or battle",
                trigger_subject(o, ctx)
            )
        }
        Trigger::DealsCombatDamageToPlayerOrPlaneswalker(TriggerObj::This) => format!(
            "whenever {} deals combat damage to a player or planeswalker",
            this(ctx)
        ),
        Trigger::DealsCombatDamageToPlayerOrPlaneswalker(o) => format!(
            "whenever {} deals combat damage to a player or planeswalker",
            trigger_subject(o, ctx)
        ),
        Trigger::UnlocksDoor => "when you unlock this door".to_string(),
        Trigger::Chapter(ns) => chapter_numerals(ns),
        Trigger::BecomesLevel(n) => format!("when {} becomes level {n}", this(ctx)),
        Trigger::FullyUnlocks => "whenever you fully unlock a Room".to_string(),
        Trigger::AttacksBattle(TriggerObj::This) => {
            format!("whenever {} attacks a battle", this(ctx))
        }
        Trigger::AttacksBattle(o) => {
            format!("whenever {} attacks a battle", trigger_subject(o, ctx))
        }
        Trigger::DealsCombatDamageToDefending(o) => {
            let s = match o {
                TriggerObj::This => this(ctx),
                o => trigger_subject(o, ctx),
            };
            format!("whenever {s} deals combat damage to defending player")
        }
        Trigger::DealsCombatDamageToPlayer(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!(
                "whenever {} deals combat damage to a player",
                trigger_subject(o, ctx)
            )
        }
        Trigger::WinsFlip { .. } => "whenever you win a coin flip".to_string(),
        Trigger::DayNight => "whenever day becomes night or night becomes day".to_string(),
        // "when you sacrifice it" (after "When this artifact enters and"),
        // "Whenever you sacrifice another permanent".
        Trigger::Sacrificed { obj, whose } => {
            let who = crate::format::sacrifices_words(*whose);
            match obj {
                TriggerObj::This if ctx.source_mentioned => format!("when {who} it"),
                TriggerObj::This => format!("when {who} {}", this(ctx)),
                o => format!("whenever {who} {}", trigger_subject(o, ctx)),
            }
        }
        Trigger::RollsDice { whose } => match whose {
            Who::You => "whenever you roll one or more dice".to_string(),
            _ => "whenever a player rolls one or more dice".to_string(),
        },
        Trigger::GainsLife { whose } => match whose {
            Who::You => "whenever you gain life".to_string(),
            _ => "whenever a player gains life".to_string(),
        },
        Trigger::LosesLife { whose } => match whose {
            Who::You => "whenever you lose life".to_string(),
            Who::Opponents => "whenever an opponent loses life".to_string(),
            Who::Any => "whenever a player loses life".to_string(),
        },
        Trigger::GainsLifeFirst { .. } => {
            "whenever you gain life for the first time each turn".to_string()
        }
        Trigger::CastsSpell {
            whose,
            noncreature,
            instant_or_sorcery,
            or_copy,
        } => {
            let who = match whose {
                Who::You if *or_copy => "you cast or copy",
                Who::You => "you cast",
                Who::Opponents => "an opponent casts",
                Who::Any => "a player casts",
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
        Trigger::BecomesAttachedTo(f) => format!(
            "whenever {} becomes attached to {}",
            trigger_filter_a(f, ctx.card),
            this_ref(ctx.card)
        ),
        // "Whenever you tap a land for mana" (a permanent you control).
        Trigger::TappedForMana(TriggerObj::Filter(f))
            if f.who == Some(Who::You) && f.relation == Relation::Any =>
        {
            let mut g = f.clone();
            g.who = None;
            format!(
                "whenever you tap {} for mana",
                trigger_filter_a(&g, ctx.card)
            )
        }
        Trigger::TappedForMana(o) => {
            format!("whenever {} is tapped for mana", trigger_subject(o, ctx))
        }
        // "Whenever you tap this creature for mana", "Whenever a player
        // taps a land for mana" (CR 106.11a).
        Trigger::TapsForMana { what, whose } => {
            let who = match whose {
                Who::You => "you tap",
                Who::Opponents => "an opponent taps",
                Who::Any => "a player taps",
            };
            let o = match what {
                TriggerObj::Filter(f) => trigger_filter_a(f, ctx.card),
                other => trigger_subject(other, ctx),
            };
            format!("whenever {who} {o} for mana")
        }
        Trigger::BecomesUntapped(o) => {
            format!("whenever {} becomes untapped", trigger_subject(o, ctx))
        }
        Trigger::CountersPut { obj, kind } => format!(
            "whenever one or more {} counters are put on {}",
            counter_name(*kind),
            trigger_subject(obj, ctx)
        ),
        Trigger::NthCounterPut { kind, n } => format!(
            "whenever the {} {} counter is put on {}",
            nth_word(*n),
            counter_name(*kind),
            trigger_subject(&TriggerObj::This, ctx)
        ),
        Trigger::YouPutCounters { obj, kind } => format!(
            "whenever you put one or more {}counters on {}",
            kind.map_or(String::new(), |k| format!("{} ", counter_name(k))),
            trigger_subject(obj, ctx)
        ),
        Trigger::DealtDamage(o) => format!("whenever {} is dealt damage", trigger_subject(o, ctx)),
        Trigger::DealtDamageAtLeast(o, n) => {
            format!(
                "whenever {} is dealt {n} or more damage",
                trigger_subject(o, ctx)
            )
        }
        Trigger::DealtCombatDamage(o) => {
            format!(
                "whenever {} is dealt combat damage",
                trigger_subject(o, ctx)
            )
        }
        Trigger::DealsDamageTo { obj, filter } => {
            let s = format!(
                "whenever {} deals damage to {}",
                trigger_subject(obj, ctx),
                filter_a(filter, ctx.card)
            );
            ctx.source_mentioned = false;
            s
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
            let mut base = filter.clone();
            let targeting = base.targeting.take();
            // Heroic: "a spell that targets this creature".
            let that = targeting.map_or(String::new(), |t| {
                format!(
                    " that targets {}",
                    crate::format::targeting(&t, |g| {
                        if g.relation == Relation::This {
                            this_ref(ctx.card)
                        } else {
                            filter_phrase(g, ctx.card, false)
                        }
                    })
                )
            });
            if base == Filter::default() {
                return format!("whenever {who} a spell{that}");
            }
            // "Whenever you cast a spell that's both white and black" (the
            // Shadowmoor Mimics, CR 105.2).
            if base.status.all_colors && base.colors.is_some() && base.targeting.is_none() {
                return format!("whenever {who} {}{that}", a_spell(&base, ctx.card));
            }
            // "an instant, sorcery, or Wizard spell" (Windrider Wizard): card
            // types and one alternative of another kind, in one list.
            if !base.any_types.is_empty()
                && let Some(alt) = &base.or
                && alt.or.is_none()
                && !base.alt_article
                && {
                    let mut g = base.clone();
                    g.any_types.clear();
                    g.or = None;
                    g == Filter::default()
                }
            {
                let mut items: Vec<String> = base
                    .any_types
                    .iter()
                    .map(|t| type_noun(*t).to_string())
                    .collect();
                let w = spells_subject(alt, ctx.card);
                let w = w.strip_suffix(" spells").unwrap_or(&w).to_string();
                items.push(if lower_spell_subject(alt) {
                    lowercase_first(&w)
                } else {
                    w
                });
                return format!(
                    "whenever {who} {}{that}",
                    with_article(&spell_noun(&or_list(&items)))
                );
            }
            // "an instant or sorcery spell with mana value 4 or less".
            if !base.any_types.is_empty() {
                let types: Vec<String> = base
                    .any_types
                    .iter()
                    .map(|t| type_noun(*t).to_string())
                    .collect();
                let mut g = base.clone();
                g.any_types.clear();
                let rest = card_filter_phrase(&g, ctx.card);
                let rest = rest.trim();
                let joined = or_list(&types);
                let what = if rest.is_empty() {
                    joined
                } else if rest.starts_with("with ") {
                    format!("{joined} {rest}")
                } else {
                    format!("{rest} {joined}")
                };
                return format!("whenever {who} {}{that}", with_article(&spell_noun(&what)));
            }
            // `noncreature or a Dragon spell`: "a noncreature spell or a
            // Dragon spell" (Runescale Stormbrood).
            if base.alt_article
                && let Some(alt) = &base.or
                && alt.or.is_none()
            {
                let mut one = base.clone();
                one.or = None;
                one.alt_article = false;
                let side = |g: &Filter| {
                    let w = spells_subject(g, ctx.card);
                    let w = w.strip_suffix(" spells").unwrap_or(&w).to_string();
                    let w = if lower_spell_subject(g) {
                        lowercase_first(&w)
                    } else {
                        w
                    };
                    with_article(&spell_noun(&w))
                };
                return format!("whenever {who} {} or {}{that}", side(&one), side(alt));
            }
            // "a Lesson, Saga, or Shrine spell" (Guru Pathik).
            if base.or.as_ref().is_some_and(|a| a.or.is_some()) {
                let s = a_spell(&base, ctx.card);
                if s.contains(", or ") {
                    return format!("whenever {who} {s}{that}");
                }
            }
            let what = spells_subject(&base, ctx.card);
            let what = what.strip_suffix(" spells").unwrap_or(&what);
            // "Instant and sorcery spells" (a cost changer's subject) is
            // one spell here: "an instant or sorcery spell that targets a
            // creature" (repartee).
            let what = what.replace("nstant and sorcery", "nstant or sorcery");
            let what = what.as_str();
            // "an artifact spell", but "a Giant spell".
            let what = if lower_spell_subject(&base) {
                lowercase_first(what)
            } else {
                what.to_string()
            };
            // Four colors or more: "a spell that's white, blue, black, or
            // red" (Quirion Dryad).
            let colors = ["white", "blue", "black", "red", "green"];
            if what.split(", ").count() >= 4
                && what
                    .split(", ")
                    .all(|w| colors.contains(&w.trim_start_matches("or ")))
            {
                return format!("whenever {who} a spell that's {what}{that}");
            }
            format!("whenever {who} {}{that}", with_article(&spell_noun(&what)))
        }
        Trigger::TurnedFaceUp(TriggerObj::This) => format!("when {} is turned face up", this(ctx)),
        Trigger::TurnedFaceUp(o @ (TriggerObj::Filter(_) | TriggerObj::ThisOr(_))) => {
            format!("whenever {} is turned face up", trigger_subject(o, ctx))
        }
        Trigger::Exploits => format!("when {} exploits a creature", this(ctx)),
        // CR 702.55b.
        Trigger::HauntedDies => "when the creature this card haunts dies".to_string(),
        // It is a card in exile, not yet a creature (CR 702.170).
        Trigger::BecomesPlotted => {
            ctx.source_mentioned = true;
            "when this card becomes plotted".to_string()
        }
        Trigger::DrawsNth {
            whose,
            n,
            in_a_turn,
        } => crate::format::draws_nth(*whose, *n, *in_a_turn),
        Trigger::TargetedByYouFirstTime => format!(
            "whenever {} becomes the target of a spell or ability you control for the first time each turn",
            this(ctx)
        ),
        Trigger::TargetedFirstTime => format!(
            "whenever {} becomes the target of a spell or ability for the first time each turn",
            this(ctx)
        ),
        Trigger::TargetedBy {
            obj,
            spells,
            whose,
            whenever,
        } => {
            let what = if *spells {
                "a spell"
            } else {
                "a spell or ability"
            };
            let whose = match whose {
                Who::Opponents => " an opponent controls",
                Who::You => " you control",
                Who::Any => "",
            };
            format!(
                "{} {} becomes the target of {what}{whose}",
                if *whenever { "whenever" } else { "when" },
                trigger_subject(obj, ctx)
            )
        }
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
            // "Whenever this creature or equipped creature attacks"
            // (reconfigure): the creature it's attached to, not another.
            if f.enchanted {
                return format!("{} or {p}", this_ref(ctx.card));
            }
            // "Whenever High Perfect Morcant or another Elf you control
            // enters": a legendary source by name (CR 201.5).
            format!("{} or another {p}", this_ref(ctx.card))
        }
    }
}

/// A filter with an article as a trigger's subject, where Oracle names the
/// controller before the qualities: "a creature you control with power 4
/// or greater" (elsewhere "a creature with power 4 or greater you
/// control").
fn trigger_filter_a(f: &Filter, card: &Card) -> String {
    let s = controller_first(f, filter_a(f, card));
    if f.and_or {
        return s.replacen(" or ", " and/or ", 1);
    }
    s
}

/// A rendered filter with its "you control" moved before its "with ..."
/// qualities.
/// A group of some of the creatures you control: "White creatures you
/// control", "Kithkin creatures you control", "Colorless creatures you
/// control".
fn narrower_creatures_you_control(r: Recipient, card: &Card) -> bool {
    let Recipient::Group(i) = r else {
        return false;
    };
    card.groups.get(usize::from(i)).is_some_and(|f| {
        f.who == Some(Who::You)
            && f.or.is_none()
            && f.types.as_slice() == [mtg_core::card::CardType::Creature]
            && (f.colors.is_some() || !f.subtypes.is_empty() || f.color_count.is_some())
    })
}

fn controller_first(f: &Filter, s: String) -> String {
    let ctl = match f.who {
        Some(Who::You) => " you control",
        Some(Who::Opponents) if f.dont_control => " you don't control",
        Some(Who::Opponents) if f.your_opponents => " your opponents control",
        Some(Who::Opponents) if f.they_control => " they control",
        // "target creature defending player controls with power less
        // than this creature's power" (Radha's Firebrand).
        Some(Who::Opponents) if f.defending => " defending player controls",
        Some(Who::Opponents) => " an opponent controls",
        _ => return s,
    };
    // `... an opponent controls late`: where it was written.
    if f.or.is_some() || f.control_late {
        return s;
    }
    // "target creature you control without flying" too.
    let quality = match (s.find(" with "), s.find(" without ")) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    match (s.strip_suffix(ctl), quality) {
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
        // "When this Equipment enters, if the gift was promised, attach
        // this Equipment to ...": named again after the gift.
        Condition::Kicked if has_gift(ctx.card) => {
            ctx.source_mentioned = false;
            "if the gift was promised".to_string()
        }
        // "if he was kicked" (Josu Vess).
        Condition::Kicked => format!("if {} was kicked", it_subj(ctx.card)),
        Condition::HadNoCounters(mtg_core::card::CounterKind::Any) => {
            "if it had no counters on it".to_string()
        }
        Condition::HadNoCounters(k) => format!("if it had no {} counters on it", counter_name(k)),
        Condition::HadCounters(k) => format!(
            "if it had {} on it",
            with_article(&format!("{} counter", counter_name(k)))
        ),
        Condition::TriggerHadCounters => "if it had counters on it".to_string(),
        Condition::NoSpellsLastTurn => "if no spells were cast last turn".to_string(),
        Condition::TwoSpellsLastTurn => "if a player cast two or more spells last turn".to_string(),
        Condition::EchoDue => {
            "if it came under your control since the beginning of your last upkeep".to_string()
        }
        Condition::Static(_) | Condition::While(_) | Condition::WhileSecond(_) => {
            unreachable!("an AST condition")
        }
        Condition::Evoked => "if it was evoked".to_string(),
        Condition::NotRenowned => "if it isn't renowned".to_string(),
        Condition::WasBlocking(true) => "if it was blocking".to_string(),
        Condition::WasBlocking(false) => "if it wasn't blocking".to_string(),
        Condition::NotPrepared => format!("if {} isn't prepared", ctx.source()),
        Condition::Evolves => format!(
            "if it has greater power or toughness than {}",
            this_ref(ctx.card)
        ),
        Condition::Increments => format!(
            "if the amount of mana you spent is greater than {}'s power or toughness",
            this_ref(ctx.card)
        ),
        Condition::Blitzed => "if its blitz cost was paid".to_string(),
        Condition::TributeNotPaid => "if tribute wasn't paid".to_string(),
        Condition::Trains => format!(
            "if at least one other creature with power greater than {}'s power attacked",
            this_ref(ctx.card)
        ),
        Condition::Dethrones => {
            "if it attacked the player with the most life or tied for most life".to_string()
        }
        Condition::AttackedOpponent => "if you attacked an opponent this combat".to_string(),
        // Printed by the meld effect itself.
        Condition::MeldPair => String::new(),
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
        Affects::OpponentsAnd(f) => format!("an opponent or {}", filter_a(f, ctx.card)),
        Affects::Target(i) => ctx.obj(Recipient::Target(*i)),
        Affects::ThatTarget(i) => ctx.obj(Recipient::ThatTarget(*i)),
        Affects::Targets(a, n) => ctx.obj(Recipient::Targets(*a, *n)),
        Affects::OtherThanTarget(f, i) => format!(
            "{} other than {}",
            filter_phrase(f, ctx.card, true),
            ctx.obj(Recipient::Target(*i))
        ),
        Affects::Chosen(f) => chosen_source(f, ctx.card),
        Affects::DamagedThisWay => "a creature dealt damage this way".to_string(),
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
    let lasting = ctx.lasting.as_ref().map(|t| format!(" {t}"));
    let turn = match &lasting {
        Some(t) if this_turn => t.as_str(),
        _ if this_turn => " this turn",
        _ => "",
    };
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
                (Some(f), DamageWording::WouldDeal | DamageWording::WouldDealBare) => {
                    // "prevent all combat damage this creature would deal
                    // this turn" (Zealot il-Vec), but a static's "Prevent
                    // all damage that this creature would deal to ...".
                    let that = if *wording == DamageWording::WouldDealBare
                        || matches!(
                            f,
                            Affects::Target(_) | Affects::ThatTarget(_) | Affects::Chosen(_)
                        )
                        || (this_turn && matches!(f, Affects::This))
                    {
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
        // A static ability prevents that much of every damage event (CR
        // 615.1): "If a source would deal damage to a Cleric creature you
        // control, prevent 1 of that damage." (Daunting Defender), "If an
        // artifact would deal damage to you, ..." (Sphere of Purity).
        (
            Replaces::Damage {
                from,
                to,
                combat: None,
                ..
            },
            Outcome::PreventNext(n),
        ) if !this_turn && r.then.is_empty() => {
            let n = digits(*n);
            let by = match from {
                Some(Affects::Objects(f)) => filter_a(f, ctx.card),
                Some(f) => damage_source(f, ctx),
                None => "a source".to_string(),
            };
            let to = match to {
                Affects::Objects(f) => filter_a(f, ctx.card),
                other => affects_to(other, ctx),
            };
            format!("If {by} would deal damage to {to}, prevent {n} of that damage.")
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
                // "enchanted creature" is one object (Fylgja).
                Affects::Objects(f) if f.enchanted => filter_phrase(f, ctx.card, false),
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
        // "The next time damage would be dealt to target creature this
        // turn, destroy that creature instead." (Kill-Suit Cultist).
        (Replaces::Damage { from: None, to, .. }, Outcome::DestroyNextTime) => {
            let to = affects_to(to, ctx);
            let turn = if this_turn { " this turn" } else { "" };
            format!(
                "The next time damage would be dealt to {to}{turn}, destroy that creature instead."
            )
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
        // `replace damage permanent you control of chosen type would deal:
        // double`: "Double all damage that sources you control of the
        // chosen type would deal." (Collective Inferno), "Double all damage
        // equipped creature would deal."
        (
            Replaces::Damage {
                from: Some(f),
                to: Affects::Any,
                combat: None,
                wording: DamageWording::WouldDeal,
            },
            Outcome::Double,
        ) => match f {
            Affects::Objects(g) => {
                let p = filter_phrase(g, ctx.card, true).replacen("permanents", "sources", 1);
                format!("Double all damage that {p} would deal.")
            }
            other => format!("Double all damage {} would deal.", affects_to(other, ctx)),
        },
        (
            Replaces::Damage {
                from, to, combat, ..
            },
            outcome @ (Outcome::Double | Outcome::PlusN { .. } | Outcome::PlusCount(_)),
        ) => {
            let kind = match combat {
                Some(true) => "combat damage",
                Some(false) => "noncombat damage",
                None => "damage",
            };
            let to_text = affects_to(to, ctx);
            // "to that permanent or player", but "to you", "to this
            // creature" (Goldnight Castigator).
            let that = match to_text.strip_prefix("a ") {
                Some(rest) => format!(" to that {rest}"),
                None => format!(" to {to_text}"),
            };
            // "it deals double that damage to that permanent or player",
            // "it deals that much damage plus 2 instead" (Torbran):
            // "an opponent or a permanent an opponent controls" is not
            // named again (Twinflame Tyrant).
            let deals = |that: &str| match outcome {
                Outcome::PlusN { n, to_that } => {
                    let that = if *to_that { that } else { "" };
                    format!("that much damage plus {n}{that}")
                }
                Outcome::PlusCount(_) => "that much damage plus X".to_string(),
                _ if matches!(to, Affects::OpponentsAnd(_)) => format!("double that {kind}"),
                _ => format!("double that {kind}{that}"),
            };
            let text = match from {
                None => format!(
                    "If a source would deal {kind} to {to_text}, it deals {} instead.",
                    deals(&that)
                ),
                // "If a source you control would deal damage to a
                // permanent or player, it deals double that damage to that
                // permanent or player instead." (Angrath's Marauders); "If
                // a Giant source you control ..." (Calamity Bearer).
                Some(Affects::Objects(f))
                    if f.types.is_empty() && f.any_types.is_empty() && !f.article =>
                {
                    let p = filter_a(f, ctx.card);
                    let by = if p.contains("permanent") {
                        p.replacen("permanent", "source", 1)
                    } else {
                        p.replacen(" you control", " source you control", 1)
                    };
                    format!(
                        "If {by} would deal {kind} to {to_text}, it deals {} instead.",
                        deals(&that)
                    )
                }
                Some(f) => {
                    let by = match f {
                        Affects::Objects(f) => filter_a(f, ctx.card),
                        other => affects_to(other, ctx),
                    };
                    format!(
                        "If {by} would deal {kind} to {to_text}, it deals {} instead.",
                        deals("")
                    )
                }
            };
            // "..., instead it deals that much damage plus X, where X is
            // Hawkeye's power." (Hawkeye, Young Avenger).
            match outcome {
                Outcome::PlusCount(Num::Count(i)) => {
                    let what = ctx.card.counts.get(usize::from(*i)).map_or_else(
                        || "X".to_string(),
                        |c| count_phrase(&c.expr, ctx.card, false),
                    );
                    let what = match what.as_str() {
                        "its power" => format!("{}'s power", this_ref(ctx.card)),
                        _ => what,
                    };
                    text.replace(
                        " it deals that much damage plus X instead.",
                        &format!(" instead it deals that much damage plus X, where X is {what}."),
                    )
                }
                _ => text,
            }
        }
        // "If a source you control would deal damage to an opponent,
        // prevent that damage and each opponent mills that many cards."
        // (The Mindskinner): the source named, and what follows joined to
        // the sentence.
        (
            Replaces::Damage {
                from: Some(Affects::Objects(f)),
                to,
                combat: None,
                ..
            },
            Outcome::Prevent,
        ) if !this_turn && f.types.is_empty() && f.any_types.is_empty() && r.then.len() == 1 => {
            let p = filter_a(f, ctx.card);
            let by = if p.contains("permanent") {
                p.replacen("permanent", "source", 1)
            } else {
                p.replacen(" you control", " source you control", 1)
            };
            let to = affects_to(to, ctx);
            let rest = sentences(&r.then, ctx);
            let rest = lowercase_first(rest.trim_end_matches('.'));
            return format!("If {by} would deal damage to {to}, prevent that damage and {rest}.");
        }
        (Replaces::Damage { to, combat, .. }, Outcome::Prevent) => {
            let kind = match combat {
                Some(true) => "combat damage",
                Some(false) => "noncombat damage",
                None => "damage",
            };
            // "If damage would be dealt to another creature you control,
            // prevent that damage." (Vigor): one object at a time.
            let to = match to {
                Affects::Objects(f) => filter_a(f, ctx.card),
                other => affects_to(other, ctx),
            };
            format!("If {kind} would be dealt to {to}, prevent that damage.")
        }
        (Replaces::Damage { to, .. }, Outcome::RedirectToAttached) => {
            let to = affects_to(to, ctx);
            format!(
                "All damage that would be dealt to {to} is dealt to enchanted creature instead."
            )
        }
        (Replaces::LoseLife { whose, your_turn }, outcome) => {
            let (subj, obj) = match whose {
                Who::You => ("you", "you lose"),
                Who::Any => ("a player", "that player loses"),
                Who::Opponents => ("an opponent", "they lose"),
            };
            let when = if *your_turn { " during your turn" } else { "" };
            match outcome {
                Outcome::Double => {
                    format!("If {subj} would lose life{when}, {obj} twice that much life instead.")
                }
                _ => format!("If {subj} would lose life{when}, something else happens instead."),
            }
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
                Outcome::PlusOne => {
                    format!("If {subj} would gain life, {obj} gains that much life plus 1 instead.")
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
        (Replaces::FilteredToGraveyard(f), outcome) => {
            let instead = match outcome {
                Outcome::Instead(mtg_core::state::Zone::Exile) => "exile it instead",
                _ => "something else happens instead",
            };
            format!(
                "If {} card would be put into a graveyard from anywhere, {instead}.",
                filter_a(f, ctx.card)
            )
        }
        // CR 614.1a, 111.1: "If one or more tokens would be created under
        // your control, twice that many of those tokens are created
        // instead." (Mondrak); "If an effect would create ..." (Parallel
        // Lives).
        (
            Replaces::CreateTokens {
                creature,
                effect,
                artifact,
                ..
            },
            Outcome::Double,
        ) => {
            let c = match (*creature, *artifact) {
                (true, _) => "creature ",
                (false, true) => "artifact ",
                _ => "",
            };
            if *effect {
                format!(
                    "If an effect would create one or more {c}tokens under your control, it creates twice that many of those tokens instead."
                )
            } else {
                format!(
                    "If one or more {c}tokens would be created under your control, twice that many of those tokens are created instead."
                )
            }
        }
        // "those tokens plus a 1/1 green Frog creature token are created
        // instead" (Quina).
        (
            Replaces::CreateTokens {
                creature,
                artifact,
                you,
                ..
            },
            Outcome::PlusToken { that_many, .. },
        ) => {
            let c = match (*creature, *artifact) {
                (true, _) => "creature ",
                (false, true) => "artifact ",
                _ => "",
            };
            let n = if *that_many { Num::ThatMuch } else { Num::N(1) };
            let desc =
                token_description(r.token.as_deref().unwrap_or_default(), n, false, false, ctx);
            // "If you would create one or more artifact tokens, instead
            // create those tokens plus an additional Map token."
            if *you {
                let rest = desc
                    .strip_prefix("an ")
                    .or_else(|| desc.strip_prefix("a "))
                    .unwrap_or(&desc);
                return format!(
                    "If you would create one or more {c}tokens, instead create those tokens plus an additional {rest}."
                );
            }
            format!(
                "If one or more {c}tokens would be created under your control, those tokens plus {desc} are created instead."
            )
        }
        // "If one or more artifact tokens would be created under your
        // control, that many 5/5 red Dragon creature tokens with flying are
        // created instead." (Draconic Visitor).
        (
            Replaces::CreateTokens {
                creature, artifact, ..
            },
            Outcome::TokensInstead { .. },
        ) => {
            let c = match (*creature, *artifact) {
                (true, _) => "creature ",
                (false, true) => "artifact ",
                _ => "",
            };
            let desc = token_description(
                r.token.as_deref().unwrap_or_default(),
                Num::ThatMuch,
                false,
                false,
                ctx,
            );
            format!(
                "If one or more {c}tokens would be created under your control, {desc} are created instead."
            )
        }
        // The card names itself: the ability works in every zone, where it
        // isn't "this creature" (CR 113.6b).
        (Replaces::ThisToGraveyard, Outcome::ShuffleIntoLibrary) => {
            let this = short_name(ctx.card);
            format!(
                "If {this} would be put into a graveyard from anywhere, reveal {this} and shuffle it into its owner's library instead."
            )
        }
        (Replaces::PlusCounters { on }, Outcome::Double) => format!(
            "If one or more +1/+1 counters would be put on {}, twice that many +1/+1 counters are put on that {} instead.",
            filter_a(on, ctx.card),
            if on.types.contains(&CardType::Creature) {
                "creature"
            } else {
                "permanent"
            }
        ),
        (Replaces::AnyCounters { on, by }, Outcome::Double | Outcome::PlusOne) => {
            use mtg_core::card::CountersBy;
            let on_text = filter_a(on, ctx.card);
            let many = if r.outcome == Outcome::Double {
                "twice that many"
            } else {
                "that many plus one"
            };
            match by {
                // Doubling Season.
                CountersBy::Effect => format!(
                    "If an effect would put one or more counters on {on_text}, it puts {many} of those counters on that permanent instead."
                ),
                // Doc Samson.
                CountersBy::You => format!(
                    "If you would put one or more counters on {on_text}, put {many} of each of those kinds of counters on that permanent instead."
                ),
                // Loading Zone.
                CountersBy::Any => format!(
                    "If one or more counters would be put on {on_text}, {many} of each of those kinds of counters are put on it instead."
                ),
            }
        }
        (Replaces::PlusCounters { on }, Outcome::PlusOne) => format!(
            "If one or more +1/+1 counters would be put on {}, that many plus one +1/+1 counters are put on it instead.",
            filter_a(on, ctx.card)
        ),
        (Replaces::Enters, Outcome::EnterTapped) => {
            format!("{} enters tapped.", capitalize(&this_ref(ctx.card)))
        }
        (Replaces::Enters, Outcome::EnterTappedUnlessPay(n)) => format!(
            "As {} enters, you may pay {n} life. If you don't, it enters tapped.",
            this_ref(ctx.card)
        ),
        (Replaces::Enters, Outcome::EnterPrepared) => {
            format!("{} enters prepared.", capitalize(&this_ref(ctx.card)))
        }
        // "enters with a +1/+1 counter on it for each other creature you
        // control", "a number of +1/+1 counters on it equal to ...".
        (
            Replaces::Enters,
            Outcome::EnterWithCounters {
                kind,
                n: Num::Count(i),
            },
        ) if ctx.card.counts.get(usize::from(*i)).is_some() => {
            let count = &ctx.card.counts[usize::from(*i)];
            let this = capitalize(&this_ref(ctx.card));
            if count.style == crate::ast::CountStyle::Each {
                // "enters with two +1/+1 counters on it for each color of
                // mana spent to cast it".
                let (n, expr) = match &count.expr {
                    crate::ast::CountExpr::Times(n, inner) => (*n, &**inner),
                    e => (1, e),
                };
                format!(
                    "{this} enters with {} on {} {}.",
                    counters(Num::N(n), *kind),
                    it_obj(ctx.card),
                    count_phrase(expr, ctx.card, true)
                )
            } else if count.style == crate::ast::CountStyle::Where {
                // "enters with X +1/+1 counters on it, where X is ...".
                format!(
                    "{this} enters with X {} counters on {}, where X is {}.",
                    counter_name(*kind),
                    it_obj(ctx.card),
                    count_phrase(&count.expr, ctx.card, false)
                )
            } else {
                format!(
                    "{this} enters with a number of {} counters on {} equal to {}.",
                    counter_name(*kind),
                    it_obj(ctx.card),
                    count_phrase(&count.expr, ctx.card, false)
                )
            }
        }
        // "Each Dragon you control enters with an additional +1/+1 counter
        // on it", "Other creatures you control enter with an additional
        // +1/+1 counter on them for each opponent who lost life this turn".
        (Replaces::OthersEnter { what, each }, Outcome::EnterWithCounters { kind, n }) => {
            let (n, per) = match n {
                Num::Count(i) => match ctx.card.counts.get(usize::from(*i)) {
                    Some(count) if count.style == crate::ast::CountStyle::Each => {
                        let (n, expr) = match &count.expr {
                            crate::ast::CountExpr::Times(n, inner) => (*n, &**inner),
                            e => (1, e),
                        };
                        // Counted before it enters: "for each Angel you
                        // already control" (Giada).
                        let p = count_phrase(expr, ctx.card, true);
                        let p = match p.strip_suffix(" you control") {
                            Some(head) => format!("{head} you already control"),
                            None => p,
                        };
                        (Num::N(n), format!(" {p}"))
                    }
                    _ => (*n, String::new()),
                },
                n => (*n, String::new()),
            };
            let what_text = if *each {
                format!(
                    "Each {} enters",
                    controller_first(what, filter_phrase(what, ctx.card, false))
                )
            } else {
                format!(
                    "{} enter",
                    capitalize(&controller_first(what, filter_phrase(what, ctx.card, true)))
                )
            };
            let c = counters(n, *kind);
            let c = match c.split_once(' ') {
                Some((a, rest)) if a == "a" || a == "an" => format!("an additional {rest}"),
                Some((a, rest)) => format!("{a} additional {rest}"),
                None => c,
            };
            format!(
                "{what_text} with {c} on {}{per}.",
                if *each { "it" } else { "them" }
            )
        }
        // "enters with a number of stun counters on it equal to three
        // minus X".
        (
            Replaces::Enters,
            Outcome::EnterWithCounters {
                kind,
                n: n @ Num::NMinusX(_),
            },
        ) => format!(
            "{} enters with a number of {} counters on {} equal to {}.",
            capitalize(&this_ref(ctx.card)),
            counter_name(*kind),
            it_obj(ctx.card),
            count_words(*n, "a")
        ),
        (Replaces::Enters, Outcome::EnterWithCounters { kind, n }) => format!(
            "{} enters with {} on {}.",
            capitalize(&this_ref(ctx.card)),
            counters(*n, *kind),
            it_obj(ctx.card)
        ),
        // "If this creature would die, put it on top of its owner's
        // library instead." (Gravebane Zombie; "on the bottom", Nissa's
        // Chosen).
        (Replaces::Dies { to }, Outcome::IntoLibrary { top }) => {
            let who = affects_to(to, ctx);
            let when = if this_turn { " this turn" } else { "" };
            let place = if *top { "on top" } else { "on the bottom" };
            format!("If {who} would die{when}, put it {place} of its owner's library instead.")
        }
        (Replaces::Dies { to }, Outcome::Instead(zone)) => {
            let who = match to {
                Affects::Target(i) if ctx.it_target == Some(*i) => "it".to_string(),
                Affects::Target(i) => {
                    ctx.target_mentioned[*i] = true;
                    // "If that creature or planeswalker would die this
                    // turn" (Fanged Flames): the target's own two nouns.
                    if matches!(
                        ctx.targets.get(*i).map(Target::base),
                        Some(Target::Spec(TargetSpec::CreatureOrPlaneswalker))
                    ) {
                        "that creature or planeswalker".to_string()
                    } else if (0..ctx.targets.len())
                        .any(|j| j != *i && ctx.target_noun[j] == ctx.target_noun[*i])
                    {
                        // "If the creature an opponent controls would die
                        // this turn" (Unnatural Aggression): another target
                        // has the same noun.
                        let phrase = ctx.target_phrase_of(*i);
                        format!("the {}", phrase.strip_prefix("target ").unwrap_or(&phrase))
                    } else {
                        format!("that {}", ctx.target_noun[*i])
                    }
                }
                // "If a creature dealt damage by this creature this turn
                // would die" (Frostwielder).
                Affects::Objects(f) => filter_a(f, ctx.card),
                other => affects_to(other, ctx),
            };
            let when = if this_turn { " this turn" } else { "" };
            let instead = if *zone == mtg_core::state::Zone::Hand {
                "return it to its owner's hand"
            } else {
                "exile it"
            };
            format!("If {who} would die{when}, {instead} instead.")
        }
        // CR 702.89a: an Aura's keyword ability.
        (Replaces::Destroy { .. }, Outcome::UmbraArmor) => "Umbra armor".to_string(),
        // A static regeneration (CR 701.15, 614.8): "If this creature
        // would be destroyed, regenerate it." (Clergy of the Holy Nimbus).
        (Replaces::Destroy { to }, Outcome::Regenerate) if !this_turn => {
            let to = affects_to(to, ctx);
            format!("If {to} would be destroyed, regenerate it.")
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
        let then = sentences(&r.then, ctx);
        // CR 615.5: after a prevention, `that much` is "the damage
        // prevented this way".
        if matches!(
            r.outcome,
            Outcome::Prevent | Outcome::PreventNext(_) | Outcome::PreventNextTime
        ) {
            out.push_str(&prevented_this_way(&then));
        } else {
            out.push_str(&then);
        }
    }
    out
}

/// "You gain that much life." after a prevention: "You gain life equal to
/// the damage prevented this way."; "Put that many +1/+1 counters on that
/// creature.": "Put a +1/+1 counter on that creature for each 1 damage
/// prevented this way."
fn prevented_this_way(s: &str) -> String {
    const WAY: &str = "the damage prevented this way";
    let s = s
        .replace("gain that much life", &format!("gain life equal to {WAY}"))
        .replace(
            "gains that much life",
            &format!("gains life equal to {WAY}"),
        )
        .replace(
            "draw that many cards",
            &format!("draw cards equal to {WAY}"),
        )
        .replace(
            "draws that many cards",
            &format!("draws cards equal to {WAY}"),
        );
    s.split_inclusive(". ")
        .map(|sentence| {
            let (body, end) = match (sentence.strip_suffix(". "), sentence.strip_suffix('.')) {
                (Some(b), _) => (b, ". "),
                (None, Some(b)) => (b, "."),
                (None, None) => (sentence, ""),
            };
            if let Some(rest) = body.strip_prefix("Put that many ")
                && let Some((kind, on)) = rest.split_once(" counters on ")
            {
                format!("Put a {kind} counter on {on} for each 1 damage prevented this way{end}")
            } else {
                format!("{body}{end}")
            }
        })
        .collect()
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
        // "Nontoken artifacts you control are lands in addition to their
        // other types." (Toph, the First Metalbender).
        Mod::AddTypes(types) if plural_form && types.len() == 1 => format!(
            "are {} in addition to their other types",
            pluralize(type_noun(types[0]))
        ),
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
        Mod::SetTypes(types) => format!(
            "{} {}",
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
        // "Enchanted land is snow."
        Mod::AddSupertype(s) => format!(
            "{} {}",
            v("is", "are"),
            match s {
                Supertype::Basic => "basic",
                Supertype::Legendary => "legendary",
                Supertype::Snow => "snow",
                Supertype::World => "world",
            }
        ),
        // "are Oozes in addition to their other types".
        Mod::AddSubtype(s) if plural_form => {
            format!("are {} in addition to their other types", pluralize(s))
        }
        Mod::AddSubtype(s) => format!(
            "{} {} in addition to its other types",
            v("is", "are"),
            with_article(s)
        ),
        Mod::AddChosenType => format!(
            "{} the chosen type in addition to {} other types",
            v("is", "are"),
            v("its", "their")
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
                    // "can attack as though they didn't have defender";
                    // "Creatures enchanted player controls attack each
                    // combat if able".
                    r.replace("though it didn't", "though they didn't")
                        .replace("attacks each combat", "attack each combat")
                } else {
                    r
                });
            }
            list(&parts)
        }
        Mod::RemoveAllAbilities => format!("{} all abilities", v("loses", "lose")),
        Mod::Goaded => format!("{} goaded", v("is", "are")),
        Mod::AllCreatureTypes { every: true } => format!("{} every creature type", v("is", "are")),
        Mod::AllCreatureTypes { every: false } => format!("{} all creature types", v("is", "are")),
        Mod::NoCreatureTypes => format!("{} all creature types", v("loses", "lose")),
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
            } else if cs.is_empty() {
                "is colorless".to_string()
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

fn cda_count_text(c: &crate::ast::CountExpr, card: &Card) -> String {
    use crate::ast::CountExpr;
    match c {
        // "twice the number of instant and sorcery cards in your
        // graveyard".
        CountExpr::Times(2, inner) => format!("twice {}", cda_count_text(inner, card)),
        CountExpr::Permanents(f) => format!(
            "the number of {}{}",
            filter_phrase(f, card, true),
            if f.who.is_none() {
                " on the battlefield"
            } else {
                ""
            }
        ),
        other => count_phrase(other, card, false),
    }
}

/// `this: add subtype Cleric, add subtype Rogue, ...`: creature types the
/// card itself has besides its printed ones, in every zone ("is also a
/// Cleric, Rogue, Warrior, and Wizard").
pub(crate) fn also_subtypes(filter: &Filter, mods: &[Mod]) -> Option<Vec<String>> {
    if filter.relation != Relation::This || mods.len() < 2 {
        return None;
    }
    mods.iter()
        .map(|m| match m {
            Mod::AddSubtype(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

#[allow(clippy::too_many_lines)] // one case per kind of static
fn static_text(filter: &Filter, mods: &[Mod], card: &Card, lookup: &dyn Lookup) -> String {
    // "Untap all permanents you control during each other player's untap
    // step." (Seedborn Muse), "Untap each creature you control with a
    // +1/+1 counter on it during ...".
    if let [Mod::AddKeywords(ks)] = mods
        && ks.as_slice() == [Keyword::UntapsDuringOthers]
        && filter.relation != Relation::This
        && !filter.enchanted
    {
        let what = if filter.each {
            format!("each {}", filter_phrase(filter, card, false))
        } else {
            format!("all {}", filter_phrase(filter, card, true))
        };
        // "all green and/or blue creatures you control".
        let what = if filter.and_or {
            what.replacen(" or ", " and/or ", 1)
        } else {
            what
        };
        return format!("Untap {what} during each other player's untap step.");
    }
    // Granted by an Aura, it is quoted: 'Enchanted land has "Untap this
    // land during each other player's untap step."' (Urban Burgeoning).
    if let [Mod::AddKeywords(ks)] = mods
        && ks.as_slice() == [Keyword::UntapsDuringOthers]
        && filter.enchanted
    {
        let who = filter_phrase(filter, card, false);
        let noun = who.rsplit(' ').next().unwrap_or("permanent").to_string();
        return format!(
            "{} has \"Untap this {noun} during each other player's untap step.\"",
            capitalize(&who)
        );
    }
    // Soulbond (CR 702.95b): "As long as this creature is paired with
    // another creature, both creatures have flying." / "each of those
    // creatures gets +1/+1." / "each of those creatures has "..."."
    if let Some(named) = filter.paired {
        let mut f = filter.clone();
        f.paired = None;
        f.relation = Relation::This;
        let this = capitalize(&this_ref(card));
        let body = static_text(&f, mods, card, lookup);
        let rest = body.strip_prefix(&format!("{this} ")).unwrap_or(&body);
        let keywords = mods.iter().all(|m| matches!(m, Mod::AddKeywords(_)));
        let tail = match rest.strip_prefix("has ") {
            Some(r) if keywords => format!("both creatures have {r}"),
            _ => format!("each of those creatures {rest}"),
        };
        let who = if named {
            short_name(card).to_string()
        } else {
            this_ref(card)
        };
        return format!("As long as {who} is paired with another creature, {tail}");
    }
    // Characteristic-defining abilities name the card.
    if let Some(Mod::CdaPt {
        count,
        sets,
        plus_toughness,
        noun,
    }) = mods.iter().find(|m| matches!(m, Mod::CdaPt { .. }))
    {
        let owner = if card.kind == Kind::Token {
            "This token's".to_string()
        } else if *noun {
            format!("{}'s", capitalize(&this_ref(card)))
        } else {
            format!("{}'s", short_name(card))
        };
        let n = cda_count_text(count, card);
        return if *sets == mtg_core::card::CdaSets::Power {
            format!("{owner} power is equal to {n}.")
        } else if *sets == mtg_core::card::CdaSets::Toughness {
            format!("{owner} toughness is equal to {n}.")
        } else if *plus_toughness == 0 {
            format!("{owner} power and toughness are each equal to {n}.")
        } else {
            format!(
                "{owner} power is equal to {n} and its toughness is equal to that number plus {plus_toughness}."
            )
        };
    }
    // "Stonework Packbeast is also a Cleric, Rogue, Warrior, and Wizard."
    if let Some(subs) = also_subtypes(filter, mods) {
        return format!(
            "{} is also {}.",
            short_name(card),
            with_article(&list(&subs))
        );
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
    } else if filter.each && filter.or.is_some() {
        // "Each land and Ally you control has ..." (Great Divide Guide).
        (
            format!(
                "Each {}",
                filter_phrase(filter, card, false).replacen(" or ", " and ", 1)
            ),
            false,
        )
    } else if (each && !filter.plural) || filter.each {
        (
            format!("Each {}", filter_phrase(filter, card, false)),
            false,
        )
    } else {
        let bare = filter.who.is_none()
            && !filter.enchanted_player
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
            // "Creature tokens get +1/+1." (Leyline of the Meek).
            && filter.status.token.is_none()
            && filter.status.commander.is_none()
            && filter.relation == Relation::Any;
        let p = filter_phrase(filter, card, true);
        if bare || filter.all {
            (format!("All {p}"), true)
        } else {
            (capitalize(&p), true)
        }
    };
    // "This artifact is a 5/5 Golem artifact creature" (Rusted Relic):
    // base P/T, subtypes, and types as one noun phrase.
    if let [
        Mod::SetPt {
            power: PtValue::Fixed(p),
            toughness: PtValue::Fixed(t),
        },
        subs @ ..,
        Mod::AddTypes(types),
    ] = mods
        && !plural_form
        && subs.iter().all(|m| matches!(m, Mod::AddSubtype(_)))
    {
        let mut words = vec![format!("{p}/{t}")];
        words.extend(subs.iter().filter_map(|m| match m {
            Mod::AddSubtype(s) => Some(s.clone()),
            _ => None,
        }));
        words.extend(types.iter().map(|t| type_noun(*t).to_string()));
        return format!("{subject} is a {}.", words.join(" "));
    }
    // "Enchanted land is a 2/2 blue Elemental creature with flying. It's
    // still a land." (Wind Zendikon): base P/T, a set color, subtypes, and
    // a type added to a land as one noun phrase.
    if let [
        Mod::SetPt {
            power: PtValue::Fixed(p),
            toughness: PtValue::Fixed(t),
        },
        Mod::SetColors(colors),
        subs @ ..,
        Mod::AddTypes(types),
    ]
    | [
        Mod::SetPt {
            power: PtValue::Fixed(p),
            toughness: PtValue::Fixed(t),
        },
        Mod::SetColors(colors),
        subs @ ..,
        Mod::AddTypes(types),
        Mod::AddKeywords(_),
    ] = mods
        && !plural_form
        && filter.enchanted
        && enchanted_noun(card) == "land"
        && !types.contains(&CardType::Land)
        && subs.iter().all(|m| matches!(m, Mod::AddSubtype(_)))
    {
        let mut words = vec![format!("{p}/{t}")];
        let color_words: Vec<String> = colors.iter().map(|c| color_name(*c).to_string()).collect();
        words.push(list(&color_words));
        words.extend(subs.iter().filter_map(|m| match m {
            Mod::AddSubtype(s) => Some(s.clone()),
            _ => None,
        }));
        words.extend(types.iter().map(|t| type_noun(*t).to_string()));
        let with = match mods.last() {
            Some(Mod::AddKeywords(ks)) => format!(" with {}", keywords_list(ks)),
            _ => String::new(),
        };
        return format!(
            "{subject} is a {}{with}. It's still a land.",
            words.join(" ")
        );
    }
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
        if stripped.is_empty() && plural_form {
            // "Activated abilities of artifacts, creatures, and
            // planeswalkers can't be activated." (Clarion Conqueror, the
            // Null Rod wording).
            let clause = activation.unwrap_or_default();
            let what = subject.strip_prefix("All ").unwrap_or(&subject);
            let what = match what.rfind(", or ") {
                Some(i) => format!("{}, and {}", &what[..i], &what[i + 5..]),
                None => what.replacen(" or ", " and ", 1),
            };
            let rest = &clause[4..];
            let rest = rest.strip_prefix("activated abilities ").unwrap_or(rest);
            return format!("Activated abilities of {} {rest}.", lowercase_first(&what));
        }
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
                // "Enchanted creature gets +3/+0 and can only attack
                // alone." (Errantry)
                if only_attacks_alone(a) {
                    return "can only attack alone".to_string();
                }
                // "has ward {2}" (CR 702.21): a keyword ability, bare.
                if let Some(k) = granted_keyword(a, card) {
                    return format!("{} {k}", if plural_form { "have" } else { "has" });
                }
                // "Other creatures are Food artifacts ... and have \"...
                // Sacrifice this permanent: ...\"" (Ygra): a holder given
                // another card type is a permanent.
                if mods.iter().any(|m| matches!(m, Mod::AddTypes(_))) {
                    let have = if plural_form { "have" } else { "has" };
                    return format!("{have} {}", quoted_as(a, card, &[], lookup));
                }
                let noun = if filter.types == [CardType::Land]
                    || (filter.enchanted && enchanted_noun(card) == "land")
                {
                    CardType::Land
                } else if filter.types == [CardType::Enchantment] {
                    // "Other enchantments have 'At the beginning of your
                    // upkeep, sacrifice this enchantment ...'".
                    CardType::Enchantment
                } else if filter.types == [CardType::Artifact] {
                    // "Artifacts you control have '{2}, Sacrifice this
                    // artifact: ...'".
                    CardType::Artifact
                } else if filter.types == [CardType::Planeswalker] {
                    // "Planeswalkers you control have \"[−4]: This
                    // planeswalker deals ...\"" (Way of the Warlord).
                    CardType::Planeswalker
                } else if filter.types.is_empty()
                    && !filter.enchanted
                    && !filter.subtypes.is_empty()
                    && filter.who.is_none()
                {
                    // "All Slivers have \"{2}: Regenerate this permanent.\""
                    // (any permanent with the subtype).
                    return format!(
                        "{} {}",
                        if plural_form { "have" } else { "has" },
                        quoted_as(a, card, &[], lookup)
                    );
                } else {
                    CardType::Creature
                };
                let have = if plural_form { "have" } else { "has" };
                return format!("{have} {}", quoted(a, card, noun, lookup));
            }
            // "gets +1/+1 and all creatures able to block it do so" (Gift
            // of the Deity).
            if let Mod::AddKeywords(ks) = m
                && ks.as_slice() == [Keyword::Lure]
            {
                return lure_sentence("it", false);
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
    // "has defender, and loses all other abilities": abilities it gains
    // before losing the rest are kept (`compile` applies the loss first).
    for (i, m) in mods.iter().enumerate() {
        // "It can't attack and loses all abilities": a restriction is no
        // ability of its own.
        if *m == Mod::RemoveAllAbilities
            && mods[..i].iter().any(|m| match m {
                Mod::AddKeywords(ks) => !ks.iter().all(|k| matches!(k, Keyword::CantAttack)),
                Mod::AddAbility(_) => true,
                _ => false,
            })
        {
            parts[i] = parts[i].replacen(" all abilities", " all other abilities", 1);
        }
    }
    // "has menace and \"Whenever ...\"", "has trample and ward {1}": one
    // "has" for both.
    // "gets +1/+1 and has reach and \"Whenever ...\"" (Web-Shooters): a
    // quoted ability joins too, except after a new base P/T ("is a Dragon
    // with base power and toughness 4/4, flying, and \"...\"").
    let base_pt = mods.iter().any(|m| matches!(m, Mod::SetPt { .. }));
    for i in 1..parts.len() {
        let keyword_grant =
            matches!(&mods[i], Mod::AddAbility(a) if granted_keyword(a, card).is_some());
        // A third "has" joins the two before it (Swashbuckler's Whip).
        let after_has = parts[i - 1].starts_with("has ")
            || (parts[i - 1].starts_with('\u{1}')
                && parts[i].starts_with("has \"")
                && i >= 2
                && parts[i - 2].starts_with("has "));
        if (parts[i].starts_with("has \"") || keyword_grant) && after_has {
            // A bare keyword joins the "has" before it as one part ("gets
            // +4/+4 and has trample and ward {1}"), marked to merge below.
            let mark = if keyword_grant || !base_pt {
                "\u{1}"
            } else {
                ""
            };
            let mut rest = parts[i]["has ".len()..].to_string();
            // "has protection from instants and from sorceries" (Sword
            // of Wealth and Power): one "protection" for both.
            let prev = parts[i - 1].trim_start_matches('\u{1}');
            let prev = prev.strip_prefix("has ").unwrap_or(prev);
            if keyword_grant
                && prev.starts_with("protection from ")
                && !prev.contains(" and ")
                && let Some(q) = rest.strip_prefix("protection ")
            {
                rest = q.to_string();
            }
            parts[i] = format!("{mark}{rest}");
        }
    }
    // "is a Golem creature with base power and toughness 5/4 in addition
    // to its other types" (Zoetic Glyph), "are Food artifacts in addition
    // to their other types" (Ygra): an added subtype and the card types
    // added after it are one noun phrase.
    for i in 0..mods.len().saturating_sub(1) {
        if let (Mod::AddSubtype(sub), Mod::AddTypes(types)) = (&mods[i], &mods[i + 1]) {
            let mut words = vec![sub.clone()];
            words.extend(types.iter().map(|t| type_noun(*t).to_string()));
            parts[i] = "\u{2}".to_string();
            parts[i + 1] = if plural_form {
                let last = words.pop().expect("nonempty");
                words.push(pluralize(&last));
                format!("are {} in addition to their other types", words.join(" "))
            } else {
                format!(
                    "is {} in addition to its other types",
                    with_article(&words.join(" "))
                )
            };
        }
    }
    // "is a Spider Hero in addition to its other types" (Spider-Suit):
    // exactly two creature types added together are one phrase.
    let run = mods
        .iter()
        .filter(|m| matches!(m, Mod::AddSubtype(_)))
        .count();
    if run == 2 && !plural_form {
        for i in 0..mods.len().saturating_sub(1) {
            if let (Mod::AddSubtype(a), Mod::AddSubtype(b)) = (&mods[i], &mods[i + 1]) {
                parts[i] = format!(
                    "is {} in addition to its other types",
                    with_article(&format!("{a} {b}"))
                );
                parts[i + 1] = "\u{2}".to_string();
            }
        }
    }
    // "is a black Zombie in addition to its other colors and types": a
    // color and a subtype added together are one phrase (the second part
    // is dropped below).
    for i in 0..mods.len().saturating_sub(1) {
        if let (Mod::AddColors(cs), Mod::AddSubtype(sub)) = (&mods[i], &mods[i + 1]) {
            let colors: Vec<String> = cs.iter().map(|c| color_name(*c).to_string()).collect();
            let what = with_article(&format!("{} {sub}", list(&colors)));
            let verb = if plural_form { "are" } else { "is" };
            parts[i] = format!("{verb} {what} in addition to its other colors and types");
            parts[i + 1] = "\u{2}".to_string();
        }
    }
    // "is a blue Frog creature": colors, a creature type, and card types
    // that replace the others are one noun phrase (in the last one's
    // place).
    let mut i = 0;
    while i < mods.len() {
        let colors = match &mods[i] {
            Mod::SetColors(cs) => Some(cs),
            _ => None,
        };
        let j = i + usize::from(colors.is_some());
        let Some(Mod::SetSubtype(sub)) = mods.get(j) else {
            i += 1;
            continue;
        };
        let types = match mods.get(j + 1) {
            Some(Mod::SetTypes(ts)) => Some(ts),
            _ => None,
        };
        if colors.is_none() && types.is_none() {
            i += 1;
            continue;
        }
        let mut words: Vec<String> = Vec::new();
        if let Some(cs) = colors {
            let names: Vec<String> = cs.iter().map(|c| color_name(*c).to_string()).collect();
            words.push(list(&names));
        }
        words.push(sub.clone());
        if let Some(ts) = types {
            words.extend(ts.iter().map(|t| type_noun(*t).to_string()));
        }
        let last = j + usize::from(types.is_some());
        let verb = if plural_form { "are" } else { "is" };
        for p in &mut parts[i..last] {
            *p = "\u{2}".to_string();
        }
        parts[last] = format!("{verb} {}", with_article(&words.join(" ")));
        i = last + 1;
    }
    // "is a Treefolk with base power and toughness 0/4", "is a creature
    // with base power and toughness 4/4 in addition to its other types":
    // a type and the base P/T that follows it are one phrase.
    for i in (0..mods.len().saturating_sub(1)).rev() {
        let (Mod::SetSubtype(_) | Mod::AddTypes(_) | Mod::AddSubtype(_) | Mod::SetTypes(_)) =
            &mods[i]
        else {
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
    parts.retain(|p| p != "\u{2}");
    let mut i = 1;
    while i < parts.len() {
        if parts[i].starts_with('\u{1}') {
            // "has reach, \"{2}, {T}: Tap target artifact or creature,\"
            // and \"{8}, {T}: Discover 10.\"" (Swashbuckler's Whip): three
            // or more are a list, the comma inside a quote.
            let mut run = vec![parts[i - 1].clone()];
            while i < parts.len()
                && let Some(rest) = parts[i].strip_prefix('\u{1}')
            {
                run.push(rest.to_string());
                parts.remove(i);
            }
            parts[i - 1] = if run.len() == 2 {
                format!("{} and {}", run[0], run[1])
            } else {
                let last = run.pop().expect("nonempty");
                let head: Vec<String> = run
                    .iter()
                    .map(
                        |p| match p.strip_suffix(".\"").or_else(|| p.strip_suffix('"')) {
                            Some(q) => format!("{q},\""),
                            None => format!("{p},"),
                        },
                    )
                    .collect();
                format!("{} and {last}", head.join(" "))
            };
        } else {
            i += 1;
        }
    }
    // "Enchanted creature is a Turtle with base power and toughness 0/1.
    // It can't attack and loses all abilities." (Retro-Mutation): after
    // the new identity, what else it has and loses is a sentence of its
    // own.
    let split = parts.len() >= 3
        && parts[0].contains(" with base power and toughness ")
        && !plural_form
        && mods.last() == Some(&Mod::RemoveAllAbilities);
    // "is a Dragon with base power and toughness 4/4, flying, and
    // \"{R}: ...\"": what it has joins the "with".
    if !split
        && parts.len() >= 3
        && parts[0].contains(" with base power and toughness ")
        && parts[1..]
            .iter()
            .all(|p| p.starts_with("has ") || p.starts_with('"'))
    {
        for p in &mut parts[1..] {
            if let Some(rest) = p.strip_prefix("has ") {
                *p = rest.to_string();
            }
        }
    }
    let mut body = if split {
        format!("{subject} {}. It {}", parts[0], parts[1..].join(" and "))
    } else {
        // A quoted ability before the end of the list drops its period:
        // "has \"Whenever ..., draw a card,\" and is a Rogue", "has
        // \"... until end of turn\" and is a Wizard" (Job select
        // Equipment).
        format!("{subject} {}", list(&parts))
            .replace(".\", ", ",\" ")
            .replace(".\" and ", "\" and ")
    };
    if let Some(clause) = activation {
        // Oracle joins it with ", and", except after "doesn't untap during
        // its controller's untap step" (Encrust).
        let untap_only =
            matches!(mods, [Mod::AddKeywords(ks)] if ks.as_slice() == [Keyword::DoesntUntap]);
        // After "can't attack, block, or crew Vehicles" it is a sentence
        // of its own (Intercessor's Arrest).
        let crew = mods
            .iter()
            .any(|m| matches!(m, Mod::AddKeywords(ks) if ks.contains(&Keyword::CantCrew)));
        if crew {
            body.push_str(". ");
            body.push_str(&capitalize(clause));
        } else {
            body.push_str(if untap_only { " and " } else { ", and " });
            body.push_str(clause);
        }
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
        // "can't be blocked by enchanted creatures or enchantment
        // creatures", "by Knights or Walls" (but colors "and/or").
        BlockRule::NotBy(f) if f.or.is_some() && f.colors.is_none() => format!(
            "can't be blocked by {}{turn}",
            filter_phrase(f, card, true).replacen(" and ", " or ", 1)
        ),
        BlockRule::NotBy(f) => format!("can't be blocked by {}{turn}", pl(f)),
        BlockRule::OnlyBy(f) => format!("can't be blocked{turn} except by {}", pl(f)),
        BlockRule::AtMostOne => format!("can't be blocked by more than one creature{turn}"),
        // Printed as a whole sentence by `ability_text`; granted, the
        // same rule in the attacker's words.
        BlockRule::NotByLesserPower => format!(
            "can't be blocked{turn} by creatures with power less than {}'s power",
            this_ref(card)
        ),
        BlockRule::BlocksOnly(f) => format!("can block only {}{turn}", pl(f)),
        BlockRule::CantBlock(f) => format!("can't block {}{turn}", pl(f)),
        BlockRule::MinBlockers(n) => format!(
            "can't be blocked{turn} except by {} or more creatures",
            count_words(Num::N(u32::from(*n)), "one")
        ),
        BlockRule::BlocksShadow { had } => format!(
            "can block creatures with shadow as though {}{turn}",
            if *had {
                "it had shadow"
            } else {
                "they didn't have shadow"
            }
        ),
        BlockRule::BlocksLandwalk => format!(
            "can block creatures with landwalk abilities as though they didn't have those abilities{turn}"
        ),
        BlockRule::BlocksAdditional(n) => format!(
            "can block an additional {} creatures {}",
            count_words(Num::N(u32::from(*n)), "one"),
            if this_turn {
                "this turn"
            } else {
                "each combat"
            }
        ),
    }
}

/// `can't attack if creature also attacks`: "can only attack alone".
fn only_attacks_alone(a: &Ability) -> bool {
    matches!(a, Ability::AttackUnless {
        cond: StaticCond::OthersInCombat { filter, at_least: 1, also: true, block: false },
        attack: true,
        block: false,
        negated: true,
    } if filter.types == [CardType::Creature] && Filter { types: Vec::new(), ..filter.clone() } == Filter::default())
}

/// An ability another grants, in quotes, about the object that has it
/// ("\"{T}: This creature deals 1 damage to any target.\"").
/// "Protection from multicolored", "Protection from artifacts" (CR
/// 702.16): a protection ability from a quality other than a color.
/// "an additional land", "two additional lands" (CR 305.2).
fn additional_lands(n: u32) -> String {
    if n == 1 {
        "an additional land".to_string()
    } else {
        format!("{} additional lands", count_words(Num::N(n), "one"))
    }
}

fn protection_text(f: &Filter, card: &Card) -> String {
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
                mtg_core::card::ColorCount::Colored => "one or more colors",
            }
        )
    } else if *f
        == (Filter {
            supertypes: vec![Supertype::Snow],
            ..Filter::default()
        })
    {
        // "Protection from snow" (Ronom Hulk): from snow sources.
        "Protection from snow".to_string()
    } else {
        format!("Protection from {}", filter_phrase(f, card, true))
    }
}

/// "snow forestwalk", "snow landwalk", "nonbasic landwalk" (CR 702.14a):
/// the land's qualities, then its basic land type or "land", then "walk".
fn landwalk_text(f: &Filter, card: &Card) -> String {
    let mut g = f.clone();
    let kind = if let [s] = g.subtypes.as_slice() {
        let k = s.to_lowercase();
        g.subtypes.clear();
        k
    } else {
        "land".to_string()
    };
    g.types.retain(|t| *t != CardType::Land);
    let qual = if g == Filter::default() {
        String::new()
    } else {
        let p = filter_phrase(&g, card, false);
        format!("{} ", p.strip_suffix(" permanent").unwrap_or(&p))
    };
    format!("{qual}{kind}walk")
}

/// A keyword ability granted bare rather than quoted: "ward {2}", "toxic
/// 1", "protection from artifacts".
fn granted_keyword(a: &Ability, card: &Card) -> Option<String> {
    match a {
        Ability::Protection(f) => {
            let t = protection_text(f, card);
            Some(format!("p{}", &t[1..]))
        }
        Ability::Landwalk(f) => Some(landwalk_text(f, card)),
        // "has hexproof from monocolored".
        Ability::HexproofFrom(f) => Some(format!(
            "hexproof{}",
            &protection_text(f, card)["Protection".len()..]
        )),
        // 'have "Ward—Pay 2 life."': quoted.
        Ability::Mechanic(
            crate::ast::Mechanic::WardCost(_) | crate::ast::Mechanic::WardAlts(_),
        ) => None,
        _ => crate::format::granted_mechanic(a),
    }
}

fn quoted(a: &Ability, card: &Card, noun: CardType, lookup: &dyn Lookup) -> String {
    quoted_as(a, card, &[noun], lookup)
}

/// A quoted ability whose holder has these card types ("this permanent"
/// with none).
fn quoted_as(a: &Ability, card: &Card, types: &[CardType], lookup: &dyn Lookup) -> String {
    let mut holder = card.blank();
    holder.name.clone_from(&card.name);
    holder.types = types.to_vec();
    holder.groups.clone_from(&card.groups);
    holder.counts.clone_from(&card.counts);
    let text = ability_text(&holder, a, lookup);
    // A quoted loyalty ability brackets its cost: "[−4]: ..." (CR 606.4
    // prints the bare symbol only on a planeswalker's own text).
    if let Ability::Activated { cost, .. } = a
        && cost.loyalty.is_some()
        && let Some((c, rest)) = text.split_once(": ")
    {
        return format!("\"[{c}]: {rest}\"");
    }
    // 'Enchanted creature has "Cumulative upkeep {1}."' (Mana Chains): a
    // quoted keyword line ends its sentence inside the quote.
    if matches!(
        a,
        Ability::Mechanic(crate::ast::Mechanic::CumulativeUpkeep(_))
    ) {
        return format!("\"{text}.\"");
    }
    format!("\"{text}\"")
}

/// "seven or more cards are in your graveyard", "you control an Island".
#[allow(clippy::too_many_lines)] // one arm per condition
/// An ability that works from its card's graveyard (activated there, or a
/// graveyard trigger): its card is "this card from your graveyard".
fn works_from_graveyard(a: Option<&Ability>) -> bool {
    match a {
        Some(Ability::Activated { cost, .. }) => cost.from_graveyard,
        Some(Ability::Triggered { from_graveyard, .. }) => *from_graveyard,
        _ => false,
    }
}

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
            let what = if *at_least == 1
                && f.relation == Relation::Other
                && f.or.as_ref().is_some_and(|a| a.relation != f.relation)
            {
                // "you control another Merfolk or an Island".
                filter_a(&f, card)
            } else if *at_least == 1 && f.relation == Relation::Other {
                // "you control another Goblin".
                let mut g = f.clone();
                crate::format::relation_any(&mut g);
                format!("another {}", filter_phrase(&g, card, false))
            } else if *at_least == 1 {
                each_with_article(&f, card)
            } else if f.or.is_some() && f.or.as_ref().is_some_and(|a| a.or.is_none()) {
                // "seven or more lands and/or Treefolk".
                let p = filter_phrase(&f, card, true).replacen(" and ", " and/or ", 1);
                format!("{} {p}", or_more(*at_least))
            } else {
                format!("{} {}", or_more(*at_least), filter_phrase(&f, card, true))
            };
            format!("you control {what}")
        }
        StaticCond::YouControlOneOrMore(filter) => {
            let mut f = filter.clone();
            f.who = None;
            format!("you control one or more {}", filter_phrase(&f, card, true))
        }
        StaticCond::YourTurn | StaticCond::DuringYourTurn => "it's your turn".to_string(),
        StaticCond::NotYourTurn => "it's not your turn".to_string(),
        StaticCond::CastDuringMain => "you cast this spell during your main phase".to_string(),
        StaticCond::ControlledAsCast(f) => format!(
            "you controlled {} as you cast this spell",
            filter_a(f, card)
        ),
        StaticCond::AnyPlayerLifeAtMost(n) => format!("a player has {n} or less life"),
        StaticCond::SourceCountersOneOrMore(kind) => format!(
            "{} has one or more {} counters on it",
            this_ref(card),
            kind.name()
        ),
        StaticCond::SourceCountersThere { kind, n } => format!(
            "there are {} or more {} counters on this {}",
            count_words(Num::N(*n), "one"),
            kind.name(),
            self_noun(card)
        ),
        StaticCond::SourceCounters { kind, n, at_least } => {
            if *at_least && *n == 1 {
                // "this creature has a +1/+1 counter on it".
                let c = kind.name();
                format!("{} has {} {c} counter on it", this_ref(card), article(c))
            } else if *at_least && *kind == CounterKind::Any {
                // "As long as this creature has three or more counters on
                // it": every kind.
                format!(
                    "{} has {} or more counters on it",
                    this_ref(card),
                    count_words(Num::N(*n), "one")
                )
            } else if *at_least {
                // "As long as Vadmir has four or more +1/+1 counters on
                // it" (a legendary card names itself).
                format!(
                    "{} has {} or more {} counters on it",
                    this_ref(card),
                    count_words(Num::N(*n), "one"),
                    kind.name()
                )
            } else if *n > 0 {
                // "it has two or fewer oil counters on it".
                format!(
                    "{} has {} or fewer {} counters on it",
                    this_ref(card),
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
            if *at_least == 0 {
                format!(
                    "defending player controls no {}",
                    filter_phrase(&f, card, true)
                )
            } else if *at_least == 1 {
                format!("defending player controls {}", each_with_article(&f, card))
            } else {
                format!(
                    "defending player controls {} {}",
                    or_more(*at_least),
                    filter_phrase(&f, card, true)
                )
            }
        }
        // "any player controls a black permanent" (Knight of Grace).
        StaticCond::OnBattlefield {
            filter,
            at_least: 1,
        } if filter.who == Some(Who::Any) => {
            let mut f = filter.clone();
            f.who = None;
            format!(
                "any player controls {}",
                with_article(&filter_phrase(&f, card, false))
            )
        }
        // "your opponents control three or more creatures" (Lashwhip
        // Predator).
        StaticCond::OnBattlefield { filter, at_least }
            if filter.who == Some(Who::Opponents) && filter.your_opponents =>
        {
            let mut f = filter.clone();
            f.who = None;
            f.your_opponents = false;
            if *at_least == 1 {
                format!(
                    "your opponents control {}",
                    with_article(&filter_phrase(&f, card, false))
                )
            } else {
                format!(
                    "your opponents control {} {}",
                    or_more(*at_least),
                    filter_phrase(&f, card, true)
                )
            }
        }
        // "If three or more creatures are attacking" (Lethargy Trap); "as
        // long as two or more creatures are blocking it" (Rampaging
        // Cyclops).
        StaticCond::OnBattlefield { filter, at_least }
            if *at_least > 1
                && filter.types == [CardType::Creature]
                && ((filter.attacking
                    && (Filter {
                        types: Vec::new(),
                        attacking: false,
                        ..filter.clone()
                    }) == Filter::default())
                    || filter_phrase(filter, card, true)
                        == format!("creatures blocking {}", this_ref(card))) =>
        {
            let verb = if filter.attacking {
                "attacking".to_string()
            } else {
                format!("blocking {}", it_obj(card))
            };
            format!("{} creatures are {verb}", or_more(*at_least))
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
            "at least {} other {} {}",
            count_words(Num::N(*at_least), "one"),
            if *at_least == 1 {
                "creature"
            } else {
                "creatures"
            },
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
        StaticCond::YouControlAtMost { filter, n } => {
            let mut f = filter.clone();
            f.who = None;
            format!(
                "you control {} or fewer {}",
                count_words(Num::N(*n), "one"),
                filter_phrase(&f, card, true)
            )
        }
        StaticCond::YouControlAtLeast { filter, n } => {
            let mut f = filter.clone();
            f.who = None;
            format!(
                "you control at least {} {}",
                count_words(Num::N(*n), "one"),
                filter_phrase(&f, card, *n != 1)
            )
        }
        // "you control an Urza's Mine and an Urza's Power-Plant".
        StaticCond::YouControlEach(filters) => {
            // "If you control creatures named Mine Worker and Tower Worker"
            // (Power Plant Worker).
            let named: Option<Vec<String>> = filters
                .iter()
                .map(|f| {
                    let mut f = f.clone();
                    f.who = None;
                    filter_phrase(&f, card, false)
                        .strip_prefix("creature named ")
                        .map(str::to_string)
                })
                .collect();
            if let Some(names) = named {
                return format!("you control creatures named {}", names.join(" and "));
            }
            let all: Vec<String> = filters
                .iter()
                .map(|f| {
                    let mut f = f.clone();
                    f.who = None;
                    with_article(&filter_phrase(&f, card, false))
                })
                .collect();
            format!("you control {}", all.join(" and "))
        }
        StaticCond::AttackedThisTurn => "you attacked this turn".to_string(),
        StaticCond::AttackedLastTurn => "it attacked during your last turn".to_string(),
        StaticCond::CreatureDiedThisTurn => "a creature died this turn".to_string(),
        StaticCond::OpponentDamagedThisTurn => "an opponent was dealt damage this turn".to_string(),
        StaticCond::OpponentLostLifeThisTurn => "an opponent lost life this turn".to_string(),
        StaticCond::PermanentLeftThisTurn => {
            "a permanent left the battlefield under your control this turn".to_string()
        }
        StaticCond::Escaped => "it escaped".to_string(),
        StaticCond::Kicked if has_gift(card) => "the gift was promised".to_string(),
        StaticCond::Kicked => "it was kicked".to_string(),
        StaticCond::NotKicked if has_gift(card) => "the gift wasn't promised".to_string(),
        StaticCond::NotKicked => format!("this {} wasn't kicked", kicked_noun(card)),
        StaticCond::ThatKicked => "that spell was kicked".to_string(),
        StaticCond::Impending => {
            "this permanent's impending cost was paid and it has a time counter on it".to_string()
        }
        StaticCond::And(a, b) => format!(
            "{} and {}",
            static_cond_text(a, card),
            static_cond_text(b, card)
        ),
        StaticCond::CastBoth(both) => format!(
            "you've cast both {} and {} this turn",
            with_article(&spell_noun(&card_filter_phrase(&both.0, card))),
            with_article(&spell_noun(&card_filter_phrase(&both.1, card)))
        ),
        StaticCond::CreaturesDied { yours: true, .. } => {
            "a creature died under your control this turn".to_string()
        }
        StaticCond::CreaturesDied { n, .. } => {
            format!("{} creatures died this turn", or_more(*n))
        }
        StaticCond::OpponentMoreLands => "an opponent controls more lands than you".to_string(),
        StaticCond::OpponentHasMore(w) => match w {
            mtg_core::card::MoreWhat::Creatures => "an opponent controls more creatures than you",
            mtg_core::card::MoreWhat::Life => "an opponent has more life than you",
            mtg_core::card::MoreWhat::CardsInHand => "an opponent has more cards in hand than you",
        }
        .to_string(),
        StaticCond::Void => {
            "a nonland permanent left the battlefield this turn or a spell was warped this turn"
                .to_string()
        }
        StaticCond::NotYourMainPhase => "it isn't your main phase".to_string(),
        StaticCond::NotCastFromHand => format!(
            "{} was cast from anywhere other than your hand",
            if is_instant_or_sorcery(card) {
                "this spell"
            } else {
                "it"
            }
        ),
        StaticCond::WasCast => "you cast it".to_string(),
        StaticCond::PutAnyCounterThisTurn => {
            "you put a counter on a creature this turn".to_string()
        }
        StaticCond::AttackedWithAtLeast(f, n) => format!(
            "you attacked with {} {} this turn",
            or_more(*n),
            filter_phrase(f, card, true)
        ),
        StaticCond::TotalToughness(n) => {
            format!("creatures you control have total toughness {n} or greater")
        }
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
        StaticCond::Teamwork => "this spell was cast using teamwork".to_string(),
        // "If a Dragon was beheld": the card's behold cost's filter.
        StaticCond::Beheld => {
            let behold = card.additional.as_ref().and_then(|c| c.behold.as_ref());
            if let Some(b) = behold.filter(|b| b.reveal) {
                // The older wording (CR 601.2b).
                return format!(
                    "you revealed {} or controlled {} as you cast this spell",
                    card_filter_a(&b.filter, card),
                    filter_a(&b.filter, card)
                );
            }
            let what = behold.map_or_else(|| "a card".to_string(), |b| filter_a(&b.filter, card));
            format!("{what} was beheld")
        }
        StaticCond::NoManaSpent => "it wasn't cast or no mana was spent to cast it".to_string(),
        StaticCond::XIs { n, at_least } => {
            format!("X is {n} or {}", if *at_least { "more" } else { "less" })
        }
        StaticCond::WebSlung(p) => format!(
            "{} cast using web-slinging",
            match p {
                1 => "he was",
                2 => "she was",
                3 => "they were",
                _ => "it was",
            }
        ),
        StaticCond::MoreVotes { word, ties, .. } => format!(
            "{word} gets more votes{}",
            if *ties { " or the vote is tied" } else { "" }
        ),
        StaticCond::ProwlPaid if is_spell(card) => "this spell's prowl cost was paid".to_string(),
        StaticCond::ProwlPaid => "its prowl cost was paid".to_string(),
        StaticCond::SneakPaid(_) if is_spell(card) => {
            "this spell's sneak cost was paid".to_string()
        }
        StaticCond::SneakPaid(3) => format!("{}'s sneak cost was paid", this_ref(card)),
        StaticCond::SneakPaid(p) => format!(
            "{} sneak cost was paid",
            match p {
                1 => "his",
                2 => "her",
                _ => "its",
            }
        ),
        StaticCond::EvidenceCollected => "evidence was collected".to_string(),
        StaticCond::AdditionalCostPaid => "this spell's additional cost was paid".to_string(),
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
        StaticCond::NoCardInHand(f) => {
            format!("you have no {} in hand", cards_phrase(f, card, true))
        }
        StaticCond::HandFewerThan(n) => format!(
            "you have fewer than {} cards in hand",
            count_words(Num::N(*n), "one")
        ),
        StaticCond::ActivatedThisTurn(n) => format!(
            "this ability has been activated {} times this turn",
            or_more(*n)
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
        StaticCond::AttackedBattleThisTurn => "it attacked a battle this turn".to_string(),
        StaticCond::AttackedWith(f) => {
            format!("you attacked with {} this turn", filter_a(f, card))
        }
        StaticCond::EnteredWith {
            filter: f,
            had: false,
        } => format!(
            "{} entered the battlefield under your control this turn",
            filter_a(f, card)
        ),
        StaticCond::EnteredWith {
            filter: f,
            had: true,
        } => format!(
            "you had {} enter the battlefield under your control this turn",
            filter_a(f, card)
        ),
        StaticCond::UnlockedDoors(n) => format!(
            "there are {} or more unlocked doors among Rooms you control",
            count_words(Num::N(*n), "one")
        ),
        StaticCond::Enchanted => "it's enchanted".to_string(),
        // "if he's equipped" (Whiplash).
        StaticCond::Equipped => format!("{}'s equipped", it_subj(card)),
        StaticCond::Monstrous => "it's monstrous".to_string(),
        StaticCond::Suspected(true) => "it's suspected".to_string(),
        StaticCond::Suspected(false) => "it's not suspected".to_string(),
        StaticCond::OpponentLifeAtMost(n) => format!("an opponent has {n} or less life"),
        StaticCond::MoreLifeThanOpponent => "you have more life than an opponent".to_string(),
        StaticCond::BasicLandTypes(n) => format!(
            "there are {} basic land types among lands you control",
            or_more(*n)
        ),
        StaticCond::YouDontControl(filter) => {
            let mut f = filter.clone();
            f.who = None;
            format!(
                "you don't control {}",
                with_article(&filter_phrase(&f, card, false))
            )
        }
        StaticCond::LifeVsStarting {
            at_least, wording, ..
        } => match wording {
            0 => "your life total is greater than your starting life total".to_string(),
            1 => "your life total is greater than or equal to your starting life total".to_string(),
            2 => format!(
                "your life total is at least {at_least} greater than your starting life total"
            ),
            3 => format!("you have at least {at_least} life more than your starting life total"),
            _ => "your life total is less than your starting life total".to_string(),
        },
        StaticCond::YourLife { n, at_least } => format!(
            "you have {n} or {} life",
            if *at_least { "more" } else { "less" }
        ),
        StaticCond::GraveyardCards {
            filter,
            at_least,
            wording,
        } => {
            use crate::ast::GraveyardWording as W;
            let one = with_article(&format!("{} card", card_filter_phrase(filter, card)));
            let have = match wording {
                W::ThereIs => return format!("there is {one} in your graveyard"),
                W::Theres => return format!("there's {one} in your graveyard"),
                W::IsIn => return format!("{one} is in your graveyard"),
                W::AreIn => {
                    return format!("{} cards are in your graveyard", or_more(*at_least));
                }
                W::YouHave => true,
                W::ThereAre => false,
            };
            let mut p = card_filter_phrase(filter, card);
            // Spell mastery: "instant and/or sorcery cards".
            if filter.or.is_some() || !filter.any_types.is_empty() {
                p = p.replace(" or ", " and/or ");
            }
            format!(
                "{} {} {p} cards in your graveyard",
                if have { "you have" } else { "there are" },
                or_more(*at_least)
            )
        }
        StaticCond::Not(a) => format!("not ({})", static_cond_text(a, card)),
        StaticCond::FlashPay(n) => format!("you pay {{{n}}} more to cast it"),
        StaticCond::Or(a, b) => format!(
            "{} or {}",
            static_cond_text(a, card),
            static_cond_text(b, card)
        ),
        StaticCond::OrIf(a, b) => format!(
            "{} or if {}",
            static_cond_text(a, card),
            static_cond_text(b, card)
        ),
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
        // "As long as this creature has a counter on it" (Demon Wall).
        StaticCond::Is(f)
            if f.status.counter == Some(mtg_core::card::CounterQual::Any)
                && (Filter {
                    status: mtg_core::card::StatusQuals {
                        counter: None,
                        ..f.status
                    },
                    ..f.clone()
                }) == Filter::default() =>
        {
            "it has a counter on it".to_string()
        }
        // "When this creature dies, if it's not a token, ...".
        StaticCond::Is(f)
            if *f
                == (Filter {
                    status: mtg_core::card::StatusQuals {
                        token: Some(false),
                        ..Default::default()
                    },
                    ..Filter::default()
                }) =>
        {
            "it's not a token".to_string()
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
            // "if its power is exactly 20" (Amalia Benavides Aguirre).
            let exactly = if cmp == mtg_core::card::Cmp::Exactly {
                "exactly "
            } else {
                ""
            };
            format!("{} power is {exactly}{n}{}", its_poss(card), cmp_words(cmp))
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
        // "if it isn't a token" (Sphinx of False Conclusions).
        StaticCond::IsntToken => "it isn't a token".to_string(),
        StaticCond::Isnt(f) => format!(
            "it isn't {}",
            or_list(
                &f.types
                    .iter()
                    .map(|t| with_article(type_noun(*t)))
                    .collect::<Vec<_>>()
            )
        ),
        StaticCond::Is(f) => format!("it's {}", is_predicate(f, card)),
        StaticCond::Modified => "it's modified".to_string(),
        StaticCond::AttackingAlone => "it's attacking alone".to_string(),
        StaticCond::AttackedOrBlocked => {
            format!("{} attacked or blocked this combat", this_ref(card))
        }
        StaticCond::AttackedTotalPower(n) => {
            format!("you attacked with creatures with total power {n} or greater this combat")
        }
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
        StaticCond::AttachedIs(f) => {
            let noun = if card.subtypes.iter().any(|s| s == "Equipment") {
                "equipped creature".to_string()
            } else {
                format!("enchanted {}", enchanted_noun(card))
            };
            // "As long as you control enchanted creature".
            if *f
                == (Filter {
                    who: Some(Who::You),
                    ..Default::default()
                })
            {
                return format!("you control {noun}");
            }
            // "As long as equipped creature's toughness is greater than its
            // power" (Bark of Doran).
            let mut bare = f.clone();
            bare.status.toughness_over_power = false;
            if f.status.toughness_over_power && bare == Filter::default() {
                return format!("{noun}'s toughness is greater than its power");
            }
            // "As long as enchanted creature isn't legendary" (Stunning
            // Strike).
            if *f
                == (Filter {
                    not_supertypes: vec![Supertype::Legendary],
                    ..Default::default()
                })
            {
                return format!("{noun} isn't legendary");
            }
            // "if enchanted creature is a Wolf or Werewolf" (Howl of the
            // Hunt): two creature types share the article.
            if let Some(alt) = &f.or
                && f.subtypes.len() == 1
                && alt.subtypes.len() == 1
                && alt.or.is_none()
                && f.subtypes[0] == "Wolf"
                && alt.subtypes[0] == "Werewolf"
                && (Filter {
                    subtypes: Vec::new(),
                    or: None,
                    ..f.clone()
                }) == Filter::default()
                && (Filter {
                    subtypes: Vec::new(),
                    ..(**alt).clone()
                }) == Filter::default()
            {
                return format!(
                    "{noun} is {} or {}",
                    with_article(&f.subtypes[0]),
                    alt.subtypes[0]
                );
            }
            let pred = is_predicate(f, card);
            // "if enchanted creature has toxic" (Maze's Mantle).
            match pred.strip_prefix("a permanent with ") {
                Some(k) => format!("{noun} has {k}"),
                None => format!("{noun} is {pred}"),
            }
        }
        StaticCond::DrawnThisTurn(n) => {
            format!("you've drawn {} cards this turn", or_more(*n))
        }
        StaticCond::CastThisTurn {
            filter: f,
            at_least,
            other,
        } => {
            // "a spell with mana value 4 or greater".
            let noun = spell_noun(&card_filter_phrase(f, card));
            if *at_least <= 1 {
                let what = if *other {
                    format!("another {noun}")
                } else {
                    with_article(&noun)
                };
                format!("you've cast {what} this turn")
            } else {
                format!(
                    "you've cast {} {}{noun}s this turn",
                    or_more(*at_least),
                    if *other { "other " } else { "" }
                )
            }
        }
        StaticCond::CastFrom(z) => match z {
            mtg_core::state::Zone::Hand => "you cast it from your hand".to_string(),
            z => format!(
                "{} was cast from {}",
                if is_instant_or_sorcery(card) {
                    "this spell"
                } else {
                    "it"
                },
                // "if it was cast from your graveyard" (a permanent's own
                // trigger, Rocket-Powered Goblin Glider).
                if *z == mtg_core::state::Zone::Graveyard && !is_instant_or_sorcery(card) {
                    "your graveyard"
                } else if *z == mtg_core::state::Zone::Graveyard {
                    "a graveyard"
                } else {
                    "exile"
                }
            ),
        },
        StaticCond::GainedLifeThisTurn { or_lost } => format!(
            "you gained {}life this turn",
            if *or_lost { "or lost " } else { "" }
        ),
        StaticCond::LostLifeThisTurn { gained: false } => "you've lost life this turn".to_string(),
        StaticCond::LostLifeThisTurn { gained: true } => {
            "you gained and lost life this turn".to_string()
        }
        StaticCond::DescendedThisTurn => "you descended this turn".to_string(),
        StaticCond::SpellNumber(n) => {
            format!("it's the {} spell you cast this turn", ordinal_word(*n))
        }
        StaticCond::SurveilledThisTurn => "you've surveilled this turn".to_string(),
        StaticCond::ScriedOrSurveilledThisTurn => {
            "you've scried or surveilled this turn".to_string()
        }
        StaticCond::CreaturesEnteredThisTurn { n, opponent: false } => format!(
            "{} creatures entered the battlefield under your control this turn",
            or_more(u32::from(*n))
        ),
        StaticCond::CreaturesEnteredThisTurn { n, opponent: true } => format!(
            "an opponent had {} creatures enter the battlefield under their control this turn",
            or_more(u32::from(*n))
        ),
        StaticCond::CounterPutThisTurn => {
            "a +1/+1 counter was put on a permanent under your control this turn".to_string()
        }
        StaticCond::FaceDownEnteredThisTurn => {
            "a face-down creature entered the battlefield under your control this turn".to_string()
        }
        StaticCond::CreatureEnteredThisTurn { other } => format!(
            "{} creature entered the battlefield under your control this turn",
            if *other { "another" } else { "a" }
        ),
        StaticCond::NonlandEnteredThisTurn(n) => format!(
            "{} nonland permanents entered the battlefield under your control this turn",
            or_more(*n)
        ),
        StaticCond::PutIntoGraveyardThisTurn(t) => {
            let names: Vec<&str> = t.iter().map(|c| type_noun(*c)).collect();
            let list = names.join(" or ");
            format!(
                "{} was put into a graveyard from the battlefield this turn",
                with_article(&list)
            )
        }
        StaticCond::DiscardedThisTurn => "you've discarded a card this turn".to_string(),
        StaticCond::Night => "it's night".to_string(),
        StaticCond::LifeGainedThisTurn(n) => {
            format!("you've gained {n} or more life this turn")
        }
        StaticCond::CrimeThisTurn => "you've committed a crime this turn".to_string(),
        StaticCond::YourMainPhase => "it's your main phase".to_string(),
        StaticCond::YourCombatPhase => "it's your combat phase".to_string(),
        StaticCond::FirstCombatPhase => "it's the first combat phase of the turn".to_string(),
        StaticCond::NthResolution { n, again: false } => format!(
            "this is the {} time this ability has resolved this turn",
            ordinal_word(u32::from(*n))
        ),
        StaticCond::NthResolution { n, again: true } => {
            format!("it's the {} time", ordinal_word(u32::from(*n)))
        }
        StaticCond::OpponentNoncombatDamagedThisTurn { was } => format!(
            "an opponent {} dealt noncombat damage this turn",
            if *was { "was" } else { "has been" }
        ),
        StaticCond::PutCounterThisTurn => {
            "you've put one or more +1/+1 counters on a creature this turn".to_string()
        }
        StaticCond::LeftGraveyardThisTurn => "a card left your graveyard this turn".to_string(),
        StaticCond::SacrificedThisTurn(t) if t.is_empty() => {
            "you sacrificed a permanent this turn".to_string()
        }
        StaticCond::SacrificedThisTurn(t) => {
            let names: Vec<&str> = t.iter().map(|c| type_noun(*c)).collect();
            format!(
                "you've sacrificed {} this turn",
                with_article(&names.join(" or "))
            )
        }
        StaticCond::CardToGraveyardThisTurn(t) => {
            let names: Vec<&str> = t.iter().map(|c| type_noun(*c)).collect();
            format!(
                "{} card was put into your graveyard from anywhere this turn",
                with_article(&names.join(" or "))
            )
        }
        StaticCond::MoreCardsInHand => "you have more cards in hand than each opponent".to_string(),
        StaticCond::OpponentPoison(1) => "an opponent is poisoned".to_string(),
        StaticCond::OpponentPoison(n) => {
            format!("an opponent has {} poison counters", or_more(*n))
        }
        StaticCond::OwnCardInExile => "you own a card in exile".to_string(),
        StaticCond::ExiledWith(n) => format!(
            "there are {} cards exiled with {}",
            or_more(*n),
            this_ref(card)
        ),
        StaticCond::TypesExiledWith(n) => format!(
            "there are {} card types among cards exiled with {}",
            or_more(*n),
            this_ref(card)
        ),
        StaticCond::Monarch => "you're the monarch".to_string(),
        StaticCond::CompletedDungeon => "you've completed a dungeon".to_string(),
        StaticCond::CityBlessing => "you have the city's blessing".to_string(),
        StaticCond::EnduringStory => "you have an enduring story".to_string(),
        StaticCond::NoEnduringStory => "you don't have an enduring story".to_string(),
        StaticCond::MaxSpeed => "you have max speed".to_string(),
        StaticCond::DidntCastThisTurn => "you didn't cast a spell this turn".to_string(),
        StaticCond::NoSpellsCastThisTurn { from_hand } => format!(
            "you haven't cast a spell{} this turn",
            if *from_hand { " from your hand" } else { "" }
        ),
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
        T::YourMain => "during your main phase",
        T::DeclareAttackers => "during the declare attackers step",
        T::BeforeBlockers => "during combat before blockers are declared",
        T::AfterBlockers => "during combat after blockers are declared",
        T::AttackedThisStep => {
            "during the declare attackers step and only if you've been attacked this step"
        }
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
    let p = target_predicate(f, card);
    // "as long as it's a Detective you control" (Burden of Proof).
    if f.who == Some(Who::You) && !p.contains("you control") {
        return format!("{p} you control");
    }
    p
}

/// Whether a condition is about the source ("it's attacking"), printed
/// "this creature is ..." when the sentence's subject is something else.
fn about_source(c: &StaticCond) -> bool {
    matches!(
        c,
        StaticCond::Attacking
            | StaticCond::Enchanted
            | StaticCond::Equipped
            | StaticCond::Monstrous
            | StaticCond::Suspected(_)
            | StaticCond::Is(_)
            | StaticCond::IsntToken
            | StaticCond::Isnt(_)
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
                    let about = match c {
                        StaticCond::Or(left, right) | StaticCond::OrIf(left, right) => {
                            about_source(left) || about_source(right)
                        }
                        c => about_source(c),
                    };
                    if about {
                        t = t.replace("it's", &format!("this {noun} is"));
                        // "Activate only if Kitsa's power is 3 or greater".
                        t = t.replace("its power", &format!("{}'s power", this_ref(card)));
                        // "Activate only if this land entered this turn".
                        t = t.replace("it entered", &format!("this {noun} entered"));
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
    // CR 602.1.
    match cost.activators {
        Some(Who::Any) => out.push_str(" Any player may activate this ability."),
        Some(Who::Opponents) => out.push_str(" Only your opponents may activate this ability."),
        _ => {}
    }
    out
}

/// Whether an equip ability's cost is more than mana ("Equip—{2}, Pay 2
/// life."), so it prints after a dash.
/// " This ability costs {1} less to activate for each ..." (an activated
/// ability's `reduced`), or nothing.
fn reduce_text(cost: &crate::ast::Cost, card: &Card) -> String {
    match cost.reduce.as_deref() {
        Some((by, crate::ast::ReduceBy::Each(e))) => format!(
            " This ability costs {by} less to activate {}.",
            count_phrase(e, card, true)
        ),
        Some((by, crate::ast::ReduceBy::If(StaticCond::DuringYourTurn))) => {
            format!(" This ability costs {by} less to activate during your turn.")
        }
        Some((by, crate::ast::ReduceBy::Targets(f))) => format!(
            " This ability costs {by} less to activate if it targets {}.",
            filter_a(f, card)
        ),
        Some((by, crate::ast::ReduceBy::PerTarget(p))) => match p {
            mtg_core::card::PerTarget::Colors => format!(
                " This ability costs {by} less to activate for each color of the creature it targets."
            ),
            mtg_core::card::PerTarget::Counters(k) => format!(
                " This ability costs {by} less to activate for each {} counter on the creature it targets.",
                k.name()
            ),
        },
        Some((by, crate::ast::ReduceBy::If(c))) => format!(
            " This ability costs {by} less to activate if {}.",
            static_cond_text(c, card)
        ),
        None => String::new(),
    }
}

pub(crate) fn equip_full_cost(cost: &crate::ast::Cost) -> bool {
    cost.order
        .iter()
        .any(|i| !matches!(i, crate::ast::CostItem::Mana))
}

/// A static ability's text with its condition: "As long as <cond>, <text>"
/// or "<text> as long as <cond>." ("During your turn, ..." for the turn).
fn conditional_static(
    body: &str,
    c: &StaticCond,
    first: bool,
    card: &Card,
    subtype_first: bool,
) -> String {
    let mut cond = static_cond_text(c, card);
    // "as long as it's enchanted" about the subject; "as long as this
    // creature is enchanted" when the subject is something else.
    if about_source(c) && !body.starts_with(&capitalize(&this_ref(card))) {
        cond = cond.replacen(
            "it has a counter",
            &format!("{} has a counter", this_ref(card)),
            1,
        );
        cond = cond.replace("it's", &format!("{} is", this_ref(card)));
        cond = cond.replace("it entered", &format!("{} entered", this_ref(card)));
        // "as long as this Equipment's power ...": not the subject's.
        if !first {
            cond = cond.replace("its power", &format!("{}'s power", this_ref(card)));
            cond = cond.replace("its toughness", &format!("{}'s toughness", this_ref(card)));
        }
    }
    if first {
        let this = format!("{} ", capitalize(&this_ref(card)));
        let rest = if matches!(
            c,
            StaticCond::SourceCounters { .. } | StaticCond::SourceCountersOneOrMore(_)
        ) && body.starts_with(&this)
        {
            // "As long as this creature has four or more oil counters on
            // it, it gets +2/+0."
            format!("it {}", &body[this.len()..])
        } else if starts_with_name(body, card) {
            body.to_string()
        } else if matches!(
            c,
            StaticCond::ExiledWith(_) | StaticCond::TypesExiledWith(_)
        ) && body.starts_with(&this)
        {
            format!("it {}", &body[this.len()..])
        } else if about_source(c) && body.starts_with(&this) {
            // "As long as this creature is equipped, it gets +1/+1."
            cond = cond.replacen(
                "it has a counter",
                &format!("{} has a counter", this_ref(card)),
                1,
            );
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
        } else if subtype_first {
            // "..., Insects and Spiders you control get +1/+1": a subtype
            // keeps its capital.
            body.to_string()
        } else {
            lowercase_first(body)
        };
        if *c == StaticCond::DuringYourTurn {
            format!("During your turn, {rest}")
        } else if *c == StaticCond::NotYourTurn {
            format!("During turns other than yours, {rest}")
        } else {
            format!("As long as {cond}, {rest}")
        }
    } else {
        let stem = body.strip_suffix('.').unwrap_or(body);
        // "Equipped creature can't be blocked as long as its power is 3 or
        // less.": the condition is about the subject.
        if let StaticCond::AttachedIs(_) = c {
            let noun = if card.subtypes.iter().any(|s| s == "Equipment") {
                "equipped creature".to_string()
            } else {
                format!("enchanted {}", enchanted_noun(card))
            };
            if lowercase_first(body).starts_with(&format!("{noun} ")) {
                if let Some(rest) = cond.strip_prefix(&format!("{noun}'s ")) {
                    cond = format!("its {rest}");
                } else if let Some(rest) = cond.strip_prefix(&format!("{noun} is ")) {
                    cond = format!("it's {rest}");
                } else if let Some(rest) = cond.strip_prefix(&format!("{noun} ")) {
                    cond = format!("it {rest}");
                }
            }
        }
        // "This creature has flying as long as it has a +1/+1 counter on
        // it."
        if matches!(c, StaticCond::SourceCounters { at_least, n, .. } if *at_least || *n > 0)
            && body.starts_with(&capitalize(&this_ref(card)))
        {
            cond = cond.replacen(&format!("{} has", this_ref(card)), "it has", 1);
        }
        // "Equipped creature has deathtouch during your turn."
        if *c == StaticCond::DuringYourTurn {
            return format!("{stem} during your turn.");
        }
        // "Enchanted creature gets +2/+2 as long as it's a Vampire."
        if let StaticCond::AttachedIs(_) = c {
            for noun in [
                "equipped creature".to_string(),
                format!("enchanted {}", enchanted_noun(card)),
            ] {
                if lowercase_first(body).starts_with(&format!("{noun} ")) {
                    if let Some(rest) = cond.strip_prefix(&format!("{noun} is ")) {
                        cond = format!("it's {rest}");
                    } else if let Some(rest) = cond.strip_prefix(&format!("{noun} ")) {
                        cond = format!("it {rest}");
                    }
                }
            }
        }
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
        Ability::BecomesChoice { forms, in_addition } => {
            let forms: Vec<String> = forms
                .iter()
                .map(|f| {
                    let pt = format!("{}/{}", f.power, f.toughness);
                    let a = if pt.starts_with('8') || pt.starts_with("11/") || pt.starts_with("18/")
                    {
                        "an"
                    } else {
                        "a"
                    };
                    let mut s = format!("{a} {pt}");
                    for t in &f.subtypes {
                        let _ = write!(s, " {t}");
                    }
                    for t in &f.types {
                        let _ = write!(s, " {}", type_noun(*t));
                    }
                    if !f.keywords.is_empty() {
                        let _ = write!(s, " with {}", keywords_list(&f.keywords));
                    }
                    s
                })
                .collect();
            format!(
                "As {} enters, it becomes your choice of {}{}.",
                this_ref(card),
                or_list(&forms),
                if *in_addition {
                    " in addition to its other types"
                } else {
                    ""
                }
            )
        }
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
            Target::Filter(f)
            | Target::Spell(f)
            | Target::SpellOr(f)
            | Target::GraveyardCard { filter: f, .. } => {
                format!("Enchant {}", filter_phrase(f, card, false))
            }
        },
        Ability::Exert(effects) => {
            let (targets, effects) = crate::ast::exert_parts(effects);
            let mut ctx = Ctx::new(card, lookup, targets);
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
        // CR 702.172b: "+" for the bullets.
        // CR 700.2i: "Choose up to five {P} worth of modes. You may choose
        // the same mode more than once.\n{P} — ...".
        // "Equip—Pay {3} or discard a card."
        Ability::EquipOr {
            mana,
            other,
            targets,
        } => {
            let sub = equip_subtype(targets)
                .map(|s| format!("{s} "))
                .unwrap_or_default();
            let mut ctx = Ctx::new(card, lookup, targets);
            let other = lowercase_first(&cost_text(other, &mut ctx));
            format!("Equip {sub}\u{2014}Pay {mana} or {other}.").replace(" \u{2014}", "\u{2014}")
        }
        Ability::Pawprint { limit, modes } => {
            let mut out = format!(
                "Choose up to {} {{P}} worth of modes. You may choose the same mode more than once.",
                count_words(Num::N(u32::from(*limit)), "one")
            );
            for (w, m) in modes {
                let mut ctx = Ctx::new(card, lookup, &m.targets);
                ctx.current = Some(a);
                let _ = write!(
                    out,
                    "\n{} — {}",
                    "{P}".repeat(usize::from(*w)),
                    sentences(&m.effects, &mut ctx)
                );
            }
            out
        }
        // CR 702.183: "Tiered (Choose one additional cost.)", then
        // "• Thunder — {3} — <effects>" per mode.
        Ability::Tiered(modes) => {
            let mut out = "Tiered (Choose one additional cost.)".to_string();
            for (c, m) in modes {
                let mut ctx = Ctx::new(card, lookup, &m.targets);
                ctx.current = Some(a);
                let _ = write!(
                    out,
                    "\n• {} — {c} — {}",
                    m.name.as_deref().unwrap_or_default(),
                    sentences(&m.effects, &mut ctx)
                );
            }
            out
        }
        Ability::Spree(modes) => {
            let mut out = "Spree".to_string();
            for (c, m) in modes {
                let mut ctx = Ctx::new(card, lookup, &m.targets);
                ctx.current = Some(a);
                let _ = write!(out, "\n+ {c} — {}", sentences(&m.effects, &mut ctx));
            }
            out
        }
        Ability::Modal(modes)
        | Ability::ModalBoth(modes)
        | Ability::ModalTwo(modes)
        | Ability::ModalOf(_, modes)
        | Ability::ModalIf { modes, .. } => {
            let mut out = match a {
                Ability::ModalBoth(_) => "Choose one or both —".to_string(),
                Ability::ModalTwo(_) => "Choose two —".to_string(),
                Ability::ModalOf(h, _) => capitalize(&format!("{} —", mode_head_text(*h))),
                // "Choose one. If this spell was cast using teamwork,
                // choose both instead."
                Ability::ModalIf {
                    more,
                    any_number,
                    may,
                    cond,
                    ..
                } => format!(
                    "Choose one. If {}, {}choose {} instead.",
                    if *cond == StaticCond::Kicked {
                        "this spell was kicked".to_string()
                    } else {
                        static_cond_text(cond, card)
                    },
                    if *may { "you may " } else { "" },
                    if *any_number {
                        "any number"
                    } else if *more {
                        "one or more"
                    } else {
                        "both"
                    }
                ),
                _ => "Choose one —".to_string(),
            };
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
                let sub = equip_subtype(targets)
                    .map(|s| format!("{s} "))
                    .unwrap_or_default();
                // "Equip—{2}, Pay 2 life." (CR 702.6a: any cost).
                if equip_full_cost(cost) {
                    let mut ctx = Ctx::new(card, lookup, targets);
                    let once = if cost.per_turn == 1 {
                        " Activate only once each turn."
                    } else {
                        ""
                    };
                    return format!("Equip {sub}\u{2014}{}.{once}", cost_text(cost, &mut ctx))
                        .replace(" \u{2014}", "\u{2014}");
                }
                let once = if cost.per_turn == 1 {
                    ". Activate only once each turn."
                } else {
                    ""
                };
                // "Equip {3}. This ability costs {1} less to activate for
                // each other Equipment you control."
                let reduce = reduce_text(cost, card);
                if !reduce.is_empty() {
                    return format!("Equip {sub}{}.{reduce}", cost.mana);
                }
                return format!("Equip {sub}{}{once}", cost.mana);
            }
            if *cycling {
                if let Some(word) = typecycling_word(effects) {
                    return format!("{word}cycling {}", cost.mana);
                }
                // "Cycling—Pay 2 life." (CR 702.29a).
                if cost.mana.is_empty() {
                    let mut c = cost.clone();
                    c.discard_self = false;
                    let mut ctx = Ctx::new(card, lookup, targets);
                    return format!("Cycling\u{2014}{}.", cost_text(&c, &mut ctx));
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
            text.push_str(&reduce_text(cost, card));
            text.push_str(&restrictions_text(cost, card));
            text
        }
        Ability::Triggered {
            trigger,
            condition,
            targets,
            effects,
            limit,
            ..
        } => {
            let mut ctx = Ctx::new(card, lookup, targets);
            ctx.current = Some(a);
            ctx.trigger = Some(trigger);
            // CR 714.2b: "I, II — Draw a card."
            if let Trigger::Chapter(ns) = trigger {
                return format!(
                    "{} — {}",
                    chapter_numerals(ns),
                    sentences(effects, &mut ctx)
                );
            }
            let mut out = capitalize(&trigger_text(trigger, &mut ctx, false));
            // "Whenever Bebop & Rocksteady attack or block": a character
            // with a plural name takes plural verbs.
            if card.pronoun == Some(crate::ast::Pronoun::They) {
                out = plural_subject_verbs(&out, &this_ref(card));
            }
            match condition {
                // "When this land enters untapped, ..." (Idyllic Grange).
                Some(crate::ast::TrigCond::Core(Condition::SourceUntapped))
                    if matches!(trigger, Trigger::Enters(TriggerObj::This)) =>
                {
                    out.push_str(" untapped");
                }
                Some(crate::ast::TrigCond::HadCounter { one_or_more }) => {
                    out.push_str(if *one_or_more {
                        ", if it had one or more counters on it"
                    } else {
                        ", if it had a counter on it"
                    });
                }
                Some(crate::ast::TrigCond::Core(c)) => {
                    out.push_str(", ");
                    let cond = condition_text(*c, &mut ctx);
                    // "if Katara is tapped, put a +1/+1 counter on her": a
                    // character named in the condition is "her" after.
                    // "if this creature is tapped, put a flying counter on
                    // it" likewise.
                    if cond.starts_with(&format!("if {} is ", this_ref(card))) {
                        ctx.source_mentioned = true;
                    }
                    out.push_str(&cond);
                }
                // "Whenever this creature attacks while saddled", "...
                // while you control two or more artifacts".
                // "Whenever you gain life for the first time during each of
                // your turns".
                Some(crate::ast::TrigCond::While(StaticCond::DuringYourTurn))
                    if matches!(trigger, Trigger::GainsLifeFirst { .. }) =>
                {
                    out = out.replace(
                        "for the first time each turn",
                        "for the first time during each of your turns",
                    );
                }
                Some(crate::ast::TrigCond::While(StaticCond::DuringYourTurn)) => {
                    out.push_str(" during your turn");
                }
                // "Whenever this creature saddles a Mount or crews a Vehicle
                // during your main phase".
                Some(crate::ast::TrigCond::While(StaticCond::YourMainPhase)) => {
                    out.push_str(" during your main phase");
                }
                // "When this land enters untapped" (judged as it enters).
                Some(crate::ast::TrigCond::While(StaticCond::Is(f)))
                    if matches!(trigger, Trigger::Enters(TriggerObj::This))
                        && static_cond_text(&StaticCond::Is(f.clone()), card)
                            == "it's untapped" =>
                {
                    out.push_str(" untapped");
                }
                Some(crate::ast::TrigCond::While(c)) => {
                    out.push_str(" while ");
                    let mut text = match c {
                        StaticCond::Saddled => "saddled".to_string(),
                        c => static_cond_text(c, card),
                    };
                    // "Whenever this creature becomes tapped while it has a
                    // -1/-1 counter on it" (Encumbered Reejerey): named
                    // once already.
                    let this = this_ref(card);
                    if (out.contains(&capitalize(&this)) || out.contains(&format!(" {this} ")))
                        && let Some(rest) = text.strip_prefix(&format!("{this} "))
                    {
                        text = format!("{} {rest}", it_subj(card));
                    }
                    // "Whenever you cast a spell while Fire Lord Azula is
                    // attacking": not named yet, so named here.
                    if !out.contains(&capitalize(&this))
                        && !out.contains(&format!(" {this}"))
                        && let Some(rest) = text.strip_prefix("it's ")
                    {
                        text = format!("{this} is {rest}");
                    }
                    out.push_str(&text);
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
                // "When Enduring Innocence dies, if it was a creature, ...":
                // the permanent as it last existed (CR 603.10a).
                Some(crate::ast::TrigCond::Static(c @ StaticCond::Is(f)))
                    if !f.types.is_empty()
                        && matches!(trigger, Trigger::Dies(TriggerObj::This)) =>
                {
                    let text = static_cond_text(c, card);
                    let text = if card.pronoun == Some(crate::ast::Pronoun::They) {
                        text.replacen("it's", "they were", 1)
                    } else {
                        text.replacen("it's", "it was", 1)
                    };
                    out.push_str(", if ");
                    out.push_str(&text);
                }
                // "When an opponent casts a creature spell, if this
                // permanent is an enchantment, it becomes ...": a card type
                // the source may lose is asked of the permanent.
                Some(crate::ast::TrigCond::Static(c @ StaticCond::Is(f)))
                    if !f.types.is_empty() =>
                {
                    out.push_str(", if ");
                    // A legendary permanent by its name ("if Hydro-Man is a
                    // creature").
                    let this = if card.kind != Kind::Token
                        && card.supertypes.contains(&Supertype::Legendary)
                    {
                        format!("{} is", short_name(card))
                    } else {
                        "this permanent is".to_string()
                    };
                    out.push_str(&static_cond_text(c, card).replacen("it's", &this, 1));
                    ctx.source_mentioned = true;
                }
                // "At the beginning of each end step, if this creature was
                // dealt damage this turn, put a +0/+1 counter on it."
                Some(crate::ast::TrigCond::Static(StaticCond::Is(f)))
                    if f.damaged
                        && (Filter {
                            damaged: false,
                            ..f.clone()
                        }) == Filter::default() =>
                {
                    let _ = write!(out, ", if {} was dealt damage this turn", this_ref(card));
                    ctx.source_mentioned = true;
                }
                // An intervening "if" in the past tense: "At the beginning of
                // each end step, if you gained 5 or more life this turn".
                Some(crate::ast::TrigCond::Static(StaticCond::LifeGainedThisTurn(n))) => {
                    let _ = write!(out, ", if you gained {n} or more life this turn");
                }
                // "At the beginning of your end step, if you discarded a
                // card this turn, transform this creature." (Ragged
                // Recluse).
                Some(crate::ast::TrigCond::Static(StaticCond::DiscardedThisTurn)) => {
                    out.push_str(", if you discarded a card this turn");
                }
                Some(crate::ast::TrigCond::Static(c)) => {
                    out.push_str(", if ");
                    let cond = static_cond_text(c, card);
                    out.push_str(&cond);
                    // "if Katara is tapped, put a +1/+1 counter on her": a
                    // condition naming the source by name.
                    let named = card.pronoun.is_some()
                        && cond.starts_with(&format!("{} is ", this_ref(card)));
                    // Oracle names the source again after a condition about
                    // something else ("..., if you control three or more
                    // artifacts, this creature gets +3/+3"), and "it" after
                    // one about the source ("if there are no arrowhead
                    // counters on this artifact, sacrifice it").
                    ctx.source_mentioned = named
                        || matches!(
                            c,
                            StaticCond::SourceCounters { .. }
                                | StaticCond::SourceCountersOneOrMore(_)
                                | StaticCond::ManaSpent { that: false, .. }
                                | StaticCond::AttackedOrBlocked
                        );
                    ctx.attached_mentioned = matches!(c, StaticCond::AttachedIs(_));
                    ctx.attached_by_cond = ctx.attached_mentioned;
                    ctx.attached_by_type =
                        matches!(c, StaticCond::AttachedIs(f) if !f.subtypes.is_empty());
                }
                None => {}
            }
            out.push_str(", ");
            let text = effects_after(effects, &mut ctx, true);
            // A triggered mana ability's mana is for the tapped permanent's
            // controller (CR 605.1b): "its controller adds an additional
            // {G}".
            match (trigger, text.strip_prefix("add ")) {
                // "Whenever you tap a land for mana, add {G}." (you are its
                // controller).
                (Trigger::TappedForMana(TriggerObj::Filter(f)), Some(_))
                    if f.who == Some(Who::You) && f.relation == Relation::Any =>
                {
                    out.push_str(&text);
                }
                (Trigger::TappedForMana(_), Some(rest)) => {
                    out.push_str("its controller adds an additional ");
                    out.push_str(rest);
                }
                _ => out.push_str(&text),
            }
            match limit {
                mtg_core::card::TurnLimit::None => {}
                mtg_core::card::TurnLimit::Triggers(n) => {
                    let _ = write!(
                        out,
                        " This ability triggers only {} each turn.",
                        match n {
                            1 => "once".to_string(),
                            2 => "twice".to_string(),
                            n => format!("{} times", count_words(Num::N(u32::from(*n)), "one")),
                        }
                    );
                }
                mtg_core::card::TurnLimit::Do => out.push_str(" Do this only once each turn."),
                mtg_core::card::TurnLimit::Ever => {
                    out.push_str(" This ability triggers only once.");
                }
            }
            out
        }
        Ability::Replacement(r) => {
            let mut ctx = Ctx::new(card, lookup, no_targets);
            ctx.current = Some(a);
            replacement_text(r, &mut ctx, false)
        }
        // "Otherwise, it gets -2/-2." (CR 611.3a): the same objects, the
        // subject named again as "it".
        Ability::Static {
            filter, otherwise, ..
        } if !otherwise.is_empty() => {
            let mut main = a.clone();
            if let Ability::Static { otherwise, .. } = &mut main {
                otherwise.clear();
            }
            let main = ability_text(card, &main, lookup);
            let other = static_text(filter, otherwise, card, lookup);
            let subjects = [
                "Equipped creature ".to_string(),
                format!("Enchanted {} ", enchanted_noun(card)),
                format!("{} ", capitalize(&this_ref(card))),
            ];
            let other = subjects
                .iter()
                .find_map(|s| other.strip_prefix(s.as_str()))
                .map_or_else(|| lowercase_first(&other), |rest| format!("it {rest}"));
            format!("{main} Otherwise, {other}")
        }
        Ability::Static {
            filter,
            mods,
            condition,
            cond_first,
            cond_if,
            ..
        } => {
            let mut body = static_text(filter, mods, card, lookup);
            // "This creature doesn't untap during your untap step if ..."
            // (Goblin Rock Sled).
            if filter.relation == Relation::This {
                body = body.replace(
                    "doesn't untap during its controller's untap step",
                    "doesn't untap during your untap step",
                );
            }
            // "Threshold — Enchanted creature gets an additional +2/-2 as
            // long as ...": a conditional pump on what an earlier static
            // ability of this card already pumps.
            if condition.is_some()
                && card
                    .abilities
                    .iter()
                    .take_while(|b| {
                        !std::ptr::eq(*b, a)
                            && !matches!(b, Ability::Labeled { ability, .. } if std::ptr::eq(&raw const **ability, a))
                    })
                    .map(|b| match b {
                        Ability::Labeled { ability, .. } => &**ability,
                        b => b,
                    })
                    .any(|b| {
                        // "Descend 8 — As long as ..., Akawalli gets an
                        // additional +2/+2" after a pump whose condition
                        // this one's implies (Descend 4).
                        matches!(b, Ability::Static { filter: f, mods: m, condition: earlier, .. }
                        if f == filter
                            && m.iter().any(|x| matches!(x, Mod::ModifyPt { .. }))
                            && match (earlier, condition) {
                                (None, _) => true,
                                (
                                    Some(StaticCond::GraveyardCards { filter: g, at_least: a, .. }),
                                    Some(StaticCond::GraveyardCards { filter: h, at_least: b, .. }),
                                ) => g == h && b > a,
                                _ => false,
                            })
                    })
            {
                body = body.replacen(" gets ", " gets an additional ", 1);
            }
            match condition {
                None => body,
                Some(c) if *cond_if => {
                    let mut cond = static_cond_text(c, card);
                    // "Enchanted creature can't block if it's black."
                    // (Hobble): the subject is what the condition is about.
                    if let StaticCond::AttachedIs(_) = c {
                        let noun = if card.subtypes.iter().any(|s| s == "Equipment") {
                            "equipped creature".to_string()
                        } else {
                            format!("enchanted {}", enchanted_noun(card))
                        };
                        if lowercase_first(&body).starts_with(&format!("{noun} ")) {
                            if let Some(rest) = cond.strip_prefix(&format!("{noun} is ")) {
                                cond = format!("it's {rest}");
                            } else if let Some(rest) = cond.strip_prefix(&format!("{noun} ")) {
                                cond = format!("it {rest}");
                            }
                        }
                    }
                    // "Bombur doesn't untap during your untap step unless
                    // you have an enduring story."
                    if *c == StaticCond::NoEnduringStory {
                        return format!(
                            "{} unless you have an enduring story.",
                            body.strip_suffix('.').unwrap_or(&body)
                        );
                    }
                    // "This creature has first strike during your turn."
                    // (Razorkin Needlehead).
                    if matches!(c, StaticCond::YourTurn | StaticCond::DuringYourTurn) {
                        return format!(
                            "{} during your turn.",
                            body.strip_suffix('.').unwrap_or(&body)
                        );
                    }
                    // "This creature attacks each combat if able unless you
                    // control another Ally." (Reckless Cohort).
                    if let StaticCond::YouControlNone(f) = c
                        && f.relation == Relation::Other
                        && body.ends_with(" if able.")
                    {
                        let mut g = f.clone();
                        g.who = None;
                        return format!(
                            "{} unless you control {}.",
                            body.strip_suffix('.').unwrap_or(&body),
                            filter_a(&g, card)
                        );
                    }
                    format!("{} if {cond}.", body.strip_suffix('.').unwrap_or(&body))
                }
                Some(c) => {
                    let subtype_first = filter.types.is_empty()
                        && filter.subtypes.iter().any(|s| body.starts_with(s.as_str()));
                    conditional_static(&body, c, *cond_first, card, subtype_first)
                }
            }
        }
        Ability::BlockRule(r) => {
            let me = capitalize(&this_ref(card));
            if *r == crate::ast::BlockRule::NotByLesserPower {
                // Wandering Wolf.
                return format!(
                    "Creatures with power less than {}'s power can't block it.",
                    this_ref(card)
                );
            }
            format!("{me} {}.", block_rule_phrase(r, card, false))
        }
        // CR 721.2a: "2+ | Other creatures you control get +1/+1."
        Ability::StationAbility { n, ability } => {
            format!("{n}+ | {}", ability_text(card, ability, lookup))
        }
        // "I — Wark — Create ...": a chapter's name follows its numerals.
        // "• Khans — At the beginning of ...".
        Ability::DuringYourTurn(ability) => {
            let text = ability_text(card, ability, lookup);
            format!("During your turn, {}", lowercase_first(&text))
        }
        Ability::Labeled { label, ability }
            if crate::compile::named_option(card, label).is_some() =>
        {
            format!("• {label} — {}", ability_text(card, ability, lookup))
        }
        Ability::Labeled { label, ability } => {
            let text = ability_text(card, ability, lookup);
            // A max speed dies trigger names no "this creature or
            // another" (Embalmed Ascendant, Zahur): "Whenever a creature
            // you control dies".
            if label == "Max speed"
                && let Some(rest) =
                    text.strip_prefix(&format!("Whenever {} or another ", this_ref(card)))
            {
                return format!("{label} — Whenever {}", with_article(rest));
            }
            match (ability.as_ref(), text.split_once(" — ")) {
                (
                    Ability::Triggered {
                        trigger: Trigger::Chapter(_),
                        ..
                    },
                    Some((numerals, rest)),
                ) => format!("{numerals} — {label} — {rest}"),
                _ => format!("{label} — {text}"),
            }
        }
        Ability::CostChange {
            filter,
            whose,
            generic,
            cond,
            per,
            first,
            flash,
        } => {
            // "The first creature spell you cast each turn costs {2} less to
            // cast [and can be cast as though it had flash]"; "... you cast
            // during each of your turns ...".
            if *first {
                let whose = match whose {
                    Who::You => " you cast",
                    Who::Opponents => " your opponents cast",
                    Who::Any => "",
                };
                let when = if *cond == Some(StaticCond::DuringYourTurn) {
                    " during each of your turns"
                } else {
                    " each turn"
                };
                let subject = lowercase_first(&cost_spells_subject(filter, card))
                    .replacen("spells", "spell", 1);
                // "non-Lemur creature spell" (a subtype keeps its case).
                let subject = match subject.strip_prefix("Non-") {
                    Some(rest) => format!("non-{rest}"),
                    None => subject,
                };
                let more = if *generic > 0 { "more" } else { "less" };
                let flash = if *flash {
                    " and can be cast as though it had flash"
                } else {
                    ""
                };
                return format!(
                    "The first {subject}{whose}{when} costs {{{}}} {more} to cast{flash}.",
                    generic.unsigned_abs()
                );
            }
            // "... cost {1} less to cast for each creature you control with
            // power 4 or greater."
            let per_count = per.and_then(|n| match n {
                Num::Count(i) => card.counts.get(usize::from(i)),
                _ => None,
            });
            // "... cost {X} less to cast, where X is your speed."
            let per_where = per_count.is_some_and(|c| c.style == crate::ast::CountStyle::Where);
            let per_text = per_count.map_or_else(String::new, |c| {
                if per_where {
                    // "where X is The Scarlet Witch's power".
                    let phrase = count_phrase(&c.expr, card, false);
                    let phrase = if matches!(
                        c.expr,
                        crate::ast::CountExpr::SourcePower | crate::ast::CountExpr::SourceToughness
                    ) {
                        phrase.replacen("its", &format!("{}'s", this_ref(card)), 1)
                    } else {
                        phrase
                    };
                    format!(", where X is {phrase}")
                } else {
                    format!(" {}", count_phrase(&c.expr, card, true))
                }
            });
            let head = match cond {
                Some(StaticCond::NotYourTurn) => "During turns other than yours, ",
                Some(StaticCond::DuringYourTurn) => "During your turn, ",
                _ => "",
            };
            let whose = match whose {
                Who::You => " you cast",
                Who::Opponents => " your opponents cast",
                Who::Any => "",
            };
            let more = if *generic > 0 { "more" } else { "less" };
            // "Each spell you cast that's red or green costs {1} less to
            // cast." (Goblin Anarchomancer).
            if filter.each
                && let Some(cs) = &filter.colors
                && (Filter {
                    colors: None,
                    each: false,
                    ..filter.clone()
                }) == Filter::default()
            {
                let names: Vec<String> = cs.iter().map(|c| color_name(*c).to_string()).collect();
                return format!(
                    "{head}Each spell{whose} that's {} costs {{{}}} {more} to cast.",
                    or_list(&names),
                    generic.unsigned_abs()
                );
            }
            // "The second spell you cast each turn costs {1} less to cast."
            if let Some(StaticCond::SpellNumber(n)) = cond
                && *filter == Filter::default()
            {
                return format!(
                    "The {} spell{whose} each turn costs {{{}}} {more} to cast.",
                    ordinal_word(*n),
                    generic.unsigned_abs()
                );
            }
            // "Creature spells you cast with mana value 6 or greater":
            // the mana value after who casts them.
            let mut f = filter.clone();
            let mv = f.mana_value.take().filter(|_| !f.status.mana_value_x);
            if let Some(alt) = &mut f.or
                && alt.mana_value == mv
            {
                alt.mana_value = None;
            }
            let (f, mv) = match mv {
                Some((cmp, n)) => (f, format!(" with mana value {n}{}", cmp_words(cmp))),
                None => (filter.clone(), String::new()),
            };
            let amount = if per_where {
                "X".to_string()
            } else {
                generic.unsigned_abs().to_string()
            };
            let body = format!(
                "{} {whose}{mv} cost {{{amount}}} {more} to cast{per_text}.",
                cost_spells_subject(&f, card),
            )
            .replace("  ", " ");
            if head.is_empty() {
                body
            } else {
                format!("{head}{}", lowercase_first(&body))
            }
        }
        // "Instant and sorcery spells you cast have storm."
        Ability::SpellsHave {
            filter,
            from_hand,
            keywords,
            storm,
            affinity,
            uncounterable,
        } => {
            if *uncounterable {
                return format!(
                    "{} you control can't be countered.",
                    cost_spells_subject(filter, card)
                );
            }
            // "You may cast noncreature spells as though they had flash."
            if keywords == &[Keyword::Flash] && !*storm && affinity.is_none() {
                let from = if *from_hand { " from your hand" } else { "" };
                return format!(
                    "You may cast {}{from} as though they had flash.",
                    lowercase_first(&cost_spells_subject(filter, card))
                );
            }
            let mut items: Vec<String> = keywords
                .iter()
                .map(|k| keyword_name(*k).to_string())
                .collect();
            if *storm {
                items.push("storm".to_string());
            }
            if let Some(a) = affinity {
                items.push(format!("affinity for {}", filter_phrase(a, card, true)));
            }
            let from = if *from_hand { " from your hand" } else { "" };
            format!(
                "{} you cast{from} have {}.",
                cost_spells_subject(filter, card),
                list(&items)
            )
        }
        // "This spell costs {1} less to cast during your turn." (Mental
        // Modulation).
        Ability::CondCost {
            delta,
            cond: StaticCond::YourTurn | StaticCond::DuringYourTurn,
            ..
        } => format!(
            "This spell costs {{{}}} {} to cast during your turn.",
            delta.unsigned_abs(),
            if *delta < 0 { "less" } else { "more" },
        ),
        Ability::CondCost {
            delta,
            cond,
            as_long_as,
        } => format!(
            "This spell costs {{{}}} {} to cast {} {}.",
            delta.unsigned_abs(),
            if *delta < 0 { "less" } else { "more" },
            if *as_long_as { "as long as" } else { "if" },
            match cond {
                StaticCond::Bargained => "it's bargained".to_string(),
                // "if one or more cards left your graveyard this turn"
                // (Wilt in the Heat).
                StaticCond::LeftGraveyardThisTurn => {
                    "one or more cards left your graveyard this turn".to_string()
                }
                _ => static_cond_text(cond, card),
            }
        ),
        Ability::TargetCost {
            delta,
            filter,
            noun,
        } => format!(
            "This spell costs {{{}}} {} to cast if it targets {}.",
            delta.unsigned_abs(),
            if *delta < 0 { "less" } else { "more" },
            match noun {
                crate::ast::TargetNoun::Permanent => filter_a(filter, card),
                crate::ast::TargetNoun::Spell => a_spell(filter, card),
                crate::ast::TargetNoun::Card => card_filter_a(filter, card),
            }
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
            // "Firebending X, where X is this creature's power."
            crate::ast::Mechanic::Firebending(amount) => match amount {
                Num::Count(idx) => {
                    let phrase = match card.counts.get(usize::from(*idx)).map(|c| &c.expr) {
                        Some(crate::ast::CountExpr::SourcePower) | None => {
                            format!("{}'s power", this_ref(card))
                        }
                        Some(expr) => count_phrase(expr, card, false),
                    };
                    format!("Firebending X, where X is {phrase}.")
                }
                _ => format!("Firebending {}", digits(*amount)),
            },
            crate::ast::Mechanic::Poisonous(n) => format!("Poisonous {n}"),
            crate::ast::Mechanic::Frenzy(n) => format!("Frenzy {n}"),
            crate::ast::Mechanic::Absorb(n) => format!("Absorb {n}"),
            crate::ast::Mechanic::Provoke => "Provoke".to_string(),
            crate::ast::Mechanic::Fading(n) => format!("Fading {n}"),
            crate::ast::Mechanic::LevelUp(c) => format!("Level up {c}"),
            crate::ast::Mechanic::Recover(c) => format!("Recover {c}"),
            crate::ast::Mechanic::Escalate(c) => format!("Escalate {c}"),
            crate::ast::Mechanic::Prowl(c) => format!("Prowl {c}"),
            crate::ast::Mechanic::Miracle(c) => format!("Miracle {c}"),
            crate::ast::Mechanic::Emerge(c) => format!("Emerge {c}"),
            // CR 702.152a: "Blitz {3}{G}"; with a cost other than mana,
            // "Blitz—{2}{B}{B}, Pay 2 life."
            crate::ast::Mechanic::Blitz(c) => {
                if *c
                    == (crate::ast::Cost {
                        mana: c.mana.clone(),
                        order: c.order.clone(),
                        ..crate::ast::Cost::default()
                    })
                {
                    format!("Blitz {}", c.mana)
                } else {
                    format!(
                        "Blitz\u{2014}{}.",
                        cost_text(c, &mut Ctx::new(card, lookup, &[]))
                    )
                }
            }
            crate::ast::Mechanic::Prototype(cost, power, toughness) => {
                format!("Prototype {cost} \u{2014} {power}/{toughness}")
            }
            crate::ast::Mechanic::Annihilator(n) => format!("Annihilator {n}"),
            crate::ast::Mechanic::Rampage(n) => format!("Rampage {n}"),
            crate::ast::Mechanic::BattleCry => "Battle cry".to_string(),
            crate::ast::Mechanic::ForMirrodin => "For Mirrodin!".to_string(),
            crate::ast::Mechanic::Training => "Training".to_string(),
            crate::ast::Mechanic::Melee => "Melee".to_string(),
            crate::ast::Mechanic::Dethrone => "Dethrone".to_string(),
            crate::ast::Mechanic::Myriad => "Myriad".to_string(),
            crate::ast::Mechanic::Toxic(n) => format!("Toxic {n}"),
            crate::ast::Mechanic::Ward(c) => format!("Ward {c}"),
            // CR 702.21a, 702.37a, 702.27a: a cost other than mana prints
            // after a dash, as a sentence.
            crate::ast::Mechanic::WardCost(c) => {
                format!(
                    "Ward\u{2014}{}.",
                    cost_text(c, &mut Ctx::new(card, lookup, &[]))
                )
            }
            // "Ward—Pay life equal to this creature's power.", "Ward—
            // Discard a card or pay {2}."
            crate::ast::Mechanic::WardAlts(alts) => {
                format!(
                    "Ward\u{2014}{}.",
                    ward_alts_text(alts, &mut Ctx::new(card, lookup, &[]))
                )
            }
            crate::ast::Mechanic::EchoCost(c) => {
                format!(
                    "Echo\u{2014}{}.",
                    cost_text(c, &mut Ctx::new(card, lookup, &[]))
                )
            }
            crate::ast::Mechanic::MorphCost(c) => {
                format!(
                    "Morph\u{2014}{}.",
                    cost_text(c, &mut Ctx::new(card, lookup, &[]))
                )
            }
            crate::ast::Mechanic::BuybackCost(c) => {
                format!(
                    "Buyback\u{2014}{}.",
                    cost_text(c, &mut Ctx::new(card, lookup, &[]))
                )
            }
            crate::ast::Mechanic::Echo(c) => format!("Echo {c}"),
            crate::ast::Mechanic::Unearth(c) => format!("Unearth {c}"),
            crate::ast::Mechanic::Buyback(c) => format!("Buyback {c}"),
            crate::ast::Mechanic::Outlast(c) => format!("Outlast {c}"),
            crate::ast::Mechanic::Crew(n) => format!("Crew {n}"),
            crate::ast::Mechanic::Morph(c) => format!("Morph {c}"),
            crate::ast::Mechanic::Evoke(c) => format!("Evoke {c}"),
            crate::ast::Mechanic::Madness(c) => format!("Madness {c}"),
            crate::ast::Mechanic::Mayhem(c) => format!("Mayhem {c}"),
            crate::ast::Mechanic::Foretell(c) => format!("Foretell {c}"),
            crate::ast::Mechanic::Plot(c) => format!("Plot {c}"),
            crate::ast::Mechanic::Gift(k) => format!("Gift {}", k.oracle()),
            crate::ast::Mechanic::Sneak(c) => format!("Sneak {c}"),
            crate::ast::Mechanic::Harmonize(c) => format!("Harmonize {c}"),
            crate::ast::Mechanic::WebSlinging(c) => format!("Web-slinging {c}"),
            crate::ast::Mechanic::Impending(n, c) => format!("Impending {n}\u{2014}{c}"),
            crate::ast::Mechanic::Suspend(n, c) => format!("Suspend {n}\u{2014}{c}"),
            crate::ast::Mechanic::Entwine(c) => format!("Entwine {c}"),
            crate::ast::Mechanic::Modular(n) => format!("Modular {n}"),
            crate::ast::Mechanic::Graft(n) => format!("Graft {n}"),
            crate::ast::Mechanic::Sunburst => "Sunburst".to_string(),
            crate::ast::Mechanic::Vanishing(0) => "Vanishing".to_string(),
            crate::ast::Mechanic::Vanishing(n) => format!("Vanishing {n}"),
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
            crate::ast::Mechanic::CounterChoice([a, b], escapes) => format!(
                "{} {} with your choice of {} or {} on it.",
                capitalize(&this_ref(card)),
                if *escapes { "escapes" } else { "enters" },
                counters(Num::N(1), *a),
                counters(Num::N(1), *b)
            ),
            crate::ast::Mechanic::Bloodthirst(n) => format!("Bloodthirst {n}"),
            crate::ast::Mechanic::KickedWith(n, more, ks) => {
                let mut what = counters(Num::N(*n), CounterKind::PlusOnePlusOne);
                // "two +1/+1 counters and a trample counter" (Voidpouncer).
                if let Some((m, k)) = more {
                    let _ = write!(what, " and {}", counters(Num::N(*m), *k));
                }
                let mut s = format!(
                    "If this {} was kicked, it enters with {what} on it",
                    self_noun(card),
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
            crate::ast::Mechanic::Reconfigure(c) => format!("Reconfigure {c}"),
            crate::ast::Mechanic::Cleave(c, _) => format!("Cleave {c}"),
            crate::ast::Mechanic::Scavenge(c) => format!("Scavenge {c}"),
            crate::ast::Mechanic::Dash(c) => format!("Dash {c}"),
            crate::ast::Mechanic::Warp(c) => format!("Warp {c}"),
            crate::ast::Mechanic::Bestow(c) => format!("Bestow {c}"),
            crate::ast::Mechanic::Overload(c) => format!("Overload {c}"),
            crate::ast::Mechanic::Splice(c) => format!("Splice onto Arcane {c}"),
            crate::ast::Mechanic::SpliceCost(c) => {
                format!(
                    "Splice onto Arcane\u{2014}{}.",
                    cost_text(c, &mut Ctx::new(card, lookup, &[]))
                )
            }
            crate::ast::Mechanic::Embalm(c) => format!("Embalm {c}"),
            crate::ast::Mechanic::Encore(c) => format!("Encore {c}"),
            crate::ast::Mechanic::Craft(f, c) => {
                format!("Craft with {} {c}", filter_phrase(f, card, false))
            }
            crate::ast::Mechanic::Eternalize(c) => format!("Eternalize {c}"),
            crate::ast::Mechanic::Backup(n) => format!("Backup {n}"),
            crate::ast::Mechanic::Offspring(c) => format!("Offspring {c}"),
            crate::ast::Mechanic::Strive(c) => format!(
                "Strive \u{2014} This spell costs {c} more to cast for each target beyond the first."
            ),
            crate::ast::Mechanic::Bargain => "Bargain".to_string(),
            crate::ast::Mechanic::Conspire => "Conspire".to_string(),
            crate::ast::Mechanic::Casualty(n) => format!("Casualty {n}"),
            crate::ast::Mechanic::Dredge(n) => format!("Dredge {n}"),
            crate::ast::Mechanic::Storm => "Storm".to_string(),
            crate::ast::Mechanic::Awaken(n, c) => format!("Awaken {n}\u{2014}{c}"),
            crate::ast::Mechanic::Assist => "Assist".to_string(),
            crate::ast::Mechanic::Enlist => "Enlist".to_string(),
            crate::ast::Mechanic::Replicate(c) => format!("Replicate {c}"),
            crate::ast::Mechanic::Multikicker(c) => format!("Multikicker {c}"),
            crate::ast::Mechanic::Transmute(c) => format!("Transmute {c}"),
            crate::ast::Mechanic::Soulbond => "Soulbond".to_string(),
            crate::ast::Mechanic::Disturb(c) => format!("Disturb {c}"),
            crate::ast::Mechanic::GraveyardExile => format!(
                "If {} would be put into a graveyard from anywhere, exile it instead.",
                card.name
            ),
            crate::ast::Mechanic::Teamwork(n) => format!("Teamwork {n}"),
            crate::ast::Mechanic::Ripple(n) => format!("Ripple {n}"),
            crate::ast::Mechanic::Station => "Station".to_string(),
            crate::ast::Mechanic::Saddle(n) => format!("Saddle {n}"),
            crate::ast::Mechanic::Ascend => "Ascend".to_string(),
            crate::ast::Mechanic::Storied => "Storied".to_string(),
            crate::ast::Mechanic::StartYourEngines => "Start your engines!".to_string(),
            crate::ast::Mechanic::BecomesDay => format!(
                "If it's neither day nor night, it becomes day as {} enters.",
                this_ref(card)
            ),
            crate::ast::Mechanic::Daybound => "Daybound".to_string(),
            crate::ast::Mechanic::Nightbound => "Nightbound".to_string(),
            crate::ast::Mechanic::Fabricate(n) => format!("Fabricate {n}"),
            crate::ast::Mechanic::Evolve => "Evolve".to_string(),
            crate::ast::Mechanic::Increment => "Increment".to_string(),
            crate::ast::Mechanic::Renown(n) => format!("Renown {n}"),
            crate::ast::Mechanic::Unleash => "Unleash".to_string(),
            crate::ast::Mechanic::Afterlife(n) => format!("Afterlife {n}"),
            // "Mobilize X, where X is the number of creature cards in your
            // graveyard."
            crate::ast::Mechanic::Mobilize(Num::Count(i)) => {
                card.counts.get(usize::from(*i)).map_or_else(
                    || "Mobilize X".to_string(),
                    |c| {
                        format!(
                            "Mobilize X, where X is {}.",
                            count_phrase(&c.expr, card, false)
                        )
                    },
                )
            }
            crate::ast::Mechanic::Mobilize(n) => format!("Mobilize {}", digits(*n)),
            crate::ast::Mechanic::Extort => "Extort".to_string(),
            crate::ast::Mechanic::Mentor => "Mentor".to_string(),
            crate::ast::Mechanic::Riot => "Riot".to_string(),
            crate::ast::Mechanic::Haunt => "Haunt".to_string(),
            crate::ast::Mechanic::ReadAhead => "Read ahead".to_string(),
            // "Champion a Goblin", "Champion an Elemental".
            crate::ast::Mechanic::Champion(f) => format!("Champion {}", filter_a(f, card)),
            crate::ast::Mechanic::Devour(None, n) => format!("Devour {n}"),
            crate::ast::Mechanic::Devour(Some(k), n) => {
                format!("Devour {} {n}", format!("{k:?}").to_lowercase())
            }
            crate::ast::Mechanic::Amplify(n) => format!("Amplify {n}"),
            crate::ast::Mechanic::Tribute(n) => format!("Tribute {n}"),
            crate::ast::Mechanic::LivingWeapon => "Living weapon".to_string(),
            crate::ast::Mechanic::JobSelect => "Job select".to_string(),
            crate::ast::Mechanic::Retrace => "Retrace".to_string(),
            crate::ast::Mechanic::Ingest => "Ingest".to_string(),
            crate::ast::Mechanic::Reinforce(n, c) => format!("Reinforce {n}\u{2014}{c}"),
            crate::ast::Mechanic::JumpStart => "Jump-start".to_string(),
            crate::ast::Mechanic::Surge(c) => format!("Surge {c}"),
            crate::ast::Mechanic::Freerunning(c) => format!("Freerunning {c}"),
            crate::ast::Mechanic::Spectacle(c) => format!("Spectacle {c}"),
        },
        Ability::EntersWithIf {
            n,
            kind,
            cond,
            first,
            more,
        } => {
            let this = this_ref(card);
            let mut what = counters(Num::N(*n), *kind);
            if let Some((m, k)) = more {
                let _ = write!(what, " and {}", counters(Num::N(*m), *k));
            }
            let mut cond = static_cond_text(cond, card);
            if *first {
                // Adamant names the permanent spell: "spent to cast this
                // spell".
                if let Some(c) = cond.strip_suffix(" to cast it") {
                    cond = format!("{c} to cast this spell");
                }
                format!("If {cond}, {this} enters with {what} on it.")
            } else {
                format!("{} enters with {what} on it if {cond}.", capitalize(&this))
            }
        }
        Ability::EntersTappedUnless(c) => format!(
            "{} enters tapped unless {}.",
            capitalize(&this_ref(card)),
            static_cond_text(c, card)
        ),
        // "As this land enters, you may reveal a Mountain or Forest card
        // from your hand. If you don't, this land enters tapped."
        Ability::EntersTappedUnlessReveal { filter, or } => {
            let this = this_ref(card);
            let what = with_article(&cards_phrase(filter, card, false));
            let rest = match or {
                None => format!("If you don't, {this} enters tapped."),
                Some(c) => format!(
                    "{} enters tapped unless you revealed {what} this way or {}.",
                    capitalize(&this),
                    static_cond_text(c, card)
                ),
            };
            format!("As {this} enters, you may reveal {what} from your hand. {rest}")
        }
        Ability::EntersTappedIfNotYourTurn => format!(
            "{} enters tapped if it's not your turn.",
            capitalize(&this_ref(card))
        ),
        Ability::EntersTappedIf(c) => format!(
            "If {}, {} enters tapped.",
            static_cond_text(c, card),
            this_ref(card)
        ),
        Ability::AttackUnless { .. } if only_attacks_alone(a) => {
            format!("{} can only attack alone.", capitalize(&this_ref(card)))
        }
        Ability::AttackUnless {
            cond,
            attack,
            block,
            negated,
        } => {
            let what = match (attack, block) {
                (true, true) => "attack or block",
                (true, false) => "attack",
                _ => "block",
            };
            // "unless an opponent has been dealt damage this turn".
            let cond = static_cond_text(cond, card).replace(" was dealt ", " has been dealt ");
            format!(
                "{} can't {what} {} {cond}.",
                capitalize(&this_ref(card)),
                if *negated { "if" } else { "unless" }
            )
        }
        Ability::DeckAnyNumber => {
            format!("A deck can have any number of cards named {}.", card.name)
        }
        Ability::Uncounterable => "This spell can't be countered.".to_string(),
        Ability::EquipDiscount(n) => format!(
            "Equip abilities you activate that target {} cost {{{n}}} less to activate.",
            this_ref(card)
        ),
        Ability::AuraStays => "This effect doesn't remove this Aura.".to_string(),
        // CR 721.2: "4+ | Flying".
        Ability::StationLevel { n, keywords } => {
            // "9+ | Flying, first strike": a keyword line, capitalized once.
            let ks: Vec<&str> = keywords.iter().map(|k| k.name()).collect();
            format!("{n}+ | {}", capitalize(&ks.join(", ")))
        }
        // CR 716.2a, 719.2.
        Ability::ClassLevel { level, cost } => format!("{cost}: Level {level}"),
        // CR 711.2: "LEVEL 1-4" over its P/T box.
        Ability::LevelBand { min, max, pt } => match max {
            Some(m) => format!("LEVEL {min}-{m}\n{}/{}", pt.0, pt.1),
            None => format!("LEVEL {min}+\n{}/{}", pt.0, pt.1),
        },
        Ability::ToSolve(c) => format!("To solve — {}.", capitalize(&static_cond_text(c, card))),
        Ability::Solved(inner) => format!("Solved — {}", ability_text(card, inner, lookup)),
        Ability::CrewsAs {
            saddles,
            toughness,
            plus,
        } => {
            let verb = if *saddles {
                "saddles Mounts and crews Vehicles"
            } else {
                "crews Vehicles"
            };
            let how = if *toughness {
                "using its toughness rather than its power".to_string()
            } else {
                format!("as though its power were {plus} greater")
            };
            format!("This {} {verb} {how}.", self_noun(card))
        }
        // "You may cast spells from your hand without paying their mana
        // costs." (Omniscience).
        Ability::CastFree {
            spells,
            from_hand,
            max,
            limit,
        } => {
            use mtg_core::card::FreeCastLimit;
            let hand = if *from_hand { " from your hand" } else { "" };
            match limit {
                FreeCastLimit::FirstEachPlayer => {
                    let what = if *spells == Filter::default() {
                        "spell".to_string()
                    } else {
                        a_spell(spells, card)
                            .trim_start_matches("a ")
                            .trim_start_matches("an ")
                            .to_string()
                    };
                    format!(
                        "The first {what} each player casts during each of their turns may be cast without paying its mana cost."
                    )
                }
                FreeCastLimit::OnceYourTurns => format!(
                    "Once during each of your turns, you may cast {}{hand} without paying its mana cost.",
                    a_spell(spells, card)
                ),
                FreeCastLimit::Always => {
                    let what = top_spells(spells, card);
                    let what = match max {
                        Some(m) => format!(
                            "{what} with mana value less than or equal to {}",
                            cda_count_text(m, card)
                        ),
                        None => what,
                    };
                    format!("You may cast {what}{hand} without paying their mana costs.")
                }
            }
        }
        Ability::PlayFromTop {
            lands,
            spells,
            once,
        } => {
            let spells = spells.as_ref().map(|f| {
                if *once {
                    a_spell(f, card)
                } else {
                    top_spells(f, card)
                }
            });
            let what = match (*lands, spells) {
                (true, None) => "play lands".to_string(),
                (true, Some(s)) => format!("play lands and cast {s}"),
                (false, Some(s)) => format!("cast {s}"),
                (false, None) => String::new(),
            };
            let once = if *once { "Once each turn, you" } else { "You" };
            format!("{once} may {what} from the top of your library.")
        }
        Ability::PlayExiled {
            lands,
            spells,
            your_turn,
            owned,
        } => {
            // CR 607.2a: "You may cast spells from among cards exiled with
            // Rona.", "During your turn, you may play cards exiled with
            // Valgavoth."
            let all = spells.as_ref().is_some_and(|f| *f == Filter::default());
            let whose = if *owned { "cards you own" } else { "cards" };
            let with = this_ref(card);
            let what = match (*lands, spells) {
                (true, Some(_)) if all && *your_turn => {
                    format!("play {whose} exiled with {with}")
                }
                (true, None) => format!("play lands from among {whose} exiled with {with}"),
                (true, Some(f)) => format!(
                    "play lands and cast {} from among {whose} exiled with {with}",
                    top_spells(f, card)
                ),
                (false, Some(f)) => format!(
                    "cast {} from among {whose} exiled with {with}",
                    top_spells(f, card)
                ),
                (false, None) => String::new(),
            };
            let lead = if *your_turn {
                "During your turn, you"
            } else {
                "You"
            };
            format!("{lead} may {what}.")
        }
        Ability::TopRevealed { hands: true, .. } => {
            "Players play with their hands revealed.".to_string()
        }
        Ability::TopRevealed { players, .. } => {
            if *players {
                "Players play with the top card of their libraries revealed.".to_string()
            } else {
                "Play with the top card of your library revealed.".to_string()
            }
        }
        Ability::MayLook {
            top,
            face_down,
            opponents,
        } => {
            let whose = if *opponents {
                "your opponents control"
            } else {
                "you don't control"
            };
            let what = match (*top, *face_down) {
                (true, true) => {
                    format!("the top card of your library and at face-down creatures {whose}")
                }
                (true, false) => "the top card of your library".to_string(),
                _ => format!("face-down creatures {whose}"),
            };
            format!("You may look at {what} any time.")
        }
        Ability::AsTurnedFaceUp { enters, kind, n } => {
            let what = if *enters {
                "enters or is turned face up"
            } else {
                "is turned face up"
            };
            let mut s = format!(
                "As {} {what}, put {} on it",
                this_ref(card),
                counters(*n, *kind)
            );
            if let Num::Count(i) = n
                && let Some(c) = card.counts.get(usize::from(*i))
            {
                s.push_str(", where X is ");
                s.push_str(&count_phrase(&c.expr, card, false));
            }
            s.push('.');
            s
        }
        Ability::ExtraVote { optional: false } => {
            "While voting, you get an additional vote.".to_string()
        }
        Ability::ExtraVote { optional: true } => {
            "While voting, you may vote an additional time.".to_string()
        }
        Ability::CombatTax {
            scope,
            generic,
            per,
            cond,
        } => {
            use mtg_core::card::TaxScope;
            let count = per.and_then(|n| match n {
                Num::Count(i) => card.counts.get(usize::from(i)),
                _ => None,
            });
            // "{2}", "{1} for each card in your hand", "{X} ..., where X
            // is the number of enchantments you control".
            let (amount, where_x) = match count {
                Some(c) if c.style == crate::ast::CountStyle::Where => (
                    "{X}".to_string(),
                    format!(", where X is {}", count_phrase(&c.expr, card, false)),
                ),
                Some(c) => (
                    format!("{{1}} {}", count_phrase(&c.expr, card, true)),
                    String::new(),
                ),
                None => (format!("{{{generic}}}"), String::new()),
            };
            let text = match scope {
                TaxScope::AttackingYou {
                    planeswalkers: true,
                    ..
                } => format!(
                    "creatures can't attack you or planeswalkers you control unless their controller pays {amount} for each of those creatures{where_x}."
                ),
                TaxScope::AttackingYou { those: true, .. } => format!(
                    "creatures can't attack you unless their controller pays {amount} for each of those creatures{where_x}."
                ),
                TaxScope::AttackingYou { .. } => format!(
                    "creatures can't attack you unless their controller pays {amount} for each creature they control that's attacking you{where_x}."
                ),
                TaxScope::Enchanted { attack, block } => {
                    let what = match (attack, block) {
                        (true, true) => "attack or block",
                        (true, false) => "attack",
                        _ => "block",
                    };
                    format!(
                        "enchanted creature can't {what} unless its controller pays {amount}{where_x}."
                    )
                }
                TaxScope::EnchantedBlocked => format!(
                    "enchanted creature can't be blocked unless defending player pays {amount} for each creature they control that's blocking it{where_x}."
                ),
            };
            match cond {
                Some(c) => format!("As long as {}, {text}", static_cond_text(c, card)),
                None => capitalize(&text),
            }
        }
        Ability::HandSize { whose, delta, set } => {
            let (subject, have) = if *whose == Who::Opponents {
                ("Each opponent's", "Each opponent has")
            } else {
                ("Your", "You have")
            };
            match delta {
                None => format!("{have} no maximum hand size."),
                Some(n) if *set => format!(
                    "{subject} maximum hand size is {}.",
                    count_words(Num::N(n.unsigned_abs()), "one")
                ),
                Some(n) => format!(
                    "{subject} maximum hand size is {} by {}.",
                    if *n > 0 { "increased" } else { "reduced" },
                    count_words(Num::N(n.unsigned_abs()), "one")
                ),
            }
        }
        Ability::Protection(f) => protection_text(f, card),
        Ability::Landwalk(f) => capitalize(&landwalk_text(f, card)),
        // "Hexproof from black": a color alone.
        Ability::HexproofFrom(f)
            if f.colors.as_ref().is_some_and(|c| c.len() == 1)
                && *f
                    == (Filter {
                        colors: f.colors.clone(),
                        ..Filter::default()
                    }) =>
        {
            let c = f.colors.as_ref().map_or(String::new(), |c| {
                format!("{:?}", c[0]).to_ascii_lowercase()
            });
            format!("Hexproof from {c}")
        }
        Ability::HexproofFrom(f) => {
            format!(
                "Hexproof{}",
                &protection_text(f, card)["Protection".len()..]
            )
        }
        // A legendary card names itself ("If Quicksilver is in your
        // opening hand, ...").
        // "You may reveal this card from your opening hand. If you do, at
        // the beginning of the first upkeep, ..." (the Chancellors).
        Ability::OpeningHandReveal {
            step,
            whose,
            after,
            effects,
        } => {
            let mut ctx = Ctx::new(card, lookup, &[]);
            ctx.current = Some(a);
            let when = match (step, whose) {
                (Step::Upkeep, Who::You) => "at the beginning of your first upkeep",
                (Step::Upkeep, _) => "at the beginning of the first upkeep",
                _ => "at the beginning of your first main phase of the game",
            };
            let body = lowercase_first(&sentences(effects, &mut ctx));
            let body = if *after {
                format!("{} {when}.", body.trim_end_matches('.'))
            } else {
                format!("{when}, {body}")
            };
            format!("You may reveal this card from your opening hand. If you do, {body}")
        }
        Ability::OpeningHand => format!(
            "If {} is in your opening hand, you may begin the game with it on the battlefield.",
            if card.supertypes.contains(&Supertype::Legendary) {
                short_name(card)
            } else {
                "this card"
            }
        ),
        // CR 603.6a: Torpor Orb.
        Ability::TriggerAgain {
            entering: Some(e),
            battlefield,
            suppress: true,
            ..
        } => format!(
            "{} entering{} don't cause abilities to trigger.",
            capitalize(&controller_first(e, filter_phrase(e, card, true))),
            if *battlefield { " the battlefield" } else { "" },
        ),
        // CR 603.2d.
        Ability::TriggerAgain {
            filter: f,
            entering: Some(e),
            battlefield,
            cause,
            ..
        } => format!(
            "If {} {}{} causes a triggered ability of {} to trigger, that ability triggers an additional time.",
            with_article(&controller_first(e, filter_phrase(e, card, false))),
            match cause {
                mtg_core::card::TriggerCause::Enters => "entering",
                mtg_core::card::TriggerCause::Dies => "dying",
                mtg_core::card::TriggerCause::Attacks => "attacking",
            },
            if *battlefield { " the battlefield" } else { "" },
            with_article(&controller_first(f, filter_phrase(f, card, false)))
        ),
        // "it" after "another Elemental" (Twinflame Travelers), "that
        // ability" otherwise.
        Ability::TriggerAgain {
            filter: f,
            entering: None,
            ..
        } => {
            let it = if f.relation == mtg_core::card::Relation::Other && f.or.is_none() {
                "it"
            } else {
                "that ability"
            };
            // "of equipped creature" (Wizard's Staff) takes no article.
            let what = controller_first(f, filter_phrase(f, card, false));
            let what = if what.starts_with("equipped ") || what.starts_with("enchanted ") {
                what
            } else {
                with_article(&what)
            };
            format!("If a triggered ability of {what} triggers, {it} triggers an additional time.")
        }
        // "Activated abilities of artifacts you control cost {1} less to
        // activate."
        Ability::ActivationCost {
            filter,
            by,
            min_one,
            keyword,
        } => {
            let what = keyword.map_or("Activated", mtg_core::card::ActivatedKeyword::name);
            let min = if *min_one {
                " This effect can't reduce the mana in that cost to less than one mana."
            } else {
                ""
            };
            format!(
                "{what} abilities of {} cost {{{by}}} less to activate.{min}",
                filter_phrase(filter, card, true)
            )
        }
        Ability::PlayerStatic { kind, cond } => {
            use mtg_core::card::PlayerStaticKind as K;
            let s = match kind {
                K::ActivationCost { .. } => unreachable!("Ability::ActivationCost"),
                K::Hexproof => "you have hexproof".to_string(),
                K::Shroud => "you have shroud".to_string(),
                K::EnchantedUntargetable(ban) => format!(
                    "enchanted {} can't be the target of {}",
                    enchanted_noun(card),
                    match ban {
                        mtg_core::card::TargetBan::Opponents => {
                            "spells or abilities your opponents control"
                        }
                        mtg_core::card::TargetBan::Spells => "spells",
                    }
                ),
                K::Flagbearer => "while an opponent is choosing targets as part of casting a spell they control or activating an ability they control, that player must choose at least one Flagbearer on the battlefield if able".to_string(),
                K::FirstEquipFree { your_turns } => format!(
                    "you may pay {{0}} rather than pay the equip cost of the first equip ability you activate {}",
                    if *your_turns {
                        "during each of your turns"
                    } else {
                        "each turn"
                    }
                ),
                K::LandsFromGraveyard => "you may play lands from your graveyard".to_string(),
                K::ChosenName(ban) => match ban {
                    mtg_core::card::NameBan::Cast => "spells with the chosen name can't be cast",
                    mtg_core::card::NameBan::ActivateNonMana => {
                        "activated abilities of sources with the chosen name can't be activated unless they're mana abilities"
                    }
                    mtg_core::card::NameBan::Activate => {
                        "activated abilities of sources with the chosen name can't be activated"
                    }
                }
                .to_string(),
                K::CantLose => {
                    "you can't lose the game and your opponents can't win the game".to_string()
                }
                K::OpponentsCantCastYourTurn => {
                    "your opponents can't cast spells during your turn".to_string()
                }
                K::Rule(mtg_core::card::RuleKind::NoLifeGain) => {
                    "players can't gain life".to_string()
                }
                K::Rule(mtg_core::card::RuleKind::DamageCantBePrevented) => {
                    "damage can't be prevented".to_string()
                }
                K::Rule(mtg_core::card::RuleKind::GraveyardUntargetable) => {
                    "cards in graveyards can't be the targets of spells or abilities".to_string()
                }
                K::ExtraLands {
                    n,
                    each_player: true,
                } => format!(
                    "each player may play {} on each of their turns",
                    additional_lands(*n)
                ),
                K::ExtraLands { n, .. } => {
                    format!(
                        "you may play {} on each of your turns",
                        additional_lands(*n)
                    )
                }
            };
            match cond {
                Some(c) => format!("As long as {}, {s}.", static_cond_text(c, card)),
                None => format!("{}.", capitalize(&s)),
            }
        }
        Ability::Affinity(f) => {
            let mut g = f.clone();
            g.who = None;
            format!("Affinity for {}", filter_phrase(&g, card, true))
        }
        Ability::EntersAsCopy(f) => {
            // "... a copy of a creature you control" (Visage Bandit).
            let mut s = if f.who == Some(mtg_core::card::Who::You) {
                format!(
                    "You may have {} enter as a copy of {}",
                    this_ref(card),
                    with_article(&filter_phrase(f, card, false))
                )
            } else {
                format!(
                    "You may have {} enter as a copy of any {} on the battlefield",
                    this_ref(card),
                    filter_phrase(f, card, false)
                )
            };
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

/// `flash with cleanup sacrifice` (Spider Climb).
const FLASH_CLEANUP: &str = "You may cast this spell as though it had flash. If you cast it any \
    time a sorcery couldn't have been cast, the controller of the permanent it becomes \
    sacrifices it at the beginning of the next cleanup step.";
/// "You may cast this card from your graveyard as long as you control a
/// Zombie.", "... by paying {2}{W} rather than paying its mana cost.", "...
/// by paying 3 life and discarding a card in addition to paying its other
/// costs." (CR 601.3).
fn graveyard_cast_text(card: &Card, g: &crate::ast::GraveyardCast) -> String {
    let mut s = String::new();
    if g.max_speed {
        s.push_str("Max speed \u{2014} ");
    }
    s.push_str("You may cast this card from your graveyard");
    if let Some(c) = &g.cond {
        let _ = write!(
            s,
            " {} {}",
            if g.as_long_as { "as long as" } else { "if" },
            static_cond_text(c, card)
        );
    }
    if let Some(m) = &g.mana {
        let _ = write!(s, " by paying {m} rather than paying its mana cost");
    } else if let Some(c) = &g.extra {
        let mut parts = Vec::new();
        if c.life > 0 {
            parts.push(format!("paying {} life", c.life));
        }
        if let Some(SacCost::Filter(f)) = &c.sacrifice {
            parts.push(format!("sacrificing {}", filter_a(f, card)));
        }
        if c.discard > 0 {
            parts.push(if c.discard == 1 {
                "discarding a card".to_string()
            } else {
                format!("discarding {} cards", count_words(Num::N(c.discard), "a"))
            });
        }
        let _ = write!(
            s,
            " by {} in addition to paying its other costs",
            parts.join(" and ")
        );
    }
    s.push('.');
    s
}

/// "Cast this spell only during combat.", "Cast this spell only if you
/// control a creature." (CR 601.3).
fn cast_only_text(card: &Card) -> Option<String> {
    // "Threshold — You can't cast this spell unless there are seven or
    // more cards in your graveyard." (Proft, Sinister Mastermind).
    if card.cast_only_cant {
        let c = card.cast_only.as_ref()?;
        let label = if *c == StaticCond::CardsInGraveyard(7) {
            "Threshold — "
        } else {
            ""
        };
        return Some(format!(
            "{label}You can't cast this spell unless {}.",
            static_cond_text(c, card)
        ));
    }
    let when = match card.cast_only.as_ref()? {
        StaticCond::During(t) => turn_time_text(*t).to_string(),
        StaticCond::DuringYourTurn => "during your turn".to_string(),
        c => format!("if {}", static_cond_text(c, card)),
    };
    Some(format!("Cast this spell only {when}."))
}

/// The card's full Oracle text.
#[allow(clippy::too_many_lines)] // one block per kind of line, in Oracle order
pub fn oracle_text(card: &Card, lookup: &dyn Lookup) -> String {
    let mut lines: Vec<String> = Vec::new();
    // "Bargain", then "This spell costs {3} less to cast if ..." come
    // first, before keywords.
    let bargain = |a: &Ability| matches!(a, Ability::Mechanic(crate::ast::Mechanic::Bargain));
    if card.abilities.iter().any(bargain) {
        lines.push("Bargain".to_string());
    }
    // "Gift a card" (CR 702.174) likewise, before an additional cost and
    // the keywords.
    let gift = |a: &Ability| matches!(a, Ability::Mechanic(crate::ast::Mechanic::Gift(_)));
    for a in card.abilities.iter().filter(|a| gift(a)) {
        lines.push(ability_text(card, a, lookup));
    }
    let cost_line = |a: &Ability| {
        matches!(a, Ability::TargetCost { .. } | Ability::CondCost { .. }) || bargain(a) || gift(a)
    };
    // "This spell costs {2} less to cast if evidence was collected." ends
    // the optional additional cost's paragraph.
    let after_optional = |a: &Ability| {
        card.optional_cost.is_some()
            && matches!(
                a,
                Ability::CondCost {
                    cond: StaticCond::EvidenceCollected | StaticCond::AdditionalCostPaid,
                    ..
                }
            )
    };
    for a in card
        .abilities
        .iter()
        .filter(|a| cost_line(a) && !bargain(a) && !gift(a) && !after_optional(a))
    {
        lines.push(ability_text(card, a, lookup));
    }
    // Oracle prints kicker before keywords ("Kicker {2}{G}\nReach").
    if let Some(c) = &card.flash_if {
        lines.push(if card.flash_if_may {
            format!(
                "You may cast this spell as though it had flash if {}.",
                static_cond_text(c, card)
            )
        } else {
            // "Colossal Rattlewurm has flash as long as you control a
            // Desert."
            let who = if card.flash_if_named {
                card.name.as_str()
            } else {
                "This spell"
            };
            format!("{who} has flash as long as {}.", static_cond_text(c, card))
        });
    }
    if card.flash_cleanup {
        lines.push(FLASH_CLEANUP.to_string());
    }
    // "Cast this spell only during combat." (CR 601.3), first.
    lines.splice(0..0, cast_only_text(card));
    // Aftermath heads its half, before an additional cost (Start //
    // Finish).
    if card.keywords.contains(&Keyword::Aftermath) {
        lines.insert(0, "Aftermath".to_string());
    }
    // Devoid, a color-defining ability, comes before everything else
    // ("Devoid\nKicker {2}{C}", Voidpouncer; "Devoid\nAs an additional
    // cost to cast this spell, put a card an opponent owns from exile
    // ...", Processor Assault).
    let devoid_first = card.keywords.contains(&Keyword::Devoid);
    if devoid_first {
        lines.insert(0, keyword_line(card, Keyword::Devoid));
    }
    if let Some(k) = &card.kicker {
        lines.push(format!("Kicker {k}"));
    }
    // "Kicker—Sacrifice two lands." (CR 702.33a).
    if let Some(c) = &card.kicker_cost {
        let c = cost_text(c, &mut Ctx::new(card, lookup, &[]));
        lines.push(format!("Kicker\u{2014}{c}."));
    }
    // Flash, though, comes before the additional cost ("Flash\nAs an
    // additional cost to cast this spell, behold a Kithkin and exile it.",
    // Champion of the Clachan).
    let flash_first = card.additional.is_some() && card.keywords.first() == Some(&Keyword::Flash);
    if flash_first {
        lines.push(keyword_line(card, Keyword::Flash));
    }
    // Oracle prints an additional cost before keywords ("... sacrifice a
    // creature or pay {1}{B}.\nMenace").
    if let Some(cost) = &card.additional {
        let mut ctx = Ctx::new(card, lookup, &[]);
        let c = additional_text(cost, &mut ctx);
        let c = match &card.additional_or {
            // "sacrifice a creature or pay {3}" (CR 601.2b).
            Some(or) => format!("{c} or {}", additional_text(or, &mut ctx)),
            None => c,
        };
        lines.push(format!("As an additional cost to cast this spell, {c}."));
    }
    // "As an additional cost to cast this spell, you may collect evidence
    // 6." (CR 601.2b).
    if let Some(cost) = &card.optional_cost {
        let mut ctx = Ctx::new(card, lookup, &[]);
        let c = additional_text(cost, &mut ctx);
        let mut line = format!("As an additional cost to cast this spell, you may {c}.");
        for a in card.abilities.iter().filter(|a| after_optional(a)) {
            line.push(' ');
            line.push_str(&ability_text(card, a, lookup));
        }
        lines.push(line);
    }
    // An alternative cost prints before keywords ("You may pay {1}{W} and
    // exile a creature card from your graveyard rather than pay this
    // spell's mana cost.\nFlying").
    if let Some(a) = &card.alt_cost {
        lines.push(alt_cost_line(card, a));
    }
    let alone_pair = card.keywords.contains(&Keyword::CantAttackAlone)
        && card.keywords.contains(&Keyword::CantBlockAlone);
    for k in &card.keywords {
        if alone_pair && *k == Keyword::CantBlockAlone
            || (flash_first && *k == Keyword::Flash)
            || (devoid_first && *k == Keyword::Devoid)
            || *k == Keyword::Rebound
            || *k == Keyword::Cipher
            || *k == Keyword::Paradigm
            || *k == Keyword::Fuse
            || *k == Keyword::Aftermath
        {
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
        if let Some(line) = enters_tapped_with(card, i).or_else(|| alone_abilities(card, i)) {
            lines.push(line);
            i += 2;
            continue;
        }
        let mut text = ability_text(card, &card.abilities[i], lookup);
        // CR 702.148a: the spell's words that cleave removes, in square
        // brackets.
        if matches!(card.abilities[i], Ability::Spell { .. })
            && let Some(cleaved) = card.abilities.iter().find_map(|a| match a {
                Ability::Mechanic(crate::ast::Mechanic::Cleave(_, s)) => Some(s),
                _ => None,
            })
        {
            text = bracket_removed(&text, &ability_text(card, cleaved, lookup));
        }
        // "This land enters tapped. As it enters, choose a color." is one
        // line.
        // "{X}: ... Spend only black mana on X." after an activated ability
        // is part of its paragraph.
        let joined = match &card.abilities[i] {
            Ability::AsEntersChoose { joined, .. } => *joined,
            Ability::AuraStays => true,
            Ability::XManaColor(_) => {
                i > 0 && matches!(card.abilities[i - 1], Ability::Activated { .. })
            }
            // "This creature enters with ... counters on it. If X is 2 or
            // less, it enters tapped."
            Ability::EntersTappedIf(_) => {
                i > 0
                    && matches!(
                        &card.abilities[i - 1],
                        Ability::Replacement(r) if r.event == Replaces::Enters
                    )
            }
            _ => false,
        };
        if joined && matches!(card.abilities[i], Ability::EntersTappedIf(_)) {
            let this = format!(", {} enters tapped.", this_ref(card));
            if let Some(head) = text.strip_suffix(&this) {
                text = format!("{head}, it enters tapped.");
            }
        }
        match lines.last_mut() {
            Some(last) if joined => *last = format!("{last} {text}"),
            _ => lines.push(text),
        }
        i += 1;
    }
    if let Some(fb) = &card.flashback {
        // "Flashback {4}{R}" for a mana cost; "Flashback—{1}{U}, Pay 3
        // life." when the cost has other parts.
        let mut ctx = Ctx::new(card, lookup, &[]);
        let mana_only = *fb
            == crate::ast::Cost {
                mana: fb.mana.clone(),
                order: fb.order.clone(),
                ..Default::default()
            };
        if mana_only {
            lines.push(format!("Flashback {}", fb.mana));
        } else {
            lines.push(format!("Flashback—{}.", cost_text(fb, &mut ctx)));
        }
    }
    if let Some(n) = &card.graveyard_alias {
        lines.push(format!(
            "If this card is in a graveyard, effects from spells named {n} count it as a card named {n}."
        ));
    }
    // CR 601.3: "You may cast this card from your graveyard ...".
    if let Some(g) = &card.graveyard_cast {
        lines.push(graveyard_cast_text(card, g));
    }
    // Oracle prints rebound after the spell's effect.
    if card.keywords.contains(&Keyword::Rebound) {
        lines.push("Rebound".to_string());
    }
    // And cipher (CR 702.99).
    if card.keywords.contains(&Keyword::Cipher) {
        lines.push("Cipher".to_string());
    }
    // And paradigm (CR 702.192).
    if card.keywords.contains(&Keyword::Paradigm) {
        lines.push("Paradigm".to_string());
    }
    // And fuse, on each half of the split card (CR 702.102).
    if card.keywords.contains(&Keyword::Fuse) {
        lines.push("Fuse".to_string());
    }
    lines.join("\n")
}

/// Cleave (CR 702.148a): `full` with the words `cut` lacks in square
/// brackets ("Return target nonland permanent [you control] to its
/// owner's hand."), or `full` unchanged if `cut` isn't `full` with words
/// removed.
fn bracket_removed(full: &str, cut: &str) -> String {
    fn tokens(s: &str) -> Vec<&str> {
        let mut out = Vec::new();
        for word in s.split(' ') {
            let mut w = word;
            // "control." is "control" and "."; a line break its own.
            while let Some(i) = w.find('\n') {
                let (a, b) = w.split_at(i);
                out.extend(split_punct(a));
                out.push("\n");
                w = &b[1..];
            }
            out.extend(split_punct(w));
        }
        out
    }
    fn split_punct(w: &str) -> Vec<&str> {
        match w.char_indices().last() {
            Some((i, c)) if matches!(c, '.' | ',' | ';' | ':') && i > 0 => {
                vec![&w[..i], &w[i..]]
            }
            _ if w.is_empty() => Vec::new(),
            _ => vec![w],
        }
    }
    let (a, b) = (tokens(full), tokens(cut));
    // The longest common subsequence, matching each word as early as it
    // can.
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    if lcs[0][0] != b.len() {
        return full.to_string();
    }
    let (mut i, mut j) = (0, 0);
    let mut kept = vec![false; a.len()];
    while i < a.len() {
        if j < b.len() && a[i] == b[j] && lcs[i][j] == lcs[i + 1][j + 1] + 1 {
            kept[i] = true;
            j += 1;
        }
        i += 1;
    }
    let mut out = String::new();
    let mut open = false;
    for (k, t) in a.iter().enumerate() {
        if *t == "\n" {
            if open {
                out.push(']');
                open = false;
            }
            out.push('\n');
            continue;
        }
        let punct = matches!(*t, "." | "," | ";" | ":");
        if open && kept[k] {
            out.push(']');
            open = false;
        }
        if !punct && !out.is_empty() && !out.ends_with('\n') {
            out.push(' ');
        }
        if !open && !kept[k] {
            out.push('[');
            open = true;
        }
        out.push_str(t);
    }
    if open {
        out.push(']');
    }
    out
}

/// "This creature can't attack or block alone." for the `i`th and next
/// abilities, after another ability line (Wojek Bodyguard's mentor).
fn alone_abilities(card: &Card, i: usize) -> Option<String> {
    matches!(
        (card.abilities.get(i), card.abilities.get(i + 1)),
        (
            Some(Ability::Keyword(Keyword::CantAttackAlone)),
            Some(Ability::Keyword(Keyword::CantBlockAlone)),
        )
    )
    .then(|| {
        format!(
            "{} can't attack or block alone.",
            capitalize(&this_ref(card))
        )
    })
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
        // "Flying; banding": older keyword lines separate with semicolons
        // (one of them had reminder text).
        let sep = if line.contains("; ") && !line.contains(", ") {
            "; "
        } else {
            ", "
        };
        let parts: Vec<&str> = line.split(sep).collect();
        // Keywords, or protection from a quality ("Flying, protection
        // from enchantments").
        // "Islandcycling {2}, swampcycling {2}": two typecycling
        // abilities on one line (CR 702.29e), one line each.
        let typecycling = |p: &str| {
            p.split_once(' ').is_some_and(|(w, cost)| {
                w.len() > "cycling".len()
                    && w.to_ascii_lowercase().ends_with("cycling")
                    && w.chars().all(|c| c.is_ascii_alphabetic())
                    && cost.starts_with('{')
                    && !cost.contains(' ')
            })
        };
        // "Flying, ward {2}", "Trample, firebending 4, haste": a keyword
        // with a cost or number among plain ones.
        let with_param = |p: &str| {
            p.split_once(' ').is_some_and(|(w, rest)| {
                matches!(
                    w.to_ascii_lowercase().as_str(),
                    "ward" | "firebending" | "rampage"
                ) && (rest.starts_with('{') && !rest.contains(' ')
                    || rest == "X"
                    || rest.chars().all(|c| c.is_ascii_digit()))
            })
        };
        let all_keywords = parts.iter().all(|p| {
            Keyword::ALL
                .iter()
                .any(|k| keyword_name(*k).eq_ignore_ascii_case(p))
                || p.to_ascii_lowercase().starts_with("protection from ")
                || p.to_ascii_lowercase().starts_with("hexproof from ")
                || with_param(p)
        }) || parts.iter().all(|p| typecycling(p));
        // "Protection from Spirits and from Arcane": one protection
        // ability per quality (CR 702.16g), so one line each.
        let protections = |p: &str| -> Option<Vec<String>> {
            let rest = p
                .strip_prefix("Protection from ")
                .or_else(|| p.strip_prefix("protection from "))?;
            let qs: Vec<&str> = rest.split(" and from ").collect();
            (qs.len() > 1 && qs.iter().all(|q| !q.contains(' ')))
                .then(|| qs.iter().map(|q| format!("Protection from {q}")).collect())
        };
        if parts.len() == 1
            && let Some(ps) = protections(parts[0])
        {
            out.extend(ps);
        } else if parts.len() > 1 && all_keywords {
            for p in parts {
                match protections(p) {
                    Some(ps) => out.extend(ps),
                    None => out.push(capitalize(p)),
                }
            }
        } else {
            out.push(line);
        }
    }
    out.join("\n")
}

/// "this spell" / "this creature" in "If this spell was kicked".
/// A card with gift (CR 702.174): its kicker conditions are whether the
/// gift was promised (the engine keeps a promised gift as the kicker
/// flag).
fn has_gift(card: &Card) -> bool {
    card.abilities
        .iter()
        .any(|a| matches!(a, Ability::Mechanic(crate::ast::Mechanic::Gift(_))))
}

/// "this spell was kicked", or "the gift was promised" on a card with
/// gift.
fn kicked_text(card: &Card) -> String {
    if has_gift(card) {
        "the gift was promised".to_string()
    } else {
        format!("this {} was kicked", kicked_noun(card))
    }
}

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

/// "choose one or more", "choose up to two", "choose one that hasn't been
/// chosen [this turn]" (CR 700.2).
fn mode_head_text(head: crate::ast::ModeHead) -> String {
    match head {
        crate::ast::ModeHead::OneOrMore => "choose one or more".to_string(),
        crate::ast::ModeHead::UpTo(n) => {
            format!("choose up to {}", count_words(Num::N(u32::from(n)), "one"))
        }
        crate::ast::ModeHead::Unchosen { this_turn } => format!(
            "choose one that hasn't been chosen{}",
            if this_turn { " this turn" } else { "" }
        ),
    }
}

/// "Search your library and/or graveyard for a card named Ajani, Valiant
/// Protector, reveal it, and put it into your hand. If you search your
/// library this way, shuffle." and "Search your library for a card named
/// Nissa's Chosen, put it onto the battlefield, then shuffle."
fn search_named_text(e: &Effect, ctx: &mut Ctx) -> Clause {
    let Effect::Search {
        filter,
        to,
        reveal,
        who,
        may,
        graveyard,
        name,
        ..
    } = e
    else {
        unreachable!("a search");
    };
    let what = match name {
        Some(n) => format!("a card named {}", n.replace('|', " or ")),
        None if filter == &Filter::default() => "a card".to_string(),
        None => with_article(&cards_phrase(filter, ctx.card, false)),
    };
    let third = who.is_some_and(|w| w != Recipient::You);
    let (search, reveal_v, put, library, hand, you, shuffle) = if third {
        (
            if *may { "may search" } else { "searches" },
            "reveals",
            "puts",
            "their library",
            "their hand",
            "they",
            "they shuffle",
        )
    } else {
        (
            if *may { "may search" } else { "search" },
            "reveal",
            "put",
            "your library",
            "your hand",
            "you",
            "shuffle",
        )
    };
    let place = match to {
        mtg_core::card::SearchTo::Hand => format!("into {hand}"),
        mtg_core::card::SearchTo::Battlefield { tapped: false } => {
            "onto the battlefield".to_string()
        }
        mtg_core::card::SearchTo::Battlefield { tapped: true } => {
            "onto the battlefield tapped".to_string()
        }
        _ => "(unsupported)".to_string(),
    };
    let text = if *graveyard {
        let zones = format!("{library} and/or graveyard");
        let body = if *reveal {
            format!("{search} {zones} for {what}, {reveal_v} it, and {put} it {place}")
        } else {
            format!("{search} {zones} for {what} and {put} it {place}")
        };
        format!("{body}. If {you} search {library} this way, {shuffle}")
    } else if *reveal {
        format!("{search} {library} for {what}, {reveal_v} it, {put} it {place}, then {shuffle}")
    } else {
        format!("{search} {library} for {what}, {put} it {place}, then {shuffle}")
    };
    match who {
        Some(w) if *w != Recipient::You => {
            let s = ctx.subj(*w);
            Clause::with_subject(*w, s, text)
        }
        _ => Clause::imperative(text),
    }
}
