//! Built-in 60-card mono-colored decks over the M1 pool.

use mtg_core::card::{CardPool, CardRef};

pub const NAMES: [&str; 5] = [
    "mono-white",
    "mono-blue",
    "mono-black",
    "mono-red",
    "mono-green",
];

fn build(pool: &CardPool, spec: &[(&str, usize)]) -> Vec<CardRef> {
    let deck: Vec<CardRef> = spec
        .iter()
        .flat_map(|(name, n)| {
            std::iter::repeat_n(
                pool.lookup(name)
                    .unwrap_or_else(|| panic!("{name} is not in the pool")),
                *n,
            )
        })
        .collect();
    assert_eq!(deck.len(), 60, "deck must be 60 cards");
    deck
}

pub fn by_name(pool: &CardPool, name: &str) -> Option<Vec<CardRef>> {
    let spec: &[(&str, usize)] = match name {
        "mono-white" => &[
            ("Plains", 24),
            ("Savannah Lions", 8),
            ("Youthful Knight", 8),
            ("Fencing Ace", 8),
            ("Serra Angel", 8),
            ("Healing Salve", 4),
        ],
        "mono-blue" => &[
            ("Island", 24),
            ("Wind Drake", 12),
            ("Air Elemental", 12),
            ("Divination", 12),
        ],
        "mono-black" => &[
            ("Swamp", 24),
            ("Typhoid Rats", 12),
            ("Vampire Nighthawk", 12),
            ("Murder", 12),
        ],
        "mono-red" => &[
            ("Mountain", 24),
            ("Raging Goblin", 8),
            ("Hill Giant", 8),
            ("Boggart Brute", 4),
            ("Wall of Stone", 4),
            ("Lightning Bolt", 4),
            ("Shock", 4),
            ("Lava Axe", 4),
        ],
        "mono-green" => &[
            ("Forest", 24),
            ("Grizzly Bears", 8),
            ("Trained Armodon", 8),
            ("Giant Spider", 4),
            ("Craw Wurm", 4),
            ("Colossal Dreadmaw", 4),
            ("Giant Growth", 8),
        ],
        _ => return None,
    };
    Some(build(pool, spec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::card::m1_pool;

    #[test]
    fn all_decks_build() {
        let pool = m1_pool();
        for name in NAMES {
            assert_eq!(by_name(&pool, name).unwrap().len(), 60, "{name}");
        }
        assert!(by_name(&pool, "nope").is_none());
    }
}
