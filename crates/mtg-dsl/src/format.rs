//! Canonical DSL text for a syntax tree: what `parse` accepts, laid out one
//! statement per line. `parse(format(card))` compiles to the same
//! definition as `card`.

use std::fmt::Write as _;

use mtg_core::card::{
    CounterKind, Keyword, Num, PtValue, Relation, Replacement as Outcome, RuleKind, Supertype, Who,
};
use mtg_core::mana::Mana;
use mtg_core::turn::Step;

use crate::ast::{
    Ability, Affects, Card, CdaCount, Cost, CostItem, Effect, Filter, Join, Kind, Mod, Recipient,
    Replaces, SacCost, Sentence, StaticCond, Target, Trigger, TriggerObj,
};
use crate::render::{basic_land_name, color_name, keyword_name, target_phrase};

pub fn format(card: &Card) -> String {
    let outer = COUNTS.with(|c| c.replace(card.counts.clone()));
    let out = format_inner(card);
    COUNTS.with(|c| c.replace(outer));
    out
}

fn format_inner(card: &Card) -> String {
    let mut out = String::new();
    let kind = match card.kind {
        Kind::Card => "card",
        Kind::Token => "token",
        Kind::Emblem => "emblem",
    };
    let _ = writeln!(out, "{kind} \"{}\" {{", card.name);
    if let Some(n) = &card.display_name {
        let _ = writeln!(out, "  name {n:?}");
    }
    if let Some(id) = &card.oracle_id {
        let _ = writeln!(out, "  oracle_id {id}");
    }
    match card.layout {
        mtg_core::card::Layout::Normal => {}
        l => {
            let _ = writeln!(out, "  layout {}", format!("{l:?}").to_lowercase());
        }
    }
    let cost = card.cost.to_string();
    if !cost.is_empty() {
        let _ = writeln!(out, "  cost {cost}");
    }
    let mut type_line: Vec<String> = card
        .supertypes
        .iter()
        .map(|s| format!("{s:?}"))
        .chain(card.types.iter().map(|t| format!("{t:?}")))
        .collect();
    if !card.subtypes.is_empty() {
        type_line.push("—".to_string());
        type_line.extend(card.subtypes.iter().cloned());
    }
    if !type_line.is_empty() {
        let _ = writeln!(out, "  type {}", type_line.join(" "));
    }
    if let Some((p, t)) = card.pt {
        let _ = writeln!(out, "  pt {p}/{t}");
    }
    if let Some(n) = card.loyalty {
        let _ = writeln!(out, "  loyalty {n}");
    }
    if let Some(colors) = &card.color_indicator {
        let names: Vec<&str> = colors.iter().map(|c| color_name(*c)).collect();
        let _ = writeln!(out, "  color {}", names.join(" "));
    }
    if !card.keywords.is_empty() {
        let _ = writeln!(out, "  keywords {}", keywords(&card.keywords));
    }
    if let Some(c) = &card.additional {
        let _ = writeln!(out, "  additional cost {}", cost_text(c));
    }
    if let Some(k) = &card.kicker {
        let _ = writeln!(out, "  kicker {k}");
    }
    if let Some(c) = &card.flashback {
        let _ = writeln!(out, "  flashback {}", cost_text(c));
    }
    for a in &card.abilities {
        ability(&mut out, a, "  ", &card.groups);
    }
    if let Some(x) = &card.except {
        out.push_str("  except {\n");
        if !x.subtypes.is_empty() {
            let _ = writeln!(out, "    subtypes {}", x.subtypes.join(" "));
        }
        if !x.keywords.is_empty() {
            let _ = writeln!(out, "    keywords {}", keywords(&x.keywords));
        }
        for a in &x.abilities {
            ability(&mut out, a, "    ", &card.groups);
        }
        out.push_str("  }\n");
    }
    if let Some(m) = &card.mismatch {
        let _ = writeln!(out, "  oracle_mismatch \"{m}\"");
    }
    for face in &card.faces {
        let text = format(face);
        for (i, line) in text.lines().enumerate() {
            if i == 0 {
                let _ = writeln!(out, "  face \"{}\" {{", face.name);
            } else {
                let _ = writeln!(out, "  {line}");
            }
        }
    }
    out.push_str("}\n");
    out
}

fn keywords(ks: &[Keyword]) -> String {
    ks.iter()
        .map(|k| keyword_name(*k))
        .collect::<Vec<_>>()
        .join(", ")
}

struct Fmt {
    mentioned: Vec<bool>,
    groups: Vec<Filter>,
}

impl Fmt {
    fn recipient(&mut self, r: Recipient, targets: &[Target]) -> String {
        match r {
            Recipient::Target(i) => {
                if self.mentioned[i] {
                    format!("target {}", i + 1)
                } else {
                    self.mentioned[i] = true;
                    let up_to = matches!(targets[i], Target::UpTo(_));
                    let phrase = match targets[i].base() {
                        Target::Spec(s) => target_phrase(*s),
                        Target::CreatureOfSubtype(sub) => format!("target {sub} creature"),
                        Target::Filter(f) => format!("target {}", filter(f)),
                        Target::Spell(f) => format!("target {} spell", filter(f)),
                        Target::GraveyardCard { filter: f, who } => graveyard_target(f, *who),
                        Target::UpTo(_) => unreachable!("base"),
                    };
                    if up_to {
                        format!("up to 1 {phrase}")
                    } else {
                        phrase
                    }
                }
            }
            Recipient::ThatTarget(i) => {
                self.mentioned[i] = true;
                format!("that target {}", i + 1)
            }
            Recipient::You => "you".to_string(),
            Recipient::EachOpponent => "each opponent".to_string(),
            Recipient::Source => "this".to_string(),
            Recipient::TriggeringObject => "that creature".to_string(),
            Recipient::TriggeringPlayer => "that player".to_string(),
            Recipient::DefendingPlayer => "defending player".to_string(),
            Recipient::ControllerOf(_) => "its controller".to_string(),
            Recipient::OwnerOf(_) => "its owner".to_string(),
            Recipient::EachCreature => "each creature".to_string(),
            Recipient::Attached => "attached".to_string(),
            Recipient::EachPlayer => "each player".to_string(),
            Recipient::EachCreatureYouControl => "each creature you control".to_string(),
            Recipient::EachPermanentYouControl => "each permanent you control".to_string(),
            Recipient::SameObject => "it".to_string(),
            Recipient::CreatedObject => "that token".to_string(),
            // `all`, not `each`: "each creature you control" is the
            // built-in recipient, which prints differently.
            Recipient::Group(i) => match self.groups.get(usize::from(i)) {
                Some(f) => format!("all {}", filter(f)),
                None => "all permanent".to_string(),
            },
        }
    }

    fn recipients(&mut self, rs: &[Recipient], targets: &[Target]) -> String {
        rs.iter()
            .map(|r| self.recipient(*r, targets))
            .collect::<Vec<_>>()
            .join(" and ")
    }

    fn affects(&mut self, a: &Affects, targets: &[Target]) -> String {
        match a {
            Affects::Any => "any".to_string(),
            Affects::This => "this".to_string(),
            Affects::You => "you".to_string(),
            Affects::Players(Who::Opponents) => "each opponent".to_string(),
            Affects::Players(_) => "each player".to_string(),
            Affects::Objects(f) => filter(f),
            Affects::YouAnd(f) => format!("you and {}", filter(f)),
            Affects::Target(i) => self.recipient(Recipient::Target(*i), targets),
        }
    }

    fn replaces(&mut self, r: &Replaces, targets: &[Target]) -> String {
        match r {
            Replaces::Damage { from, to, combat } => {
                let mut s = String::new();
                match combat {
                    Some(true) => s.push_str("combat "),
                    Some(false) => s.push_str("noncombat "),
                    None => {}
                }
                s.push_str("damage");
                if let Some(f) = from {
                    let _ = write!(s, " from {}", filter(f));
                }
                let _ = write!(s, " to {}", self.affects(to, targets));
                s
            }
            Replaces::GainLife { whose } => match whose {
                Who::You => "you gain life",
                Who::Any => "a player gains life",
                Who::Opponents => "an opponent gains life",
            }
            .to_string(),
            Replaces::Draw {
                whose,
                library_empty,
            } => {
                let mut s = match whose {
                    Who::You => "you draw",
                    _ => "a player draws",
                }
                .to_string();
                if *library_empty {
                    s.push_str(" with empty library");
                }
                s
            }
            Replaces::ToGraveyard {
                whose,
                from_battlefield,
            } => {
                let mut s = match whose {
                    Who::Any => "card put into a graveyard",
                    Who::You => "card put into your graveyard",
                    Who::Opponents => "card put into an opponent's graveyard",
                }
                .to_string();
                if *from_battlefield {
                    s.push_str(" from the battlefield");
                }
                s
            }
            Replaces::PlusCounters { on } => format!("+1/+1 counters put on {}", filter(on)),
            Replaces::Destroy { to } => format!("destroy {}", self.affects(to, targets)),
            Replaces::Dies { to } => format!("{} dies", self.affects(to, targets)),
            Replaces::Enters => "enters".to_string(),
        }
    }

    fn sentence(&mut self, s: &Sentence, targets: &[Target]) -> String {
        let mut out = String::new();
        if s.eot_first {
            out.push_str("until end of turn, ");
        }
        for (i, e) in s.effects.iter().enumerate() {
            if i > 0 {
                out.push_str(match s.joins.get(i - 1) {
                    Some(Join::Then) => " then ",
                    _ => " and ",
                });
            }
            out.push_str(&self.effect(e, targets));
        }
        out
    }

    fn line(&mut self, ss: &[Sentence], targets: &[Target]) -> String {
        ss.iter()
            .map(|s| self.sentence(s, targets))
            .collect::<Vec<_>>()
            .join("; ")
    }

    #[allow(clippy::too_many_lines)]
    fn effect(&mut self, effect: &Effect, t: &[Target]) -> String {
        match effect {
            Effect::DealDamage { amount, to } => {
                format!("deal {} damage to {}", num(*amount), self.recipients(to, t))
            }
            Effect::Pump {
                power,
                toughness,
                to,
            } => format!("{} gets {power:+}/{toughness:+}", self.recipient(*to, t)),
            Effect::Destroy { to, no_regen } => {
                let mut s = format!("destroy {}", self.recipients(to, t));
                if *no_regen {
                    s.push_str(" without regeneration");
                }
                s
            }
            Effect::GainLife { amount, who } => {
                let v = if *who == Recipient::You {
                    "gain"
                } else {
                    "gains"
                };
                format!("{} {v} {} life", self.recipient(*who, t), num(*amount))
            }
            Effect::Proliferate => "proliferate".to_string(),
            Effect::Explore { what } => format!("{} explores", self.recipient(*what, t)),
            Effect::Amass { kind, count } => format!("amass {kind} {}", num(*count)),
            Effect::ClearEcho => "clear echo".to_string(),
            Effect::Modes(modes) => {
                let mut s = "choose one {\n".to_string();
                for m in modes {
                    let mut inner = String::new();
                    body(
                        &mut inner,
                        "    ",
                        "mode",
                        &m.effects,
                        &m.targets,
                        &self.groups,
                    );
                    s.push_str(&inner);
                }
                s.push_str("  }");
                s
            }
            Effect::SwitchPt { to } => format!("switch pt of {}", self.recipient(*to, t)),
            Effect::GrantAbility { ability, to } => format!(
                "{} gains ability {{ {} }}",
                self.recipient(*to, t),
                nested(ability, &self.groups)
            ),
            Effect::SkipNextUntap { to } => {
                format!("{} doesn't untap next untap step", self.recipient(*to, t))
            }
            Effect::PutFromHand { filter: f, tapped } => format!(
                "put {} card from hand onto battlefield{}",
                filter(f),
                if *tapped { " tapped" } else { "" }
            ),
            Effect::AddPoison { count, who } => {
                let v = if *who == Recipient::You {
                    "get"
                } else {
                    "gets"
                };
                format!(
                    "{} {v} {} poison counters",
                    self.recipient(*who, t),
                    num(*count)
                )
            }
            Effect::LoseLife { amount, who } => {
                let v = if *who == Recipient::You {
                    "lose"
                } else {
                    "loses"
                };
                format!("{} {v} {} life", self.recipient(*who, t), num(*amount))
            }
            Effect::DrawCards { count, who } => match who {
                None => format!("draw {} {}", num(*count), cards(*count)),
                Some(w) => {
                    let v = if *w == Recipient::You {
                        "draw"
                    } else {
                        "draws"
                    };
                    format!(
                        "{} {v} {} {}",
                        self.recipient(*w, t),
                        num(*count),
                        cards(*count)
                    )
                }
            },
            Effect::Mill { count, who } => match who {
                None => format!("mill {} {}", num(*count), cards(*count)),
                Some(w) => {
                    let v = if *w == Recipient::You {
                        "mill"
                    } else {
                        "mills"
                    };
                    format!(
                        "{} {v} {} {}",
                        self.recipient(*w, t),
                        num(*count),
                        cards(*count)
                    )
                }
            },
            Effect::AddCounters { kind, count, to } => format!(
                "put {} {} on {}",
                num(*count),
                counter(*kind, *count),
                self.recipient(*to, t)
            ),
            Effect::RemoveCounters { kind, count, to } => format!(
                "remove {} {} from {}",
                num(*count),
                counter(*kind, *count),
                self.recipient(*to, t)
            ),
            Effect::RemoveAllCounters { to } => {
                format!("remove all counters from {}", self.recipient(*to, t))
            }
            Effect::ReturnToHand { what } => format!("return {} to hand", self.recipients(what, t)),
            Effect::ReturnToBattlefield { what, counters } => {
                let mut s = format!("return {} to battlefield", self.recipient(*what, t));
                if let Some((kind, n)) = counters {
                    let _ = write!(s, " with {n} {}", counter(*kind, Num::N(*n)));
                }
                s
            }
            Effect::Regenerate { to } => format!("regenerate {}", self.recipient(*to, t)),
            Effect::Exile { what } => format!("exile {}", self.recipients(what, t)),
            Effect::ExileAllGraveyards => "exile all graveyards".to_string(),
            Effect::Sacrifice { what } => format!("sacrifice {}", self.recipient(*what, t)),
            Effect::Tap { to } => format!("tap {}", self.recipients(to, t)),
            Effect::Untap { to } => format!("untap {}", self.recipients(to, t)),
            Effect::Grant { keywords: ks, to } => {
                let v = if matches!(
                    to,
                    Recipient::You
                        | Recipient::EachCreature
                        | Recipient::EachCreatureYouControl
                        | Recipient::EachPermanentYouControl
                        | Recipient::Group(_)
                ) {
                    "gain"
                } else {
                    "gains"
                };
                format!("{} {v} {}", self.recipient(*to, t), keywords(ks))
            }
            Effect::GainControl { of } => format!("gain control of {}", self.recipient(*of, t)),
            Effect::Attach { to } => format!("attach to {}", self.recipient(*to, t)),
            Effect::CreateToken { token, count } => format!("create {} \"{token}\"", num(*count)),
            Effect::CreateTokenCopy { of, except } => {
                let mut s = format!("create token copy of {}", self.recipient(*of, t));
                if *except {
                    s.push_str(" with exceptions");
                }
                s
            }
            Effect::BecomeCopy { of, except } => {
                let mut s = format!("this becomes a copy of {}", self.recipient(*of, t));
                if *except {
                    s.push_str(" with exceptions");
                }
                s
            }
            Effect::CreateEmblem { emblem } => format!("emblem \"{emblem}\""),
            Effect::Counter { to } => format!("counter {}", self.recipient(*to, t)),
            Effect::CounterUnlessPays { to, mana } => format!(
                "counter {} unless its controller pays {mana}",
                self.recipient(*to, t)
            ),
            Effect::Scry(n) => format!("scry {}", num(*n)),
            Effect::Investigate => "investigate".to_string(),
            Effect::PumpVar {
                power,
                toughness,
                times,
                to,
            } => format!(
                "{} gets {power:+}/{toughness:+} {}",
                self.recipient(*to, t),
                num(*times)
            ),
            Effect::PutInLibrary { what, top } => format!(
                "put {} on {} of library",
                self.recipient(*what, t),
                if *top { "top" } else { "bottom" }
            ),
            Effect::SacrificeChosen {
                who,
                count,
                filter: f,
            } => format!(
                "{} sacrifices {} {}",
                self.recipient(*who, t),
                num(*count),
                filter(f)
            ),
            Effect::Strip { keywords: ks, to } => {
                format!("{} loses {}", self.recipient(*to, t), keywords(ks))
            }
            Effect::RevealHand { who } => format!("{} reveals hand", self.recipient(*who, t)),
            Effect::DiscardRandom { count, who } => format!(
                "{} discards {} cards at random",
                self.recipient(*who, t),
                num(*count)
            ),
            Effect::DiscardChosen { who, filter: f } => match f {
                None => format!("{} reveals hand discarding card", self.recipient(*who, t)),
                Some(f) => format!(
                    "{} reveals hand discarding {} card",
                    self.recipient(*who, t),
                    filter(f)
                ),
            },
            Effect::Transform { what } => format!("transform {}", self.recipient(*what, t)),
            Effect::AddMana(m) => format!("add {}", crate::render::mana_symbols(m, "any")),
            Effect::AddManaOneOf(m) => format!(
                "add {}",
                m.iter()
                    .map(|x| crate::render::mana_symbols(&[*x], "any"))
                    .collect::<Vec<_>>()
                    .join(" or ")
            ),
            Effect::AddManaTimes { mana, count } => {
                format!(
                    "add {} {}",
                    crate::render::mana_symbols(&[*mana], "any"),
                    num(*count)
                )
            }
            Effect::Search {
                filter: f,
                count,
                up_to,
                to,
                reveal,
                that_card,
            } => {
                let mut t = "search ".to_string();
                if *up_to {
                    t.push_str("up to ");
                }
                if *count != Num::N(1) {
                    let _ = write!(t, "{} ", num(*count));
                }
                if *f != Filter::default() {
                    let _ = write!(t, "{} ", filter(f));
                }
                t.push_str("card");
                if *reveal {
                    t.push_str(" reveal");
                }
                t.push_str(match to {
                    mtg_core::card::SearchTo::Hand => " to hand",
                    mtg_core::card::SearchTo::Battlefield { tapped: true } => {
                        " to battlefield tapped"
                    }
                    mtg_core::card::SearchTo::Battlefield { tapped: false } => " to battlefield",
                    mtg_core::card::SearchTo::TopOfLibrary => " to top",
                });
                if *that_card {
                    t.push_str(" that card");
                }
                t
            }
            Effect::Surveil(n) => format!("surveil {}", num(*n)),
            Effect::Discard { count, who } => match who {
                None => format!("discard {} {}", num(*count), cards(*count)),
                Some(w) => {
                    let v = if *w == Recipient::You {
                        "discard"
                    } else {
                        "discards"
                    };
                    format!(
                        "{} {v} {} {}",
                        self.recipient(*w, t),
                        num(*count),
                        cards(*count)
                    )
                }
            },
            Effect::Fight { a, b } => {
                format!("{} fights {}", self.recipient(*a, t), self.recipient(*b, t))
            }
            Effect::Animate {
                to,
                types,
                subtypes,
                power,
                toughness,
            } => {
                let mut words: Vec<String> = subtypes.clone();
                words.extend(types.iter().map(|t| format!("{t:?}").to_lowercase()));
                format!(
                    "{} becomes {power}/{toughness} {}",
                    self.recipient(*to, t),
                    words.join(" ")
                )
            }
            Effect::YouMay(inner) => format!("you may {}", self.line(inner, t)),
            Effect::IfKicked(inner) => format!("if kicked, {}", self.line(inner, t)),
            Effect::IfYouDo(inner) => format!("if you do, {}", self.line(inner, t)),
            Effect::Pay { mana, life } => format!("pay {}", pay_amount(mana, *life)),
            Effect::UnlessPays {
                payer,
                mana,
                life,
                effect,
            } => {
                let inner = self.effect(effect, t);
                let who = if *payer == Recipient::You {
                    "you pay".to_string()
                } else {
                    format!("{} pays", self.recipient(*payer, t))
                };
                format!("{inner} unless {who} {}", pay_amount(mana, *life))
            }
            Effect::SetBasePt {
                to,
                power,
                toughness,
            } => format!(
                "{} has base power and toughness {power}/{toughness}",
                self.recipient(*to, t)
            ),
            Effect::Delayed {
                about,
                trigger,
                effects,
                trigger_first,
            } => {
                let about = self.recipient(*about, t);
                let body = self.line(effects, t);
                let trig = trigger_text(trigger, true);
                if *trigger_first {
                    format!("delayed {about}: {trig}, {body}")
                } else {
                    format!("delayed {about}: {body} {trig}")
                }
            }
            Effect::CreateReplacement(rep) => match (&rep.event, &rep.outcome) {
                (
                    Replaces::Damage {
                        from: None,
                        to,
                        combat,
                    },
                    Outcome::Prevent,
                ) => {
                    let mut s = "prevent all ".to_string();
                    match combat {
                        Some(true) => s.push_str("combat "),
                        Some(false) => s.push_str("noncombat "),
                        None => {}
                    }
                    s.push_str("damage");
                    if *to != Affects::Any {
                        let _ = write!(s, " to {}", self.affects(to, t));
                    }
                    s.push_str(" this turn");
                    s
                }
                (
                    Replaces::Damage {
                        from: None,
                        to,
                        combat,
                    },
                    Outcome::PreventNext(n),
                ) => {
                    let mut s = format!("prevent next {n} ");
                    match combat {
                        Some(true) => s.push_str("combat "),
                        Some(false) => s.push_str("noncombat "),
                        None => {}
                    }
                    let _ = write!(s, "damage to {} this turn", self.affects(to, t));
                    s
                }
                _ => format!(
                    "this turn, replace {}: {}",
                    self.replaces(&rep.event, t),
                    outcome(&rep.outcome)
                ),
            },
            Effect::Rule(RuleKind::NoLifeGain) => "players can't gain life this turn".to_string(),
            Effect::Rule(RuleKind::DamageCantBePrevented) => {
                "damage can't be prevented this turn".to_string()
            }
        }
    }
}

thread_local! {
    /// The counts of the card being formatted (`Num::Count` indexes them).
    static COUNTS: std::cell::RefCell<Vec<crate::ast::Count>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn pay_amount(mana: &mtg_core::mana::ManaCost, life: u32) -> String {
    if life > 0 {
        format!("{life} life")
    } else {
        mana.to_string()
    }
}

fn num(n: Num) -> String {
    match n {
        Num::N(n) => n.to_string(),
        Num::X => "X".to_string(),
        Num::Count(i) => COUNTS.with(|c| {
            c.borrow()
                .get(usize::from(i))
                .map_or_else(|| "count where life".to_string(), count_text)
        }),
    }
}

fn count_text(c: &crate::ast::Count) -> String {
    use crate::ast::{CountExpr, CountStyle};
    let style = match c.style {
        CountStyle::Where => "where",
        CountStyle::Equal => "equal",
        CountStyle::Each => "each",
    };
    let expr = match &c.expr {
        CountExpr::Permanents(f) if f.who.is_none() => format!("{} on battlefield", filter(f)),
        CountExpr::Permanents(f) => filter(f),
        CountExpr::Domain => "domain".to_string(),
        CountExpr::CardsInHand => "hand".to_string(),
        CountExpr::CardsInGraveyard(None) => "graveyard".to_string(),
        CountExpr::CardsInGraveyard(Some(f)) => format!("graveyard {}", filter(f)),
        CountExpr::SourcePower => "power".to_string(),
        CountExpr::TargetPower(i) => format!("power of target {}", i + 1),
        CountExpr::Life => "life".to_string(),
    };
    format!("count {style} {expr}")
}

fn cards(n: Num) -> &'static str {
    if n == Num::N(1) { "card" } else { "cards" }
}

fn counter(k: CounterKind, n: Num) -> String {
    let name = match k {
        CounterKind::PlusOnePlusOne => "+1/+1",
        CounterKind::MinusOneMinusOne => "-1/-1",
        CounterKind::Loyalty => "loyalty",
    };
    format!(
        "{name} {}",
        if n == Num::N(1) {
            "counter"
        } else {
            "counters"
        }
    )
}

fn outcome(o: &Outcome) -> String {
    match o {
        Outcome::Prevent => "prevent".to_string(),
        Outcome::PreventNext(n) => format!("prevent next {n}"),
        Outcome::Double => "double".to_string(),
        Outcome::PlusOne => "plus one".to_string(),
        Outcome::Instead(_) => "exile".to_string(),
        Outcome::DrawTwo => "draw two".to_string(),
        Outcome::WinGame => "win".to_string(),
        Outcome::EnterTappedUnless(_) => unreachable!("an AST ability of its own"),
        Outcome::EnterTapped => "tapped".to_string(),
        Outcome::EnterWithCounters { kind, n } => {
            format!("with {} {}", num(*n), counter(*kind, *n))
        }
        Outcome::Regenerate => "regenerate".to_string(),
        Outcome::RedirectToAttached => "redirect to attached".to_string(),
    }
}

fn cmp_suffix(c: mtg_core::card::Cmp) -> &'static str {
    match c {
        mtg_core::card::Cmp::AtMost => " or less",
        mtg_core::card::Cmp::AtLeast => " or greater",
        mtg_core::card::Cmp::Exactly => "",
    }
}

/// `target card from a graveyard`, `target instant card from your
/// graveyard`.
pub fn graveyard_target(f: &Filter, who: Who) -> String {
    let whose = match who {
        Who::You => "your",
        Who::Opponents => "an opponent's",
        Who::Any => "a",
    };
    if *f == Filter::default() {
        format!("target card from {whose} graveyard")
    } else {
        format!("target {} card from {whose} graveyard", filter(f))
    }
}

pub fn filter(f: &Filter) -> String {
    let mut words: Vec<String> = Vec::new();
    if f.all {
        words.push("all".to_string());
    }
    match f.relation {
        Relation::Other => words.push("other".to_string()),
        Relation::This => words.push("this".to_string()),
        Relation::Any => {}
    }
    if f.enchanted {
        words.push("enchanted".to_string());
    }
    for s in &f.supertypes {
        words.push(format!("{s:?}").to_lowercase());
    }
    for s in &f.not_supertypes {
        words.push(
            match s {
                Supertype::Basic => "nonbasic",
                Supertype::Legendary => "nonlegendary",
                Supertype::Snow => "nonsnow",
                Supertype::World => "nonworld",
            }
            .to_string(),
        );
    }
    for t in &f.not_types {
        words.push(format!("non{}", format!("{t:?}").to_lowercase()));
    }
    for s in &f.not_subtypes {
        words.push(format!("non-{s}"));
    }
    for c in &f.not_colors {
        words.push(format!("non{}", color_name(*c)));
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
    if f.damaged {
        words.push("damaged".to_string());
    }
    if let Some(cc) = f.color_count {
        words.push(format!("{cc:?}").to_lowercase());
    }
    if let Some(colors) = &f.colors {
        let names: Vec<String> = colors.iter().map(|c| color_name(*c).to_string()).collect();
        words.push(names.join(" or "));
    }
    words.extend(f.subtypes.iter().cloned());
    words.extend(f.types.iter().map(|t| format!("{t:?}").to_lowercase()));
    if !f.any_types.is_empty() {
        let names: Vec<String> = f
            .any_types
            .iter()
            .map(|t| format!("{t:?}").to_lowercase())
            .collect();
        words.push(names.join(" or "));
    }
    if f.subtypes.is_empty()
        && f.types.is_empty()
        && f.any_types.is_empty()
        && !f.enchanted
        && f.relation != Relation::This
    {
        words.push("permanent".to_string());
    }
    for (name, v) in [("power", f.power), ("toughness", f.toughness)] {
        if let Some((cmp, n)) = v {
            words.push(format!("with {name} {n}{}", cmp_suffix(cmp)));
        }
    }
    if let Some((cmp, n)) = f.mana_value {
        words.push(format!("with mana value {n}{}", cmp_suffix(cmp)));
    }
    if !f.keywords.is_empty() {
        words.push(format!("with {}", keywords(&f.keywords)));
    }
    if !f.not_keywords.is_empty() {
        words.push(format!("without {}", keywords(&f.not_keywords)));
    }
    match f.who {
        Some(Who::You) => words.push("you control".to_string()),
        Some(Who::Opponents) if f.dont_control => words.push("you don't control".to_string()),
        Some(Who::Opponents) => words.push("an opponent controls".to_string()),
        Some(Who::Any) | None => {}
    }
    if f.no_abilities {
        words.push("with no abilities".to_string());
    }
    words.join(" ")
}

fn trigger_obj(o: &TriggerObj) -> String {
    match o {
        TriggerObj::This => "this".to_string(),
        TriggerObj::Filter(f) => filter(f),
    }
}

fn step_name(s: Step) -> &'static str {
    match s {
        Step::Untap => "untap",
        Step::Upkeep => "upkeep",
        Step::Draw => "draw",
        Step::BeginningOfCombat => "combat",
        _ => "end",
    }
}

fn trigger_text(t: &Trigger, next: bool) -> String {
    match t {
        Trigger::Enters(o) => format!("whenever {} enters", trigger_obj(o)),
        Trigger::Dies(o) => format!("whenever {} dies", trigger_obj(o)),
        Trigger::EveryStep(step) => {
            let w = match step {
                Step::Untap => "untap",
                Step::Upkeep => "upkeep",
                Step::Draw => "draw",
                Step::BeginningOfCombat => "combat",
                _ => "end",
            };
            format!("at beginning of every {w}")
        }
        Trigger::BeginningOfStep { step, whose } => {
            let whose = match whose {
                Who::You => "your",
                Who::Opponents => "each opponent's",
                Who::Any => {
                    if next {
                        "the next"
                    } else {
                        "each"
                    }
                }
            };
            format!("at beginning of {whose} {} step", step_name(*step))
        }
        Trigger::Attacks(o) => format!("whenever {} attacks", trigger_obj(o)),
        Trigger::AttacksAlone(o) => format!("whenever {} attacks alone", trigger_obj(o)),
        Trigger::BecomesBlocked(o) => format!("whenever {} becomes blocked", trigger_obj(o)),
        Trigger::BlocksOrBlocked(o) => {
            format!("whenever {} blocks or becomes blocked", trigger_obj(o))
        }
        Trigger::Blocks(o) => format!("whenever {} blocks", trigger_obj(o)),
        Trigger::DealsCombatDamageToPlayer(o) => {
            format!(
                "whenever {} deals combat damage to a player",
                trigger_obj(o)
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
        Trigger::BecomesTarget(o) => format!("when {} becomes targeted", trigger_obj(o)),
        Trigger::TargetedByOpponent => "whenever this becomes targeted by opponent".to_string(),
        Trigger::BecomesTargetWhenever(o) => {
            format!("whenever {} becomes targeted", trigger_obj(o))
        }
        Trigger::BecomesTapped(o) => format!("whenever {} becomes tapped", trigger_obj(o)),
        Trigger::BecomesUntapped(o) => format!("whenever {} becomes untapped", trigger_obj(o)),
        Trigger::DealtDamage(o) => format!("whenever {} is dealt damage", trigger_obj(o)),
        Trigger::DealsDamage { obj, to, combat } => format!(
            "whenever {} deals {}damage{}",
            trigger_obj(obj),
            if *combat { "combat " } else { "" },
            match to {
                mtg_core::card::DamageTo::Any => "",
                mtg_core::card::DamageTo::Creature => " to creature",
                mtg_core::card::DamageTo::Player => " to player",
                mtg_core::card::DamageTo::Opponent => " to opponent",
            }
        ),
        Trigger::CastsFiltered { whose, filter: f } => format!(
            "whenever {} {} spell",
            match whose {
                Who::You => "you cast",
                Who::Opponents => "an opponent casts",
                Who::Any => "a player casts",
            },
            filter(f)
        ),
    }
}

fn cost_text(c: &Cost) -> String {
    let default_order = [
        CostItem::Mana,
        CostItem::Tap,
        CostItem::Sacrifice,
        CostItem::Discard,
        CostItem::Life,
    ];
    let order: &[CostItem] = if c.order.is_empty() {
        &default_order
    } else {
        &c.order
    };
    let mut parts = Vec::new();
    for item in order {
        match item {
            CostItem::Mana => {
                let mana = c.mana.to_string();
                if !mana.is_empty() {
                    parts.push(mana);
                }
            }
            CostItem::Tap => {
                if c.tap {
                    parts.push("{T}".to_string());
                }
            }
            CostItem::Sacrifice => match &c.sacrifice {
                Some(SacCost::This) => parts.push("sacrifice this".to_string()),
                Some(SacCost::Filter(f)) => parts.push(format!("sacrifice {}", filter(f))),
                None => {}
            },
            CostItem::Exile => {
                if c.exile_self {
                    parts.push("exile this from graveyard".to_string());
                }
            }
            CostItem::Discard => {
                if c.discard_self {
                    parts.push("discard this".to_string());
                } else if let Some(f) = &c.discard_filter {
                    parts.push(format!("discard {} card", filter(f)));
                } else if c.discard > 0 {
                    parts.push(format!(
                        "discard {} {}",
                        c.discard,
                        if c.discard == 1 { "card" } else { "cards" }
                    ));
                }
            }
            CostItem::Life => {
                if c.life > 0 {
                    parts.push(format!("pay {} life", c.life));
                }
            }
        }
    }
    if c.sorcery && c.loyalty.is_none() {
        parts.push("sorcery".to_string());
    }
    if c.once {
        parts.push("once".to_string());
    }
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(", ")
    }
}

/// An ability granted by another, on one line.
fn nested(a: &Ability, groups: &[Filter]) -> String {
    let mut s = String::new();
    ability(&mut s, a, "", groups);
    s.trim().replace('\n', " ")
}

fn modification(m: &Mod) -> String {
    match m {
        Mod::AddAbility(_) => unreachable!("formatted with its groups"),
        Mod::AddTypes(ts) => format!(
            "add type {}",
            ts.iter()
                .map(|t| format!("{t:?}").to_lowercase())
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Mod::AddLandType(c) => format!("add land {}", basic_land_name(*c)),
        Mod::SetLandType(c) => format!("set land {}", basic_land_name(*c)),
        Mod::AddSubtype(s) => format!("add subtype {s}"),
        Mod::Control => "control".to_string(),
        Mod::AddColors(cs) => format!(
            "add color {}",
            cs.iter()
                .map(|c| color_name(*c))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Mod::SetColors(cs) => format!(
            "set color {}",
            cs.iter()
                .map(|c| color_name(*c))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Mod::AddKeywords(ks) => format!("has {}", keywords(ks)),
        Mod::LosesKeywords(ks) => format!("loses {}", keywords(ks)),
        Mod::RemoveAllAbilities => "loses all abilities".to_string(),
        Mod::SetPt {
            power: PtValue::Fixed(p),
            toughness: PtValue::Fixed(t),
        } => format!("base {p}/{t}"),
        Mod::SetPt { .. } => "base mv".to_string(),
        Mod::ModifyPt { power, toughness } => format!("{power:+}/{toughness:+}"),
        Mod::ModifyPtPer {
            power,
            toughness,
            count,
        } => format!("{power:+}/{toughness:+} {}", num(*count)),
        Mod::SwitchPt => "switch pt".to_string(),
        Mod::CdaColors(cs) => {
            if cs.len() == 5 {
                "cda color all".to_string()
            } else {
                format!(
                    "cda color {}",
                    cs.iter()
                        .map(|c| color_name(*c))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            }
        }
        Mod::CdaPt {
            count,
            plus_toughness,
        } => {
            let c = match count {
                CdaCount::CardTypesInGraveyards => "card types in graveyards".to_string(),
                CdaCount::CreatureCardsInGraveyards => "creature cards in graveyards".to_string(),
                CdaCount::Permanents(f) => format!("count {}", filter(f)),
                CdaCount::ControllerLife => "your life".to_string(),
            };
            if *plus_toughness == 0 {
                format!("cda pt {c}")
            } else {
                format!("cda pt {c} {plus_toughness:+}")
            }
        }
    }
}

/// Writes `head: body` on one line, or `head {` ... `}` if the effects
/// span paragraphs.
fn body(
    out: &mut String,
    indent: &str,
    head: &str,
    ss: &[Sentence],
    targets: &[Target],
    groups: &[Filter],
) {
    let mut f = Fmt {
        mentioned: vec![false; targets.len()],
        groups: groups.to_vec(),
    };
    if ss.iter().any(|s| s.para) {
        let _ = writeln!(out, "{indent}{head} {{");
        let mut line: Vec<String> = Vec::new();
        for s in ss {
            if s.para && !line.is_empty() {
                let _ = writeln!(out, "{indent}  {}", line.join("; "));
                line.clear();
            }
            line.push(f.sentence(s, targets));
        }
        if !line.is_empty() {
            let _ = writeln!(out, "{indent}  {}", line.join("; "));
        }
        let _ = writeln!(out, "{indent}}}");
    } else {
        let _ = writeln!(out, "{indent}{head}: {}", f.line(ss, targets));
    }
}

fn static_cond(c: &StaticCond) -> String {
    match c {
        StaticCond::CardsInGraveyard(n) => format!("{n} cards in your graveyard"),
        StaticCond::CardTypesInGraveyard(n) => format!("{n} card types in your graveyard"),
        StaticCond::YouControl {
            filter: fl,
            at_least: 1,
        } => format!("you control {}", filter(fl)),
        StaticCond::YouControl {
            filter: fl,
            at_least,
        } => format!("you control {at_least} {}", filter(fl)),
        StaticCond::YourTurn | StaticCond::DuringYourTurn => "your turn".to_string(),
        StaticCond::Attacking => "attacking".to_string(),
        StaticCond::Enchanted => "enchanted".to_string(),
        StaticCond::Equipped => "equipped".to_string(),
        StaticCond::OpponentLifeAtMost(n) => format!("opponent life {n} or less"),
        StaticCond::AnyPlayerLifeAtMost(n) => format!("player life {n} or less"),
        StaticCond::DefenderControls {
            filter: fl,
            at_least: 1,
        } => format!("defending player controls {}", filter(fl)),
        StaticCond::DefenderControls {
            filter: fl,
            at_least,
        } => format!("defending player controls {at_least} {}", filter(fl)),
        StaticCond::YouControlNone(fl) => format!("you control no {}", filter(fl)),
        StaticCond::AttackedThisTurn => "attacked this turn".to_string(),
        StaticCond::CreatureDiedThisTurn => "creature died this turn".to_string(),
        StaticCond::CardsInHand { n, at_most } => {
            format!("hand {n} or {}", if *at_most { "less" } else { "more" })
        }
        StaticCond::YourLife { n, at_least } => {
            format!(
                "your life {n} or {}",
                if *at_least { "more" } else { "less" }
            )
        }
    }
}

#[allow(clippy::too_many_lines)] // one arm per statement kind
fn ability(out: &mut String, ability: &Ability, indent: &str, groups: &[Filter]) {
    match ability {
        Ability::Keyword(k) => {
            let _ = writeln!(out, "{indent}keyword {}", keyword_name(*k));
        }
        Ability::Mana(m) => {
            let syms: Vec<String> = m
                .iter()
                .map(|m| match m {
                    Mana::Colorless => "{C}".to_string(),
                    Mana::Colored(c) => format!("{{{}}}", c.letter()),
                    Mana::Any => "any".to_string(),
                })
                .collect();
            let _ = writeln!(out, "{indent}mana {}", syms.join(" or "));
        }
        Ability::SacMana(m) => {
            let sym = match m {
                Mana::Colorless => "{C}".to_string(),
                Mana::Colored(c) => format!("{{{}}}", c.letter()),
                Mana::Any => "any".to_string(),
            };
            let _ = writeln!(out, "{indent}mana {sym}, sacrifice this");
        }
        Ability::Enchant(t) => {
            let phrase = match t.base() {
                Target::Spec(s) => target_phrase(*s).replace("target ", ""),
                Target::CreatureOfSubtype(sub) => format!("{sub} creature"),
                Target::Filter(f) | Target::Spell(f) | Target::GraveyardCard { filter: f, .. } => {
                    filter(f)
                }
                Target::UpTo(_) => unreachable!("base"),
            };
            let _ = writeln!(out, "{indent}enchant {phrase}");
        }
        Ability::Spell { targets, effects } => body(out, indent, "spell", effects, targets, groups),
        Ability::Modal(modes) | Ability::ModalBoth(modes) => {
            let head = if matches!(ability, Ability::ModalBoth(_)) {
                "choose one or both"
            } else {
                "choose one"
            };
            let _ = writeln!(out, "{indent}{head} {{");
            for m in modes {
                body(
                    out,
                    &format!("{indent}  "),
                    "mode",
                    &m.effects,
                    &m.targets,
                    groups,
                );
            }
            let _ = writeln!(out, "{indent}}}");
        }
        Ability::Activated {
            cost,
            targets,
            effects,
            equip,
            cycling,
        } => {
            if *equip {
                let _ = writeln!(out, "{indent}equip {}", cost.mana);
                return;
            }
            if *cycling {
                let word = match effects.first().and_then(|s| s.effects.first()) {
                    Some(Effect::Search { filter: f, .. }) => {
                        if let [sub] = f.subtypes.as_slice() {
                            format!("{}cycling", sub.to_lowercase())
                        } else if f.supertypes.contains(&Supertype::Basic) {
                            "basic landcycling".to_string()
                        } else {
                            "landcycling".to_string()
                        }
                    }
                    _ => "cycling".to_string(),
                };
                let _ = writeln!(out, "{indent}{word} {}", cost.mana);
                return;
            }
            if let Some(n) = cost.loyalty {
                body(
                    out,
                    indent,
                    &format!("loyalty {n:+}"),
                    effects,
                    targets,
                    groups,
                );
                return;
            }
            body(
                out,
                indent,
                &format!("activated {}", cost_text(cost)),
                effects,
                targets,
                groups,
            );
        }
        Ability::Triggered {
            trigger,
            condition,
            targets,
            effects,
        } => {
            let mut head = format!("triggered {}", trigger_text(trigger, false));
            let condition = match condition {
                Some(crate::ast::TrigCond::Static(c)) => {
                    let _ = write!(head, " if {}", static_cond(c));
                    None
                }
                Some(crate::ast::TrigCond::Core(c)) => Some(c),
                None => None,
            };
            match condition {
                Some(mtg_core::card::Condition::Static(_)) => unreachable!("an AST condition"),
                Some(mtg_core::card::Condition::SourceUntapped) => {
                    head.push_str(" if this is untapped");
                }
                Some(mtg_core::card::Condition::NoCreatures) => {
                    head.push_str(" if no creatures are on the battlefield");
                }
                Some(mtg_core::card::Condition::EchoDue) => head.push_str(" if echo due"),
                Some(mtg_core::card::Condition::Kicked) => head.push_str(" if kicked"),
                Some(mtg_core::card::Condition::NoSpellsLastTurn) => {
                    head.push_str(" if no spells were cast last turn");
                }
                Some(mtg_core::card::Condition::TwoSpellsLastTurn) => {
                    head.push_str(" if a player cast two or more spells last turn");
                }
                Some(mtg_core::card::Condition::HadNoCounters(k)) => {
                    let _ = write!(head, " if it had no {}", counter(*k, Num::N(2)));
                }
                None => {}
            }
            body(out, indent, &head, effects, targets, groups);
        }
        Ability::Replacement(rep) => {
            let mut fmt = Fmt {
                mentioned: Vec::new(),
                groups: groups.to_vec(),
            };
            let head = match (&rep.event, &rep.outcome) {
                (Replaces::Enters, o) => format!("enters {}", outcome(o)),
                (event, o) => format!("replace {}: {}", fmt.replaces(event, &[]), outcome(o)),
            };
            if rep.then.is_empty() {
                let _ = writeln!(out, "{indent}{head}");
            } else {
                let _ = writeln!(out, "{indent}{head} {{");
                let _ = writeln!(out, "{indent}  {}", fmt.line(&rep.then, &[]));
                let _ = writeln!(out, "{indent}}}");
            }
        }
        Ability::Static {
            filter: fl,
            mods,
            condition,
            cond_first,
        } => {
            let mods = mods
                .iter()
                .map(|m| match m {
                    Mod::AddAbility(a) => format!("has ability {{ {} }}", nested(a, groups)),
                    other => modification(other),
                })
                .collect::<Vec<_>>()
                .join(", ");
            match condition {
                Some(StaticCond::DuringYourTurn) => {
                    let _ = writeln!(
                        out,
                        "{indent}static during your turn, {}: {mods}",
                        filter(fl)
                    );
                }
                Some(c) if *cond_first => {
                    let _ = writeln!(
                        out,
                        "{indent}static as long as {}, {}: {mods}",
                        static_cond(c),
                        filter(fl)
                    );
                }
                Some(c) => {
                    let _ = writeln!(
                        out,
                        "{indent}static {}: {mods} as long as {}",
                        filter(fl),
                        static_cond(c)
                    );
                }
                None => {
                    let _ = writeln!(out, "{indent}static {}: {mods}", filter(fl));
                }
            }
        }
        Ability::Labeled {
            label,
            ability: inner,
        } => {
            let mut tmp = String::new();
            self::ability(&mut tmp, inner, indent, groups);
            let _ = write!(out, "{indent}label \"{label}\" {}", &tmp[indent.len()..]);
        }
        Ability::BlockRule(r) => {
            let text = match r {
                crate::ast::BlockRule::NotBy(f) => format!("can't be blocked by {}", filter(f)),
                crate::ast::BlockRule::OnlyBy(f) => {
                    format!("can't be blocked except by {}", filter(f))
                }
                crate::ast::BlockRule::AtMostOne => {
                    "can't be blocked by more than one creature".to_string()
                }
                crate::ast::BlockRule::BlocksOnly(f) => format!("can block only {}", filter(f)),
                crate::ast::BlockRule::CantBlock(f) => format!("can't block {}", filter(f)),
            };
            let _ = writeln!(out, "{indent}{text}");
        }
        Ability::EntersAsCopy(f) => {
            let _ = writeln!(out, "{indent}enters as copy of {}", filter(f));
        }
        Ability::CostChange {
            filter: f,
            whose,
            generic,
        } => {
            let subject = if *f == Filter::default() {
                String::new()
            } else {
                format!(" {}", filter(f))
            };
            let whose = match whose {
                Who::You => " you cast",
                Who::Opponents => " your opponents cast",
                Who::Any => "",
            };
            let more = if *generic > 0 { "more" } else { "less" };
            let _ = writeln!(
                out,
                "{indent}spells{subject}{whose} cost {{{}}} {more}",
                generic.unsigned_abs()
            );
        }
        Ability::SelfCost { per, times } => {
            let _ = writeln!(
                out,
                "{indent}this spell costs {{{per}}} less {}",
                num(*times)
            );
        }
        Ability::Affinity(f) => {
            let _ = writeln!(out, "{indent}affinity for {}", filter(f));
        }
        Ability::Mechanic(m) => {
            let text = match m {
                crate::ast::Mechanic::Bushido(n) => format!("bushido {n}"),
                crate::ast::Mechanic::Soulshift(n) => format!("soulshift {n}"),
                crate::ast::Mechanic::Afflict(n) => format!("afflict {n}"),
                crate::ast::Mechanic::Toxic(n) => format!("toxic {n}"),
                crate::ast::Mechanic::Ward(c) => format!("ward {c}"),
                crate::ast::Mechanic::Echo(c) => format!("echo {c}"),
                crate::ast::Mechanic::Unearth(c) => format!("unearth {c}"),
                crate::ast::Mechanic::Buyback(c) => format!("buyback {c}"),
                crate::ast::Mechanic::Outlast(c) => format!("outlast {c}"),
            };
            let _ = writeln!(out, "{indent}{text}");
        }
        Ability::EntersTappedUnless(c) => {
            let _ = writeln!(out, "{indent}enters tapped unless {}", static_cond(c));
        }
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
            let _ = writeln!(out, "{indent}can't {what} unless {}", static_cond(cond));
        }
        Ability::HandSize { whose, delta } => {
            let who = if *whose == Who::Opponents {
                " opponents"
            } else {
                ""
            };
            let d = match delta {
                None => "none".to_string(),
                Some(n) => format!("{n:+}"),
            };
            let _ = writeln!(out, "{indent}hand size{who} {d}");
        }
        Ability::Protection(f) => {
            if *f == Filter::default() {
                let _ = writeln!(out, "{indent}protection from everything");
            } else {
                let _ = writeln!(out, "{indent}protection from {}", filter(f));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    #[test]
    fn format_is_a_fixed_point() {
        let src = "card \"Siege-Gang Commander\" {\n  cost {3}{R}{R}\n  type Creature — Goblin\n  pt 2/2\n  triggered whenever this enters: create 3 \"Goblin\"\n  activated {1}{R}, sacrifice Goblin you control: deal 2 damage to any target\n}\n";
        let cards = parse(src).unwrap();
        let text = format(&cards[0]);
        assert_eq!(text, src);
        assert_eq!(parse(&text).unwrap(), cards);
    }

    #[test]
    fn target_spec_round_trips() {
        for (ws, spec) in crate::parse::TARGETS {
            let _ = ws;
            let cards = parse(&format!(
                "card \"X\" {{\n  type Instant\n  spell: destroy {}\n}}\n",
                target_phrase(*spec)
            ))
            .unwrap();
            let Ability::Spell { targets, .. } = &cards[0].abilities[0] else {
                panic!()
            };
            assert_eq!(targets, &[Target::Spec(*spec)], "{}", target_phrase(*spec));
        }
    }
}
