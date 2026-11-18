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
    Ability, Affects, Card, CdaCount, Cost, CostItem, Effect, Filter, Join, Kind, Mod, Recipient,
    Replacement, Replaces, SacCost, Sentence, StaticCond, Target, Trigger, TriggerObj,
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

/// A count in words ("a", "two", "X").
fn count_words(n: Num, article: &str) -> String {
    match n {
        Num::X | Num::Count(_) => "X".to_string(),
        Num::N(1) => article.to_string(),
        Num::N(n) => NUMBER_WORDS
            .get(n as usize - 1)
            .map_or_else(|| n.to_string(), |w| (*w).to_string()),
    }
}

fn digits(n: Num) -> String {
    match n {
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
        "Plains" | "Equipment" | "Kavu" | "Sheep" | "Fish" | "Moonfolk" | "Kithkin" | "Merfolk"
        | "Samurai" | "Kor" | "Vedalken" | "Aetherborn" | "Eldrazi" | "Treefolk" => {
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
            Mana::Any => "{?}".to_string(),
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
        k => k.name(),
    }
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
    )
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
            Keyword::MustAttack | Keyword::Unblockable | Keyword::DoesntUntap
        ) {
            parts.push(keyword_name(*k).to_string());
        }
    }
    list(&parts)
}

/// A keyword as a line of text: the name, or a restriction as a sentence
/// about the card.
fn keyword_line(card: &Card, k: Keyword) -> String {
    if k == Keyword::DoesntUntap {
        return format!(
            "This {} doesn't untap during your untap step.",
            self_noun(card)
        );
    }
    if is_restriction(k) {
        format!("This {} {}.", self_noun(card), keyword_name(k))
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

/// "a" or "an".
fn article(word: &str) -> &'static str {
    match word.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u' | 'A' | 'E' | 'I' | 'O' | 'U') => "an",
        _ => "a",
    }
}

fn with_article(s: &str) -> String {
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
    let p = filter_phrase(f, card, false);
    if f.types.is_empty() && f.subtypes.is_empty() && f.any_types.is_empty() {
        p.replace(" permanent", "").replace("permanent", "")
    } else {
        p
    }
}

/// The subject of a cost change: "Artifact spells", "Instant and
/// sorcery spells", "Spells".
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
    /// The ability being rendered, for "except it has this ability".
    current: Option<&'a Ability>,
    /// The trigger of the ability, for wording that depends on it.
    trigger: Option<&'a Trigger>,
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
            current: None,
            trigger: None,
        }
    }

    /// The source in subject position ("this creature", "Lightning Bolt",
    /// then "it").
    fn source(&mut self) -> String {
        if self.source_mentioned {
            return "it".to_string();
        }
        self.source_mentioned = true;
        if is_spell(self.card) {
            self.card.name.clone()
        } else {
            format!("this {}", self_noun(self.card))
        }
    }

    fn target_first(&mut self, i: usize) -> String {
        self.target_mentioned[i] = true;
        let phrase = self.target_phrase_of(i);
        if matches!(self.targets[i], Target::UpTo(_)) {
            format!("up to one {phrase}")
        } else {
            phrase
        }
    }

    fn target_phrase_of(&self, i: usize) -> String {
        match self.targets[i].base() {
            Target::UpTo(_) => unreachable!("base"),
            Target::Spec(s) => target_phrase(*s),
            Target::CreatureOfSubtype(sub) => format!("target {sub} creature"),
            Target::Filter(f) if f.relation == Relation::Other => {
                let mut g = f.clone();
                g.relation = Relation::Any;
                format!("another target {}", filter_phrase(&g, self.card, false))
            }
            Target::Filter(f) => format!("target {}", filter_phrase(f, self.card, false)),
            Target::Spell(f) => format!("target {} spell", card_filter_phrase(f, self.card)),
            Target::GraveyardCard { filter, who } => {
                let whose = match who {
                    Who::You => "your",
                    Who::Opponents => "an opponent's",
                    Who::Any => "a",
                };
                let mut g = filter.clone();
                let tgt = if g.relation == Relation::Other {
                    g.relation = Relation::Any;
                    "another target"
                } else {
                    "target"
                };
                if g == Filter::default() {
                    format!("{tgt} card from {whose} graveyard")
                } else {
                    format!(
                        "{tgt} {} card from {whose} graveyard",
                        card_filter_phrase(&g, self.card)
                    )
                }
            }
        }
    }

    /// A recipient as the object of a verb.
    fn obj(&mut self, r: Recipient) -> String {
        match r {
            Recipient::Target(i) => {
                if self.target_mentioned[i] {
                    "it".to_string()
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
            Recipient::Source => {
                if self.source_mentioned {
                    "it".to_string()
                } else {
                    self.source_mentioned = true;
                    if is_spell(self.card) {
                        self.card.name.clone()
                    } else {
                        format!("this {}", self_noun(self.card))
                    }
                }
            }
            Recipient::TriggeringObject => "that creature".to_string(),
            Recipient::TriggeringPlayer => "that player".to_string(),
            Recipient::ControllerOf(_) => "its controller".to_string(),
            Recipient::OwnerOf(_) => "its owner".to_string(),
            Recipient::DefendingPlayer => "the player or planeswalker it's attacking".to_string(),
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
            Recipient::SameObject | Recipient::CreatedObject => "it".to_string(),
            Recipient::Group(i) => {
                self.groups_mentioned.insert(r);
                format!("each {}", self.group_phrase(i, false))
            }
            Recipient::Attached => filter_phrase(&Filter::enchanted(), self.card, false),
        }
    }

    /// The filter of the card's `i`th group, as a noun phrase.
    fn group_phrase(&self, i: u16, plural_form: bool) -> String {
        match self.card.groups.get(usize::from(i)) {
            Some(f) => filter_phrase(f, self.card, plural_form),
            None => "permanent".to_string(),
        }
    }

    /// A recipient as the subject of a sentence.
    fn subj(&mut self, r: Recipient) -> String {
        match r {
            Recipient::Target(i) | Recipient::ThatTarget(i) => {
                if self.target_mentioned[i] {
                    "it".to_string()
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
                if self.groups_mentioned.insert(r) {
                    self.group_phrase(i, true)
                } else {
                    "they".to_string()
                }
            }
            Recipient::Source => self.source(),
            other => self.obj(other),
        }
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
            let noun = match self.targets[fresh[0]].base() {
                Target::UpTo(_) => unreachable!("base"),
                Target::Spec(TargetSpec::CreatureCardInYourGraveyard) => {
                    "creature cards from your graveyard".to_string()
                }
                Target::Spec(s) => pluralize(&target_phrase(*s).replace("target ", "")),
                Target::CreatureOfSubtype(sub) => format!("{sub} creatures"),
                Target::Filter(f) => filter_phrase(f, self.card, true),
                Target::Spell(f) => format!("{} spells", filter_phrase(f, self.card, false)),
                Target::GraveyardCard { filter, who } => {
                    let whose = match who {
                        Who::You => "your graveyard",
                        Who::Opponents => "an opponent's graveyard",
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
            };
            return format!(
                "{up_to}{} target {noun}",
                count_words(Num::N(u32::try_from(fresh.len()).expect("small")), "a")
            );
        }
        let items: Vec<String> = rs
            .iter()
            .map(|r| if all { self.obj_all(*r) } else { self.obj(*r) })
            .collect();
        list(&items)
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
        )
    }

    fn verb(&self, r: Recipient, singular: &'static str, plural: &'static str) -> &'static str {
        let _ = self;
        if r == Recipient::You || Self::is_plural(r) {
            plural
        } else {
            singular
        }
    }
}

fn target_noun(t: &Target) -> String {
    match t.base() {
        Target::UpTo(_) => unreachable!("base"),
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
            | TargetSpec::Filtered(_) => "permanent".to_string(),
            TargetSpec::Artifact => "artifact".to_string(),
            TargetSpec::Enchantment => "enchantment".to_string(),
            TargetSpec::CreatureCardInYourGraveyard | TargetSpec::GraveyardCard { .. } => {
                "card".to_string()
            }
            TargetSpec::Spell
            | TargetSpec::CreatureSpell
            | TargetSpec::NoncreatureSpell
            | TargetSpec::SpellFiltered(_) => "spell".to_string(),
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
    match s {
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
        Some(Target::Filter(f)) if f.types.len() + f.any_types.len() > 1 => "permanent",
        _ => "creature",
    }
}

// ---- filters ----

/// The noun phrase of a filter without an article, singular ("creature you
/// control", "nonbasic land") or plural.
#[allow(clippy::too_many_lines)] // one block per filter part
fn filter_phrase(f: &Filter, card: &Card, plural_form: bool) -> String {
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
    let (b, a) = crate::format::factor_qualifiers(&base, alt);
    format!(
        "{} or {}",
        filter_phrase_one(&b, card, plural_form),
        filter_phrase(&a, card, plural_form)
    )
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
    if let Some(t) = f.tapped {
        words.push(if t { "tapped" } else { "untapped" }.to_string());
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
        let _ = write!(s, " with mana value {n}{}", cmp_words(cmp));
    }
    if !f.keywords.is_empty() {
        let _ = write!(s, " with {}", keywords_list(&f.keywords));
    }
    if !f.not_keywords.is_empty() {
        let _ = write!(s, " without {}", keywords_list(&f.not_keywords));
    }
    match f.who {
        Some(Who::You) => s.push_str(" you control"),
        Some(Who::Opponents) if f.dont_control => s.push_str(" you don't control"),
        Some(Who::Opponents) => s.push_str(" an opponent controls"),
        Some(Who::Any) | None => {}
    }
    if f.no_abilities {
        s.push_str(" with no abilities");
    }
    if f.damaged {
        s.push_str(" that was dealt damage this turn");
    }
    s
}

/// A filter as a kind of card, with an article: "a creature card", "a
/// nonblack card".
fn card_filter_a(f: &Filter, card: &Card) -> String {
    let p = filter_phrase(f, card, false);
    let p = p.strip_suffix(" permanent").unwrap_or(&p);
    let p = if p == "permanent" { "" } else { p };
    if p.is_empty() {
        "a card".to_string()
    } else {
        with_article(&format!("{p} card"))
    }
}

/// A filter with an article: "a creature you control", "another creature".
fn filter_a(f: &Filter, card: &Card) -> String {
    let p = filter_phrase(f, card, false);
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
            CostItem::Exile => {
                if cost.exile_self {
                    parts.push("Exile this card from your graveyard".to_string());
                }
            }
            CostItem::Discard => {
                if cost.discard_self {
                    parts.push("Discard this card".to_string());
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

fn token_description(name: &str, count: Num, ctx: &Ctx) -> String {
    let Some(token) = ctx.lookup.get(name) else {
        return format!(
            "{} {name} {}",
            count_words(count, "a"),
            plural(count, "token")
        );
    };
    // Predefined tokens (CR 111.10: Treasure, Food, Clue, ...) are named,
    // not described: "create a Treasure token".
    if token.pt.is_none()
        && !token.types.contains(&CardType::Creature)
        && token.subtypes.len() == 1
        && token.subtypes[0] == token.display_name.as_deref().unwrap_or(&token.name)
        && token.keywords.is_empty()
    {
        let sub = &token.subtypes[0];
        return format!(
            "{} {sub} {}",
            count_words(count, article(sub)),
            plural(count, "token")
        );
    }
    let mut words: Vec<String> = Vec::new();
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
    if !token.keywords.is_empty() {
        let _ = write!(s, " with {}", keywords_list(&token.keywords));
    }
    let abilities: Vec<String> = token
        .abilities
        .iter()
        .map(|a| ability_text(token, a, ctx.lookup))
        .collect();
    if !abilities.is_empty() {
        // Ability text is quoted after the sentence.
        let _ = write!(s, ". It has \"{}\"", abilities.join("\n"));
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
fn x_sign(v: i32) -> &'static str {
    match v.cmp(&0) {
        std::cmp::Ordering::Equal => "+0",
        std::cmp::Ordering::Greater => "+X",
        std::cmp::Ordering::Less => "-X",
    }
}

/// The variable amounts an effect uses.
fn nums_of(e: &Effect) -> Vec<Num> {
    match e {
        Effect::DealDamage { amount, .. }
        | Effect::GainLife { amount, .. }
        | Effect::LoseLife { amount, .. } => vec![*amount],
        Effect::DrawCards { count, .. }
        | Effect::Mill { count, .. }
        | Effect::Discard { count, .. }
        | Effect::AddCounters { count, .. }
        | Effect::AddManaTimes { count, .. }
        | Effect::CreateToken { count, .. } => vec![*count],
        _ => Vec::new(),
    }
}

/// "the number of creatures you control" (or "for each creature you
/// control" with `each`).
fn count_phrase(c: &crate::ast::CountExpr, card: &Card, each: bool) -> String {
    use crate::ast::CountExpr;
    let (one, many) = match c {
        CountExpr::Permanents(f) => {
            let place = if f.who.is_none() {
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
                let p = card_filter_phrase(f, card);
                (
                    format!("{p} card in your graveyard"),
                    format!("{p} cards in your graveyard"),
                )
            }
        },
        CountExpr::SourcePower | CountExpr::TargetPower(_) => {
            return if each {
                "for each power".to_string()
            } else {
                "its power".to_string()
            };
        }
        CountExpr::Life => return "your life total".to_string(),
    };
    if each {
        format!("for each {one}")
    } else {
        format!("the number of {many}")
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
            CountStyle::Where => {
                let _ = write!(
                    c.tail,
                    ", where X is {}",
                    count_phrase(&count.expr, ctx.card, false)
                );
            }
            CountStyle::Equal => {
                c.text = c.text.replacen("X ", "", 1);
                let _ = write!(
                    c.tail,
                    " equal to {}",
                    count_phrase(&count.expr, ctx.card, false)
                );
            }
            CountStyle::Each => {
                c.text = c
                    .text
                    .replacen("X ", "a ", 1)
                    .replacen("a cards", "a card", 1);
                let _ = write!(c.tail, " {}", count_phrase(&count.expr, ctx.card, true));
            }
        }
    }
    c
}

#[allow(clippy::too_many_lines)] // one arm per effect kind
fn clause_inner(e: &Effect, ctx: &mut Ctx) -> Clause {
    match e {
        Effect::DealDamage { amount, to } => {
            let s = ctx.source();
            let objs = ctx.obj_list(to, false);
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
                Some(c) => {
                    let x = |v: i32| match v {
                        0 => "+0".to_string(),
                        v if v > 0 => "+X".to_string(),
                        _ => "-X".to_string(),
                    };
                    (
                        format!("{}/{}", x(*power), x(*toughness)),
                        format!(", where X is {}", count_phrase(&c.expr, ctx.card, false)),
                    )
                }
                None => (
                    format!("{}/{}", x_sign(*power), x_sign(*toughness)),
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
            Clause::with_subject(
                *to,
                s,
                format!(
                    "{} {}",
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
            let s = ctx.subj(*who);
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{} {} life",
                    ctx.verb(*who, "gains", "gain"),
                    digits(*amount)
                ),
            )
        }
        Effect::Proliferate => Clause::imperative("proliferate".to_string()),
        Effect::Manifest => Clause::imperative("manifest the top card of your library".to_string()),
        Effect::Adapt(n) => Clause::imperative(format!("adapt {n}")),
        Effect::Discover(n) => Clause::imperative(format!("discover {n}")),
        Effect::Connive { what } => {
            let s = ctx.subj(*what);
            Clause::with_subject(*what, s, "connives".to_string())
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
                } else if two {
                    format!("{word} on the bottom of your library")
                } else {
                    format!("{word} on the bottom of your library in any order")
                }
            };
            let what = filter.as_ref().map(|f| card_filter_a(f, ctx.card));
            let body = match (what, all, optional) {
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
                    "You may reveal {w} from among them and put it into your hand. Put {}",
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
            Clause::imperative(format!("{head}. {body}"))
        }
        Effect::Modes(modes) => {
            let mut out = "choose one —".to_string();
            for m in modes {
                let mut inner = Ctx::new(ctx.card, ctx.lookup, &m.targets);
                inner.current = ctx.current;
                out.push_str("\n• ");
                out.push_str(&sentences(&m.effects, &mut inner));
            }
            Clause::imperative(out)
        }
        Effect::GrantAbility { ability, to } => {
            let s = ctx.subj(*to);
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
        Effect::SkipNextUntap { to } => {
            let s = ctx.subj(*to);
            Clause::with_subject(
                *to,
                s,
                "doesn't untap during its controller's next untap step".to_string(),
            )
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
        Effect::RemoveCounters { kind, count, to } => {
            let c = counters(*count, *kind);
            let o = ctx.obj(*to);
            Clause::imperative(format!("remove {c} from {o}"))
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
            let plural = what.len() > 1;
            let o = if what.len() == 1 {
                ctx.obj(what[0])
            } else {
                ctx.obj_list(what, false)
            };
            let hand = match (yours, plural) {
                (true, _) => "your hand",
                (false, false) => "its owner's hand",
                (false, true) => "their owners' hands",
            };
            Clause::imperative(format!("return {o} to {hand}"))
        }
        Effect::ReturnToBattlefield { what, counters: c } => {
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
            let mut text = if yours {
                format!("return {o} to the battlefield")
            } else {
                format!("return {o} to the battlefield under its owner's control")
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
        Effect::Sacrifice { what } => {
            let o = ctx.obj(*what);
            Clause::imperative(format!("sacrifice {o}"))
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
            let s = ctx.subj(*to);
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
                format!("this {}", self_noun(ctx.card))
            } else {
                ctx.source()
            };
            Clause::imperative(format!("attach {me} to {o}"))
        }
        Effect::CreateToken { token, count } => {
            Clause::imperative(format!("create {}", token_description(token, *count, ctx)))
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
        Effect::Counter { to } => {
            let o = ctx.obj(*to);
            Clause::imperative(format!("counter {o}"))
        }
        Effect::CounterUnlessPays { to, mana } => {
            let o = ctx.obj(*to);
            Clause::imperative(format!("counter {o} unless its controller pays {mana}"))
        }
        Effect::Scry(n) => Clause::imperative(format!("scry {}", digits(*n))),
        Effect::Investigate => Clause::imperative("investigate".to_string()),
        Effect::PutInLibrary { what, top } => {
            let o = ctx.obj(*what);
            let place = if *top {
                "on top of"
            } else {
                "on the bottom of"
            };
            Clause::imperative(format!("put {o} {place} its owner's library"))
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
        Effect::DiscardChosen { who, filter } => {
            let s = ctx.subj(*who);
            let what = match filter {
                None => "a card".to_string(),
                Some(f) => with_article(&format!("{} card", card_filter_phrase(f, ctx.card))),
            };
            Clause::with_subject(
                *who,
                s,
                format!(
                    "{} their hand. You choose {what} from it. That player discards that card",
                    ctx.verb(*who, "reveals", "reveal")
                ),
            )
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
        } => {
            let one = *count == Num::N(1) && !*up_to;
            let what = if filter == &Filter::default() {
                if one {
                    "a card".to_string()
                } else {
                    format!("{} cards", count_words(*count, "a"))
                }
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
            let pron = if one { it } else { them };
            let mut text = format!("search your library for {what}, ");
            if *reveal {
                let _ = write!(text, "reveal {}, ", if one { "it" } else { "them" });
            }
            match to {
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
        Effect::AddManaTimes { mana, .. } => Clause::imperative(format!(
            "add {}",
            mana_symbols(&[*mana], "X mana of any one color")
        )),
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
        } => {
            let s = ctx.subj(*to);
            let mut words = vec![format!("{power}/{toughness}")];
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
        Effect::IfKicked(inner) => {
            let body = sentences(inner, ctx);
            let body = lowercase_first(&body);
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
            Clause::imperative(format!("you may {}", body.trim_end_matches('.')))
        }
        Effect::Delayed {
            about: _,
            trigger,
            effects,
            trigger_first,
        } => {
            let was = ctx.in_delayed;
            ctx.in_delayed = true;
            let body = sentences(effects, ctx);
            ctx.in_delayed = was;
            let body = body.trim_end_matches('.').to_string();
            let t = trigger_text(trigger, ctx, true);
            if *trigger_first {
                Clause::imperative(format!("{t}, {}", lowercase_first(&body)))
            } else {
                Clause::imperative(format!("{} {t}", lowercase_first(&body)))
            }
        }
        Effect::CreateReplacement(r) => {
            Clause::imperative(lowercase_first(&replacement_text(r, ctx, true)))
        }
        Effect::Rule(RuleKind::NoLifeGain) => {
            Clause::imperative("players can't gain life this turn".to_string())
        }
        Effect::Rule(RuleKind::DamageCantBePrevented) => {
            Clause::imperative("damage can't be prevented this turn".to_string())
        }
    }
}

fn lowercase_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// One sentence (no final period, lowercase start).
fn sentence_text(s: &Sentence, ctx: &mut Ctx) -> String {
    let clauses: Vec<Clause> = s.effects.iter().map(|e| clause(e, ctx)).collect();
    let shared_suffix = clauses.len() > 1
        && clauses[0].suffix.is_some()
        && clauses.iter().all(|c| c.suffix == clauses[0].suffix);
    let mut out = String::new();
    if s.eot_first {
        out.push_str("until end of turn, ");
    }
    let mut prev_subject: Option<Recipient> = None;
    for (i, c) in clauses.iter().enumerate() {
        if i > 0 {
            out.push_str(match s.joins.get(i - 1) {
                Some(Join::Then) => ", then ",
                _ => " and ",
            });
        }
        match &c.subject {
            Some((r, text)) => {
                if prev_subject != Some(*r) {
                    out.push_str(text);
                    out.push(' ');
                }
                prev_subject = Some(*r);
            }
            None => prev_subject = None,
        }
        out.push_str(&c.text);
        if let Some(suffix) = c.suffix
            && !shared_suffix
            && !s.eot_first
        {
            out.push(' ');
            out.push_str(suffix);
        }
        out.push_str(&c.tail);
    }
    if shared_suffix && !s.eot_first {
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
        out.push_str(&sentence(&sentence_text(s, ctx)));
    }
    out
}

/// Sentences after a trigger or cost: the first starts lowercase after a
/// trigger's comma, uppercase after a cost's colon.
fn effects_after(ss: &[Sentence], ctx: &mut Ctx, lowercase: bool) -> String {
    let s = sentences(ss, ctx);
    if lowercase && !s.starts_with(&ctx.card.name) {
        lowercase_first(&s)
    } else {
        s
    }
}

// ---- triggers ----

fn step_name(step: Step, whose: Who) -> String {
    let name = match step {
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
    let this = |ctx: &mut Ctx| {
        ctx.source_mentioned = true;
        format!("this {}", self_noun(ctx.card))
    };
    match t {
        Trigger::Enters(TriggerObj::This) => format!("when {} enters", this(ctx)),
        Trigger::Enters(TriggerObj::Filter(f)) => {
            format!("whenever {} enters", filter_a(f, ctx.card))
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
            if f.relation == Relation::Other {
                format!("whenever {} dies", filter_a(f, ctx.card))
            } else {
                let t = this(ctx);
                format!(
                    "whenever {t} or another {} dies",
                    filter_phrase(f, ctx.card, false)
                )
            }
        }
        Trigger::EveryStep(step) => {
            let name = step_name(*step, Who::You);
            format!(
                "at the beginning of each {}",
                name.trim_start_matches("your ")
            )
        }
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
        Trigger::Attacks(TriggerObj::Filter(f)) => {
            format!("whenever {} attacks", filter_a(f, ctx.card))
        }
        Trigger::AttacksAlone(TriggerObj::This) => {
            format!("whenever {} attacks alone", this(ctx))
        }
        Trigger::AttacksAlone(TriggerObj::Filter(f)) => {
            format!("whenever {} attacks alone", filter_a(f, ctx.card))
        }
        Trigger::BecomesBlocked(TriggerObj::This) => {
            format!("whenever {} becomes blocked", this(ctx))
        }
        Trigger::BecomesBlocked(TriggerObj::Filter(f)) => {
            format!("whenever {} becomes blocked", filter_a(f, ctx.card))
        }
        Trigger::BlocksOrBlocked(TriggerObj::This) => {
            format!("whenever {} blocks or becomes blocked", this(ctx))
        }
        Trigger::BlocksOrBlocked(TriggerObj::Filter(f)) => {
            format!(
                "whenever {} blocks or becomes blocked",
                filter_a(f, ctx.card)
            )
        }
        Trigger::Blocks(TriggerObj::This) => format!("whenever {} blocks", this(ctx)),
        Trigger::Blocks(TriggerObj::Filter(f)) => {
            format!("whenever {} blocks", filter_a(f, ctx.card))
        }
        Trigger::DealsCombatDamageToPlayer(TriggerObj::This) => {
            format!("whenever {} deals combat damage to a player", this(ctx))
        }
        Trigger::DealsCombatDamageToPlayer(TriggerObj::Filter(f)) => {
            format!(
                "whenever {} deals combat damage to a player",
                filter_a(f, ctx.card)
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
            format!("whenever {who} {} spell", with_article(&what))
        }
        Trigger::TurnedFaceUp => format!("when {} is turned face up", this(ctx)),
        Trigger::Exploits => format!("when {} exploits a creature", this(ctx)),
        Trigger::TargetedByOpponent => format!(
            "whenever {} becomes the target of a spell or ability an opponent controls",
            this(ctx)
        ),
        Trigger::BecomesTarget(TriggerObj::This) => format!(
            "when {} becomes the target of a spell or ability",
            this(ctx)
        ),
        Trigger::BecomesTarget(TriggerObj::Filter(f))
        | Trigger::BecomesTargetWhenever(TriggerObj::Filter(f)) => format!(
            "whenever {} becomes the target of a spell or ability",
            filter_a(f, ctx.card)
        ),
    }
}

/// A trigger's subject: "this creature" or "a creature you control".
fn trigger_subject(o: &TriggerObj, ctx: &mut Ctx) -> String {
    match o {
        TriggerObj::This => {
            ctx.source_mentioned = true;
            format!("this {}", self_noun(ctx.card))
        }
        TriggerObj::Filter(f) => filter_a(f, ctx.card),
    }
}

fn condition_text(c: Condition, ctx: &mut Ctx) -> String {
    match c {
        Condition::SourceUntapped => {
            format!("if this {} is untapped", self_noun(ctx.card))
        }
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
            format!("this {}", self_noun(ctx.card))
        }
        Affects::You => "you".to_string(),
        Affects::Players(Who::Opponents) => "an opponent".to_string(),
        Affects::Players(_) => "a player".to_string(),
        Affects::Objects(f) => filter_phrase(f, ctx.card, true),
        Affects::YouAnd(f) => format!("you and {}", filter_phrase(f, ctx.card, true)),
        Affects::Target(i) => ctx.obj(Recipient::Target(*i)),
    }
}

/// A replacement as a sentence (capitalized, terminated). `this_turn`
/// renders the until-end-of-turn form a spell creates.
#[allow(clippy::too_many_lines)]
fn replacement_text(r: &Replacement, ctx: &mut Ctx, this_turn: bool) -> String {
    let mut out = match (&r.event, &r.outcome) {
        (Replaces::Damage { from, to, combat }, Outcome::Prevent) if this_turn => {
            let kind = match combat {
                Some(true) => "combat damage",
                Some(false) => "noncombat damage",
                None => "damage",
            };
            let from = from
                .as_ref()
                .map(|f| format!(" by {}", filter_phrase(f, ctx.card, true)))
                .unwrap_or_default();
            let to = match to {
                Affects::Any => String::new(),
                a => format!(" to {}", affects_to(a, ctx)),
            };
            format!("Prevent all {kind} that would be dealt{to}{from} this turn.")
        }
        (Replaces::Damage { to, combat, .. }, Outcome::PreventNext(n)) => {
            let kind = match combat {
                Some(true) => "combat damage",
                Some(false) => "noncombat damage",
                None => "damage",
            };
            let to = match to {
                Affects::Objects(f) => format!("each {}", filter_phrase(f, ctx.card, false)),
                other => affects_to(other, ctx),
            };
            format!("Prevent the next {n} {kind} that would be dealt to {to} this turn.")
        }
        (Replaces::Damage { from, to, combat }, Outcome::Double) => {
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
                Some(f) => format!(
                    "If {} would deal {kind} to {to_text}, it deals double that {kind} instead.",
                    filter_a(f, ctx.card)
                ),
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
            format!("This {} enters tapped.", self_noun(ctx.card))
        }
        (Replaces::Enters, Outcome::EnterWithCounters { kind, n }) => format!(
            "This {} enters with {} on it.",
            self_noun(ctx.card),
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
        Mod::AddSubtype(s) => format!(
            "{} {} in addition to its other types",
            v("is", "are"),
            with_article(s)
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
            if !restricted.is_empty() {
                parts.push(restriction_phrase(&restricted));
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
        CdaCount::Permanents(f) => format!("the number of {}", filter_phrase(f, card, true)),
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
            format!("{}'s", card.name)
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
            card.name.clone()
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
            Mod::AddTypes(_) | Mod::AddLandType(_) | Mod::AddSubtype(_) | Mod::AddColors(_)
        )
    });
    let (subject, plural_form) = if filter.relation == Relation::This {
        (format!("This {}", self_noun(card)), false)
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
            && filter.relation == Relation::Any;
        let p = filter_phrase(filter, card, true);
        if bare || filter.all {
            (format!("All {p}"), true)
        } else {
            (capitalize(&p), true)
        }
    };
    let parts: Vec<String> = mods
        .iter()
        .map(|m| {
            if let Mod::AddAbility(a) = m {
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
                ..
            } = m
                && let Some(c) = card.counts.get(usize::from(*i))
            {
                t.push(' ');
                t.push_str(&count_phrase(&c.expr, card, true));
            }
            t
        })
        .collect();
    let body = format!("{subject} {}", list(&parts));
    if body.ends_with(".\"") {
        body
    } else {
        format!("{body}.")
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
        StaticCond::YouControlNone(filter) => {
            let mut f = filter.clone();
            f.who = None;
            format!("you control no {}", filter_phrase(&f, card, true))
        }
        StaticCond::AttackedThisTurn => "you attacked this turn".to_string(),
        StaticCond::CreatureDiedThisTurn => "a creature died this turn".to_string(),
        StaticCond::OpponentDamagedThisTurn => "an opponent was dealt damage this turn".to_string(),
        StaticCond::Escaped => "it escaped".to_string(),
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
    }
}

/// A static ability's text with its condition: "As long as <cond>, <text>"
/// or "<text> as long as <cond>." ("During your turn, ..." for the turn).
fn conditional_static(body: &str, c: &StaticCond, first: bool, card: &Card) -> String {
    let mut cond = static_cond_text(c, card);
    // "as long as it's enchanted" about the subject; "as long as this
    // creature is enchanted" when the subject is something else.
    if matches!(
        c,
        StaticCond::Attacking | StaticCond::Enchanted | StaticCond::Equipped
    ) && !body.starts_with("This ")
    {
        cond = cond.replace("it's", &format!("this {} is", self_noun(card)));
    }
    if first {
        let this = format!("This {} ", self_noun(card));
        let rest = if body.starts_with(&card.name) {
            body.to_string()
        } else if matches!(
            c,
            StaticCond::Attacking | StaticCond::Enchanted | StaticCond::Equipped
        ) && body.starts_with(&this)
        {
            // "As long as this creature is equipped, it gets +1/+1."
            cond = cond.replace("it's", &format!("this {} is", self_noun(card)));
            format!("it {}", &body[this.len()..])
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
        Ability::Mana(m) => {
            let syms: Vec<String> = m
                .iter()
                .map(|m| match m {
                    Mana::Colorless => "{C}".to_string(),
                    Mana::Colored(c) => format!("{{{}}}", c.letter()),
                    Mana::Any => "one mana of any color".to_string(),
                })
                .collect();
            format!("{{T}}: Add {}.", or_list(&syms))
        }
        Ability::SacMana(m) => {
            let what = match m {
                Mana::Colorless => "{C}".to_string(),
                Mana::Colored(c) => format!("{{{}}}", c.letter()),
                Mana::Any => "one mana of any color".to_string(),
            };
            format!("{{T}}, Sacrifice this {}: Add {what}.", self_noun(card))
        }
        Ability::Enchant(t) => match t.base() {
            Target::UpTo(_) => unreachable!("base"),
            Target::Spec(s) => format!("Enchant {}", target_phrase(*s).replace("target ", "")),
            Target::CreatureOfSubtype(sub) => format!("Enchant {sub} creature"),
            Target::Filter(f) | Target::Spell(f) | Target::GraveyardCard { filter: f, .. } => {
                format!("Enchant {}", filter_phrase(f, card, false))
            }
        },
        Ability::Spell { targets, effects } => {
            let mut ctx = Ctx::new(card, lookup, targets);
            ctx.current = Some(a);
            sentences(effects, &mut ctx)
        }
        Ability::Modal(modes) | Ability::ModalBoth(modes) => {
            let mut out = if matches!(a, Ability::ModalBoth(_)) {
                "Choose one or both —".to_string()
            } else {
                "Choose one —".to_string()
            };
            for m in modes {
                let mut ctx = Ctx::new(card, lookup, &m.targets);
                ctx.current = Some(a);
                out.push_str("\n• ");
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
            // Loyalty abilities are sorcery-speed by rule (CR 606.3).
            if cost.sorcery && cost.loyalty.is_none() {
                text.push_str(" Activate only as a sorcery.");
            }
            if cost.once {
                text.push_str(" Activate only once each turn.");
            }
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
                Some(crate::ast::TrigCond::Static(c)) => {
                    out.push_str(", if ");
                    out.push_str(&static_cond_text(c, card));
                    // Oracle names the source again after a condition about
                    // something else ("..., if you control three or more
                    // artifacts, this creature gets +3/+3").
                    ctx.source_mentioned = false;
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
            let body = static_text(filter, mods, card);
            match condition {
                None => body,
                Some(c) => conditional_static(&body, c, *cond_first, card),
            }
        }
        Ability::BlockRule(r) => {
            let me = format!("This {}", self_noun(card));
            let pl = |f: &Filter| filter_phrase(f, card, true);
            match r {
                crate::ast::BlockRule::NotBy(f) => format!("{me} can't be blocked by {}.", pl(f)),
                crate::ast::BlockRule::OnlyBy(f) => {
                    format!("{me} can't be blocked except by {}.", pl(f))
                }
                crate::ast::BlockRule::AtMostOne => {
                    format!("{me} can't be blocked by more than one creature.")
                }
                crate::ast::BlockRule::BlocksOnly(f) => format!("{me} can block only {}.", pl(f)),
                crate::ast::BlockRule::CantBlock(f) => format!("{me} can't block {}.", pl(f)),
            }
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
                spells_subject(filter, card),
                generic.unsigned_abs()
            )
            .replace("  ", " ")
        }
        Ability::SelfCost { per, times } => {
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
            crate::ast::Mechanic::Escape(c, n) => format!(
                "Escape\u{2014}{c}, Exile {} other cards from your graveyard.",
                count_words(Num::N(*n), "a")
            ),
            crate::ast::Mechanic::EscapesWith(n) => format!(
                "This {} escapes with {} on it.",
                self_noun(card),
                counters(Num::N(*n), CounterKind::PlusOnePlusOne)
            ),
            crate::ast::Mechanic::Bloodthirst(n) => format!("Bloodthirst {n}"),
            crate::ast::Mechanic::Megamorph(c) => format!("Megamorph {c}"),
            crate::ast::Mechanic::Disguise(c) => format!("Disguise {c}"),
        },
        Ability::EntersTappedUnless(c) => format!(
            "This {} enters tapped unless {}.",
            self_noun(card),
            static_cond_text(c, card)
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
            format!(
                "This {} can't {what} unless {}.",
                self_noun(card),
                static_cond_text(cond, card)
            )
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
                "You may have this {} enter as a copy of any {} on the battlefield",
                self_noun(card),
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

/// The text of one mode, for naming the choice.
pub fn mode_text(card: &Card, m: &crate::ast::Mode, lookup: &dyn Lookup) -> String {
    let mut ctx = Ctx::new(card, lookup, &m.targets);
    sentences(&m.effects, &mut ctx)
}

/// The card's full Oracle text.
pub fn oracle_text(card: &Card, lookup: &dyn Lookup) -> String {
    let mut lines: Vec<String> = Vec::new();
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
                "This {} can't attack or block alone.",
                self_noun(card)
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
        lines.push(ability_text(card, &card.abilities[i], lookup));
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
