//! Converter for `XMage`'s card tests (design.md §7.2): the JUnit methods under
//! `Mage.Tests/src/test/java/org/mage/test/cards` use a small scripting API
//! (`addCard`, `castSpell`, `setStopAt`, `assertLife`, ...) that maps almost
//! one-to-one onto the scenario DSL. A test converts when every statement
//! in it is one the converter knows and every card it names is in the
//! corpus; anything else is skipped with the reason, and the missing-card
//! and unsupported-call counts say where the next authoring and converter
//! work pays off. XMage is MIT-licensed (© XMage contributors); each
//! converted file records its source.

#![allow(clippy::doc_markdown)] // "XMage" is a name, not code

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use mtg_dsl::ast::{Ability, Card};
use mtg_dsl::render;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct Converted {
    /// Path relative to the XMage test root.
    pub source: String,
    pub class: String,
    pub method: String,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Report {
    pub files: usize,
    pub tests: usize,
    pub converted: Vec<Converted>,
    /// (class, method, reason)
    pub skipped: Vec<(String, String, String)>,
    /// Cards named by skipped tests that are not in the corpus.
    pub missing_cards: BTreeMap<String, usize>,
}

impl Report {
    pub fn reasons(&self) -> Vec<(String, usize)> {
        let mut m: BTreeMap<String, usize> = BTreeMap::new();
        for (_, _, r) in &self.skipped {
            let key = r.split(':').next().unwrap_or(r).to_string();
            *m.entry(key).or_insert(0) += 1;
        }
        let mut v: Vec<(String, usize)> = m.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v
    }
}

/// The corpus as the converter needs it: names, and each card's activated
/// ability texts (to find which permanent an `activateAbility` prefix
/// names).
pub struct Corpus {
    names: HashSet<String>,
    activated: HashMap<String, Vec<String>>,
}

impl Corpus {
    pub fn new(cards: &[Card]) -> Self {
        let by_name: HashMap<&str, &Card> = cards.iter().map(|c| (c.name.as_str(), c)).collect();
        let mut activated = HashMap::new();
        for c in cards {
            let texts: Vec<String> = c
                .abilities
                .iter()
                .filter(|a| matches!(a, Ability::Activated { .. }))
                .map(|a| render::ability_text(c, a, &by_name))
                .collect();
            if !texts.is_empty() {
                activated.insert(c.name.clone(), texts);
            }
        }
        Corpus {
            names: cards.iter().map(|c| c.name.clone()).collect(),
            activated,
        }
    }
}

/// Convert every test under `root` (the `.../org/mage/test` directory).
pub fn convert_dir(root: &Path, corpus: &Corpus) -> Result<Report> {
    let mut files = Vec::new();
    collect(&root.join("cards"), &mut files);
    files.sort();
    let mut report = Report::default();
    for f in &files {
        let text =
            std::fs::read_to_string(f).with_context(|| format!("reading {}", f.display()))?;
        let rel = f
            .strip_prefix(root)
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/");
        report.files += 1;
        convert_file(&rel, &text, corpus, &mut report);
    }
    Ok(report)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "java") {
            out.push(p);
        }
    }
}

/// Strip `//` and `/* */` comments, keeping string literals intact.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let b: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < b.len() {
                out.push(b[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            i += 1;
        } else if c == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && b.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Split a method body into statements on `;` outside strings.
fn statements(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if in_str {
            cur.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                cur.push(c);
            }
            ';' => {
                let s = cur.split_whitespace().collect::<Vec<_>>().join(" ");
                if !s.is_empty() {
                    out.push(s);
                }
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    let s = cur.split_whitespace().collect::<Vec<_>>().join(" ");
    if !s.is_empty() {
        out.push(s);
    }
    out
}

/// Top-level arguments of a call's parenthesised list.
fn args(call: &str) -> Vec<String> {
    let Some(open) = call.find('(') else {
        return Vec::new();
    };
    let Some(close) = call.rfind(')') else {
        return Vec::new();
    };
    let inner = &call[open + 1..close];
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if in_str {
            cur.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                cur.push(c);
            }
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn step_name(java: &str) -> Option<&'static str> {
    Some(match java {
        "UNTAP" => "untap",
        "UPKEEP" => "upkeep",
        "DRAW" => "draw",
        "PRECOMBAT_MAIN" => "precombat_main",
        "BEGIN_COMBAT" => "begin_combat",
        "DECLARE_ATTACKERS" => "declare_attackers",
        "DECLARE_BLOCKERS" => "declare_blockers",
        "FIRST_COMBAT_DAMAGE" => "first_strike_damage",
        "COMBAT_DAMAGE" => "combat_damage",
        "END_COMBAT" => "end_combat",
        "POSTCOMBAT_MAIN" => "postcombat_main",
        "END_TURN" => "end",
        "CLEANUP" => "cleanup",
        _ => return None,
    })
}

fn keyword_name(class: &str) -> Option<&'static str> {
    Some(match class {
        "FlyingAbility" => "flying",
        "HasteAbility" => "haste",
        "TrampleAbility" => "trample",
        "IndestructibleAbility" => "indestructible",
        "FirstStrikeAbility" => "first strike",
        "DoubleStrikeAbility" => "double strike",
        "LifelinkAbility" => "lifelink",
        "HexproofAbility" => "hexproof",
        "DeathtouchAbility" => "deathtouch",
        "VigilanceAbility" => "vigilance",
        "ReachAbility" => "reach",
        "DefenderAbility" => "defender",
        "MenaceAbility" => "menace",
        "FlashAbility" => "flash",
        "ProwessAbility" => "prowess",
        _ => return None,
    })
}

/// One test method being converted.
struct Conv<'a> {
    corpus: &'a Corpus,
    consts: &'a HashMap<String, String>,
    setup: Vec<String>,
    actions: Vec<String>,
    asserts: Vec<String>,
    run_to: Option<String>,
    /// Where each named card was set up, to qualify ambiguous names.
    owners: HashMap<String, HashSet<char>>,
    /// The player each battlefield/hand card was given to, with its name.
    placed: Vec<(char, String)>,
    library_cleared: HashSet<char>,
    library_added: HashMap<char, Vec<String>>,
    names_used: Vec<String>,
}

impl Conv<'_> {
    fn fail(reason: impl Into<String>) -> Result<(), String> {
        Err(reason.into())
    }

    /// A string-valued expression: a literal or a class constant.
    fn string(&mut self, expr: &str) -> Result<String, String> {
        let e = expr.trim();
        if e.starts_with('"') && e.ends_with('"') && e.len() >= 2 {
            let s = e[1..e.len() - 1].replace("\\\"", "\"");
            self.names_used.push(s.clone());
            return Ok(s);
        }
        if let Some(v) = self.consts.get(e) {
            self.names_used.push(v.clone());
            return Ok(v.clone());
        }
        Err(format!("expression: {e}"))
    }

    fn player(expr: &str) -> Result<char, String> {
        match expr.trim() {
            "playerA" => Ok('A'),
            "playerB" => Ok('B'),
            "playerC" | "playerD" => Err("multiplayer".to_string()),
            other => Err(format!("player expression: {other}")),
        }
    }

    fn int(expr: &str) -> Result<i64, String> {
        let e = expr.trim();
        // Simple arithmetic over integer literals: "20 - 3 - 3".
        let mut total = 0i64;
        let mut sign = 1i64;
        let mut cur = String::new();
        let flush = |cur: &mut String, total: &mut i64, sign: i64| -> Result<(), String> {
            if cur.is_empty() {
                return Ok(());
            }
            let n: i64 = cur.parse().map_err(|_| format!("number: {e}"))?;
            *total += sign * n;
            cur.clear();
            Ok(())
        };
        for c in e.chars() {
            match c {
                '0'..='9' => cur.push(c),
                '+' => {
                    flush(&mut cur, &mut total, sign)?;
                    sign = 1;
                }
                '-' => {
                    flush(&mut cur, &mut total, sign)?;
                    sign = -1;
                }
                ' ' => {}
                _ => return Err(format!("number: {e}")),
            }
        }
        flush(&mut cur, &mut total, sign)?;
        Ok(total)
    }

    fn step(expr: &str) -> Result<&'static str, String> {
        let s = expr
            .trim()
            .strip_prefix("PhaseStep.")
            .unwrap_or(expr.trim());
        step_name(s).ok_or_else(|| format!("step: {s}"))
    }

    fn require_card(&self, name: &str) -> Result<(), String> {
        if self.corpus.names.contains(name) {
            Ok(())
        } else {
            Err(format!("card not in corpus: {name}"))
        }
    }

    /// A card name as a scenario object reference, qualified by controller
    /// when both players were given one.
    fn obj_ref(&self, name: &str, player: Option<char>) -> String {
        match (player, self.owners.get(name)) {
            (Some(p), Some(set)) if set.len() > 1 => format!("{p} \"{name}\""),
            _ => format!("\"{name}\""),
        }
    }

    /// A target argument: a player or a `^`-separated list of card names.
    fn targets(&mut self, expr: &str, actor: char) -> Result<Vec<String>, String> {
        if let Ok(p) = Self::player(expr) {
            return Ok(vec![format!("player {p}")]);
        }
        let s = self.string(expr)?;
        let mut out = Vec::new();
        for part in s.split('^') {
            if part == "playerA" || part == "playerB" {
                out.push(format!("player {}", &part[6..]));
                continue;
            }
            self.require_card(part)?;
            // The actor's own card when both have one, else the opponent's.
            let qualifier = self.owners.get(part).and_then(|set| {
                if set.len() > 1 {
                    Some(if set.contains(&actor) { actor } else { 'A' })
                } else {
                    None
                }
            });
            out.push(self.obj_ref(part, qualifier));
        }
        Ok(out)
    }

    #[allow(clippy::too_many_lines)] // one arm per XMage call
    fn statement(&mut self, st: &str) -> Result<(), String> {
        let call = st.split('(').next().unwrap_or("").trim();
        let a = args(st);
        match call {
            "setStrictChooseMode"
            | "skipInitShuffling"
            | "assertAllCommandsUsed"
            | "setStopOnAllTriggers"
            | "removeAllCardsFromHand" => Ok(()),
            c if c.starts_with("show") => Ok(()),
            "execute" => {
                if self.run_to.is_none() {
                    return Self::fail("execute before setStopAt");
                }
                Ok(())
            }
            "setStopAt" => {
                if self.run_to.is_some() {
                    return Self::fail("second setStopAt");
                }
                let turn = Self::int(&a[0])?;
                let step = Self::step(&a[1])?;
                self.run_to = Some(format!("run_to turn {turn} {step}"));
                Ok(())
            }
            "addCard" => {
                let zone = a[0].trim().strip_prefix("Zone.").unwrap_or("");
                let p = Self::player(&a[1])?;
                let name = self.string(&a[2])?;
                self.require_card(&name)?;
                let count = if a.len() > 3 { Self::int(&a[3])? } else { 1 };
                let tapped = a.len() > 4 && a[4].trim() == "true";
                let zone_word = match zone {
                    "BATTLEFIELD" => "battlefield",
                    "HAND" => "hand",
                    "GRAVEYARD" => "graveyard",
                    "EXILE" => "exile",
                    "LIBRARY" => {
                        for _ in 0..count {
                            self.library_added.entry(p).or_default().push(name.clone());
                        }
                        return Ok(());
                    }
                    other => return Self::fail(format!("zone: {other}")),
                };
                self.owners.entry(name.clone()).or_default().insert(p);
                if zone == "BATTLEFIELD" || zone == "HAND" {
                    self.placed.push((p, name.clone()));
                }
                let mut spec = if count == 1 {
                    format!("\"{name}\"")
                } else {
                    format!("{count} \"{name}\"")
                };
                if tapped {
                    spec.push_str(" tapped");
                }
                self.setup.push(format!("{zone_word} {p}: {spec}"));
                Ok(())
            }
            "removeAllCardsFromLibrary" => {
                let p = Self::player(&a[0])?;
                self.library_cleared.insert(p);
                Ok(())
            }
            "setLife" => {
                let p = Self::player(&a[0])?;
                let n = Self::int(&a[1])?;
                self.setup.push(format!("life {p} {n}"));
                Ok(())
            }
            "castSpell" => {
                if a.len() > 5
                    || (a.len() == 5
                        && Self::player(&a[4]).is_err()
                        && !a[4].trim().starts_with('"')
                        && !self.consts.contains_key(a[4].trim()))
                {
                    return Self::fail("castSpell form");
                }
                let turn = Self::int(&a[0])?;
                let step = Self::step(&a[1])?;
                let p = Self::player(&a[2])?;
                let name = self.string(&a[3])?;
                self.require_card(&name)?;
                // XMage picks the only legal target itself; `auto` does the
                // same here (and is harmless for a spell without targets).
                let mut line = format!("turn {turn} {step} {p}: cast \"{name}\" targeting auto");
                if a.len() == 5 {
                    let ts = self.targets(&a[4], p)?;
                    line = format!(
                        "turn {turn} {step} {p}: cast \"{name}\" targeting {}",
                        ts.join(", ")
                    );
                }
                self.actions.push(line);
                Ok(())
            }
            "playLand" => {
                let turn = Self::int(&a[0])?;
                let step = Self::step(&a[1])?;
                let p = Self::player(&a[2])?;
                let name = self.string(&a[3])?;
                self.require_card(&name)?;
                self.actions
                    .push(format!("turn {turn} {step} {p}: play \"{name}\""));
                Ok(())
            }
            "activateAbility" => {
                if a.len() > 5 {
                    return Self::fail("activateAbility form");
                }
                let turn = Self::int(&a[0])?;
                let step = Self::step(&a[1])?;
                let p = Self::player(&a[2])?;
                let prefix = self.string(&a[3])?;
                let prefix_l = prefix.to_ascii_lowercase();
                // Which of the player's permanents has an activated ability
                // whose text starts with the prefix?
                let mut found: Vec<(String, usize)> = Vec::new();
                for (owner, name) in &self.placed {
                    if *owner != p {
                        continue;
                    }
                    if let Some(texts) = self.corpus.activated.get(name) {
                        for (i, t) in texts.iter().enumerate() {
                            if t.to_ascii_lowercase().starts_with(&prefix_l)
                                && !found.iter().any(|(n, _)| n == name)
                            {
                                found.push((name.clone(), i + 1));
                            }
                        }
                    }
                }
                let [(source, index)] = found.as_slice() else {
                    return Self::fail(format!(
                        "activateAbility source: {} match(es) for \"{prefix}\"",
                        found.len()
                    ));
                };
                let (source, index) = (source.clone(), *index);
                let mut line = format!(
                    "turn {turn} {step} {p}: activate {}",
                    self.obj_ref(&source, Some(p))
                );
                if index > 1 {
                    let _ = write!(line, " ability {index}");
                }
                if a.len() == 5 {
                    let ts = self.targets(&a[4], p)?;
                    let _ = write!(line, " targeting {}", ts.join(", "));
                } else {
                    line.push_str(" targeting auto");
                }
                self.actions.push(line);
                Ok(())
            }
            "attack" => {
                if a.len() != 3 && !(a.len() == 4 && Self::player(&a[3]).is_ok()) {
                    return Self::fail("attack form");
                }
                let turn = Self::int(&a[0])?;
                let p = Self::player(&a[1])?;
                let name = self.string(&a[2])?;
                self.require_card(&name)?;
                let mut line = format!("turn {turn} {p}: attack {}", self.obj_ref(&name, Some(p)));
                if a.len() == 4 {
                    let t = Self::player(&a[3])?;
                    let _ = write!(line, " at player {t}");
                }
                self.actions.push(line);
                Ok(())
            }
            "block" => {
                if a.len() != 4 {
                    return Self::fail("block form");
                }
                let turn = Self::int(&a[0])?;
                let p = Self::player(&a[1])?;
                let blocker = self.string(&a[2])?;
                let attacker = self.string(&a[3])?;
                self.require_card(&blocker)?;
                self.require_card(&attacker)?;
                let other = if p == 'A' { 'B' } else { 'A' };
                let line = format!(
                    "turn {turn} {p}: block {} with {}",
                    self.obj_ref(&attacker, Some(other)),
                    self.obj_ref(&blocker, Some(p))
                );
                self.actions.push(line);
                Ok(())
            }
            "waitStackResolved" => {
                let turn = Self::int(&a[0])?;
                let step = Self::step(&a[1])?;
                self.actions.push(format!("turn {turn} {step} A: wait"));
                self.actions.push(format!("turn {turn} {step} B: wait"));
                Ok(())
            }
            "assertLife" => {
                let p = Self::player(&a[0])?;
                let n = Self::int(&a[1])?;
                self.asserts.push(format!("assert life {p} {n}"));
                Ok(())
            }
            "assertPermanentCount"
            | "assertGraveyardCount"
            | "assertExileCount"
            | "assertHandCount"
            | "assertLibraryCount" => {
                let zone = match call {
                    "assertPermanentCount" => "battlefield",
                    "assertGraveyardCount" => "graveyard",
                    "assertExileCount" => "exile",
                    "assertHandCount" => "hand",
                    _ => "library",
                };
                if a.len() == 2 {
                    let p = Self::player(&a[0])?;
                    let n = Self::int(&a[1])?;
                    if zone == "library" {
                        return Self::fail("assertLibraryCount (libraries differ)");
                    }
                    self.asserts.push(format!("assert {zone} {p} count {n}"));
                    return Ok(());
                }
                if a.len() != 3 {
                    return Self::fail(format!("{call} form"));
                }
                let p = Self::player(&a[0])?;
                let name = self.string(&a[1])?;
                self.require_card(&name)?;
                let n = Self::int(&a[2])?;
                if n == 0 {
                    self.asserts
                        .push(format!("assert {zone} {p} lacks \"{name}\""));
                } else {
                    self.asserts
                        .push(format!("assert {zone} {p} contains {n} \"{name}\""));
                }
                Ok(())
            }
            "assertPowerToughness" => {
                if a.len() != 4 {
                    return Self::fail("assertPowerToughness form");
                }
                let p = Self::player(&a[0])?;
                let name = self.string(&a[1])?;
                self.require_card(&name)?;
                let pw = Self::int(&a[2])?;
                let tg = Self::int(&a[3])?;
                self.asserts
                    .push(format!("assert power {p} \"{name}\" {pw}"));
                self.asserts
                    .push(format!("assert toughness {p} \"{name}\" {tg}"));
                Ok(())
            }
            "assertTapped" => {
                let name = self.string(&a[0])?;
                self.require_card(&name)?;
                let tapped = a[1].trim() == "true";
                let word = if tapped { "tapped" } else { "untapped" };
                self.asserts
                    .push(format!("assert {word} {}", self.obj_ref(&name, None)));
                Ok(())
            }
            "assertCounterCount" => {
                let (name_expr, kind, n_expr) = match a.len() {
                    3 => (&a[0], &a[1], &a[2]),
                    4 => (&a[1], &a[2], &a[3]),
                    _ => return Self::fail("assertCounterCount form"),
                };
                let name = self.string(name_expr)?;
                self.require_card(&name)?;
                let kind = match kind.trim() {
                    "CounterType.P1P1" => "+1/+1",
                    "CounterType.M1M1" => "-1/-1",
                    "CounterType.LOYALTY" => "loyalty",
                    other => return Self::fail(format!("counter kind: {other}")),
                };
                let n = Self::int(n_expr)?;
                self.asserts.push(format!(
                    "assert counters {} {kind} {n}",
                    self.obj_ref(&name, None)
                ));
                Ok(())
            }
            "assertAbility" => {
                if a.len() != 4 {
                    return Self::fail("assertAbility form");
                }
                let p = Self::player(&a[0])?;
                let name = self.string(&a[1])?;
                self.require_card(&name)?;
                let class = a[2].trim().split('.').next().unwrap_or("");
                let Some(kw) = keyword_name(class) else {
                    return Self::fail(format!("assertAbility: {class}"));
                };
                let has = a[3].trim() == "true";
                let not = if has { "" } else { " not" };
                self.asserts.push(format!(
                    "assert has{not} {} {kw}",
                    self.obj_ref(&name, Some(p))
                ));
                Ok(())
            }
            "assertDamageReceived" => {
                let p = Self::player(&a[0])?;
                let name = self.string(&a[1])?;
                self.require_card(&name)?;
                let n = Self::int(&a[2])?;
                self.asserts.push(format!(
                    "assert damage {} {n}",
                    self.obj_ref(&name, Some(p))
                ));
                Ok(())
            }
            other => Self::fail(format!("unsupported: {other}")),
        }
    }

    fn finish(mut self, class: &str, method: &str, source: &str) -> Result<String, String> {
        let Some(run_to) = self.run_to.clone() else {
            return Err("no setStopAt".to_string());
        };
        // Libraries: XMage tests draw from a real deck; here the named cards
        // go on top of Plains unless the test emptied the library.
        for p in ['A', 'B'] {
            let added = self.library_added.get(&p).cloned().unwrap_or_default();
            let cleared = self.library_cleared.contains(&p);
            if added.is_empty() && !cleared {
                continue;
            }
            // XMage puts each added card on top: the last added is drawn first.
            let mut cards: Vec<String> = added.iter().rev().map(|n| format!("\"{n}\"")).collect();
            if !cleared {
                cards.push("30 \"Plains\"".to_string());
            }
            self.setup
                .push(format!("library {p}: {}", cards.join(", ")));
        }
        let mut out = String::new();
        let _ = writeln!(out, "scenario \"{class}.{method}\" {{");
        let _ = writeln!(out, "  # from {source}");
        for s in &self.setup {
            let _ = writeln!(out, "  {s}");
        }
        for s in &self.actions {
            let _ = writeln!(out, "  {s}");
        }
        let _ = writeln!(out, "  {run_to}");
        for s in &self.asserts {
            let _ = writeln!(out, "  {s}");
        }
        out.push_str("}\n");
        Ok(out)
    }
}

fn convert_file(rel: &str, text: &str, corpus: &Corpus, report: &mut Report) {
    let src = strip_comments(text);
    let class = Regex::new(r"public class (\w+)")
        .unwrap()
        .captures(&src)
        .map_or_else(|| "Unknown".to_string(), |c| c[1].to_string());
    if !src.contains("extends CardTestPlayerBase") {
        return;
    }
    // Class-level string constants.
    let mut consts = HashMap::new();
    for c in Regex::new(r#"(?m)^\s*(?:private|protected|public)?\s*(?:static\s+)?(?:final\s+)?String\s+(\w+)\s*=\s*"((?:[^"\\]|\\.)*)"\s*;"#)
        .unwrap()
        .captures_iter(&src)
    {
        consts.insert(c[1].to_string(), c[2].replace("\\\"", "\""));
    }
    let method_re = Regex::new(
        r"@Test(?:\s*\([^)]*\))?\s*public\s+void\s+(\w+)\s*\(\s*\)\s*(?:throws[^{]*)?\{",
    )
    .unwrap();
    for m in method_re.captures_iter(&src) {
        let method = m[1].to_string();
        let start = m.get(0).unwrap().end();
        let Some(body) = brace_body(&src[start..]) else {
            continue;
        };
        report.tests += 1;
        let mut conv = Conv {
            corpus,
            consts: &consts,
            setup: Vec::new(),
            actions: Vec::new(),
            asserts: Vec::new(),
            run_to: None,
            owners: HashMap::new(),
            placed: Vec::new(),
            library_cleared: HashSet::new(),
            library_added: HashMap::new(),
            names_used: Vec::new(),
        };
        let mut result = Ok(());
        for st in statements(body) {
            if let Err(e) = conv.statement(&st) {
                result = Err(e);
                break;
            }
        }
        let outcome = result.and_then(|()| {
            let names = conv.names_used.clone();
            conv.finish(&class, &method, rel).map(|t| (t, names))
        });
        match outcome {
            Ok((text, _)) => report.converted.push(Converted {
                source: rel.to_string(),
                class: class.clone(),
                method,
                text,
            }),
            Err(reason) => {
                if let Some(name) = reason.strip_prefix("card not in corpus: ") {
                    *report.missing_cards.entry(name.to_string()).or_insert(0) += 1;
                }
                report.skipped.push((class.clone(), method, reason));
            }
        }
    }
}

/// The text up to the brace closing the one just opened.
fn brace_body(s: &str) -> Option<&str> {
    let mut depth = 1i32;
    let mut in_str = false;
    let mut prev = ' ';
    for (i, c) in s.char_indices() {
        if in_str {
            if c == '"' && prev != '\\' {
                in_str = false;
            }
        } else {
            match c {
                '"' => in_str = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&s[..i]);
                    }
                }
                _ => {}
            }
        }
        prev = c;
    }
    None
}

/// Where a converted test's scenario file goes under `out`: the source's
/// path with `.java` → `.scn`.
pub fn output_path(out: &Path, source: &str) -> PathBuf {
    let rel = source.strip_prefix("cards/").unwrap_or(source);
    out.join(rel.replace(".java", ".scn"))
}
