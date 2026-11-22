//! Colors, mana symbols, mana costs, and the mana pool (CR 105, 106, 107,
//! 202). Single-colored, generic, colorless, `X`, hybrid (`{R/G}`),
//! monocolored hybrid (`{2/W}`), Phyrexian (`{R/P}`), and snow (`{S}`)
//! symbols.

use std::fmt;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use thiserror::Error;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub enum Color {
    White,
    Blue,
    Black,
    Red,
    Green,
}

impl Color {
    pub const ALL: [Color; 5] = [
        Color::White,
        Color::Blue,
        Color::Black,
        Color::Red,
        Color::Green,
    ];

    /// The basic land type that produces this color (CR 305.6).
    pub const fn basic_land_type(self) -> &'static str {
        match self {
            Color::White => "Plains",
            Color::Blue => "Island",
            Color::Black => "Swamp",
            Color::Red => "Mountain",
            Color::Green => "Forest",
        }
    }

    pub const fn letter(self) -> char {
        match self {
            Color::White => 'W',
            Color::Blue => 'U',
            Color::Black => 'B',
            Color::Red => 'R',
            Color::Green => 'G',
        }
    }

    pub const fn from_letter(c: char) -> Option<Color> {
        match c {
            'W' => Some(Color::White),
            'U' => Some(Color::Blue),
            'B' => Some(Color::Black),
            'R' => Some(Color::Red),
            'G' => Some(Color::Green),
            _ => None,
        }
    }

    const fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// A set of colors (CR 105.2). Colorless is the empty set.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
pub struct ColorSet(u8);

impl ColorSet {
    pub const COLORLESS: ColorSet = ColorSet(0);

    pub fn of(colors: impl IntoIterator<Item = Color>) -> Self {
        colors.into_iter().fold(ColorSet(0), ColorSet::with)
    }

    #[must_use]
    pub const fn with(self, c: Color) -> Self {
        ColorSet(self.0 | c.bit())
    }

    pub const fn contains(self, c: Color) -> bool {
        self.0 & c.bit() != 0
    }

    pub const fn is_colorless(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn union(self, other: ColorSet) -> Self {
        ColorSet(self.0 | other.0)
    }

    pub const fn intersects(self, other: ColorSet) -> bool {
        self.0 & other.0 != 0
    }

    pub fn iter(self) -> impl Iterator<Item = Color> {
        Color::ALL.into_iter().filter(move |c| self.contains(*c))
    }

    /// How many colors.
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// All five colors.
    pub const ALL: ColorSet = ColorSet(0b1_1111);
}

impl fmt::Debug for ColorSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_colorless() {
            return write!(f, "C");
        }
        for c in self.iter() {
            write!(f, "{}", c.letter())?;
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum ManaSymbol {
    /// `{W}` etc.
    Colored(Color),
    /// `{C}`: must be paid with colorless mana (CR 107.4c).
    Colorless,
    /// `{N}`: generic, payable with any mana.
    Generic(u8),
    /// `{X}`.
    X,
    /// `{R/G}`: payable with either color (CR 107.4e).
    Hybrid(Color, Color),
    /// `{2/W}`: payable with the color or with two generic (CR 107.4e).
    TwoHybrid(Color),
    /// `{R/P}`: payable with the color or with 2 life (CR 107.4f).
    Phyrexian(Color),
    /// `{S}`: one mana of any type produced by a snow source (CR 107.4h).
    Snow,
}

#[derive(Clone, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
pub struct ManaCost {
    symbols: SmallVec<[ManaSymbol; 6]>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManaCostParseError {
    #[error("unterminated mana symbol in {0:?}")]
    Unterminated(String),
    #[error("unknown mana symbol {{{0}}}")]
    Unknown(String),
    #[error("text outside braces in {0:?}")]
    Stray(String),
}

impl ManaCost {
    /// No symbols at all (not even {0}).
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Parse Scryfall-style text such as `{1}{R}` or `{X}{G}{G}`.
    pub fn parse(text: &str) -> Result<ManaCost, ManaCostParseError> {
        let mut symbols = SmallVec::new();
        let mut rest = text.trim();
        while !rest.is_empty() {
            let Some(stripped) = rest.strip_prefix('{') else {
                return Err(ManaCostParseError::Stray(text.to_string()));
            };
            let Some(end) = stripped.find('}') else {
                return Err(ManaCostParseError::Unterminated(text.to_string()));
            };
            let sym = &stripped[..end];
            rest = &stripped[end + 1..];
            let parsed = match sym {
                "X" => ManaSymbol::X,
                "C" => ManaSymbol::Colorless,
                "S" => ManaSymbol::Snow,
                s if s.chars().all(|c| c.is_ascii_digit()) => {
                    let n: u32 = s
                        .parse()
                        .map_err(|_| ManaCostParseError::Unknown(s.to_string()))?;
                    ManaSymbol::Generic(
                        u8::try_from(n).map_err(|_| ManaCostParseError::Unknown(s.to_string()))?,
                    )
                }
                s if s.contains('/') => {
                    let unknown = || ManaCostParseError::Unknown(s.to_string());
                    let (left, right) = s.split_once('/').ok_or_else(unknown)?;
                    let color = |t: &str| {
                        let mut chars = t.chars();
                        match (chars.next(), chars.next()) {
                            (Some(letter), None) => Color::from_letter(letter),
                            _ => None,
                        }
                    };
                    match (left, right) {
                        ("2", one) => ManaSymbol::TwoHybrid(color(one).ok_or_else(unknown)?),
                        (one, "P") => ManaSymbol::Phyrexian(color(one).ok_or_else(unknown)?),
                        (x, y) => {
                            let (x, y) =
                                (color(x).ok_or_else(unknown)?, color(y).ok_or_else(unknown)?);
                            if x == y {
                                return Err(unknown());
                            }
                            ManaSymbol::Hybrid(x, y)
                        }
                    }
                }
                s if s.len() == 1 => {
                    let c = s.chars().next().unwrap_or(' ');
                    Color::from_letter(c)
                        .map(ManaSymbol::Colored)
                        .ok_or_else(|| ManaCostParseError::Unknown(s.to_string()))?
                }
                s => return Err(ManaCostParseError::Unknown(s.to_string())),
            };
            symbols.push(parsed);
        }
        Ok(ManaCost { symbols })
    }

    pub fn symbols(&self) -> &[ManaSymbol] {
        &self.symbols
    }

    /// Mana value (CR 202.3). `X` counts as 0 outside the stack.
    pub fn mana_value(&self) -> u32 {
        self.symbols
            .iter()
            .map(|s| match s {
                ManaSymbol::Generic(n) => u32::from(*n),
                ManaSymbol::Colored(_)
                | ManaSymbol::Colorless
                | ManaSymbol::Hybrid(..)
                | ManaSymbol::Phyrexian(_)
                | ManaSymbol::Snow => 1,
                ManaSymbol::TwoHybrid(_) => 2,
                ManaSymbol::X => 0,
            })
            .sum()
    }

    /// Colors of the symbols in this cost; the object's color unless a
    /// color indicator or CDA says otherwise (CR 202.2).
    pub fn colors(&self) -> ColorSet {
        self.symbols
            .iter()
            .fold(ColorSet::COLORLESS, |set, s| match s {
                ManaSymbol::Colored(c) | ManaSymbol::TwoHybrid(c) | ManaSymbol::Phyrexian(c) => {
                    set.with(*c)
                }
                ManaSymbol::Hybrid(a, b) => set.with(*a).with(*b),
                _ => set,
            })
    }

    /// The symbols of this cost that are of any of these colors (devotion,
    /// CR 700.5): a hybrid or Phyrexian symbol counts once.
    pub fn devotion(&self, colors: ColorSet) -> u32 {
        let n = self
            .symbols
            .iter()
            .filter(|s| match s {
                ManaSymbol::Colored(c) | ManaSymbol::TwoHybrid(c) | ManaSymbol::Phyrexian(c) => {
                    colors.contains(*c)
                }
                ManaSymbol::Hybrid(a, b) => colors.contains(*a) || colors.contains(*b),
                _ => false,
            })
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    pub fn has_x(&self) -> bool {
        self.symbols.contains(&ManaSymbol::X)
    }

    /// Requirements with `X` fixed: per-color counts, colorless count, generic.
    pub fn requirements(&self, x: u32) -> ManaRequirement {
        let mut req = ManaRequirement::default();
        for s in &self.symbols {
            match s {
                ManaSymbol::Colored(c) => req.colored[*c as usize] += 1,
                ManaSymbol::Colorless => req.colorless += 1,
                ManaSymbol::Generic(n) => req.generic += u32::from(*n),
                ManaSymbol::X => req.generic += x,
                ManaSymbol::Hybrid(a, b) => req.hybrid[hybrid_index(*a, *b)] += 1,
                ManaSymbol::TwoHybrid(c) => req.two_hybrid[*c as usize] += 1,
                ManaSymbol::Phyrexian(c) => req.phyrexian[*c as usize] += 1,
                ManaSymbol::Snow => req.snow += 1,
            }
        }
        req
    }
}

impl fmt::Debug for ManaCost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for ManaCost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for s in &self.symbols {
            match s {
                ManaSymbol::Colored(c) => write!(f, "{{{}}}", c.letter())?,
                ManaSymbol::Colorless => write!(f, "{{C}}")?,
                ManaSymbol::Generic(n) => write!(f, "{{{n}}}")?,
                ManaSymbol::X => write!(f, "{{X}}")?,
                ManaSymbol::Hybrid(a, b) => write!(f, "{{{}/{}}}", a.letter(), b.letter())?,
                ManaSymbol::TwoHybrid(c) => write!(f, "{{2/{}}}", c.letter())?,
                ManaSymbol::Phyrexian(c) => write!(f, "{{{}/P}}", c.letter())?,
                ManaSymbol::Snow => write!(f, "{{S}}")?,
            }
        }
        Ok(())
    }
}

/// The ten two-color pairs in WUBRG order: WU, WB, WR, WG, UB, UR, UG,
/// BR, BG, RG.
pub const HYBRID_PAIRS: [(Color, Color); 10] = {
    use Color::{Black as B, Blue as U, Green as G, Red as R, White as W};
    [
        (W, U),
        (W, B),
        (W, R),
        (W, G),
        (U, B),
        (U, R),
        (U, G),
        (B, R),
        (B, G),
        (R, G),
    ]
};

/// Index into `HYBRID_PAIRS` of the pair `{a/b}` in either order.
pub fn hybrid_index(a: Color, b: Color) -> usize {
    let (a, b) = if a < b { (a, b) } else { (b, a) };
    HYBRID_PAIRS
        .iter()
        .position(|p| *p == (a, b))
        .expect("two distinct colors")
}

/// What a cost needs once `X` is chosen. Hybrid and Phyrexian symbols are
/// counted apart; `expansions` lists the plain requirements they can be
/// paid as.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct ManaRequirement {
    /// Indexed by `Color as usize`.
    pub colored: [u32; 5],
    pub colorless: u32,
    pub generic: u32,
    /// `{a/b}` symbols, indexed like `HYBRID_PAIRS`.
    pub hybrid: [u8; 10],
    /// `{2/c}` symbols, indexed by color.
    pub two_hybrid: [u8; 5],
    /// `{c/P}` symbols, indexed by color.
    pub phyrexian: [u8; 5],
    /// `{S}` symbols (CR 107.4h): each paid with one mana from a snow
    /// source, so a plan picks the sources (see the engine's
    /// `snow_payment`).
    pub snow: u32,
}

/// Most expansions a requirement offers; beyond it the rest are dropped
/// (no printed cost comes near it).
const MAX_EXPANSIONS: usize = 64;

impl ManaRequirement {
    /// Mana symbols to pay, counting `{2/c}` as two and `{c/P}` as one.
    pub fn total(&self) -> u32 {
        self.colored.iter().sum::<u32>()
            + self.colorless
            + self.generic
            + self.hybrid.iter().map(|n| u32::from(*n)).sum::<u32>()
            + self
                .two_hybrid
                .iter()
                .map(|n| 2 * u32::from(*n))
                .sum::<u32>()
            + self.phyrexian.iter().map(|n| u32::from(*n)).sum::<u32>()
            + self.snow
    }

    /// No hybrid, Phyrexian, or snow symbols.
    pub fn is_plain(&self) -> bool {
        self.hybrid == [0; 10]
            && self.two_hybrid == [0; 5]
            && self.phyrexian == [0; 5]
            && self.snow == 0
    }

    /// Reduce by an amount of mana (CR 118.7): generic reduces generic
    /// only (118.7a); colored or colorless reduces mana of that type and
    /// any excess reduces generic (118.7b–d). A hybrid symbol reduces by
    /// a color the cost needs if either half is one, else by its generic
    /// half or one generic (118.7e); a Phyrexian symbol by its color
    /// (118.7f); X is zero.
    pub fn reduce_by(&mut self, by: &ManaCost) {
        let mut generic = 0u32;
        let color = |req: &mut ManaRequirement, c: Color, generic: &mut u32| {
            let have = &mut req.colored[c as usize];
            if *have > 0 {
                *have -= 1;
            } else {
                *generic += 1;
            }
        };
        for s in &by.symbols {
            match *s {
                ManaSymbol::Generic(n) => generic += u32::from(n),
                ManaSymbol::X => {}
                // No reduction is printed in snow mana; one mana of it
                // would reduce generic mana.
                ManaSymbol::Snow => generic += 1,
                ManaSymbol::Colored(c) | ManaSymbol::Phyrexian(c) => {
                    color(self, c, &mut generic);
                }
                ManaSymbol::Colorless => {
                    if self.colorless > 0 {
                        self.colorless -= 1;
                    } else {
                        generic += 1;
                    }
                }
                ManaSymbol::Hybrid(a, b) => {
                    let c = if self.colored[b as usize] > 0 && self.colored[a as usize] == 0 {
                        b
                    } else {
                        a
                    };
                    color(self, c, &mut generic);
                }
                ManaSymbol::TwoHybrid(c) => {
                    if self.colored[c as usize] > 0 {
                        color(self, c, &mut generic);
                    } else {
                        generic += 2;
                    }
                }
            }
        }
        self.generic = self.generic.saturating_sub(generic);
    }

    /// Add another requirement (a kicker on top of a mana cost).
    pub fn add(&mut self, other: &ManaRequirement) {
        for (a, b) in self.colored.iter_mut().zip(other.colored) {
            *a += b;
        }
        self.colorless += other.colorless;
        self.generic += other.generic;
        self.snow += other.snow;
        for (a, b) in self.hybrid.iter_mut().zip(other.hybrid) {
            *a += b;
        }
        for (a, b) in self.two_hybrid.iter_mut().zip(other.two_hybrid) {
            *a += b;
        }
        for (a, b) in self.phyrexian.iter_mut().zip(other.phyrexian) {
            *a += b;
        }
    }

    /// The plain requirements this one can be paid as, each with the life
    /// it costs, in the canonical order of preference: each hybrid symbol
    /// as its first color before its second, `{2/c}` as the color before
    /// two generic, `{c/P}` as the color before 2 life (the engine pays
    /// mana when it can; CR 107.4e–f leave the choice to the player).
    pub fn expansions(&self) -> SmallVec<[(ManaRequirement, u32); 8]> {
        let mut base = *self;
        base.hybrid = [0; 10];
        base.two_hybrid = [0; 5];
        base.phyrexian = [0; 5];
        let mut out: SmallVec<[(ManaRequirement, u32); 8]> = SmallVec::new();
        out.push((base, 0));
        let branch = |out: &mut SmallVec<[(ManaRequirement, u32); 8]>,
                      a: &dyn Fn(&mut ManaRequirement, &mut u32),
                      b: &dyn Fn(&mut ManaRequirement, &mut u32)| {
            let mut next = SmallVec::new();
            for (r, l) in out.iter() {
                for f in [a, b] {
                    if next.len() >= MAX_EXPANSIONS {
                        break;
                    }
                    let (mut r, mut l) = (*r, *l);
                    f(&mut r, &mut l);
                    if !next.contains(&(r, l)) {
                        next.push((r, l));
                    }
                }
            }
            *out = next;
        };
        for (i, n) in self.hybrid.iter().enumerate() {
            let (x, y) = HYBRID_PAIRS[i];
            for _ in 0..*n {
                branch(&mut out, &|r, _| r.colored[x as usize] += 1, &|r, _| {
                    r.colored[y as usize] += 1;
                });
            }
        }
        for (i, n) in self.two_hybrid.iter().enumerate() {
            for _ in 0..*n {
                branch(&mut out, &|r, _| r.colored[i] += 1, &|r, _| r.generic += 2);
            }
        }
        for (i, n) in self.phyrexian.iter().enumerate() {
            for _ in 0..*n {
                branch(&mut out, &|r, _| r.colored[i] += 1, &|_, l| *l += 2);
            }
        }
        out
    }
}

/// Mana a player has available to spend (CR 106.4). Empties between steps
/// and phases (CR 500.4).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ManaPool {
    /// Indexed by `Color as usize`.
    pub colored: [u32; 5],
    pub colorless: u32,
}

impl ManaPool {
    pub fn add(&mut self, mana: Mana) {
        match mana {
            Mana::Colored(c) => self.colored[c as usize] += 1,
            // A payment plan names the color before mana is added; treat a
            // stray `Any` or `Chosen` as colorless.
            Mana::Colorless | Mana::Any | Mana::Chosen => self.colorless += 1,
        }
    }

    pub fn total(&self) -> u32 {
        self.colored.iter().sum::<u32>() + self.colorless
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    pub fn empty(&mut self) {
        *self = ManaPool::default();
    }

    /// The smaller amount of each kind of mana of the two pools.
    #[must_use]
    pub fn min_each(&self, other: &ManaPool) -> ManaPool {
        let mut out = *self;
        for (a, b) in out.colored.iter_mut().zip(other.colored) {
            *a = (*a).min(b);
        }
        out.colorless = out.colorless.min(other.colorless);
        out
    }

    /// Whether the pool alone can pay `req` (hybrid and Phyrexian symbols
    /// as mana).
    pub fn can_pay(&self, req: &ManaRequirement) -> bool {
        req.expansions()
            .iter()
            .any(|(r, life)| *life == 0 && self.payment_plan(r).is_some())
    }

    /// A canonical way to pay a plain `req` (see `ManaRequirement::
    /// expansions`) from this pool: colored symbols from
    /// matching mana, `{C}` from colorless, then generic from whatever is
    /// left, colorless first and then colors in WUBRG order. Returns the
    /// pool after payment. The pool doesn't remember which of its mana a
    /// snow source made, so it pays no `{S}` (the engine taps snow sources
    /// for those, see `snow_payment`).
    pub fn payment_plan(&self, req: &ManaRequirement) -> Option<ManaPool> {
        if req.snow > 0 {
            return None;
        }
        let mut left = *self;
        for (i, need) in req.colored.iter().enumerate() {
            if left.colored[i] < *need {
                return None;
            }
            left.colored[i] -= need;
        }
        if left.colorless < req.colorless {
            return None;
        }
        left.colorless -= req.colorless;
        let mut generic = req.generic;
        let take = generic.min(left.colorless);
        left.colorless -= take;
        generic -= take;
        for slot in &mut left.colored {
            let take = generic.min(*slot);
            *slot -= take;
            generic -= take;
        }
        if generic == 0 { Some(left) } else { None }
    }
}

/// One mana.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum Mana {
    Colored(Color),
    Colorless,
    /// "Add one mana of any color": the color is chosen as it is spent.
    Any,
    /// "Add one mana of the chosen color": the color chosen as the source
    /// entered (CR 607.2a); none if no color was chosen.
    Chosen,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_display_round_trip() {
        for text in ["{R}", "{1}{R}", "{X}{G}{G}", "{2}{W}{U}", "{C}{C}", ""] {
            let cost = ManaCost::parse(text).unwrap();
            assert_eq!(cost.to_string(), text);
        }
    }

    #[test]
    fn mana_value_and_colors() {
        let cost = ManaCost::parse("{3}{W}{U}{X}").unwrap();
        assert_eq!(cost.mana_value(), 5);
        assert_eq!(cost.colors(), ColorSet::of([Color::White, Color::Blue]));
        assert!(cost.has_x());
        assert_eq!(ManaCost::default().mana_value(), 0);
        assert!(ManaCost::default().colors().is_colorless());
    }

    #[test]
    fn hybrid_and_phyrexian() {
        for text in ["{R/G}", "{2/W}", "{B/P}", "{1}{W/U}{W/U}"] {
            assert_eq!(ManaCost::parse(text).unwrap().to_string(), text);
        }
        let c = ManaCost::parse("{1}{R/G}{2/W}{B/P}").unwrap();
        assert_eq!(c.mana_value(), 5);
        assert_eq!(
            c.colors(),
            ColorSet::of([Color::White, Color::Black, Color::Red, Color::Green])
        );
        let req = c.requirements(0);
        assert_eq!(req.total(), 5);
        assert!(!req.is_plain());
        let ex = req.expansions();
        assert_eq!(ex.len(), 8);
        assert_eq!(ex[0].0.colored, [1, 0, 1, 1, 0]);
        assert_eq!(ex[0].1, 0);
        assert!(
            ex.iter()
                .any(|(r, l)| *l == 2 && r.generic == 3 && r.colored == [0, 0, 0, 0, 1])
        );
        assert!(ManaCost::parse("{R/R}").is_err());
        let mut pool = ManaPool::default();
        pool.add(Mana::Colored(Color::Green));
        assert!(pool.can_pay(&ManaCost::parse("{R/G}").unwrap().requirements(0)));
        assert!(!pool.can_pay(&ManaCost::parse("{W/U}").unwrap().requirements(0)));
    }

    #[test]
    fn snow() {
        let c = ManaCost::parse("{1}{S}{S}").unwrap();
        assert_eq!(c.to_string(), "{1}{S}{S}");
        assert_eq!(c.mana_value(), 3);
        assert!(c.colors().is_colorless());
        let req = c.requirements(0);
        assert_eq!((req.snow, req.generic, req.total()), (2, 1, 3));
        assert!(!req.is_plain());
        let mut pool = ManaPool::default();
        for _ in 0..3 {
            pool.add(Mana::Colorless);
        }
        assert!(!pool.can_pay(&req));
    }

    #[test]
    fn parse_errors() {
        assert_eq!(
            ManaCost::parse("{Q}"),
            Err(ManaCostParseError::Unknown("Q".into()))
        );
        assert_eq!(
            ManaCost::parse("{1"),
            Err(ManaCostParseError::Unterminated("{1".into()))
        );
        assert_eq!(
            ManaCost::parse("R"),
            Err(ManaCostParseError::Stray("R".into()))
        );
    }

    #[test]
    fn pool_payment() {
        let mut pool = ManaPool::default();
        pool.add(Mana::Colored(Color::Red));
        pool.add(Mana::Colored(Color::Green));
        pool.add(Mana::Colorless);
        let bolt = ManaCost::parse("{R}").unwrap().requirements(0);
        let left = pool.payment_plan(&bolt).unwrap();
        assert_eq!(left.total(), 2);
        assert_eq!(left.colored[Color::Red as usize], 0);

        let two_generic_r = ManaCost::parse("{2}{R}").unwrap().requirements(0);
        let left = pool.payment_plan(&two_generic_r).unwrap();
        assert_eq!(left.total(), 0);

        let gg = ManaCost::parse("{G}{G}").unwrap().requirements(0);
        assert!(!pool.can_pay(&gg));

        // {C} cannot be paid with colored mana.
        let c = ManaCost::parse("{C}{C}").unwrap().requirements(0);
        assert!(!pool.can_pay(&c));

        // Generic prefers colorless first.
        let one = ManaCost::parse("{1}").unwrap().requirements(0);
        let left = pool.payment_plan(&one).unwrap();
        assert_eq!(left.colorless, 0);
        assert_eq!(left.total(), 2);
    }
}
