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
    Ability, Affects, Card, ChoiceKind, Cost, CostItem, DamageWording, Effect, Filter, Join, Kind,
    Mod, Recipient, Replaces, Restrict, SacCost, Sentence, StaticCond, Target, Trigger, TriggerObj,
};
use crate::render::{basic_land_name, color_name, keyword_name, target_phrase};

pub fn format(card: &Card) -> String {
    let outer = COUNTS.with(|c| c.replace(card.counts.clone()));
    let noun = NOUN.with(|n| n.replace(crate::render::self_noun(card)));
    let out = format_inner(card);
    COUNTS.with(|c| c.replace(outer));
    NOUN.with(|n| n.replace(noun));
    out
}

#[allow(clippy::too_many_lines)] // one line per header field
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
    if let Some(n) = card.defense {
        let _ = writeln!(out, "  defense {n}");
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
    if let Some(c) = &card.additional_or {
        let _ = writeln!(out, "  additional cost or {}", cost_text(c));
    }
    if let Some(a) = &card.alt_cost {
        let mut text = cost_text(&a.cost);
        if a.count > 1 {
            text = text.replacen("sacrifice ", &format!("sacrifice {} ", a.count), 1);
        }
        if a.others_gain > 0 {
            let gain = format!("each other player gains {} life", a.others_gain);
            text = if text.is_empty() {
                gain
            } else {
                format!("{gain}, {text}")
            };
        }
        let _ = writeln!(out, "  alternative cost {text}");
    }
    if let Some(c) = &card.flash_if {
        let _ = writeln!(out, "  flash as long as {}", static_cond(c));
    }
    if card.flash_cleanup {
        let _ = writeln!(out, "  flash with cleanup sacrifice");
    }
    if let Some(c) = &card.cast_only {
        let c = static_cond(c);
        if c.starts_with("during") || c.starts_with("before") {
            let _ = writeln!(out, "  cast only {c}");
        } else {
            let _ = writeln!(out, "  cast only if {c}");
        }
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
    #[allow(clippy::too_many_lines)] // one arm per recipient kind
    fn recipient(&mut self, r: Recipient, targets: &[Target]) -> String {
        match r {
            Recipient::Target(i) => {
                if self.mentioned[i] {
                    format!("target {}", i + 1)
                } else {
                    self.mentioned[i] = true;
                    let up_to = matches!(targets[i], Target::UpTo(_));
                    // `1 to 2 target creature`, `any number of targets`.
                    if let Target::Many { min, max, .. } = &targets[i] {
                        let prefix = many_prefix(*min, *max);
                        if let Target::Spec(mtg_core::card::TargetSpec::Any) = targets[i].base() {
                            return format!("{prefix} targets");
                        }
                        let mut inner = targets.to_vec();
                        inner[i] = targets[i].base().clone();
                        let mut f = Fmt {
                            mentioned: vec![false; inner.len()],
                            groups: self.groups.clone(),
                        };
                        return format!("{prefix} {}", f.recipient(Recipient::Target(i), &inner));
                    }
                    let phrase = match targets[i].base() {
                        Target::Spec(s) => target_phrase(*s),
                        Target::CreatureOfSubtype(sub) => format!("target {sub} creature"),
                        // "target other creature you control" would read as
                        // "target other creature" and a stray "you control".
                        Target::Filter(f)
                            if f.relation == Relation::Other
                                && f.who.is_some()
                                && !f.other_than_this =>
                        {
                            let mut g = f.clone();
                            crate::format::relation_any(&mut g);
                            format!("another target {}", filter(&g))
                        }
                        Target::Filter(f) => format!("target {}", filter(f)),
                        Target::Spell(f) => spell_target(f),
                        Target::GraveyardCard {
                            filter: f,
                            who,
                            graveyards,
                        } if f.relation == Relation::Other => {
                            let mut g = f.clone();
                            crate::format::relation_any(&mut g);
                            format!("another {}", graveyard_target(&g, *who, *graveyards))
                        }
                        Target::GraveyardCard {
                            filter: f,
                            who,
                            graveyards,
                        } => graveyard_target(f, *who, *graveyards),
                        Target::UpTo(_) | Target::Many { .. } => unreachable!("base"),
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
            Recipient::TheTarget(i) => {
                self.mentioned[i] = true;
                format!("the target {}", i + 1)
            }
            // "up to 2 target creature": declares them together.
            Recipient::Targets(a, n) if self.mentioned[a] => {
                format!("targets {} to {}", a + 1, a + n)
            }
            Recipient::Targets(a, n) => {
                let first = self.recipient(Recipient::Target(a), targets);
                for m in &mut self.mentioned[a..a + n] {
                    *m = true;
                }
                match first.strip_prefix("up to 1 ") {
                    Some(rest) => format!("up to {n} {rest}"),
                    None if first.starts_with("target ") => format!("{n} {first}"),
                    None => first,
                }
            }
            Recipient::You => "you".to_string(),
            Recipient::EachOpponent => "each opponent".to_string(),
            Recipient::EachOtherPlayer => "each other player".to_string(),
            Recipient::Source => "this".to_string(),
            Recipient::SourceNamed => NOUN.with(|n| format!("this {}", n.borrow())),
            Recipient::SourceCard => "this card".to_string(),
            Recipient::TriggeringObject | Recipient::SameThat => "that creature".to_string(),
            Recipient::TheTriggeringObject => "the creature".to_string(),
            Recipient::TriggeringCard => "triggering card".to_string(),
            Recipient::TriggeringController { that: true } => {
                "controller of that creature".to_string()
            }
            Recipient::TheTriggeringController => "controller of the creature".to_string(),
            Recipient::TriggeringPlayer | Recipient::ThatOwner(_) => "that player".to_string(),
            Recipient::DefendingPlayer => "defending player".to_string(),
            Recipient::DefendingPlayerWord => "that defending player".to_string(),
            Recipient::ControllerOf(_) | Recipient::TriggeringController { that: false } => {
                "its controller".to_string()
            }
            Recipient::ThatControllerOf(i) => format!("controller of that target {}", i + 1),
            Recipient::TheControllerOf(i) => format!("controller of the target {}", i + 1),
            Recipient::OwnerOf(_) => "its owner".to_string(),
            Recipient::EachCreature => "each creature".to_string(),
            Recipient::Attached => "attached".to_string(),
            Recipient::AttachedController => "controller of enchanted".to_string(),
            Recipient::BlockingIt => "each creature blocking it".to_string(),
            Recipient::ExiledCard => "exiled card".to_string(),
            Recipient::ThoseCards(..) => "those cards".to_string(),
            Recipient::ThoseTargets(..) => "those targets".to_string(),
            Recipient::EachPlayer => "each player".to_string(),
            Recipient::EachCreatureYouControl => "each creature you control".to_string(),
            Recipient::EachPermanentYouControl => "each permanent you control".to_string(),
            Recipient::SameObject | Recipient::TriggeringIt | Recipient::SourceIt => {
                "it".to_string()
            }
            Recipient::CreatedObject => "that token".to_string(),
            Recipient::CreatedTokens | Recipient::SameTokens => "those tokens".to_string(),
            // `all`, not `each`: "each creature you control" is the
            // built-in recipient, which prints differently.
            // `all` prints with the filter.
            // `target creature and all other creature with same name as
            // target 1`.
            Recipient::TargetAnd(t, i) => {
                let first = self.recipient(Recipient::Target(t), targets);
                let group = self.recipient(Recipient::Group(i), targets);
                // `... from your graveyard and all other card with same
                // name as target 1`.
                if matches!(
                    targets[t].base(),
                    Target::Spec(mtg_core::card::TargetSpec::CreatureCardInYourGraveyard)
                        | Target::GraveyardCard { .. }
                ) {
                    let rest = group.strip_prefix("all other").unwrap_or(&group);
                    return format!("{first} and all other card{rest}");
                }
                format!("{first} and {group}")
            }
            Recipient::DamagedThisWay => "creature dealt damage this way".to_string(),
            Recipient::OpponentsWithout(i) => match self.groups.get(usize::from(i)) {
                Some(f) => format!("each opponent who doesn't control {}", filter(f)),
                None => "each opponent".to_string(),
            },
            Recipient::RandomOf(i) => match self.groups.get(usize::from(i)) {
                Some(f) => format!("random {}", filter(f)),
                None => "random permanent".to_string(),
            },
            Recipient::Group(i) => {
                let text = match self.groups.get(usize::from(i)) {
                    Some(f) if f.all => filter(f),
                    Some(f) if f.each => format!(
                        "each single {}",
                        filter(&Filter {
                            each: false,
                            ..f.clone()
                        })
                    ),
                    Some(f) => format!("each {}", filter(f)),
                    None => "each permanent".to_string(),
                };
                // `all Aura attached to target creature`: the target is
                // declared inside the group's filter.
                let attached = self.groups.get(usize::from(i)).and_then(|f| f.attached_to);
                match attached {
                    Some(crate::ast::AttachedRef::Target(n)) if !self.mentioned[n] => {
                        let declared = self.recipient(Recipient::Target(n), targets);
                        text.replace(
                            &format!("attached to target {}", n + 1),
                            &format!("attached to {declared}"),
                        )
                    }
                    _ => text,
                }
            }
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
            Affects::ThatTarget(i) => self.recipient(Recipient::ThatTarget(*i), targets),
            Affects::Targets(a, n) => self.recipient(Recipient::Targets(*a, *n), targets),
            Affects::OtherThanTarget(f, i) => format!(
                "{} except {}",
                filter(f),
                self.recipient(Recipient::Target(*i), targets)
            ),
            Affects::Chosen(f) => {
                let f = filter(f);
                if f == "permanent" {
                    "chosen source".to_string()
                } else {
                    let f = f
                        .strip_suffix(" permanent")
                        .map_or(f.clone(), |p| format!("{p} source"));
                    format!("chosen {f}")
                }
            }
            Affects::DamagedThisWay => "creature dealt damage this way".to_string(),
        }
    }

    /// A damage event after `damage` (see `Parser::damage_event`); `turn`
    /// is " this turn" for an effect's `prevent all`, where it also places
    /// a late source.
    fn damage_event(&mut self, r: &Replaces, targets: &[Target], turn: &str) -> String {
        match r {
            Replaces::DamageToAndBy { what, .. } => {
                format!(" to and from {}{turn}", self.affects(what, targets))
            }
            Replaces::Damage {
                from, to, wording, ..
            } => {
                let mut s = String::new();
                let to_any_written = turn.is_empty();
                match (from, wording) {
                    (Some(f), DamageWording::WouldDeal) => {
                        let _ = write!(s, " {} would deal", self.affects(f, targets));
                        if *to != Affects::Any {
                            let _ = write!(s, " to {}", self.affects(to, targets));
                        }
                        s.push_str(turn);
                    }
                    (_, DamageWording::TurnFirst) if !turn.is_empty() => {
                        let _ = write!(s, "{turn} to {}", self.affects(to, targets));
                        if let Some(f) = from {
                            let _ = write!(s, " from {}", self.affects(f, targets));
                        }
                    }
                    (Some(f), DamageWording::DealtLate) if !turn.is_empty() => {
                        if *to != Affects::Any {
                            let _ = write!(s, " to {}", self.affects(to, targets));
                        }
                        let _ = write!(s, "{turn} from {}", self.affects(f, targets));
                    }
                    _ => {
                        if let Some(f) = from {
                            let _ = write!(s, " from {}", self.affects(f, targets));
                        }
                        if *to != Affects::Any || to_any_written {
                            let _ = write!(s, " to {}", self.affects(to, targets));
                        }
                        s.push_str(turn);
                    }
                }
                s
            }
            _ => unreachable!("a damage event"),
        }
    }

    fn damage_kind(r: &Replaces) -> &'static str {
        match r {
            Replaces::Damage { combat, .. } | Replaces::DamageToAndBy { combat, .. } => {
                match combat {
                    Some(true) => "combat damage",
                    Some(false) => "noncombat damage",
                    None => "damage",
                }
            }
            _ => unreachable!("a damage event"),
        }
    }

    fn replaces(&mut self, r: &Replaces, targets: &[Target]) -> String {
        match r {
            Replaces::Damage { .. } | Replaces::DamageToAndBy { .. } => {
                let kind = Self::damage_kind(r);
                format!("{kind}{}", self.damage_event(r, targets, ""))
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
        if let Some(l) = &s.label {
            let _ = write!(out, "label \"{l}\" ");
        }
        if s.then_first {
            out.push_str("then ");
        }
        if s.eot_first {
            out.push_str("until end of turn, ");
        }
        for (i, e) in s.effects.iter().enumerate() {
            if i > 0 {
                out.push_str(match s.joins.get(i - 1) {
                    Some(Join::Then) => " then ",
                    Some(Join::AndRepeat) => ", and ",
                    Some(Join::Comma) => ", ",
                    _ => " and ",
                });
                // "gets +0/+1 and gains all creature types": the subject
                // is implied (written again, a group naming "target
                // player" would declare a second target).
                if let Effect::CreatureTypes { to, all } = e
                    && matches!(&s.effects[i - 1],
                        Effect::Pump { to: p, .. } | Effect::SetBasePt { to: p, .. } if p == to)
                {
                    out.push_str(if *all { "gains" } else { "loses" });
                    out.push_str(" all creature types");
                    continue;
                }
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
            Effect::DealDamage { amount, to, by } => {
                let verb = match by {
                    Recipient::Source => "deal".to_string(),
                    r => format!("{} deals", self.recipient(*r, t)),
                };
                if to.is_empty() {
                    format!("{verb} {} damage", num(*amount))
                } else if let Some(expr) = equal_before(*amount) {
                    format!("{verb} damage equal {expr} to {}", self.recipients(to, t))
                } else {
                    format!(
                        "{verb} {} damage to {}",
                        num(*amount),
                        self.recipients(to, t)
                    )
                }
            }
            Effect::Pump {
                power,
                toughness,
                to,
            } => format!("{} gets {power:+}/{toughness:+}", self.recipient(*to, t)),
            Effect::Destroy {
                to,
                no_regen,
                joined,
            } => {
                let mut s = format!("destroy {}", self.recipients(to, t));
                if *no_regen {
                    s.push_str(" without regeneration");
                }
                if *joined {
                    s.push_str(" joined");
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
            Effect::RingTempts => "the ring tempts you".to_string(),
            Effect::BecomeMonarch => "become monarch".to_string(),
            Effect::TakeInitiative => "take initiative".to_string(),
            Effect::Venture { undercity: false } => "venture".to_string(),
            Effect::Venture { undercity: true } => "venture into undercity".to_string(),
            Effect::Explore { what } => format!("{} explores", self.recipient(*what, t)),
            Effect::PhaseOut { what } => format!("{} phases out", self.recipient(*what, t)),
            Effect::Amass { kind, count } => format!("amass {kind} {}", num(*count)),
            Effect::ClearEcho => "clear echo".to_string(),
            Effect::Manifest => "manifest".to_string(),
            Effect::ManifestDread => "manifest dread".to_string(),
            Effect::Blight(n) => format!("blight {n}"),
            Effect::Adapt(n) => format!("adapt {n}"),
            Effect::Bolster(n) => format!("bolster {}", num(*n)),
            Effect::Monstrosity(n) => format!("monstrosity {}", num(*n)),
            Effect::Firebend(_) => unreachable!("only in expansions"),
            Effect::Earthbend { count, .. } => format!("earthbend {}", num(*count)),
            Effect::Support(to) => format!("support {}", to.len()),
            Effect::Populate => "populate".to_string(),
            Effect::Incubate(n) => format!("incubate {}", num(*n)),
            Effect::Discover(n) => format!("discover {n}"),
            Effect::MoveSourceCounters { .. } | Effect::CumulativeUpkeep(_) => {
                unreachable!("only in expansions")
            }
            Effect::Connive { what } => format!("{} connives", self.recipient(*what, t)),
            Effect::LookAtTop {
                count,
                of,
                by,
                only,
            } => {
                let mut s = String::new();
                if let Some(b) = by {
                    s.push_str(&self.recipient(*b, t));
                    s.push_str(" looks");
                } else {
                    s.push_str("look");
                }
                let _ = write!(s, " at top {}", num(*count));
                if let Some(o) = of {
                    let _ = write!(s, " of {}", self.recipient(*o, t));
                }
                if *only {
                    s.push_str(" only");
                }
                s
            }
            Effect::RevealTop { who, look, of } => match (who, look, of) {
                (None, true, _) => "look at top card".to_string(),
                (None, false, _) => "reveal top card".to_string(),
                (Some(w), true, true) => format!("look at top card of {}", self.recipient(*w, t)),
                (Some(w), false, true) => format!("reveal top card of {}", self.recipient(*w, t)),
                (Some(w), _, false) => {
                    let v = if *w == Recipient::You {
                        "reveal"
                    } else {
                        "reveals"
                    };
                    format!("{} {v} top card", self.recipient(*w, t))
                }
            },
            Effect::Shuffle { who } => format!("{} shuffles", self.recipient(*who, t)),
            Effect::MoveCounters {
                from,
                to,
                kind,
                count,
            } => match (kind, count) {
                (None, None) if *from == Recipient::Source => {
                    format!("put its counters on {}", self.recipient(*to, t))
                }
                (Some(k), Some(n)) => format!(
                    "move {n} {} from {} onto {}",
                    counter(*k, Num::N(*n)),
                    self.recipient(*from, t),
                    self.recipient(*to, t)
                ),
                _ => format!(
                    "move all counters from {} onto {}",
                    self.recipient(*from, t),
                    self.recipient(*to, t)
                ),
            },
            Effect::RevealUntil { who, filter: f } => match who {
                None => format!("reveal until {} card", filter(f)),
                Some(w) => {
                    let v = if *w == Recipient::You {
                        "reveal"
                    } else {
                        "reveals"
                    };
                    format!("{} {v} until {} card", self.recipient(*w, t), filter(f))
                }
            },
            Effect::RevealIt => "reveal it".to_string(),
            Effect::IfYouDont { cant, then } => format!(
                "if you {}, {}",
                if *cant { "cant" } else { "dont" },
                self.line(then, t)
            ),
            Effect::IfYouDontPut { to, then } => format!(
                "if you dont put it {}, {}",
                match to {
                    mtg_core::card::CardDest::Hand => "into hand",
                    mtg_core::card::CardDest::Graveyard => "into graveyard",
                    mtg_core::card::CardDest::Battlefield { .. } => "onto battlefield",
                    mtg_core::card::CardDest::LibraryBottom => "on bottom of library",
                },
                self.line(then, t)
            ),
            Effect::DrawReveal => "draw 1 card and reveal it".to_string(),
            Effect::DiscardIt => "discard it".to_string(),
            Effect::IfCard {
                shares_type: true,
                then,
                ..
            } => format!(
                "if it shares creature type with this, {}",
                self.line(then, t)
            ),
            Effect::IfCard {
                filter: f,
                negated,
                card,
                that,
                then,
                ..
            } => format!(
                "if {} {} {}{}, {}",
                if *that { "that card" } else { "it" },
                if *negated { "isnt" } else { "is" },
                filter(f),
                if *card { " card" } else { "" },
                self.line(then, t)
            ),
            Effect::PutCard {
                what,
                to,
                who,
                that_player,
            } => {
                use crate::ast::CardWords;
                use mtg_core::card::CardDest;
                let w = match what {
                    CardWords::It => "it",
                    CardWords::ThatCard => "that card",
                    CardWords::AllRevealed => "all revealed",
                    CardWords::Those => "those revealed",
                    CardWords::TheRevealed => "the revealed",
                    CardWords::OtherRevealed => "all other revealed",
                };
                let d = match to {
                    CardDest::Hand => "into hand",
                    CardDest::Graveyard if *that_player => "into graveyard of that player",
                    CardDest::Graveyard => "into graveyard",
                    CardDest::Battlefield { tapped: false } => "onto battlefield",
                    CardDest::Battlefield { tapped: true } => "onto battlefield tapped",
                    CardDest::LibraryBottom => "on bottom of library",
                };
                match who {
                    None => format!("put {w} {d}"),
                    Some(r) => format!("{} puts {w} {d}", self.recipient(*r, t)),
                }
            }
            Effect::ExilePlayable {
                count,
                until,
                first,
                it,
            } => {
                let mut s = format!(
                    "impulse {} {}",
                    num(*count),
                    match until {
                        mtg_core::card::PlayUntil::ThisTurn => "this turn",
                        mtg_core::card::PlayUntil::EndOfYourNextTurn => "next turn",
                        mtg_core::card::PlayUntil::YourNextEndStep => "next end step",
                        mtg_core::card::PlayUntil::Forever => "forever",
                    }
                );
                if *first {
                    s.push_str(" first");
                }
                if *it {
                    s.push_str(" it");
                }
                s
            }
            Effect::Dig {
                count,
                filter: f,
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
                reword,
            } => {
                let mut s = format!("dig {}", num(*count));
                if *then {
                    s.push_str(" then");
                }
                if *reveal_top {
                    s.push_str(" reveal");
                }
                s.push_str(" take");
                if *all {
                    s.push_str(" all");
                }
                if *take == crate::ast::ANY_NUMBER {
                    s.push_str(" any");
                } else if *take != 1 {
                    let _ = write!(s, " {take}");
                }
                match f {
                    Some(f) => {
                        let _ = write!(s, " {} card", filter(f));
                    }
                    None => s.push_str(" card"),
                }
                if let Some(a) = also {
                    let _ = write!(s, " and {} card", filter(a));
                }
                if *optional {
                    s.push_str(" may");
                }
                if *revealing {
                    s.push_str(" revealing");
                }
                if *top {
                    s.push_str(" top");
                }
                s.push_str(if *rest_graveyard {
                    " rest graveyard"
                } else {
                    " rest bottom"
                });
                if *random {
                    s.push_str(" random");
                }
                if *reword {
                    s.push_str(" reword");
                }
                s
            }
            Effect::Modes(modes) => {
                let mut s = "choose one {\n".to_string();
                for m in modes {
                    let mut inner = String::new();
                    body(
                        &mut inner,
                        "    ",
                        &mode_head(m),
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
            Effect::ProtectionOr { to, filter: f } => format!(
                "{} gains protection from {} or from color of choice",
                self.recipient(*to, t),
                filter(f)
            ),
            Effect::CreatureTypes { to, all } => format!(
                "{} {} all creature types",
                self.recipient(*to, t),
                if *all { "gains" } else { "loses" }
            ),
            Effect::GrantAbility { ability, to } => match granted_mechanic(ability) {
                Some(m) => format!("{} gains {m}", self.recipient(*to, t)),
                None => format!(
                    "{} gains ability {{ {} }}",
                    self.recipient(*to, t),
                    nested(ability, &self.groups)
                ),
            },
            Effect::ReturnChosen {
                count,
                filter: f,
                who: Some(who),
            } => format!(
                "{} returns {} {}",
                self.recipient(*who, t),
                num(*count),
                filter(f)
            ),
            Effect::ReturnChosen {
                count, filter: f, ..
            } => {
                format!("return {} {} chosen", num(*count), filter(f))
            }
            Effect::BlocksIfAble { blocker, attacker } => format!(
                "{} blocks {} if able",
                self.recipient(*blocker, t),
                self.recipient(*attacker, t)
            ),
            Effect::CantBlockThis { blocker, attacker } => format!(
                "{} can't block {}",
                self.recipient(*blocker, t),
                self.recipient(*attacker, t)
            ),
            Effect::CantBeRegenerated { to } => {
                format!("{} can't be regenerated", self.recipient(*to, t))
            }
            Effect::SkipNext { who, what } => {
                let v = if *who == Recipient::You {
                    "skip"
                } else {
                    "skips"
                };
                let w = match what {
                    mtg_core::card::SkipWhat::DrawStep => "draw step",
                    mtg_core::card::SkipWhat::CombatPhase => "combat phase",
                    mtg_core::card::SkipWhat::CombatPhaseThisTurn => "combat phase this turn",
                };
                format!("{} {v} next {w}", self.recipient(*who, t))
            }
            Effect::AssignsNoCombatDamage { to } => {
                format!(
                    "{} assigns no combat damage this turn",
                    self.recipient(*to, t)
                )
            }
            Effect::SkipNextUntap { to, steps } => format!(
                "{} doesn't untap next untap step{}",
                self.recipient(*to, t),
                if *steps { "s" } else { "" }
            ),
            Effect::PutFromHand { filter: f, tapped } => format!(
                "put {} card from hand onto battlefield{}",
                filter(f),
                if *tapped { " tapped" } else { "" }
            ),
            Effect::AddEnergy { count, who } => {
                let v = if *who == Recipient::You {
                    "get"
                } else {
                    "gets"
                };
                format!("{} {v} {} energy", self.recipient(*who, t), num(*count))
            }
            Effect::PayEnergy(n) => format!("pay {n} energy"),
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
            Effect::AddCounterList { counters, to } => format!(
                "put {} on {}",
                counters
                    .iter()
                    .map(|(k, n)| format!("{} {}", num(*n), counter(*k, *n)))
                    .collect::<Vec<_>>()
                    .join(" and "),
                self.recipient(*to, t)
            ),
            Effect::DealDamageDivided { amount, to } => format!(
                "deal {} damage divided among {}",
                num(*amount),
                self.recipient(*to, t)
            ),
            Effect::DistributeCounters { kind, count, to } => format!(
                "distribute {} {} among {}",
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
            Effect::RemoveCounterChosen { to } => {
                format!("remove 1 counter from {}", self.recipient(*to, t))
            }
            Effect::RemoveAllCounters { to } => {
                format!("remove all counters from {}", self.recipient(*to, t))
            }
            Effect::ReturnToHand { what } => format!("return {} to hand", self.recipients(what, t)),
            Effect::ReturnToBattlefield {
                what,
                counters,
                tapped,
                under_you,
                attached,
            } => {
                let mut s = format!("return {} to battlefield", self.recipient(*what, t));
                if *tapped {
                    s.push_str(" tapped");
                }
                if *under_you {
                    s.push_str(" under your control");
                }
                if let Some((kind, n)) = counters {
                    let _ = write!(s, " with {n} {}", counter(*kind, Num::N(*n)));
                }
                if let Some(a) = attached {
                    let _ = write!(s, " attached to {}", self.recipient(*a, t));
                }
                s
            }
            Effect::Regenerate { to } => format!("regenerate {}", self.recipient(*to, t)),
            Effect::Exile { what } => format!("exile {}", self.recipients(what, t)),
            Effect::ExileUntil { what } => {
                format!("exile {} until this leaves", self.recipient(*what, t))
            }
            Effect::ExileAllGraveyards => "exile all graveyards".to_string(),
            Effect::ExileGraveyard { who } => {
                format!("exile graveyard of {}", self.recipient(*who, t))
            }
            Effect::Sacrifice { what } => format!("sacrifice {}", self.recipient(*what, t)),
            Effect::SacrificeBy { who, what } => format!(
                "{} sacrifices {}",
                self.recipient(*who, t),
                self.recipient(*what, t)
            ),
            Effect::Tap { to } => format!("tap {}", self.recipients(to, t)),
            Effect::TapOrUntap { to } => format!("tap or untap {}", self.recipient(*to, t)),
            Effect::NextSpellLess(n) => format!("next spell costs {{{n}}} less"),
            Effect::FaceDownSpellsLess(n) => {
                format!("face-down spells cost {{{n}}} less this turn")
            }
            Effect::Goad { what } => format!("goad {}", self.recipient(*what, t)),
            Effect::TurnFaceUp { what } => format!("turn face up {}", self.recipient(*what, t)),
            Effect::TurnFaceDown { what } => {
                format!("turn face down {}", self.recipient(*what, t))
            }
            Effect::LookAtFaceDown { what } => {
                format!("look at face-down {}", self.recipient(*what, t))
            }
            Effect::LookAtPlayer { who } => {
                format!(
                    "look at hand top and face-down of {}",
                    self.recipient(*who, t)
                )
            }
            Effect::Detain { what } => format!("detain {}", self.recipient(*what, t)),
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
            Effect::Attach {
                what: Recipient::Source,
                to,
            } => format!("attach to {}", self.recipient(*to, t)),
            Effect::Attach { what, to } => format!(
                "attach {} to {}",
                self.recipient(*what, t),
                self.recipient(*to, t)
            ),
            Effect::Unattach { what } => format!("unattach {}", self.recipient(*what, t)),
            Effect::CreateToken {
                token,
                count,
                tapped,
                who,
                it_has,
                attached,
            } => {
                let mut s = match who {
                    None => String::from("create"),
                    Some(Recipient::You) => String::from("you create"),
                    Some(w) => format!("{} creates", self.recipient(*w, t)),
                };
                let _ = write!(
                    s,
                    " {}{} \"{token}\"",
                    num(*count),
                    if *tapped { " tapped" } else { "" }
                );
                if *it_has {
                    s.push_str(" it has");
                }
                if let Some(a) = attached {
                    let _ = write!(s, " attached to {}", self.recipient(*a, t));
                }
                s
            }
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
            Effect::Counter { to, dest } => {
                format!("counter {}{}", self.recipient(*to, t), countered_to(*dest))
            }
            Effect::CounterUnlessPays { to, mana, x, dest } => format!(
                "counter {} unless its controller pays {mana}{}{}",
                self.recipient(*to, t),
                if *x == Num::X {
                    String::new()
                } else {
                    format!(" {}", num(*x))
                },
                countered_to(*dest)
            ),
            Effect::Scry(n) => format!("scry {}", num(*n)),
            Effect::ScryEach { who, count, may } => format!(
                "{} {} {}",
                self.recipient(*who, t),
                if *may { "may scry" } else { "scries" },
                num(*count)
            ),
            Effect::Investigate(1) => "investigate".to_string(),
            Effect::Investigates(w) => format!("{} investigates", self.recipient(*w, t)),
            Effect::Investigate(2) => "investigate twice".to_string(),
            Effect::Investigate(n) => format!("investigate {n} times"),
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
            Effect::PutInLibrary {
                what,
                place,
                wording,
            } => {
                use crate::ast::{LibraryPlace as P, LibraryWording as W};
                let what = self.recipient(*what, t);
                let shuffle = *place == P::Shuffle;
                let place = match place {
                    P::Top => "on top of library".to_string(),
                    P::Bottom => "on bottom of library".to_string(),
                    P::TopOrBottom => "on top or bottom of library".to_string(),
                    P::FromTop(n) => format!("into library {n} from top"),
                    P::FromTopOrBottom(n) => format!("into library {n} from top or bottom"),
                    P::Shuffle => "into library".to_string(),
                };
                let (put, puts) = if shuffle {
                    ("shuffle", "shuffles")
                } else {
                    ("put", "puts")
                };
                match wording {
                    W::Put => format!("{put} {what} {place}"),
                    W::Owner => format!("owner {puts} {what} {place}"),
                    W::OwnerOf => format!("owner of {what} {puts} it {place}"),
                }
            }
            Effect::SacrificeChosen {
                who,
                count,
                filter: f,
                and,
                choice_first,
            } => {
                let mut s = format!("{} sacrifices {} ", self.recipient(*who, t), num(*count));
                // `1 creature of their choice with flying`.
                if *choice_first && !f.keywords.is_empty() && f.or.is_none() {
                    let mut g = f.clone();
                    let ks = std::mem::take(&mut g.keywords);
                    s.push_str(&filter(&g));
                    s.push_str(" of their choice with ");
                    s.push_str(&keywords(&ks).replace(", ", " and "));
                } else {
                    s.push_str(&filter(f));
                }
                if let Some(b) = and {
                    let _ = write!(s, " and {} {}", num(b.0), filter(&b.1));
                }
                s
            }
            Effect::Strip { keywords: ks, to } => {
                format!("{} loses {}", self.recipient(*to, t), keywords(ks))
            }
            Effect::RevealHand { who } => format!("{} reveals hand", self.recipient(*who, t)),
            Effect::RevealRandom { who } => format!(
                "{} reveals 1 card at random from hand",
                self.recipient(*who, t)
            ),
            Effect::LookAtHand { who } => format!("look at hand of {}", self.recipient(*who, t)),
            Effect::RollDie { sides, .. } => format!("roll d{sides}"),
            Effect::FlipCoin => "flip coin".to_string(),
            Effect::Clash => "clash".to_string(),
            Effect::IfWonClash(inner) => format!("if you win, {}", self.line(inner, t)),
            Effect::Vote(words) => format!("vote {}", words.join(" or ")),
            Effect::VoteReturn(f) => format!("vote return {} card", filter(f)),
            Effect::IfWonFlip(inner) => format!("if you win the flip, {}", self.line(inner, t)),
            Effect::ChooseTarget(r) => format!("choose {}", self.recipient(*r, t)),
            Effect::DiscardRandom {
                count, who: None, ..
            } => format!("discard {} cards at random", num(*count)),
            Effect::DiscardRandom {
                who: Some(who),
                another: true,
                ..
            } => format!(
                "{} discards another card at random",
                self.recipient(*who, t)
            ),
            Effect::DiscardRandom {
                count,
                who: Some(who),
                ..
            } => format!(
                "{} discards {} cards at random",
                self.recipient(*who, t),
                num(*count)
            ),
            Effect::ExileChosen {
                who,
                count,
                filter: f,
            } => format!(
                "{} exiles {} {} they control",
                self.recipient(*who, t),
                num(*count),
                filter(f)
            ),
            Effect::ExileFromHand {
                count,
                who,
                graveyard,
            } => format!(
                "{} exiles {} {} from {}",
                self.recipient(*who, t),
                num(*count),
                cards(*count),
                if *graveyard { "graveyard" } else { "hand" }
            ),
            Effect::DrawDiscardDrawn { draw, discard } => format!(
                "draw {} {} then discard {} of them",
                num(*draw),
                cards(*draw),
                num(*discard)
            ),
            Effect::ExileFromGraveyardOrHand { who, filter: f } => {
                let what = if *f == Filter::default() {
                    "card".to_string()
                } else {
                    format!("{} card", filter(f))
                };
                format!(
                    "{} reveals hand exiling {what} from graveyard or hand",
                    self.recipient(*who, t)
                )
            }
            Effect::DiscardChosen {
                who,
                filter: f,
                exile: Some(joined),
                ..
            } => {
                let what = match f {
                    None => "card".to_string(),
                    Some(f) => format!("{} card", filter(f)),
                };
                let split = if *joined { "" } else { " split" };
                format!(
                    "{} reveals hand exiling {what}{split}",
                    self.recipient(*who, t)
                )
            }
            Effect::DiscardChosen {
                who,
                filter: f,
                may,
                look,
                ..
            } => {
                let lead = if *look {
                    format!("look at hand of {}", self.recipient(*who, t))
                } else {
                    format!("{} reveals hand", self.recipient(*who, t))
                };
                let may = if *may { "may " } else { "" };
                match f {
                    None => format!("{lead} discarding {may}card"),
                    Some(f) => format!("{lead} discarding {may}{} card", filter(f)),
                }
            }
            Effect::Transform { what } => format!("transform {}", self.recipient(*what, t)),
            Effect::AddMana(m) if m == &[Mana::Chosen] => "add chosen".to_string(),
            Effect::AddMana(m) => format!("add {}", crate::render::mana_symbols(m, "any")),
            Effect::AddManaLandCould { whose, basic } => format!(
                "add any that {}land {} could produce",
                if *basic { "basic " } else { "" },
                match whose {
                    Who::Opponents => "opponent controls",
                    _ => "you control",
                }
            ),
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
            Effect::AddManaCombination {
                count,
                colors,
                different,
            } => {
                // "Add X mana ..., where X is ...": the count after.
                let (n, tail) = match count {
                    Num::Count(_) => ("X".to_string(), format!(" {}", num(*count))),
                    n => (num(*n), String::new()),
                };
                if *different {
                    format!("add {n} mana different colors{tail}")
                } else if colors.count() == 5 {
                    format!("add {n} mana any combination{tail}")
                } else {
                    let syms = colors
                        .iter()
                        .map(|c| format!("{{{}}}", c.letter()))
                        .collect::<Vec<_>>()
                        .concat();
                    format!("add {n} mana any combination of {syms}{tail}")
                }
            }
            Effect::Search {
                filter: f,
                count,
                up_to,
                to,
                reveal,
                that_card,
                or_card,
                who,
                may,
            } => {
                let searcher = who.map(|w| self.recipient(w, t));
                let mut t = String::new();
                if let Some(w) = &searcher {
                    let _ = write!(t, "{w} ");
                }
                t.push_str(match (*may, searcher.is_some()) {
                    (true, _) => "may search ",
                    (false, true) => "searches ",
                    (false, false) => "search ",
                });
                if *up_to {
                    t.push_str("up to ");
                }
                if *count == Num::N(crate::ast::ANY_NUMBER) {
                    t.push_str("any number ");
                } else if *count != Num::N(1) {
                    let _ = write!(t, "{} ", num(*count));
                }
                // "a card named ~": the name after `card`.
                let mut g = f.clone();
                g.same = None;
                let alt = if *or_card == crate::ast::OrCard::No {
                    None
                } else {
                    g.or.take()
                };
                // "card with mana value 3 or less": after the noun.
                let mut mv = None;
                if g.or.is_none() {
                    mv = g.mana_value.take().map(|(cmp, n)| {
                        let n = if g.status.mana_value_x {
                            "X".to_string()
                        } else {
                            n.to_string()
                        };
                        format!(" with mana value {n}{}", cmp_suffix(cmp))
                    });
                    if mv.is_some() {
                        g.status.mana_value_x = false;
                    }
                }
                if g != Filter::default() {
                    let _ = write!(t, "{} ", filter(&g));
                }
                t.push_str("card");
                if let Some(r) = f.same {
                    let _ = write!(t, " {}", same_ref(r));
                }
                if let Some(mv) = mv {
                    t.push_str(&mv);
                }
                if let Some(a) = alt {
                    let art = if *or_card == crate::ast::OrCard::Article {
                        "a "
                    } else {
                        ""
                    };
                    let _ = write!(t, " or {art}{} card", filter(&a));
                }
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
                    mtg_core::card::SearchTo::BattlefieldAndHand => {
                        " to battlefield tapped and hand"
                    }
                });
                if *that_card {
                    t.push_str(" that card");
                }
                t
            }
            Effect::Surveil(n) => format!("surveil {}", num(*n)),
            Effect::Learn => "learn".to_string(),
            Effect::DoublePt { to, toughness, of } => {
                let pt = if *toughness {
                    "power and toughness"
                } else {
                    "power"
                };
                let who = self.recipient(*to, t);
                if *of {
                    format!("double the {pt} of {who}")
                } else {
                    format!("double {who} {pt}")
                }
            }
            Effect::DoubleCounters { kind, to } => format!(
                "double the number of {} on {}",
                kind.map_or("each kind of counter".to_string(), |k| counter(
                    k,
                    Num::N(2)
                )),
                self.recipient(*to, t)
            ),
            Effect::DoubleLife { who } => {
                if *who == Recipient::You {
                    "double your life total".to_string()
                } else {
                    format!("double {} life total", self.recipient(*who, t))
                }
            }
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
            Effect::HandToLibrary {
                count,
                who,
                shuffle: true,
                ..
            } => {
                let rest = format!("{} {} from hand into library", num(*count), cards(*count));
                match who {
                    None => format!("shuffle {rest}"),
                    Some(w) => {
                        let v = if *w == Recipient::You {
                            "shuffle"
                        } else {
                            "shuffles"
                        };
                        format!("{} {v} {rest}", self.recipient(*w, t))
                    }
                }
            }
            Effect::ShuffleZone {
                who,
                of,
                hand,
                filter: fl,
                cards,
            } => {
                let mut s = match who {
                    None => "shuffle".to_string(),
                    Some(w) => {
                        let v = if *w == Recipient::You {
                            "shuffle"
                        } else {
                            "shuffles"
                        };
                        format!("{} {v}", self.recipient(*w, t))
                    }
                };
                if let Some(fl) = fl {
                    let _ = write!(s, " all {} card from", filter(fl));
                } else if *cards {
                    s.push_str(" cards from");
                }
                s.push_str(if *hand { " hand" } else { " graveyard" });
                if let Some(o) = of {
                    let _ = write!(s, " of {}", self.recipient(*o, t));
                }
                s.push_str(" into library");
                s
            }
            Effect::HandToLibrary {
                count, who, bottom, ..
            } => {
                let rest = format!(
                    "{} {} from hand on {} of library",
                    num(*count),
                    cards(*count),
                    if *bottom { "bottom" } else { "top" }
                );
                match who {
                    None => format!("put {rest}"),
                    Some(w) => {
                        let v = if *w == Recipient::You { "put" } else { "puts" };
                        format!("{} {v} {rest}", self.recipient(*w, t))
                    }
                }
            }
            Effect::Fight { a, b } => {
                format!("{} fights {}", self.recipient(*a, t), self.recipient(*b, t))
            }
            Effect::Bite { source, to, .. } if source == to => {
                format!("{} bites itself", self.recipient(*source, t))
            }
            Effect::Bite { source, to, twice } => {
                format!(
                    "{} bites {}{}",
                    self.recipient(*source, t),
                    if *twice { "twice " } else { "" },
                    self.recipient(*to, t)
                )
            }
            Effect::Animate {
                to,
                types,
                subtypes,
                pt,
                base,
                colors,
                keywords: ks,
                indefinite,
                loses_abilities,
                in_addition,
            } => {
                // `loses all abilities and has base power and toughness 0/1`.
                if let (Some((p, q)), true, true) = (pt, base, *loses_abilities)
                    && types.is_empty()
                    && subtypes.is_empty()
                    && colors.is_empty()
                    && ks.is_empty()
                    && !*in_addition
                    && !*indefinite
                {
                    return format!(
                        "{} loses all abilities and has base power and toughness {p}/{q}",
                        self.recipient(*to, t)
                    );
                }
                let mut words: Vec<String> = Vec::new();
                if let (Some((p, q)), false) = (pt, base) {
                    words.push(format!("{p}/{q}"));
                }
                words.extend(colors.iter().map(|c| format!("{c:?}").to_lowercase()));
                words.extend(subtypes.iter().cloned());
                words.extend(types.iter().map(|t| format!("{t:?}").to_lowercase()));
                if let (Some((p, q)), true) = (pt, base) {
                    words.push(format!("base {p}/{q}"));
                }
                if !ks.is_empty() {
                    words.push(format!("with {}", keywords(ks)));
                }
                if *in_addition {
                    words.push("in addition".to_string());
                }
                if *indefinite {
                    words.push("indefinitely".to_string());
                }
                format!(
                    "{} {}becomes {}",
                    self.recipient(*to, t),
                    if *loses_abilities {
                        "loses all abilities and "
                    } else {
                        ""
                    },
                    words.join(" ")
                )
            }
            Effect::BecomeColor {
                to,
                colors,
                indefinite,
            } => {
                let c: Vec<String> = colors
                    .iter()
                    .map(|c| format!("{c:?}").to_lowercase())
                    .collect();
                format!(
                    "{} becomes {}{}",
                    self.recipient(*to, t),
                    if c.is_empty() {
                        "colorless".to_string()
                    } else if c.len() == 5 {
                        "all colors".to_string()
                    } else {
                        c.join(" and ")
                    },
                    if *indefinite { " indefinitely" } else { "" }
                )
            }
            Effect::OfChoice {
                to,
                kind,
                protection,
                in_addition,
            } => {
                let verb = if *protection {
                    "gains protection from"
                } else {
                    "becomes"
                };
                format!(
                    "{} {verb} {}{}",
                    self.recipient(*to, t),
                    choice_kind(kind, true),
                    if *in_addition { " in addition" } else { "" }
                )
            }
            Effect::BecomeLandType {
                to,
                land,
                indefinite,
            } => format!(
                "{} becomes {}{}",
                self.recipient(*to, t),
                crate::render::basic_land_name(*land),
                if *indefinite { " indefinitely" } else { "" }
            ),
            Effect::Choose(kind) => format!("choose {}", choice_kind(kind, false)),
            Effect::ProtectionFromChosen { to } => format!(
                "{} gains protection from chosen color",
                self.recipient(*to, t)
            ),
            Effect::BecomeChosen { to } => {
                format!("{} becomes that type", self.recipient(*to, t))
            }
            Effect::YouMay(inner) => format!("you may {}", self.line(inner, t)),
            Effect::PlayerMay { who, inner } => {
                let who = self.recipient(*who, t);
                format!("{who} may {}", self.line(inner, t))
            }
            Effect::IfKicked(inner) => format!("if kicked, {}", self.line(inner, t)),
            Effect::EachPutsFromGraveyard { who, filter: f } => format!(
                "{} puts 1 {} card from graveyard onto battlefield",
                self.recipient(*who, t),
                filter(f)
            ),
            Effect::ForEachCouldnt { opponent, inner } => format!(
                "for each {} who can't, {}",
                if *opponent { "opponent" } else { "player" },
                self.line(inner, t)
            ),
            Effect::IfTarget(n, f, inner, check)
                if check.tense == crate::ast::TargetTense::Was
                    && *f == Filter::controlled_by_you() =>
            {
                format!(
                    "if you controlled target {}, {}",
                    n + 1,
                    self.line(inner, t)
                )
            }
            Effect::IfTarget(n, f, inner, check) if check.after => format!(
                "{} if {}target {} is {}",
                self.line(inner, t),
                if check.tense == crate::ast::TargetTense::ThatIs {
                    "that "
                } else {
                    ""
                },
                n + 1,
                filter(f)
            ),
            Effect::IfTarget(n, f, inner, check) => {
                use crate::ast::TargetTense;
                let (that, verb) = match check.tense {
                    TargetTense::Was => ("", "was"),
                    TargetTense::Had => ("", "had"),
                    TargetTense::Is => ("", "is"),
                    TargetTense::ThatIs => ("that ", "is"),
                };
                format!(
                    "if {that}target {} {verb} {}, {}{}",
                    n + 1,
                    filter(f),
                    if check.also { "also " } else { "" },
                    self.line(inner, t)
                )
            }
            Effect::IfCondition(c, inner, crate::ast::IfWording::After) => {
                format!("{} if {}", self.line(inner, t), static_cond(c))
            }
            Effect::IfCondition(c, inner, crate::ast::IfWording::Unless) => {
                format!("{} unless {}", self.line(inner, t), static_cond(c))
            }
            Effect::IfCondition(c, inner, also) => format!(
                "if {}, {}{}",
                static_cond(c),
                if *also == crate::ast::IfWording::Also {
                    "also "
                } else {
                    ""
                },
                self.line(inner, t)
            ),
            Effect::IfYouDo(inner) => format!("if you do, {}", self.line(inner, t)),
            Effect::Otherwise(inner) => format!("otherwise, {}", self.line(inner, t)),
            Effect::IfThisWay(c, inner) => {
                format!("if {}, {}", this_way_cond(c), self.line(inner, t))
            }
            Effect::IfExcessDamage(inner) => {
                format!("if excess damage dealt this way, {}", self.line(inner, t))
            }
            Effect::ChooseAct {
                act,
                count,
                up_to,
                filter: f,
                among_them,
                this_way,
            } => {
                use mtg_core::card::ChooseAct;
                let n = format!("{}{}", if *up_to { "up to " } else { "" }, num(*count));
                let unfiltered = *f == Filter::default();
                let f = filter(f);
                let cs = cards(*count);
                match act {
                    ChooseAct::Tap | ChooseAct::Enlist => format!("tap {n} {f} chosen"),
                    ChooseAct::Untap => format!("untap {n} {f} chosen"),
                    ChooseAct::Sacrifice => format!("sacrifice {n} {f} chosen"),
                    ChooseAct::Bounce => format!("return {n} {f} to hand chosen"),
                    ChooseAct::Discard if unfiltered => format!("discard {n} {cs} chosen"),
                    ChooseAct::Discard => format!("discard {n} {f} {cs} chosen"),
                    // "put a card from your hand": no filter, no word for it.
                    ChooseAct::HandToBottom if unfiltered => {
                        format!("put {n} {cs} from hand on bottom of library chosen")
                    }
                    ChooseAct::HandToBottom => {
                        format!("put {n} {f} {cs} from hand on bottom of library chosen")
                    }
                    ChooseAct::ExileFromGraveyard => {
                        format!("exile {n} {f} {cs} from your graveyard chosen")
                    }
                    ChooseAct::GraveyardToTop => {
                        format!("put {n} {f} {cs} from your graveyard on top of library chosen")
                    }
                    ChooseAct::ReturnFromGraveyard => {
                        format!("return {n} {f} {cs} from your graveyard to hand chosen")
                    }
                    ChooseAct::ReturnRandomFromGraveyard => {
                        format!("return {n} {f} {cs} from your graveyard to hand at random")
                    }
                    ChooseAct::MilledToHand => format!(
                        "put {n} {f} {cs} from among {} into hand chosen",
                        if *among_them {
                            "them"
                        } else if *this_way {
                            "milled this way cards"
                        } else {
                            "milled cards"
                        }
                    ),
                    ChooseAct::AttachToTarget(i) => {
                        format!("attach {n} {f} to target {} chosen", i + 1)
                    }
                }
            }
            Effect::OneOf(options) => options
                .iter()
                .map(|o| self.effect(o, t))
                .collect::<Vec<_>>()
                .join(" or "),
            Effect::GrantChoice { to, keywords: ks } => {
                let v = if matches!(to, Recipient::Group(_)) {
                    "gain"
                } else {
                    "gains"
                };
                format!(
                    "{} {v} choice of {}",
                    self.recipient(*to, t),
                    ks.iter()
                        .map(|k| keyword_name(*k))
                        .collect::<Vec<_>>()
                        .join(" or ")
                )
            }
            Effect::DiscardUnless {
                who,
                count,
                filter: f,
            } => match who {
                None => format!(
                    "discard {} {} unless you discard {} card",
                    num(*count),
                    cards(*count),
                    filter(f)
                ),
                Some(w) => format!(
                    "{} discards {} {} unless they discard {} card",
                    self.recipient(*w, t),
                    num(*count),
                    cards(*count),
                    filter(f)
                ),
            },
            Effect::DiscardAll {
                who,
                filter: f,
                all_cards,
            } => {
                let what = match (f, all_cards) {
                    (Some(f), _) => format!("all {} card", filter(f)),
                    (None, true) => "all cards in hand".to_string(),
                    (None, false) => "hand".to_string(),
                };
                match who {
                    None => format!("discard {what}"),
                    Some(w) => format!("{} discards {what}", self.recipient(*w, t)),
                }
            }
            Effect::WhenYouDo { targets, effects } => {
                // Its own targets, declared afresh (CR 603.12).
                let outer = std::mem::replace(&mut self.mentioned, vec![false; targets.len()]);
                let body = self.line(effects, targets);
                self.mentioned = outer;
                format!("when you do, {body}")
            }
            Effect::Instead {
                cond,
                wording,
                label,
                effects,
            } => {
                let cond = match cond {
                    crate::ast::InsteadCond::Kicked => "kicked".to_string(),
                    crate::ast::InsteadCond::Static(c) => static_cond(c),
                    crate::ast::InsteadCond::Target { n, that, filter: f } => format!(
                        "{}target {} is {}",
                        if *that { "that " } else { "" },
                        n + 1,
                        filter(f)
                    ),
                    crate::ast::InsteadCond::ThisWay(c) => this_way_cond(c),
                };
                let body = self.line(effects, t);
                let text = match wording {
                    crate::ast::InsteadWording::Lead => format!("if {cond}, instead {body}"),
                    crate::ast::InsteadWording::Trail => format!("if {cond}, {body} instead"),
                    crate::ast::InsteadWording::After => format!("{body} instead if {cond}"),
                };
                match label {
                    Some(l) => format!("label \"{l}\" {text}"),
                    None => text,
                }
            }
            Effect::Pay { mana, life } => format!("pay {}", pay_amount(mana, *life)),
            Effect::Waterbend(mana) => format!("waterbend {mana}"),
            Effect::UnlessYou { act, effect } => {
                let inner = self.effect(effect, t);
                let act = self.effect(act, t);
                format!("{inner} unless you {act}")
            }
            Effect::UnlessPays {
                payer,
                mana,
                life,
                discard,
                effect,
            } if *discard > 0 => {
                let inner = self.effect(effect, t);
                let n = if *discard == 1 { "card" } else { "cards" };
                format!(
                    "{inner} unless {} discards {discard} {n}",
                    self.recipient(*payer, t)
                )
            }
            Effect::UnlessPays {
                payer,
                mana,
                life,
                effect,
                ..
            } => {
                let inner = self.effect(effect, t);
                let who = if *payer == Recipient::You {
                    "you pay".to_string()
                } else if *payer == Recipient::EachPlayer {
                    "any player pays".to_string()
                } else {
                    format!("{} pays", self.recipient(*payer, t))
                };
                let amount = if *life > 0 && *mana != mtg_core::mana::ManaCost::default() {
                    format!("{mana} or {life} life")
                } else {
                    pay_amount(mana, *life)
                };
                format!("{inner} unless {who} {amount}")
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
                this_turn,
            } => {
                let about = self.recipient(*about, t);
                let body = self.line(effects, t);
                let trig = if *this_turn && !about_that_creature(trigger) {
                    format!("{} this turn", trigger_text(trigger, true))
                } else if *this_turn {
                    this_turn_trigger(trigger).to_string()
                } else {
                    trigger_text(trigger, true)
                };
                if *trigger_first {
                    format!("delayed {about}: {trig}, {body}")
                } else {
                    format!("delayed {about}: {body} {trig}")
                }
            }
            Effect::CreateReplacement(rep) => match (&rep.event, &rep.outcome) {
                (Replaces::Damage { .. } | Replaces::DamageToAndBy { .. }, Outcome::Prevent)
                    if rep.then.is_empty() =>
                {
                    let kind = Self::damage_kind(&rep.event);
                    format!(
                        "prevent all {kind}{}",
                        self.damage_event(&rep.event, t, " this turn")
                    )
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
                    let mut s = format!("prevent next {} ", num(*n));
                    match combat {
                        Some(true) => s.push_str("combat "),
                        Some(false) => s.push_str("noncombat "),
                        None => {}
                    }
                    if *wording == DamageWording::TurnFirst {
                        let _ = write!(s, "damage this turn to {}", self.affects(to, t));
                    } else {
                        let _ = write!(s, "damage to {} this turn", self.affects(to, t));
                    }
                    if let Some(f) = from {
                        let _ = write!(s, " from {}", self.affects(f, t));
                    }
                    s
                }
                // A redirection to a target declares the target after the
                // event's.
                (_, Outcome::Redirect { to, next }) => {
                    let event = self.replaces(&rep.event, t);
                    let next = next.map_or(String::new(), |n| format!("next {n} "));
                    let to = match to {
                        mtg_core::card::RedirectTo::This => "this".to_string(),
                        mtg_core::card::RedirectTo::Target(i) => {
                            self.recipient(Recipient::Target(*i), t)
                        }
                    };
                    format!("this turn, replace {event}: redirect {next}to {to}")
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
            Effect::DamageUnpreventable => "the damage can't be prevented".to_string(),
            Effect::ExcessToController => "excess damage to its controller instead".to_string(),
            Effect::PlayerRule { who, kind } => format!(
                "{} can't {} this turn",
                self.recipient(*who, t),
                player_rule_words(*kind)
            ),
        }
    }
}

thread_local! {
    /// The counts of the card being formatted (`Num::Count` indexes them).
    static COUNTS: std::cell::RefCell<Vec<crate::ast::Count>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// The noun the card being formatted uses for itself (`this creature`).
    static NOUN: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

fn pay_amount(mana: &mtg_core::mana::ManaCost, life: u32) -> String {
    if life > 0 {
        format!("{life} life")
    } else {
        mana.to_string()
    }
}

/// The count of a variable target group: `X`, `1 to 2`, `any number of`.
fn many_prefix(min: Num, max: Option<Num>) -> String {
    match max {
        None => "any number of".to_string(),
        Some(m) if m == min => num(m),
        Some(m) => format!("{} to {}", num(min), num(m)),
    }
}

/// ` to exile`, ` to library top`, or nothing for the graveyard.
fn countered_to(d: crate::ast::CounteredTo) -> &'static str {
    use crate::ast::CounteredTo;
    match d {
        CounteredTo::Graveyard => "",
        CounteredTo::Exile => " to exile",
        CounteredTo::LibraryTop => " to library top",
    }
}

fn num(n: Num) -> String {
    match n {
        Num::N(n) => n.to_string(),
        Num::X => "X".to_string(),
        Num::XPlus(n) => format!("X plus {n}"),
        Num::ThatMuch => "that much".to_string(),
        Num::Count(i) => COUNTS.with(|c| {
            c.borrow()
                .get(usize::from(i))
                .map_or_else(|| "count where life".to_string(), count_text)
        }),
    }
}

/// A count printed before the recipient (`deal damage equal <expr> to
/// ...`), as its expression.
fn equal_before(n: Num) -> Option<String> {
    let Num::Count(i) = n else {
        return None;
    };
    COUNTS.with(|c| {
        c.borrow()
            .get(usize::from(i))
            .filter(|c| c.style == crate::ast::CountStyle::EqualBefore)
            .map(|c| count_expr_text(&c.expr))
    })
}

/// `creature card was exiled this way`, `you discarded card this way`,
/// `the sacrificed creature was Human` (after `if`).
fn this_way_cond(c: &crate::ast::ThisWayCond) -> String {
    use crate::ast::ThisWayWording;
    let verb = crate::ast::this_way_word(c.verb);
    let what = match (&c.filter, c.card) {
        (Some(f), true) => format!("{} card", filter(f)),
        (Some(f), false) => filter(f),
        (None, _) => "card".to_string(),
    };
    match &c.wording {
        ThisWayWording::Was => format!("{what} was {verb} this way"),
        ThisWayWording::Is => format!("{what} is {verb} this way"),
        ThisWayWording::You if c.verb == mtg_core::card::ThisWay::PutInHand => {
            format!("you put {what} into hand this way")
        }
        ThisWayWording::You => format!("you {verb} {what} this way"),
        ThisWayWording::The(noun) => format!(
            "the {verb} {noun} {} {what}",
            if c.negated { "wasn't" } else { "was" }
        ),
    }
}

fn count_text(c: &crate::ast::Count) -> String {
    use crate::ast::CountStyle;
    let style = match c.style {
        CountStyle::Where => "where",
        CountStyle::Equal => "equal",
        CountStyle::Each => "each",
        CountStyle::ThatMany => "that many",
        // Printed by the damage clause: `deal damage equal <expr> to`.
        CountStyle::EqualBefore => return count_expr_text(&c.expr),
    };
    format!("count {style} {}", count_expr_text(&c.expr))
}

#[allow(clippy::too_many_lines)] // one arm per count
fn count_expr_text(e: &crate::ast::CountExpr) -> String {
    use crate::ast::CountExpr;
    match e {
        CountExpr::Times(n, inner) => format!("{n} {}", count_expr_text(inner)),
        CountExpr::Plus { n, word, inner } => {
            let n = if *word {
                crate::render::NUMBER_WORDS
                    .get(*n as usize - 1)
                    .map_or_else(|| n.to_string(), |w| (*w).to_string())
            } else {
                n.to_string()
            };
            format!("plus {n} {}", count_expr_text(inner))
        }
        CountExpr::CardsInGraveyards { filter: f, each } => {
            let place = if *each {
                "each graveyard"
            } else {
                "all graveyards"
            };
            match f {
                None => place.to_string(),
                Some(f) => format!("{place} {}", card_filter(f)),
            }
        }
        // "creatures on the battlefield named ~": the name after the place.
        CountExpr::Permanents(f) if f.who.is_none() && f.same.is_some() && !f.named_first => {
            let mut g = f.clone();
            g.same = None;
            format!(
                "{} on battlefield {}",
                filter(&g),
                same_ref(f.same.expect("checked"))
            )
        }
        CountExpr::Permanents(f) if f.who.is_none() && f.named_first => {
            let mut g = f.clone();
            g.named_first = false;
            format!("{} on battlefield", filter(&g))
        }
        CountExpr::Permanents(f) if f.who.is_none() && f.that_player.is_none() => {
            format!("{} on battlefield", filter(f))
        }
        CountExpr::Permanents(f) => filter(f),
        CountExpr::ThisWay { verb, filter: f } => {
            let verb = crate::ast::this_way_word(*verb);
            match f {
                None => format!("{verb} this way"),
                Some(f) => format!("{} {verb} this way", filter(f)),
            }
        }
        CountExpr::LifeLostThisWay => "life lost this way".to_string(),
        CountExpr::Votes { word, .. } => format!("votes for {word}"),
        CountExpr::Domain => "domain".to_string(),
        CountExpr::CardsInHand => "hand".to_string(),
        CountExpr::CardsInGraveyard(None) => "graveyard".to_string(),
        CountExpr::HostGraveyard(None) => "attached controller graveyard".to_string(),
        CountExpr::Devotion { colors, chroma } => {
            let names: Vec<&str> = colors
                .iter()
                .map(|c| crate::render::color_name(*c))
                .collect();
            if *chroma {
                format!("{} mana symbols", names.join(" and "))
            } else {
                format!("devotion {}", names.join(" and "))
            }
        }
        CountExpr::HostGraveyard(Some(f)) => {
            format!("attached controller graveyard {}", card_filter(f))
        }
        CountExpr::CardsInGraveyard(Some(f)) => format!("graveyard {}", card_filter(f)),
        CountExpr::SourcePower => "power".to_string(),
        CountExpr::TargetStat { n, toughness, that } => format!(
            "{} of {}target {}",
            if *toughness { "toughness" } else { "power" },
            if *that { "that " } else { "" },
            n + 1
        ),
        CountExpr::Life => "life".to_string(),
        CountExpr::BlockingIt { beyond: 0 } => "creature blocking it".to_string(),
        CountExpr::BlockingIt { beyond } => format!("creature blocking it beyond {beyond}"),
        CountExpr::GreatestPower(f) => format!("greatest power {}", filter(f)),
        CountExpr::DifferentlyNamed(f) => format!("differently named {}", filter(f)),
        CountExpr::Party => "party".to_string(),
        CountExpr::GreatestManaValue => "greatest mana value".to_string(),
        CountExpr::CardsDrawnThisTurn => "drawn this turn".to_string(),
        CountExpr::SpellsCastThisTurn { other: false } => "spells cast this turn".to_string(),
        CountExpr::SpellsCastThisTurn { other: true } => "other spells cast this turn".to_string(),
        CountExpr::SourceToughness => "toughness".to_string(),
        CountExpr::CardManaValue => "mana value of that card".to_string(),
        CountExpr::TargetManaValue { n, that, the_spell } => format!(
            "mana value of {}target {}",
            if *the_spell {
                "the spell "
            } else if *that {
                "that "
            } else {
                ""
            },
            n + 1
        ),
        CountExpr::ColorsAmongPermanents => "colors among permanents".to_string(),
        CountExpr::SpentColors => "colors spent".to_string(),
        CountExpr::SpentTreasure => "treasure spent".to_string(),
        CountExpr::CardsInOpponentsGraveyards(None) => "opponents graveyards".to_string(),
        CountExpr::CardsInOpponentsGraveyards(Some(f)) => {
            format!("opponents graveyards {}", card_filter(f))
        }
        CountExpr::OpponentsAttacked => "opponents attacked".to_string(),
        CountExpr::AttackedWithThisTurn => "attacked with this turn".to_string(),
        CountExpr::AndOr(inner) => format!("andor {}", count_expr_text(inner)),
        CountExpr::DiedThisTurn(f) => format!("{} died this turn", filter(f)),
        CountExpr::GreatestPowerInGraveyard(f) => {
            format!("greatest power graveyard {}", card_filter(f))
        }
        CountExpr::AttachedStat(stat) => format!("{} of attached", stat_word(*stat)),
        CountExpr::TriggerStat { stat, that } => format!(
            "{} of {}",
            stat_word(*stat),
            if *that { "that creature" } else { "it" }
        ),
        CountExpr::MovedStat {
            verb,
            stat,
            noun,
            that,
        } => format!(
            "{} of {} {} {noun}",
            stat_word(*stat),
            if *that { "that" } else { "the" },
            crate::ast::this_way_word(*verb)
        ),
        CountExpr::TargetHand(n) => format!("hand of target {}", n + 1),
        CountExpr::GreatestToughness(f) => format!("greatest toughness {}", filter(f)),
        CountExpr::Sum(a, b) => format!("{} plus {}", count_expr_text(a), count_expr_text(b)),
        CountExpr::LifeLostThisTurn => "life you lost this turn".to_string(),
        CountExpr::DamageThisWay { excess: false } => "damage dealt this way".to_string(),
        CountExpr::DamageThisWay { excess: true } => "excess damage dealt this way".to_string(),
        CountExpr::CardTypesInGraveyards => "card types in graveyards".to_string(),
        CountExpr::SourceCounters { kind, it } => format!(
            "{} counters on {}",
            kind.name(),
            if *it { "it" } else { "this" }
        ),
    }
}

/// `power`, `toughness`, `mana value`.
fn stat_word(s: mtg_core::card::Stat) -> &'static str {
    match s {
        mtg_core::card::Stat::Power => "power",
        mtg_core::card::Stat::Toughness => "toughness",
        mtg_core::card::Stat::ManaValue => "mana value",
    }
}

fn cards(n: Num) -> &'static str {
    if n == Num::N(1) { "card" } else { "cards" }
}

fn counter(k: CounterKind, n: Num) -> String {
    let name = k.name();
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
        Outcome::PreventNext(n) => format!("prevent next {}", num(*n)),
        Outcome::Double => "double".to_string(),
        Outcome::PlusOne => "plus one".to_string(),
        Outcome::Instead(mtg_core::state::Zone::Hand) => "hand".to_string(),
        Outcome::Instead(_) => "exile".to_string(),
        Outcome::DrawTwo => "draw two".to_string(),
        Outcome::WinGame => "win".to_string(),
        Outcome::EnterTappedUnless(_)
        | Outcome::EnterWithCountersIf { .. }
        | Outcome::EnterWithCountersMay { .. }
        | Outcome::EnterWithCounterChoice { .. } => {
            unreachable!("an AST ability of its own")
        }
        Outcome::EnterTapped => "tapped".to_string(),
        Outcome::EnterWithCounters { kind, n } => {
            format!("with {} {}", num(*n), counter(*kind, *n))
        }
        Outcome::Regenerate => "regenerate".to_string(),
        Outcome::UmbraArmor => "umbra armor".to_string(),
        Outcome::RedirectToAttached => "redirect to attached".to_string(),
        Outcome::PreventNextTime => "prevent next time".to_string(),
        Outcome::Redirect { to, next } => {
            let next = next.map_or(String::new(), |n| format!("next {n} "));
            match to {
                mtg_core::card::RedirectTo::This => format!("redirect {next}to this"),
                mtg_core::card::RedirectTo::Target(i) => {
                    format!("redirect {next}to target {}", i + 1)
                }
            }
        }
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
pub fn graveyard_target(f: &Filter, who: Who, graveyards: bool) -> String {
    let whose = match who {
        _ if f.that_player.is_some() => "that player's graveyard",
        Who::You => "your graveyard",
        Who::Opponents => "an opponent's graveyard",
        Who::Any if graveyards => "graveyards",
        Who::Any => "a graveyard",
    };
    // "target card named this from your graveyard".
    let named = if f.same.is_some() { " named this" } else { "" };
    let mut g = f.clone();
    g.same = None;
    g.that_player = None;
    // "creature card with mana value X or less": the mana value after the
    // noun.
    let mut mv = String::new();
    if f.same.is_none()
        && f.or.is_none()
        && let Some((cmp, n)) = g.mana_value.take()
    {
        let n = if g.status.mana_value_x {
            "X".to_string()
        } else {
            n.to_string()
        };
        g.status.mana_value_x = false;
        mv = format!(" with mana value {n}{}", cmp_suffix(cmp));
    }
    if g == Filter::default() {
        format!("target card{named}{mv} from {whose}")
    } else {
        format!("target {} card{named}{mv} from {whose}", filter(&g))
    }
}

pub fn filter(f: &Filter) -> String {
    if f.and_or {
        let g = Filter {
            and_or: false,
            ..f.clone()
        };
        return format!("andor {}", filter(&g));
    }
    // "Aura or Equipment attached to target creature": after both sides.
    if f.attached_to.is_some() && f.or.is_some() {
        let base = Filter {
            attached_to: None,
            ..f.clone()
        };
        let only = Filter {
            attached_to: f.attached_to,
            ..Filter::default()
        };
        let tail = filter_one(&only);
        let tail = tail.trim_start_matches("permanent ");
        return format!("{} {tail}", filter(&base));
    }
    if let Some(alt) = &f.or {
        let mut base = f.clone();
        base.or = None;
        if let Some(rest) = attacking_or_blocking(&base, alt) {
            let late = if rest.attacking_late { " late" } else { "" };
            return format!("attacking or blocking{late} {}", filter(&rest));
        }
        if let Some(k) = keyword_alternative(&base, alt) {
            return format!("{} or {}", filter_one(&base), keyword_name(k));
        }
        // "another creature or an artifact": only the first side is
        // other.
        if base.relation == Relation::Other && alt.relation == Relation::Any {
            let a = filter(alt);
            let article = if a.starts_with(['a', 'e', 'i', 'o', 'u']) {
                "an"
            } else {
                "a"
            };
            return format!("{} or {article} {a}", filter_one(&base));
        }
        let (b, a) = factor_qualifiers(&base, alt);
        if f.alt_article {
            let a = filter(&a);
            let article = if a.starts_with(['a', 'e', 'i', 'o', 'u']) {
                "an"
            } else {
                "a"
            };
            return format!("{} or {article} {a}", filter_one(&b));
        }
        return format!("{} or {}", filter_one(&b), filter(&a));
    }
    filter_one(f)
}

#[allow(clippy::too_many_lines)] // one clause per qualifier
fn filter_one(f: &Filter) -> String {
    let mut words: Vec<String> = Vec::new();
    if f.all {
        words.push("all".to_string());
    }
    if f.each {
        words.push("each".to_string());
    }
    match f.relation {
        Relation::Other if f.other_than_this => {}
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
    if f.status.token == Some(false) {
        words.push("nontoken".to_string());
    }
    if let Some(t) = f.tapped {
        words.push(if t { "tapped" } else { "untapped" }.to_string());
    }
    if f.status.face_down {
        words.push("face-down".to_string());
    }
    if f.attacking {
        words.push(if f.attacking_late {
            "attacking late".to_string()
        } else {
            "attacking".to_string()
        });
        // "attacking equipped creature": written where it prints.
        if f.status.equipped && f.equipped_late {
            words.push("with Equipment late".to_string());
        }
    }
    if f.blocking {
        words.push("blocking".to_string());
    }
    if let Some(b) = f.status.blocked {
        words.push(if b { "blocked" } else { "unblocked" }.to_string());
    }
    if f.damaged && !f.damaged_short {
        words.push("damaged".to_string());
    }
    if f.status.modified {
        words.push("modified".to_string());
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
    if f.status.token == Some(true) {
        words.push("token".to_string());
    } else if f.subtypes.is_empty()
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
        if f.status.mana_value_x {
            words.push(format!("with mana value X{}", cmp_suffix(cmp)));
        } else {
            words.push(format!("with mana value {n}{}", cmp_suffix(cmp)));
        }
    }
    if let Some(e) = f.status.enchanted {
        words.push(
            match (e, f.enchanted_late) {
                (true, true) => "with aura late",
                (true, false) => "with aura",
                (false, _) => "without aura",
            }
            .to_string(),
        );
    }
    if f.status.equipped && !(f.equipped_late && f.attacking) {
        words.push(if f.equipped_late {
            "with Equipment late".to_string()
        } else {
            "with Equipment".to_string()
        });
    }
    if f.status.toxic {
        words.push("with toxic".to_string());
    }
    if f.status.morph {
        words.push("with morph".to_string());
    }
    match f.status.on {
        Some(mtg_core::card::OnHost::Creature) => words.push("attached to creature".to_string()),
        Some(mtg_core::card::OnHost::Land) => words.push("attached to land".to_string()),
        None => {}
    }
    match f.status.counter {
        Some(mtg_core::card::CounterQual::Any) => words.push("with counter".to_string()),
        Some(mtg_core::card::CounterQual::Kind(k)) => {
            words.push(format!("with {} counter", k.name()));
        }
        Some(mtg_core::card::CounterQual::AtLeast(k, n)) => {
            words.push(format!("with {n} or more {} counters", k.name()));
        }
        None => {}
    }
    if !f.keywords.is_empty() {
        words.push(format!("with {}", keywords(&f.keywords)));
    }
    if !f.not_keywords.is_empty() {
        words.push(format!("without {}", keywords(&f.not_keywords)));
    }
    if f.named_first
        && let Some(r) = f.same
    {
        words.push(same_ref(r));
    }
    match f.who {
        Some(Who::You) => words.push("you control".to_string()),
        Some(Who::Opponents) if f.dont_control => words.push("you don't control".to_string()),
        Some(Who::Opponents) if f.your_opponents => {
            words.push("your opponents control".to_string());
        }
        Some(Who::Opponents) if f.they_control => words.push("they control".to_string()),
        Some(Who::Opponents) if f.defending => {
            words.push("defending player controls".to_string());
        }
        Some(Who::Opponents) => words.push("an opponent controls".to_string()),
        Some(Who::Any) | None => {}
    }
    match f.that_player {
        Some(mtg_core::card::ThatPlayer::Triggering) => {
            words.push("that player controls".to_string());
        }
        Some(mtg_core::card::ThatPlayer::TargetOrController { n, planeswalker }) => {
            words.push(format!(
                "that player {}{} controls",
                if planeswalker { "or planeswalker " } else { "" },
                n + 1
            ));
        }
        Some(mtg_core::card::ThatPlayer::ControllerOf(n)) => {
            words.push(format!("controller of target {} controls", n + 1));
        }
        Some(mtg_core::card::ThatPlayer::Target(_)) => words.push(
            if f.that_opponent {
                "target opponent controls"
            } else {
                "target player controls"
            }
            .to_string(),
        ),
        None => {}
    }
    if f.not_attached {
        words.push("other than enchanted".to_string());
    }
    if f.other_than_this {
        words.push("other than this".to_string());
    }
    if f.no_abilities {
        words.push("with no abilities".to_string());
    }
    if f.damaged_short {
        words.push("dealt damage this turn".to_string());
    }
    if let Some(r) = f.combat_with {
        words.push(
            match r {
                crate::ast::CombatRef::BlockingThis => "blocking this",
                crate::ast::CombatRef::BlockingIt => "blocking it",
                crate::ast::CombatRef::BlockedByThis => "blocked by this",
                crate::ast::CombatRef::BlockedByIt => "blocked by it",
                crate::ast::CombatRef::BlockingOrBlockedByThis => "blocking or blocked by this",
            }
            .to_string(),
        );
    }
    if f.status.entered_this_turn {
        words.push("that entered this turn".to_string());
    }
    if f.status.dealt_damage {
        words.push("that dealt damage this turn".to_string());
    }
    if f.status.blocked_this_turn {
        words.push("that blocked this turn".to_string());
    }
    if f.status.damaged_by_source {
        words.push("dealt damage by this this turn".to_string());
    }
    match f.attached_to {
        Some(crate::ast::AttachedRef::Source) => words.push("attached to this".to_string()),
        Some(crate::ast::AttachedRef::Host) => words.push("attached to attached".to_string()),
        Some(crate::ast::AttachedRef::TriggeringObject) => {
            words.push("attached to that creature".to_string());
        }
        Some(crate::ast::AttachedRef::Target(n)) => {
            words.push(format!("attached to target {}", n + 1));
        }
        Some(crate::ast::AttachedRef::ThatTarget(n)) => {
            words.push(format!("attached to that target {}", n + 1));
        }
        None => {}
    }
    if let Some(c) = f.chosen {
        words.push(of_chosen(c).to_string());
    }
    if !f.named_first
        && let Some(r) = f.same
    {
        words.push(same_ref(r));
    }
    words.join(" ")
}

/// A filter on cards: `named this` alone rather than `permanent named
/// this`.
fn card_filter(f: &Filter) -> String {
    let mut g = f.clone();
    g.same = None;
    match f.same {
        Some(r) if g == Filter::default() => same_ref(r),
        _ => filter(f),
    }
}

/// `named this`, `with same name as target 1`, `that shares color with
/// target 1`.
pub fn same_ref(r: crate::ast::SameRef) -> String {
    use crate::ast::SameRef;
    match r {
        SameRef::NamedThis => "named this".to_string(),
        SameRef::NotNamedThis => "not named this".to_string(),
        SameRef::SameNameAs(n) => format!("with same name as target {}", n + 1),
        SameRef::SharesColorWith(n) => format!("that shares color with target {}", n + 1),
        SameRef::LesserPower => "with lesser power".to_string(),
    }
}

/// `of chosen type`, `of that type`, `of color of choice`.
fn of_chosen(c: crate::ast::OfChosen) -> &'static str {
    use crate::ast::OfChosen;
    match c {
        OfChosen::ChosenColor => "of chosen color",
        OfChosen::ChosenType => "of chosen type",
        OfChosen::ThatType => "of that type",
        OfChosen::ThatColor => "of that color",
        OfChosen::ColorOfChoice => "of color of choice",
        OfChosen::CreatureTypeOfChoice => "of creature type of choice",
    }
}

/// `target instant spell that targets a creature`.
fn spell_target(f: &Filter) -> String {
    let mut g = f.clone();
    let t = g.targeting.take();
    let base = filter(&g);
    let mut s = if base.is_empty() {
        "target spell".to_string()
    } else {
        format!("target {base} spell")
    };
    if let Some(t) = t {
        s.push_str(" that targets ");
        s.push_str(&targeting(&t, filter));
    }
    s
}

/// "you or a creature you control", "a player", "a creature".
pub fn targeting(t: &crate::ast::Targeting, phrase: impl Fn(&Filter) -> String) -> String {
    let mut parts = Vec::new();
    if t.you {
        parts.push("you".to_string());
    }
    if t.player {
        parts.push("a player".to_string());
    }
    if let Some(f) = &t.filter {
        let p = phrase(f);
        // "that targets this creature": no article.
        if f.relation == Relation::This {
            parts.push(p);
            return parts.join(" or ");
        }
        let a = if p.starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        parts.push(format!("{a} {p}"));
    }
    parts.join(" or ")
}

/// "Goblin or Orc you control", "another Wolf or Werewolf", "Elf or
/// Soldier creature": qualifiers printed once apply to both sides of an
/// `or`. Parsing shares them out (this); printing factors them back
/// (`factor_qualifiers`).
pub fn share_qualifiers(base: &mut Filter, alt: &mut Filter) {
    if base.who.is_none() && alt.who.is_some() {
        base.who = alt.who;
        base.dont_control = alt.dont_control;
        base.they_control = alt.they_control;
    }
    if base.types.is_empty()
        && !base.subtypes.is_empty()
        && !alt.subtypes.is_empty()
        && !alt.types.is_empty()
    {
        base.types.clone_from(&alt.types);
    }
    if base.relation == Relation::Other && alt.relation == Relation::Any {
        alt.relation = Relation::Other;
    }
}

/// The filter without "other", on each side of an `or` ("another Wolf or
/// Werewolf" printed as "another" and the filter).
pub fn relation_any(f: &mut Filter) {
    f.relation = Relation::Any;
    let mut cur = f.or.as_deref_mut();
    while let Some(alt) = cur {
        if alt.relation == Relation::Other {
            alt.relation = Relation::Any;
        }
        cur = alt.or.as_deref_mut();
    }
}

/// The two sides of an `or` as printed: shared qualifiers once.
pub fn factor_qualifiers(base: &Filter, alt: &Filter) -> (Filter, Filter) {
    let (mut b, mut a) = (base.clone(), alt.clone());
    b.or = None;
    if b.who.is_some()
        && b.who == a.who
        && b.dont_control == a.dont_control
        && b.they_control == a.they_control
    {
        b.who = None;
        b.dont_control = false;
        b.they_control = false;
    }
    if !b.subtypes.is_empty() && !a.subtypes.is_empty() && !a.types.is_empty() && b.types == a.types
    {
        b.types.clear();
    }
    // The relation prints once, before the first side ("another Wolf or
    // Werewolf"), however the caller printed it.
    if a.relation == Relation::Other && b.relation == Relation::Other {
        a.relation = Relation::Any;
    }
    (b, a)
}

/// "creatures with flying or reach": the two sides differ only in one
/// keyword each; the base and the alternative keyword.
pub fn keyword_alternative(base: &Filter, alt: &Filter) -> Option<Keyword> {
    let (&[_], &[k]) = (base.keywords.as_slice(), alt.keywords.as_slice()) else {
        return None;
    };
    let mut a = alt.clone();
    a.keywords.clone_from(&base.keywords);
    (a == *base && a.or.is_none()).then_some(k)
}

/// "attacking or blocking creature": the two sides differ only there; the
/// shared rest.
pub fn attacking_or_blocking(base: &Filter, alt: &Filter) -> Option<Filter> {
    if !base.attacking || base.blocking || !alt.blocking || alt.attacking {
        return None;
    }
    let mut b = base.clone();
    b.attacking = false;
    let mut a = alt.clone();
    a.blocking = false;
    (b == a && a.or.is_none()).then_some(b)
}

fn trigger_obj(o: &TriggerObj) -> String {
    match o {
        TriggerObj::This => "this".to_string(),
        TriggerObj::Filter(f) => filter(f),
        TriggerObj::ThisOr(f) => format!("this or {}", filter(f)),
    }
}

fn step_name(s: Step) -> &'static str {
    match s {
        Step::Untap => "untap",
        Step::Upkeep => "upkeep",
        Step::Draw => "draw",
        Step::BeginningOfCombat => "combat",
        Step::PrecombatMain => "first main",
        Step::PostcombatMain => "second main",
        _ => "end",
    }
}

/// "blocks", "becomes blocked by", or "blocks or becomes blocked by" (a
/// creature), in the DSL and in Oracle text alike.
pub fn block_pair_verb(blocks: bool, blocked: bool) -> &'static str {
    match (blocks, blocked) {
        (true, true) => "blocks or becomes blocked by",
        (false, _) => "becomes blocked by",
        (true, false) => "blocks",
    }
}

/// `whenever this blocks a creature with flying`.
fn block_pair_text(
    obj: &TriggerObj,
    other: &Filter,
    blocks: bool,
    blocked: bool,
    any: bool,
) -> String {
    let f = filter(other);
    let a = if any {
        "one or more"
    } else if f.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!(
        "whenever {} {} {a} {f}",
        trigger_obj(obj),
        block_pair_verb(blocks, blocked)
    )
}

/// "whenever you draw your second card each turn".
pub fn draws_nth(whose: Who, n: u8, in_a_turn: bool) -> String {
    let nth = match n {
        1 => "first",
        2 => "second",
        _ => "third",
    };
    let turn = if in_a_turn { "in a turn" } else { "each turn" };
    match whose {
        Who::Opponents => format!("whenever an opponent draws their {nth} card {turn}"),
        _ => format!("whenever you draw your {nth} card {turn}"),
    }
}

/// Who sacrifices in a sacrifice trigger: "you sacrifice", "a player
/// sacrifices", "an opponent sacrifices" (the same in the DSL and Oracle).
pub(crate) fn sacrifices_words(whose: Who) -> &'static str {
    match whose {
        Who::You => "you sacrifice",
        Who::Any => "a player sacrifices",
        Who::Opponents => "an opponent sacrifices",
    }
}

/// Triggers Oracle words "When" (the rest "Whenever", or "At").
pub fn natural_when(t: &Trigger) -> bool {
    matches!(
        t,
        Trigger::Enters(TriggerObj::This)
            | Trigger::Dies(TriggerObj::This)
            | Trigger::Leaves(TriggerObj::This)
            | Trigger::Sacrificed {
                obj: TriggerObj::This,
                ..
            }
            | Trigger::TurnedFaceUp(TriggerObj::This)
            | Trigger::Exploits
            | Trigger::BecomesTarget(TriggerObj::This)
            | Trigger::When(_)
            | Trigger::BeginningOfStep { .. }
            | Trigger::AttachedStep(_)
            | Trigger::State(_)
            | Trigger::EveryStep(_)
            | Trigger::Cycles { this: true, .. }
            | Trigger::CastThis { .. }
    ) || matches!(t, Trigger::Either(a, _) if natural_when(a))
        || matches!(
            t,
            Trigger::Or {
                whenever: false,
                ..
            }
        )
}

/// "your second spell each turn" and kin, after "you cast" / "an opponent
/// casts": the same in the DSL and in Oracle text.
pub fn spell_ordinal(nth: mtg_core::card::SpellOrdinal, whose: Who) -> String {
    use mtg_core::card::SpellOrdinal;
    let (who, their) = match whose {
        Who::You => ("you cast", "your"),
        Who::Opponents => ("an opponent casts", "their"),
        Who::Any => ("a player casts", "their"),
    };
    match nth {
        SpellOrdinal::First => format!("{who} {their} first spell each turn"),
        SpellOrdinal::Second => format!("{who} {their} second spell each turn"),
        SpellOrdinal::NotFirst => {
            format!("{who} a spell other than {their} first spell each turn")
        }
        SpellOrdinal::FirstOnOpponentsTurn => {
            format!("{who} {their} first spell during each opponent's turn")
        }
        SpellOrdinal::OnOpponentsTurn => format!("{who} a spell during an opponent's turn"),
    }
}

/// "you cycle this card", "you cycle or discard another card".
pub fn cycles_text(this: bool, another: bool, or_discard: bool) -> String {
    if this {
        return "when you cycle this card".to_string();
    }
    format!(
        "whenever you cycle {}{}",
        if or_discard { "or discard " } else { "" },
        if another { "another card" } else { "a card" }
    )
}

/// "you draw", "a player draws", "an opponent draws".
pub fn draws_words(whose: Who) -> &'static str {
    match whose {
        Who::You => "you draw",
        Who::Any => "a player draws",
        Who::Opponents => "an opponent draws",
    }
}

/// "you discard", "a player discards", "an opponent discards".
pub fn discards_words(whose: Who) -> &'static str {
    match whose {
        Who::You => "you discard",
        Who::Any => "a player discards",
        Who::Opponents => "an opponent discards",
    }
}

pub fn trigger_text(t: &Trigger, next: bool) -> String {
    if let Trigger::When(inner) = t {
        return trigger_text(inner, next).replacen("whenever", "when", 1);
    }
    trigger_text_one(t, next)
}

#[allow(clippy::too_many_lines)] // one arm per trigger kind
fn trigger_text_one(t: &Trigger, next: bool) -> String {
    match t {
        Trigger::When(_) => unreachable!("unwrapped by trigger_text"),
        Trigger::State(c) => format!("when {}", static_cond(c)),
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
        Trigger::BeginningOfStep {
            step: Step::EndOfCombat,
            ..
        } => "at end of combat".to_string(),
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
            // `your second main`: a phase, not a step.
            let tail = if matches!(step, Step::PrecombatMain | Step::PostcombatMain) {
                ""
            } else {
                " step"
            };
            format!("at beginning of {whose} {}{tail}", step_name(*step))
        }
        Trigger::AttachedStep(step) => {
            let tail = if *step == Step::End { " step" } else { "" };
            format!(
                "at beginning of enchanted controller's {}{tail}",
                step_name(*step)
            )
        }
        Trigger::Attacks(o) => format!("whenever {} attacks", trigger_obj(o)),
        Trigger::AttacksAlone(o) => format!("whenever {} attacks alone", trigger_obj(o)),
        Trigger::AttacksWith {
            this: false,
            min: 1,
        } => "whenever you attack".to_string(),
        Trigger::AttacksWith { this: false, min } => {
            format!("whenever you attack with {min} or more creatures")
        }
        Trigger::AttacksWith { this: true, min } => format!(
            "whenever this and at least {} other creatures attack",
            min.saturating_sub(1)
        ),
        Trigger::AttacksUnblocked(o) => {
            format!("whenever {} attacks and isn't blocked", trigger_obj(o))
        }
        Trigger::BecomesBlocked(o) => format!("whenever {} becomes blocked", trigger_obj(o)),
        Trigger::BlocksOrBlocked(o) => {
            format!("whenever {} blocks or becomes blocked", trigger_obj(o))
        }
        Trigger::Blocks(o) => format!("whenever {} blocks", trigger_obj(o)),
        Trigger::BlockPair {
            obj,
            other,
            blocks,
            blocked,
            any,
        } => block_pair_text(obj, other, *blocks, *blocked, *any),
        Trigger::AttacksOrBlocks(o) => format!("whenever {} attacks or blocks", trigger_obj(o)),
        Trigger::Leaves(o) => format!("whenever {} leaves the battlefield", trigger_obj(o)),
        Trigger::DealsCombatDamageToPlayer(o) => {
            format!(
                "whenever {} deals combat damage to a player",
                trigger_obj(o)
            )
        }
        Trigger::DealsCombatDamageToPlayerOrBattle(o) => {
            format!(
                "whenever {} deals combat damage to a player or battle",
                trigger_obj(o)
            )
        }
        Trigger::AttacksBattle(o) => format!("whenever {} attacks a battle", trigger_obj(o)),
        Trigger::UnlocksDoor => "when you unlock this door".to_string(),
        Trigger::FullyUnlocks => "whenever you fully unlock a Room".to_string(),
        Trigger::DealsCombatDamageToDefending(o) => {
            format!(
                "whenever {} deals combat damage to defending player",
                trigger_obj(o)
            )
        }
        Trigger::WinsFlip { .. } => "whenever you win a coin flip".to_string(),
        Trigger::DayNight => "whenever day becomes night or night becomes day".to_string(),
        Trigger::Sacrificed { obj, whose } => {
            format!("whenever {} {}", sacrifices_words(*whose), trigger_obj(obj))
        }
        Trigger::RollsDice { whose } => match whose {
            Who::You => "whenever you roll dice".to_string(),
            _ => "whenever a player rolls dice".to_string(),
        },
        Trigger::GainsLife { whose } => match whose {
            Who::You => "whenever you gain life".to_string(),
            _ => "whenever a player gains life".to_string(),
        },
        Trigger::GainsLifeFirst { .. } => "whenever you gain life first time each turn".to_string(),
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
        Trigger::BecomesTarget(o) => format!("when {} becomes targeted", trigger_obj(o)),
        Trigger::TargetedByOpponent => "whenever this becomes targeted by opponent".to_string(),
        Trigger::TargetedByYouFirstTime => {
            "whenever this becomes targeted by you first time each turn".to_string()
        }
        Trigger::TurnedFaceUp(o) => format!("whenever {} is turned face up", trigger_obj(o)),
        Trigger::Exploits => "when this exploits a creature".to_string(),
        Trigger::DrawsNth {
            whose,
            n,
            in_a_turn,
        } => draws_nth(*whose, *n, *in_a_turn),
        Trigger::BecomesTargetWhenever(o) => {
            format!("whenever {} becomes targeted", trigger_obj(o))
        }
        Trigger::BecomesTapped(o) => format!("whenever {} becomes tapped", trigger_obj(o)),
        Trigger::BecomesAttachedTo(f) => {
            format!("whenever {} becomes attached to this", filter(f))
        }
        Trigger::TappedForMana(o) => format!("whenever {} is tapped for mana", trigger_obj(o)),
        Trigger::BecomesUntapped(o) => format!("whenever {} becomes untapped", trigger_obj(o)),
        Trigger::CountersPut { obj, kind } => format!(
            "whenever one or more {} counters put on {}",
            kind.name(),
            trigger_obj(obj)
        ),
        Trigger::DealtDamage(o) => format!("whenever {} is dealt damage", trigger_obj(o)),
        Trigger::DealtCombatDamage(o) => {
            format!("whenever {} is dealt combat damage", trigger_obj(o))
        }
        Trigger::DealsDamageTo { obj, filter: f } => {
            format!(
                "whenever {} deals damage to {}",
                trigger_obj(obj),
                filter(f)
            )
        }
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
        Trigger::CastsFiltered { whose, filter: f } => {
            let mut g = f.clone();
            let t = g.targeting.take();
            let base = filter(&g);
            let mut s = format!(
                "whenever {} {} spell",
                match whose {
                    Who::You => "you cast",
                    Who::Opponents => "an opponent casts",
                    Who::Any => "a player casts",
                },
                if base.is_empty() { "a" } else { &base }
            );
            if let Some(t) = t {
                s.push_str(" that targets ");
                s.push_str(&targeting(&t, filter));
            }
            s
        }
        Trigger::Cycles {
            this,
            another,
            or_discard,
        } => cycles_text(*this, *another, *or_discard),
        Trigger::CastThis { or_cycle: false } => "when you cast this".to_string(),
        Trigger::Sacrifices(f) => format!("whenever you sacrifice {}", filter(f)),
        Trigger::CastThis { or_cycle: true } => "when you cast or cycle this".to_string(),
        Trigger::CastsNth { whose, nth } => format!("whenever {}", spell_ordinal(*nth, *whose)),
        Trigger::Either(a, b) => format!("{} and {}", trigger_text(a, next), trigger_text(b, next)),
        Trigger::Or {
            second, whenever, ..
        } => {
            let b = trigger_text(second, next);
            let verb = b.split_once("this ").map_or(b.as_str(), |(_, v)| v);
            format!(
                "{} this enters or {verb}",
                if *whenever { "whenever" } else { "when" }
            )
        }
        Trigger::Draws { whose } => format!("whenever {} a card", draws_words(*whose)),
        Trigger::Discards { whose, filter: f } => format!(
            "whenever {} {}",
            discards_words(*whose),
            if *f == Filter::default() {
                "a card".to_string()
            } else {
                format!("{} card", filter(f))
            }
        ),
        Trigger::LeavesGraveyard(f) => format!(
            "whenever one or more {}cards leave your graveyard",
            if *f == Filter::default() {
                String::new()
            } else {
                format!("{} ", filter(f))
            }
        ),
        Trigger::PutIntoGraveyard(f) => format!(
            "whenever {} card is put into your graveyard from anywhere",
            filter(f)
        ),
        Trigger::EntersFromGraveyard(f) => {
            format!("whenever {} enters from your graveyard", filter(f))
        }
        Trigger::Expends(n) => format!("whenever you expend {n}"),
        Trigger::EntersBatch(f) => format!("whenever one or more {} enter", filter(f)),
        Trigger::LeavesBatch { filter: f, dies } => format!(
            "whenever one or more {} {}",
            filter(f),
            if *dies {
                "die"
            } else {
                "leave the battlefield"
            }
        ),
    }
}

#[allow(clippy::too_many_lines)] // one branch per cost part
fn cost_text(c: &Cost) -> String {
    let default_order = [
        CostItem::Mana,
        CostItem::Tap,
        CostItem::Sacrifice,
        CostItem::Discard,
        CostItem::Life,
        CostItem::ExileCards,
        CostItem::Blight,
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
                if c.waterbend {
                    parts.push(format!("waterbend {mana}"));
                } else if !mana.is_empty() {
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
                Some(SacCost::Filter(f)) => parts.push(format!(
                    "{} {}{}",
                    match c.chosen {
                        mtg_core::card::ChosenCost::Sacrifice => "sacrifice",
                        mtg_core::card::ChosenCost::Return => "return",
                        mtg_core::card::ChosenCost::Tap => "tap",
                    },
                    if c.count > 1 {
                        format!("{} ", c.count)
                    } else {
                        String::new()
                    },
                    filter(f)
                )),
                None => {}
            },
            CostItem::RemoveCounters => {
                if let Some((k, n)) = c.remove_counters {
                    parts.push(format!("remove {n} {} from this", counter(k, Num::N(n))));
                }
            }
            CostItem::PutCounters => {
                if let Some((k, n)) = c.put_counters {
                    parts.push(format!("put {n} {} on this", counter(k, Num::N(n))));
                }
            }
            CostItem::Exile => {
                if c.exile_self {
                    parts.push("exile this from graveyard".to_string());
                } else if c.exile_this {
                    parts.push("exile this".to_string());
                }
            }
            CostItem::ExileCards => {
                if let Some(g) = &c.exile_cards {
                    let n = if g.x {
                        "X ".to_string()
                    } else if g.n > 1 {
                        format!("{} ", g.n)
                    } else {
                        String::new()
                    };
                    let what = g.filter.as_ref().map_or(String::new(), |f| filter(f) + " ");
                    let cards = if g.x || g.n > 1 { "cards" } else { "card" };
                    let top = if g.top { "top " } else { "" };
                    parts.push(format!("exile {top}{n}{what}{cards} from graveyard"));
                }
            }
            CostItem::FromGraveyard => parts.push("from graveyard".to_string()),
            CostItem::Discard => {
                if c.discard_self {
                    parts.push("discard this".to_string());
                } else if c.discard_x {
                    let what = c
                        .discard_filter
                        .as_ref()
                        .map_or(String::new(), |f| filter(f) + " ");
                    parts.push(format!("discard X {what}cards"));
                } else if let Some(f) = &c.discard_filter {
                    parts.push(format!("discard {} card", filter(f)));
                } else if c.discard > 0 {
                    parts.push(format!(
                        "discard {} {}{}",
                        c.discard,
                        if c.discard == 1 { "card" } else { "cards" },
                        if c.discard_random { " at random" } else { "" }
                    ));
                }
            }
            CostItem::Life => {
                if c.life > 0 {
                    parts.push(format!("pay {} life", c.life));
                }
            }
            CostItem::RevealHand => parts.push("reveal hand".to_string()),
            CostItem::Energy => parts.push(format!("pay {} energy", c.energy)),
            CostItem::Blight => {
                if c.blight > 0 {
                    parts.push(format!("blight {}", c.blight));
                }
            }
        }
    }
    if c.restrict.is_empty() {
        if c.sorcery && c.loyalty.is_none() {
            parts.push("sorcery".to_string());
        }
        if c.per_turn == 1 {
            parts.push("once".to_string());
        }
    }
    for r in &c.restrict {
        match r {
            Restrict::Sorcery => parts.push("sorcery".to_string()),
            Restrict::Once => parts.push("once".to_string()),
            Restrict::OnceEver => parts.push("once ever".to_string()),
            Restrict::Cond(i) => parts.push(match &c.conditions[*i] {
                StaticCond::DuringYourTurn => "during your turn".to_string(),
                d @ StaticCond::During(_) => static_cond(d),
                d => format!("if {}", static_cond(d)),
            }),
        }
    }
    if c.per_turn > 1 {
        parts.push(format!("{} times per turn", c.per_turn));
    }
    match c.reduce.as_deref() {
        Some((by, crate::ast::ReduceBy::Each(e))) => {
            parts.push(format!("reduced {by} each {}", count_expr_text(e)));
        }
        Some((by, crate::ast::ReduceBy::If(d))) => {
            parts.push(format!("reduced {by} if {}", static_cond(d)));
        }
        None => {}
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

#[allow(clippy::too_many_lines)] // one arm per modification
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
        Mod::SetTypes(ts) => format!(
            "set type {}",
            ts.iter()
                .map(|t| format!("{t:?}").to_lowercase())
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Mod::AddLandType(c) => format!("add land {}", basic_land_name(*c)),
        Mod::SetLandType(c) => format!("set land {}", basic_land_name(*c)),
        Mod::SetLandTypes(cs) => format!(
            "set land {}",
            cs.iter()
                .map(|c| basic_land_name(*c))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Mod::ChosenLandType => "set land chosen".to_string(),
        Mod::AddChosenType => "add chosen type".to_string(),
        Mod::AddSubtype(s) => format!("add subtype {s}"),
        Mod::SetSubtype(s) => format!("set subtype {s}"),
        Mod::AddAllLandTypes => "add land all".to_string(),
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
        Mod::Goaded => "goaded".to_string(),
        Mod::AllCreatureTypes { every: true } => "add subtype every".to_string(),
        Mod::AllCreatureTypes { every: false } => "add subtype all".to_string(),
        Mod::NoCreatureTypes => "loses all creature types".to_string(),
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
        Mod::ChosenColor => "set color chosen".to_string(),
        Mod::ProtectionFromChosen => "has protection from chosen color".to_string(),
        Mod::CdaColors(cs) => {
            if cs.len() == 5 {
                "cda color all".to_string()
            } else if cs.is_empty() {
                "cda color colorless".to_string()
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
            sets,
            plus_toughness,
        } => {
            use crate::ast::CountExpr;
            let c = match count {
                CountExpr::Permanents(f) if f.who.is_none() => {
                    format!("count {} on battlefield", filter(f))
                }
                CountExpr::Permanents(f) => format!("count {}", filter(f)),
                CountExpr::Life => "your life".to_string(),
                other => count_expr_text(other),
            };
            let what = match sets {
                mtg_core::card::CdaSets::Both => "pt",
                mtg_core::card::CdaSets::Power => "power",
                mtg_core::card::CdaSets::Toughness => "toughness",
            };
            if *plus_toughness == 0 {
                format!("cda {what} {c}")
            } else {
                format!("cda {what} {c} {plus_toughness:+}")
            }
        }
    }
}

/// Writes `head: body` on one line, or `head {` ... `}` if the effects
/// span paragraphs.
/// `mode`, or `mode "Smash the Chest"` for a named mode.
fn mode_head(m: &crate::ast::Mode) -> String {
    match &m.name {
        Some(n) => format!("mode \"{n}\""),
        None => "mode".to_string(),
    }
}

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
    let rows_of = |s: &Sentence| match s.effects.last() {
        Some(Effect::RollDie { rows, .. }) if !rows.is_empty() => Some(rows.clone()),
        _ => None,
    };
    if ss.iter().any(|s| s.para || rows_of(s).is_some()) {
        let _ = writeln!(out, "{indent}{head} {{");
        let mut line: Vec<String> = Vec::new();
        for s in ss {
            if s.para && !line.is_empty() {
                let _ = writeln!(out, "{indent}  {}", line.join("; "));
                line.clear();
            }
            line.push(f.sentence(s, targets));
            // A roll's results table: a `result` line per row (CR 706.3).
            if let Some(rows) = rows_of(s) {
                let _ = writeln!(out, "{indent}  {}", line.join("; "));
                line.clear();
                for row in &rows {
                    let range = match row.hi {
                        Some(hi) if hi == row.lo => format!("{hi}"),
                        Some(hi) => format!("{} to {hi}", row.lo),
                        None => format!("{} or more", row.lo),
                    };
                    let _ = writeln!(
                        out,
                        "{indent}  result {range}: {}",
                        f.line(&row.effects, targets)
                    );
                }
            }
        }
        if !line.is_empty() {
            let _ = writeln!(out, "{indent}  {}", line.join("; "));
        }
        let _ = writeln!(out, "{indent}}}");
    } else {
        let _ = writeln!(out, "{indent}{head}: {}", f.line(ss, targets));
    }
}

#[allow(clippy::too_many_lines)] // one arm per condition
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
        StaticCond::NotYourTurn => "not your turn".to_string(),
        StaticCond::CastDuringMain => "cast during main phase".to_string(),
        StaticCond::ControlledAsCast(f) => format!("controlled {} as cast", filter(f)),
        StaticCond::Attacking => "attacking".to_string(),
        StaticCond::Enchanted => "enchanted".to_string(),
        StaticCond::Equipped => "equipped".to_string(),
        StaticCond::Monstrous => "monstrous".to_string(),
        StaticCond::OpponentLifeAtMost(n) => format!("opponent life {n} or less"),
        StaticCond::MoreLifeThanOpponent => "your life more than opponent".to_string(),
        StaticCond::BasicLandTypes(n) => format!("{n} basic land types"),
        StaticCond::YouDontControl(fl) => format!("you don't control {}", filter(fl)),
        StaticCond::AnyPlayerLifeAtMost(n) => format!("player life {n} or less"),
        StaticCond::OpponentPoison(1) => "opponent poisoned".to_string(),
        StaticCond::OpponentPoisonAtLeast(n) | StaticCond::OpponentPoison(n) => {
            format!("opponent poison {n} or more")
        }
        StaticCond::SourceCountersThere { kind, n } => {
            format!("there are {n} or more {} counters on this", kind.name())
        }
        StaticCond::SourceCounters { kind, n, at_least } => {
            if *at_least {
                format!("this has {n} or more {} counters", kind.name())
            } else {
                format!("this has no {} counters", kind.name())
            }
        }
        StaticCond::DefenderControls {
            filter: fl,
            at_least: 1,
        } => format!("defending player controls {}", filter(fl)),
        StaticCond::DefenderControls {
            filter: fl,
            at_least: 0,
        } => format!("defending player controls no {}", filter(fl)),
        StaticCond::DefenderControls {
            filter: fl,
            at_least,
        } => format!("defending player controls {at_least} {}", filter(fl)),
        StaticCond::YouControlNone(fl) => format!("you control no {}", filter(fl)),
        StaticCond::YouControlEach(fs) => format!(
            "you control {}",
            fs.iter().map(filter).collect::<Vec<_>>().join(" and ")
        ),
        StaticCond::OnBattlefield {
            filter: fl,
            at_least: 1,
        } => format!("on battlefield {}", filter(fl)),
        StaticCond::OnBattlefield {
            filter: fl,
            at_least,
        } => format!("on battlefield {at_least} {}", filter(fl)),
        StaticCond::DefenderPoisoned => "defending player poisoned".to_string(),
        StaticCond::OthersInCombat {
            filter: fl,
            also: true,
            block,
            ..
        } => format!(
            "{} also {}",
            filter(fl),
            if *block { "blocks" } else { "attacks" }
        ),
        StaticCond::OthersInCombat {
            at_least, block, ..
        } => format!(
            "at least {at_least} other creatures {}",
            if *block { "block" } else { "attack" }
        ),
        StaticCond::MoreCreaturesThanOpponent { attacking } => format!(
            "you control more creatures than {} player",
            if *attacking { "attacking" } else { "defending" }
        ),
        StaticCond::AttackedThisTurn => "attacked this turn".to_string(),
        StaticCond::UnlockedDoors(n) => format!("{n} unlocked doors"),
        StaticCond::AttackedWith(f) => format!("attacked with {} this turn", filter(f)),
        StaticCond::EnteredWith {
            filter: f,
            had: false,
        } => {
            format!("entered {} this turn", filter(f))
        }
        StaticCond::EnteredWith {
            filter: f,
            had: true,
        } => {
            format!("had {} enter this turn", filter(f))
        }
        StaticCond::AttackedBattleThisTurn => "attacked a battle this turn".to_string(),
        StaticCond::AttackedLastTurn => "attacked last turn".to_string(),
        StaticCond::CreatureDiedThisTurn => "creature died this turn".to_string(),
        StaticCond::OpponentDamagedThisTurn => "opponent damaged this turn".to_string(),
        StaticCond::OpponentLostLifeThisTurn => "opponent lost life this turn".to_string(),
        StaticCond::PermanentLeftThisTurn => "permanent left this turn".to_string(),
        StaticCond::Escaped => unreachable!("only in `escapes with`"),
        StaticCond::Kicked => "kicked".to_string(),
        StaticCond::Bargained => "bargained".to_string(),
        StaticCond::Teamwork => "teamwork".to_string(),
        StaticCond::ManaSpent {
            kind,
            at_least,
            that,
        } => format!(
            "{}{}",
            if *that { "that spell " } else { "" },
            spent_cond(*kind, *at_least)
        ),
        StaticCond::Saddled => "saddled".to_string(),
        StaticCond::LandEnteredThisTurn => "land entered this turn".to_string(),
        StaticCond::ArtifactEnteredThisTurn => "artifact entered this turn".to_string(),
        StaticCond::CardsDrawnThisTurn(n) => format!("drew {n} cards this turn"),
        StaticCond::CardsInHand { n, at_most } => {
            format!("hand {n} or {}", if *at_most { "less" } else { "more" })
        }
        StaticCond::NoCardInHand(f) => format!("hand has no {} card", filter(f)),
        StaticCond::YourLife { n, at_least } => {
            format!(
                "your life {n} or {}",
                if *at_least { "more" } else { "less" }
            )
        }
        StaticCond::GraveyardCards {
            filter: fl,
            at_least,
            wording,
        } => {
            use crate::ast::GraveyardWording as W;
            match wording {
                W::ThereAre => format!("{at_least} {} cards in your graveyard", filter(fl)),
                W::YouHave => {
                    format!("you have {at_least} {} cards in your graveyard", filter(fl))
                }
                W::ThereIs => format!("there is {} card in your graveyard", filter(fl)),
                W::Theres => format!("there's {} card in your graveyard", filter(fl)),
                W::IsIn => format!("1 {} card is in your graveyard", filter(fl)),
            }
        }
        StaticCond::Or(a, b) => format!("{} or {}", static_cond(a), static_cond(b)),
        StaticCond::OpponentGraveyard(n) => format!("opponent graveyard {n} or more"),
        StaticCond::ManaValuesInGraveyard(n) => format!("{n} mana values in your graveyard"),
        StaticCond::OpponentControls {
            filter: fl,
            at_least: 1,
        } => format!("opponent controls {}", filter(fl)),
        StaticCond::OpponentControls {
            filter: fl,
            at_least,
        } => format!("opponent controls {at_least} {}", filter(fl)),
        StaticCond::Is(fl) => format!("this is {}", filter(fl)),
        StaticCond::Modified => "modified".to_string(),
        StaticCond::AttackingAlone => "attacking alone".to_string(),
        StaticCond::Blocked => "blocked".to_string(),
        StaticCond::BeingBlocked => "being blocked".to_string(),
        StaticCond::AttachedIs(fl) => format!("attached is {}", filter(fl)),
        StaticCond::DrawnThisTurn(n) => format!("drew {n} this turn"),
        StaticCond::CastThisTurn {
            filter: fl,
            at_least,
            other,
        } => {
            let mut s = "cast ".to_string();
            if *at_least != 1 {
                let _ = write!(s, "{at_least} ");
            }
            if *other {
                s.push_str("another ");
            }
            if *fl != Filter::default() {
                s.push_str(&filter(fl));
                s.push(' ');
            }
            s.push_str("spell this turn");
            s
        }
        StaticCond::CastFrom(z) => match z {
            mtg_core::state::Zone::Hand => "cast from hand",
            mtg_core::state::Zone::Graveyard => "cast from graveyard",
            _ => "cast from exile",
        }
        .to_string(),
        StaticCond::GainedLifeThisTurn { or_lost: false } => "gained life this turn".to_string(),
        StaticCond::GainedLifeThisTurn { or_lost: true } => {
            "gained or lost life this turn".to_string()
        }
        StaticCond::DescendedThisTurn => "descended this turn".to_string(),
        StaticCond::SpellNumber(n) => format!("spell number {n} this turn"),
        StaticCond::SurveilledThisTurn => "surveilled this turn".to_string(),
        StaticCond::CounterPutThisTurn => "counter put this turn".to_string(),
        StaticCond::FaceDownEnteredThisTurn => "face-down creature entered this turn".to_string(),
        StaticCond::CreatureEnteredThisTurn { other } => format!(
            "{}creature entered this turn",
            if *other { "another " } else { "" }
        ),
        StaticCond::NonlandEnteredThisTurn(n) => format!("{n} nonland entered this turn"),
        StaticCond::PutIntoGraveyardThisTurn(t) => {
            let names: Vec<&str> = t.iter().map(|c| crate::render::type_noun(*c)).collect();
            format!("{} to graveyard this turn", names.join(" or "))
        }
        StaticCond::DiscardedThisTurn => "discarded this turn".to_string(),
        StaticCond::Night => "night".to_string(),
        StaticCond::LifeGainedThisTurn(n) => format!("gained {n} life this turn"),
        StaticCond::CrimeThisTurn => "committed crime this turn".to_string(),
        StaticCond::LeftGraveyardThisTurn => "card left your graveyard this turn".to_string(),
        StaticCond::SacrificedThisTurn(t) => {
            let names: Vec<&str> = t.iter().map(|c| crate::render::type_noun(*c)).collect();
            format!("sacrificed {} this turn", names.join(" or "))
        }
        StaticCond::CardToGraveyardThisTurn(t) => {
            let names: Vec<&str> = t.iter().map(|c| crate::render::type_noun(*c)).collect();
            format!("{} card to your graveyard this turn", names.join(" or "))
        }
        StaticCond::MoreCardsInHand => "more cards in hand than each opponent".to_string(),
        StaticCond::OwnCardInExile => "you own card in exile".to_string(),
        StaticCond::Monarch => "you are monarch".to_string(),
        StaticCond::CompletedDungeon => "completed dungeon".to_string(),
        StaticCond::CityBlessing => "city's blessing".to_string(),
        StaticCond::MaxSpeed => "max speed".to_string(),
        StaticCond::TotalPower(n) => format!("total power {n} or more"),
        StaticCond::DifferentPowers(n) => format!("{n} different powers"),
        StaticCond::During(t) => {
            use mtg_core::card::TurnTime as T;
            match t {
                T::YourUpkeep => "during your upkeep",
                T::OpponentUpkeep => "during opponent upkeep",
                T::Combat => "during combat",
                T::BeforeAttackers => "before attackers",
                T::YourTurnBeforeAttackers => "during your turn before attackers",
                T::DeclareBlockers => "during declare blockers",
                T::EndOfCombat => "during end of combat",
                T::YourMain => "during your main phase",
                T::DeclareAttackers => "during declare attackers",
                T::BeforeBlockers => "during combat before blockers",
                T::AfterBlockers => "during combat after blockers",
                T::AttackedThisStep => "during declare attackers if attacked",
            }
            .to_string()
        }
    }
}

/// `{C} only to cast instant or sorcery spell`: a restricted mana
/// ability's mana and restriction (CR 106.6).
fn restricted_mana(m: Mana, r: &crate::ast::ManaRestriction) -> String {
    use mtg_core::card::SpendWording;
    let sym = match m {
        Mana::Colorless => "{C}".to_string(),
        Mana::Colored(c) => format!("{{{}}}", c.letter()),
        Mana::Any => "any".to_string(),
        Mana::Chosen => "chosen".to_string(),
    };
    // Each side of an alternative whole: "Elemental spell or Chandra
    // planeswalker spell".
    let spells = r.spells.as_ref().map(|f| match &f.or {
        Some(alt) => {
            let mut base = f.clone();
            base.or = None;
            format!("cast {} spell or {} spell", filter(&base), filter(alt))
        }
        None => format!("cast {} spell", filter(f)),
    });
    let of = r
        .ability_source
        .as_ref()
        .map(|f| format!(" of {}", filter(f)))
        .unwrap_or_default();
    let spells = spells.map(|s| {
        if r.foretell {
            format!("foretell or {s}")
        } else if r.disturb {
            format!("pay disturb or {s}")
        } else {
            s
        }
    });
    let rest = match (r.wording, spells) {
        (SpendWording::CantCast, Some(s)) => format!("not to {s}"),
        (SpendWording::ActivateFirst, Some(s)) => format!("only to activate{of} or {s}"),
        (SpendWording::CastFirstTo, Some(s)) if r.abilities => {
            format!("only to {s} or to activate{of}")
        }
        (_, Some(s)) if r.abilities => format!("only to {s} or activate{of}"),
        (_, Some(s)) => format!("only to {s}"),
        (_, None) => format!("only to activate{of}"),
    };
    format!("{sym} {rest}")
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
                    Mana::Chosen => "chosen".to_string(),
                })
                .collect();
            let _ = writeln!(out, "{indent}mana {}", syms.join(" or "));
        }
        Ability::RestrictedMana { mana, restriction } => {
            let _ = writeln!(out, "{indent}mana {}", restricted_mana(*mana, restriction));
        }
        Ability::XManaColor(c) => {
            let _ = writeln!(
                out,
                "{indent}spend only {} mana on x",
                crate::render::color_name(*c)
            );
        }
        Ability::SacMana(m) => {
            let sym = match m {
                Mana::Colorless => "{C}".to_string(),
                Mana::Colored(c) => format!("{{{}}}", c.letter()),
                Mana::Any => "any".to_string(),
                Mana::Chosen => "chosen".to_string(),
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
                Target::UpTo(_) | Target::Many { .. } => unreachable!("base"),
            };
            let _ = writeln!(out, "{indent}enchant {phrase}");
        }
        Ability::Spell { targets, effects } => body(out, indent, "spell", effects, targets, groups),
        Ability::Exert(effects) => body(out, indent, "exert", effects, &[], groups),
        Ability::Modal(modes) | Ability::ModalBoth(modes) | Ability::ModalTwo(modes) => {
            let head = match ability {
                Ability::ModalBoth(_) => "choose one or both",
                Ability::ModalTwo(_) => "choose two",
                _ => "choose one",
            };
            let _ = writeln!(out, "{indent}{head} {{");
            for m in modes {
                body(
                    out,
                    &format!("{indent}  "),
                    &mode_head(m),
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
                let sub = crate::render::equip_subtype(targets)
                    .map(|s| format!("{s} "))
                    .unwrap_or_default();
                let once = if cost.per_turn == 1 { ", once" } else { "" };
                let _ = writeln!(out, "{indent}equip {sub}{}{once}", cost.mana);
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
                // `cycling pay 2 life`: a cost other than mana.
                if cost.mana.is_empty() && word == "cycling" {
                    let mut c = cost.clone();
                    c.discard_self = false;
                    let _ = writeln!(out, "{indent}cycling {}", cost_text(&c));
                    return;
                }
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
            let word = match cost.keyword {
                Some(mtg_core::card::ActivatedKeyword::Boast) => "boast",
                Some(mtg_core::card::ActivatedKeyword::Exhaust) => "exhaust",
                Some(mtg_core::card::ActivatedKeyword::PowerUp) => "power-up",
                // Only a `ninjutsu` line's compiled form has this.
                Some(mtg_core::card::ActivatedKeyword::Ninjutsu) | None => "activated",
            };
            body(
                out,
                indent,
                &format!("{word} {}", cost_text(cost)),
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
            from_graveyard,
        } => {
            let mut head = format!(
                "triggered {}{}",
                if *from_graveyard {
                    "from graveyard "
                } else {
                    ""
                },
                trigger_text(trigger, false)
            );
            let condition = match condition {
                Some(crate::ast::TrigCond::While(StaticCond::DuringYourTurn)) => {
                    head.push_str(" during your turn");
                    None
                }
                Some(crate::ast::TrigCond::While(c)) => {
                    let _ = write!(head, " while {}", static_cond(c));
                    None
                }
                Some(crate::ast::TrigCond::Static(c)) => {
                    let _ = write!(head, " if {}", static_cond(c));
                    None
                }
                Some(crate::ast::TrigCond::Core(c)) => Some(c),
                None => None,
            };
            match condition {
                Some(
                    mtg_core::card::Condition::Static(_) | mtg_core::card::Condition::While(_),
                ) => unreachable!("an AST condition"),
                Some(mtg_core::card::Condition::SourceUntapped) => {
                    head.push_str(" if this is untapped");
                }
                Some(mtg_core::card::Condition::SourceTapped) => {
                    head.push_str(" if this is tapped");
                }
                Some(mtg_core::card::Condition::NoCreatures) => {
                    head.push_str(" if no creatures are on the battlefield");
                }
                Some(mtg_core::card::Condition::EchoDue) => head.push_str(" if echo due"),
                Some(mtg_core::card::Condition::NotRenowned) => head.push_str(" if not renowned"),
                Some(mtg_core::card::Condition::Evolves) => head.push_str(" if evolves"),
                Some(mtg_core::card::Condition::Evoked) => head.push_str(" if evoked"),
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
                Some(mtg_core::card::Condition::HadCounters(k)) => {
                    let _ = write!(head, " if it had {}", counter(*k, Num::N(1)));
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
            cond_if,
        } => {
            let mods = mods
                .iter()
                .map(|m| match m {
                    Mod::AddAbility(a) => match granted_mechanic(a) {
                        Some(m) => format!("has {m}"),
                        None => format!("has ability {{ {} }}", nested(a, groups)),
                    },
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
                Some(StaticCond::NotYourTurn) => {
                    let _ = writeln!(
                        out,
                        "{indent}static during turns other than yours, {}: {mods}",
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
                        "{indent}static {}: {mods} {} {}",
                        filter(fl),
                        if *cond_if { "if" } else { "as long as" },
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
                crate::ast::BlockRule::NotByLesserPower => {
                    "can't be blocked by creature with lesser power".to_string()
                }
                crate::ast::BlockRule::BlocksOnly(f) => format!("can block only {}", filter(f)),
                crate::ast::BlockRule::CantBlock(f) => format!("can't block {}", filter(f)),
                crate::ast::BlockRule::MinBlockers(n) => {
                    format!("can't be blocked except by {n} or more creatures")
                }
                crate::ast::BlockRule::BlocksShadow { had: true } => {
                    "can block shadow as though had shadow".to_string()
                }
                crate::ast::BlockRule::BlocksShadow { had: false } => {
                    "can block shadow as though didn't have shadow".to_string()
                }
                crate::ast::BlockRule::BlocksLandwalk => {
                    "can block landwalk as though didn't have".to_string()
                }
                crate::ast::BlockRule::BlocksAdditional(n) => {
                    format!("can block an additional {n} creatures each combat")
                }
            };
            let _ = writeln!(out, "{indent}{text}");
        }
        Ability::EntersAsCopy(f) => {
            let _ = writeln!(out, "{indent}enters as copy of {}", filter(f));
        }
        Ability::AsEntersChoose { kind, joined } => {
            let who = if *joined { "it" } else { "this" };
            let _ = writeln!(
                out,
                "{indent}as {who} enters choose {}",
                choice_kind(kind, false)
            );
        }
        Ability::CostChange {
            filter: f,
            whose,
            generic,
            cond,
        } => {
            let head = match cond {
                Some(StaticCond::NotYourTurn) => "static during turns other than yours, ",
                Some(StaticCond::DuringYourTurn) => "static during your turn, ",
                Some(c @ StaticCond::SpellNumber(_)) => {
                    &format!("static as long as {}, ", static_cond(c))
                }
                _ => "",
            };
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
                "{indent}{head}spells{subject}{whose} cost {{{}}} {more}",
                generic.unsigned_abs()
            );
        }
        Ability::CondCost { delta, cond } => {
            let _ = writeln!(
                out,
                "{indent}this spell costs {{{}}} {} if {}",
                delta.unsigned_abs(),
                if *delta < 0 { "less" } else { "more" },
                static_cond(cond)
            );
        }
        Ability::TargetCost {
            delta,
            filter: f,
            noun,
        } => {
            let _ = writeln!(
                out,
                "{indent}this spell costs {{{}}} {} if it targets {}{}",
                delta.unsigned_abs(),
                if *delta < 0 { "less" } else { "more" },
                match noun {
                    crate::ast::TargetNoun::Permanent => "",
                    crate::ast::TargetNoun::Spell => "spell ",
                    crate::ast::TargetNoun::Card => "card ",
                },
                filter(f)
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
                crate::ast::Mechanic::Firebending(n) => format!("firebending {}", num(*n)),
                crate::ast::Mechanic::Poisonous(n) => format!("poisonous {n}"),
                crate::ast::Mechanic::Frenzy(n) => format!("frenzy {n}"),
                crate::ast::Mechanic::Absorb(n) => format!("absorb {n}"),
                crate::ast::Mechanic::Provoke => "provoke".to_string(),
                crate::ast::Mechanic::Toxic(n) => format!("toxic {n}"),
                crate::ast::Mechanic::Ward(c) => format!("ward {c}"),
                crate::ast::Mechanic::WardCost(c) => format!("ward {}", cost_text(c)),
                crate::ast::Mechanic::EchoCost(c) => format!("echo {}", cost_text(c)),
                crate::ast::Mechanic::MorphCost(c) => format!("morph {}", cost_text(c)),
                crate::ast::Mechanic::BuybackCost(c) => format!("buyback {}", cost_text(c)),
                crate::ast::Mechanic::Echo(c) => format!("echo {c}"),
                crate::ast::Mechanic::Unearth(c) => format!("unearth {c}"),
                crate::ast::Mechanic::Buyback(c) => format!("buyback {c}"),
                crate::ast::Mechanic::Outlast(c) => format!("outlast {c}"),
                crate::ast::Mechanic::Crew(n) => format!("crew {n}"),
                crate::ast::Mechanic::Morph(c) => format!("morph {c}"),
                crate::ast::Mechanic::Evoke(c) => format!("evoke {c}"),
                crate::ast::Mechanic::Madness(c) => format!("madness {c}"),
                crate::ast::Mechanic::Foretell(c) => format!("foretell {c}"),
                crate::ast::Mechanic::Suspend(n, c) => format!("suspend {n} {c}"),
                crate::ast::Mechanic::Entwine(c) => format!("entwine {c}"),
                crate::ast::Mechanic::Modular(n) => format!("modular {n}"),
                crate::ast::Mechanic::CumulativeUpkeep(c) => format!("cumulative upkeep {c}"),
                crate::ast::Mechanic::Escape(c, n) => format!("escape {c} exile {n}"),
                crate::ast::Mechanic::EscapesWith(n) => {
                    format!("escapes with {n} +1/+1 counters")
                }
                crate::ast::Mechanic::CounterChoice([a, b], escapes) => format!(
                    "{} with choice {} or {}",
                    if *escapes { "escapes" } else { "enters" },
                    counter(*a, Num::N(1)),
                    counter(*b, Num::N(1))
                ),
                crate::ast::Mechanic::Bloodthirst(n) => format!("bloodthirst {n}"),
                crate::ast::Mechanic::KickedWith(n, ks) if ks.is_empty() => {
                    format!("kicked with {n} +1/+1 counters")
                }
                crate::ast::Mechanic::KickedWith(n, ks) => {
                    format!("kicked with {n} +1/+1 counters and {}", keywords(ks))
                }
                crate::ast::Mechanic::Megamorph(c) => format!("megamorph {c}"),
                crate::ast::Mechanic::Disguise(c) => format!("disguise {c}"),
                crate::ast::Mechanic::Ninjutsu(c) => format!("ninjutsu {c}"),
                crate::ast::Mechanic::Scavenge(c) => format!("scavenge {c}"),
                crate::ast::Mechanic::Dash(c) => format!("dash {c}"),
                crate::ast::Mechanic::Warp(c) => format!("warp {c}"),
                crate::ast::Mechanic::Bestow(c) => format!("bestow {c}"),
                crate::ast::Mechanic::Overload(c) => format!("overload {c}"),
                crate::ast::Mechanic::Splice(c) => format!("splice {c}"),
                crate::ast::Mechanic::SpliceCost(c) => format!("splice {}", cost_text(c)),
                crate::ast::Mechanic::Embalm(c) => format!("embalm {c}"),
                crate::ast::Mechanic::Encore(c) => format!("encore {c}"),
                crate::ast::Mechanic::Eternalize(c) => format!("eternalize {c}"),
                crate::ast::Mechanic::Backup(n) => format!("backup {n}"),
                crate::ast::Mechanic::Offspring(c) => format!("offspring {c}"),
                crate::ast::Mechanic::Strive(c) => format!("strive {c}"),
                crate::ast::Mechanic::Bargain => "bargain".to_string(),
                crate::ast::Mechanic::Conspire => "conspire".to_string(),
                crate::ast::Mechanic::Casualty(n) => format!("casualty {n}"),
                crate::ast::Mechanic::Dredge(n) => format!("dredge {n}"),
                crate::ast::Mechanic::Storm => "storm".to_string(),
                crate::ast::Mechanic::Awaken(n, c) => format!("awaken {n} {c}"),
                crate::ast::Mechanic::Assist => "assist".to_string(),
                crate::ast::Mechanic::Enlist => "enlist".to_string(),
                crate::ast::Mechanic::Replicate(c) => format!("replicate {c}"),
                crate::ast::Mechanic::Teamwork(n) => format!("teamwork {n}"),
                crate::ast::Mechanic::Ripple(n) => format!("ripple {n}"),
                crate::ast::Mechanic::Station => "station".to_string(),
                crate::ast::Mechanic::Saddle(n) => format!("saddle {n}"),
                crate::ast::Mechanic::Ascend => "ascend".to_string(),
                crate::ast::Mechanic::StartYourEngines => "start your engines".to_string(),
                crate::ast::Mechanic::BecomesDay => "becomes day as this enters".to_string(),
                crate::ast::Mechanic::Fabricate(n) => format!("fabricate {n}"),
                crate::ast::Mechanic::Evolve => "evolve".to_string(),
                crate::ast::Mechanic::Renown(n) => format!("renown {n}"),
                crate::ast::Mechanic::Unleash => "unleash".to_string(),
                crate::ast::Mechanic::Afterlife(n) => format!("afterlife {n}"),
                crate::ast::Mechanic::Extort => "extort".to_string(),
                crate::ast::Mechanic::Mentor => "mentor".to_string(),
                crate::ast::Mechanic::Riot => "riot".to_string(),
                crate::ast::Mechanic::LivingWeapon => "living weapon".to_string(),
                crate::ast::Mechanic::Retrace => "retrace".to_string(),
                crate::ast::Mechanic::Ingest => "ingest".to_string(),
                crate::ast::Mechanic::Reinforce(n, c) => format!("reinforce {n} {c}"),
                crate::ast::Mechanic::JumpStart => "jump start".to_string(),
                crate::ast::Mechanic::Surge(c) => format!("surge {c}"),
                crate::ast::Mechanic::Spectacle(c) => format!("spectacle {c}"),
            };
            let _ = writeln!(out, "{indent}{text}");
        }
        Ability::EntersTappedUnless(c) => {
            let _ = writeln!(out, "{indent}enters tapped unless {}", static_cond(c));
        }
        Ability::EntersWithIf {
            n,
            kind,
            cond,
            first,
        } => {
            let what = format!(
                "{n} {} {}",
                kind.name(),
                if *n == 1 { "counter" } else { "counters" }
            );
            if *first {
                let _ = writeln!(out, "{indent}if {}, enters with {what}", static_cond(cond));
            } else {
                let _ = writeln!(out, "{indent}enters with {what} if {}", static_cond(cond));
            }
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
            let word = if *negated { "if" } else { "unless" };
            let _ = writeln!(out, "{indent}can't {what} {word} {}", static_cond(cond));
        }
        Ability::DeckAnyNumber => {
            let _ = writeln!(out, "{indent}deck any number");
        }
        Ability::Uncounterable => {
            let _ = writeln!(out, "{indent}this spell can't be countered");
        }
        Ability::AuraStays => {
            let _ = writeln!(out, "{indent}protection doesn't remove this");
        }
        Ability::StationLevel { n, keywords } => {
            let ks: Vec<&str> = keywords.iter().map(|k| k.name()).collect();
            let _ = writeln!(out, "{indent}station {n}: {}", ks.join(", "));
        }
        Ability::CrewsAs {
            saddles,
            toughness,
            plus,
        } => {
            let _ = writeln!(
                out,
                "{indent}{}crews {}",
                if *saddles { "saddles and " } else { "" },
                if *toughness {
                    "using toughness".to_string()
                } else {
                    format!("plus {plus}")
                }
            );
        }
        Ability::AsTurnedFaceUp { enters, kind, n } => {
            let _ = writeln!(
                out,
                "{indent}as {}turned face up put {} {}",
                if *enters { "enters or " } else { "" },
                num(*n),
                counter(*kind, *n)
            );
        }
        Ability::MayLook {
            top,
            face_down,
            opponents,
        } => {
            let _ = writeln!(
                out,
                "{indent}may look{}{}{}",
                if *top { " top" } else { "" },
                if *face_down { " face-down" } else { "" },
                if *opponents { " opponents" } else { "" }
            );
        }
        Ability::ExtraVote { optional } => {
            let _ = writeln!(
                out,
                "{indent}extra vote{}",
                if *optional { " may" } else { "" }
            );
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

/// `color of choice except blue`, `creature type of choice`, `land type
/// of choice`, `Plains or Island` (`of choice` only with `of_choice`).
fn choice_kind(k: &ChoiceKind, of_choice: bool) -> String {
    let of = if of_choice { " of choice" } else { "" };
    match k {
        ChoiceKind::Colors => format!("colors{of}"),
        ChoiceKind::Color { except } => match except {
            Some(c) => format!("color{of} except {}", format!("{c:?}").to_lowercase()),
            None => format!("color{of}"),
        },
        ChoiceKind::CreatureType { except } => match except {
            Some(t) => format!("creature type{of} except {t}"),
            None => format!("creature type{of}"),
        },
        ChoiceKind::LandType(lands) if lands.len() == 5 || !of_choice => format!("land type{of}"),
        ChoiceKind::LandType(lands) => lands
            .iter()
            .map(|c| crate::render::basic_land_name(*c))
            .collect::<Vec<_>>()
            .join(" or "),
    }
}

/// A delayed trigger about its recipient that ends with the turn (CR
/// 603.7c), as written and printed: "when that creature dies this turn".
/// A delayed "this turn" trigger on the object it is about ("When that
/// creature dies this turn"), rather than one that triggers on any event
/// of its kind until the turn ends ("Whenever a creature blocks this
/// turn").
pub(crate) fn about_that_creature(t: &Trigger) -> bool {
    matches!(
        t,
        Trigger::Dies(TriggerObj::This) | Trigger::Leaves(TriggerObj::This)
    )
}

pub(crate) fn this_turn_trigger(t: &Trigger) -> &'static str {
    match t {
        Trigger::Leaves(_) => "when that creature leaves the battlefield this turn",
        _ => "when that creature dies this turn",
    }
}

/// `spent {B}`, `spent {R}{R}`, `spent 3 red`, `spent treasure`.
fn spent_cond(kind: mtg_core::card::SpentKind, at_least: u8) -> String {
    use mtg_core::card::SpentKind;
    match kind {
        SpentKind::Color(c) if at_least <= 2 => {
            format!(
                "spent {}",
                format!("{{{}}}", c.letter()).repeat(at_least as usize)
            )
        }
        SpentKind::Color(c) => format!("spent {at_least} {}", crate::render::color_name(c)),
        SpentKind::Treasure => "spent treasure".to_string(),
        SpentKind::Total => format!("spent {at_least} mana"),
    }
}

/// A keyword ability with a parameter granted bare: `ward {2}`, `toxic 1`.
pub(crate) fn granted_mechanic(a: &Ability) -> Option<String> {
    match a {
        Ability::Mechanic(crate::ast::Mechanic::Ward(c)) => Some(format!("ward {c}")),
        Ability::Mechanic(crate::ast::Mechanic::Toxic(n)) => Some(format!("toxic {n}")),
        Ability::Mechanic(crate::ast::Mechanic::Poisonous(n)) => Some(format!("poisonous {n}")),
        Ability::Mechanic(crate::ast::Mechanic::Frenzy(n)) => Some(format!("frenzy {n}")),
        Ability::Mechanic(crate::ast::Mechanic::Absorb(n)) => Some(format!("absorb {n}")),
        Ability::Mechanic(crate::ast::Mechanic::Provoke) => Some("provoke".to_string()),
        Ability::Mechanic(crate::ast::Mechanic::Mentor) => Some("mentor".to_string()),
        Ability::Mechanic(crate::ast::Mechanic::Outlast(c)) => Some(format!("outlast {c}")),
        Ability::Protection(f) if *f == Filter::default() => {
            Some("protection from everything".to_string())
        }
        Ability::Protection(f) => Some(format!("protection from {}", filter(f))),
        _ => None,
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

/// What a player can't do: "play lands", "cast creature spells".
pub fn player_rule_words(kind: mtg_core::card::PlayerRuleKind) -> &'static str {
    match kind {
        mtg_core::card::PlayerRuleKind::CantPlayLands => "play lands",
        mtg_core::card::PlayerRuleKind::CantCastCreatureSpells => "cast creature spells",
    }
}
