//! Colors, mana symbols, mana costs, and the mana pool (CR 105, 106, 107,
//! 202). M1 covers single-colored symbols, generic, and colorless; hybrid,
//! Phyrexian, and snow symbols arrive with the cards that need them.

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
                s if s.chars().all(|c| c.is_ascii_digit()) => {
                    let n: u32 = s
                        .parse()
                        .map_err(|_| ManaCostParseError::Unknown(s.to_string()))?;
                    ManaSymbol::Generic(
                        u8::try_from(n).map_err(|_| ManaCostParseError::Unknown(s.to_string()))?,
                    )
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
                ManaSymbol::Colored(_) | ManaSymbol::Colorless => 1,
                ManaSymbol::X => 0,
            })
            .sum()
    }

    /// Colors of the symbols in this cost; the object's color unless a
    /// color indicator or CDA says otherwise (CR 202.2).
    pub fn colors(&self) -> ColorSet {
        ColorSet::of(self.symbols.iter().filter_map(|s| match s {
            ManaSymbol::Colored(c) => Some(*c),
            _ => None,
        }))
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
            }
        }
        Ok(())
    }
}

/// What a cost needs once `X` is chosen.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct ManaRequirement {
    /// Indexed by `Color as usize`.
    pub colored: [u32; 5],
    pub colorless: u32,
    pub generic: u32,
}

impl ManaRequirement {
    pub fn total(&self) -> u32 {
        self.colored.iter().sum::<u32>() + self.colorless + self.generic
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
            // stray `Any` as colorless.
            Mana::Colorless | Mana::Any => self.colorless += 1,
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

    /// Whether the pool alone can pay `req`.
    pub fn can_pay(&self, req: &ManaRequirement) -> bool {
        self.payment_plan(req).is_some()
    }

    /// A canonical way to pay `req` from this pool: colored symbols from
    /// matching mana, `{C}` from colorless, then generic from whatever is
    /// left, colorless first and then colors in WUBRG order. Returns the
    /// pool after payment.
    pub fn payment_plan(&self, req: &ManaRequirement) -> Option<ManaPool> {
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
