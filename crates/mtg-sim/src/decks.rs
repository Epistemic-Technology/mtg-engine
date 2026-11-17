//! Built-in 60-card mono-colored decks over the M1 pool.

use mtg_core::card::{CardPool, CardRef};

pub const NAMES: [&str; 11] = [
    "mono-white",
    "mono-blue",
    "mono-black",
    "mono-red",
    "mono-green",
    "layers",
    "triggers",
    "replacement",
    "activated",
    "copy",
    "slice6",
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

#[allow(clippy::too_many_lines)] // one entry per deck
pub fn by_name(pool: &CardPool, name: &str) -> Option<Vec<CardRef>> {
    let spec: &[(&str, usize)] = match name {
        "mono-white" => &[
            ("Plains", 24),
            ("Savannah Lions", 8),
            ("Youthful Knight", 6),
            ("Fencing Ace", 6),
            ("Serra Angel", 6),
            ("Healing Salve", 2),
            ("Glorious Anthem", 3),
            ("Crusade", 2),
            ("Holy Strength", 3),
        ],
        "mono-blue" => &[
            ("Island", 24),
            ("Wind Drake", 10),
            ("Air Elemental", 10),
            ("Divination", 8),
            ("Flight", 4),
            ("Levitation", 4),
        ],
        "mono-black" => &[
            ("Swamp", 24),
            ("Typhoid Rats", 10),
            ("Vampire Nighthawk", 10),
            ("Murder", 8),
            ("Dead Weight", 4),
            ("Unholy Strength", 4),
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
            ("Giant Growth", 6),
            ("Dragonscale Boon", 2),
        ],
        // Static-ability stress deck: every layer puzzle card at once.
        "layers" => &[
            ("Plains", 10),
            ("Forest", 6),
            ("Mountain", 4),
            ("Darksteel Citadel", 2),
            ("Dryad Arbor", 2),
            ("Humility", 3),
            ("Opalescence", 3),
            ("Glorious Anthem", 3),
            ("Crusade", 2),
            ("Muraganda Petroglyphs", 3),
            ("Blood Moon", 3),
            ("Levitation", 2),
            ("Savannah Lions", 4),
            ("Serra Angel", 4),
            ("Grizzly Bears", 4),
            ("Holy Strength", 3),
            ("Dead Weight", 2),
        ],
        // Triggered-ability stress deck: every trigger card at once.
        "triggers" => &[
            ("Plains", 6),
            ("Swamp", 6),
            ("Mountain", 6),
            ("Forest", 4),
            ("Elvish Visionary", 2),
            ("Wall of Omens", 2),
            ("Soul Warden", 3),
            ("Flametongue Kavu", 2),
            ("Ajani's Pridemate", 3),
            ("Blood Artist", 3),
            ("Zulaport Cutthroat", 2),
            ("Festering Goblin", 3),
            ("Perilous Myr", 2),
            ("Rancor", 2),
            ("Phyrexian Arena", 2),
            ("Juzám Djinn", 1),
            ("Howling Mine", 2),
            ("Reckless Pangolin", 2),
            ("Hellrider", 2),
            ("Markov Blademaster", 2),
            ("Monastery Swiftspear", 3),
        ],
        // Replacement-effect stress deck: every replacement and prevention
        // card at once, plus damage and life to replace.
        "replacement" => &[
            ("Plains", 7),
            ("Mountain", 6),
            ("Forest", 4),
            ("Island", 3),
            ("Azorius Guildgate", 2),
            ("Fog", 2),
            ("Holy Day", 1),
            ("Safe Passage", 2),
            ("Mending Hands", 2),
            ("Bandage", 2),
            ("Skullcrack", 2),
            ("Furnace of Rath", 2),
            ("Gratuitous Violence", 1),
            ("Phantom Tiger", 3),
            ("Sulfuric Vortex", 2),
            ("Rhox Faithmender", 2),
            ("Laboratory Maniac", 2),
            ("Thought Reflection", 1),
            ("Rest in Peace", 2),
            ("Leyline of the Void", 1),
            ("Hardened Scales", 2),
            ("Ajani's Pridemate", 2),
            ("Vampire Nighthawk", 2),
            ("Savannah Lions", 2),
            ("Lightning Bolt", 2),
            ("Dragonscale Boon", 1),
        ],
        // Activated-ability stress deck: every slice 4 card at once (tokens,
        // Equipment, control changes, the legend rule).
        "activated" => &[
            ("Mountain", 7),
            ("Plains", 5),
            ("Swamp", 4),
            ("Island", 3),
            ("Forest", 2),
            ("Mishra's Factory", 2),
            ("Prodigal Sorcerer", 2),
            ("Llanowar Elves", 2),
            ("Mogg Fanatic", 3),
            ("Bottle Gnomes", 2),
            ("Shivan Dragon", 1),
            ("Rod of Ruin", 1),
            ("Pestilence", 1),
            ("Royal Assassin", 2),
            ("Icy Manipulator", 1),
            ("Seal of Fire", 2),
            ("Bonesplitter", 2),
            ("Loxodon Warhammer", 1),
            ("Raise the Alarm", 2),
            ("Dragon Fodder", 2),
            ("Siege-Gang Commander", 2),
            ("Bitterblossom", 1),
            ("Young Pyromancer", 2),
            ("Disenchant", 1),
            ("Act of Treason", 2),
            ("Control Magic", 2),
            ("Isamaru, Hound of Konda", 3),
        ],
        // Slice 6 stress deck: X costs, life and discard costs, additional
        // costs, regeneration, redirection, planeswalkers, emblems.
        "slice6" => &[
            ("Mountain", 5),
            ("Swamp", 5),
            ("Island", 4),
            ("Forest", 4),
            ("Plains", 5),
            ("Blaze", 2),
            ("Mind Spring", 1),
            ("Endless One", 2),
            ("Bone Splinters", 2),
            ("Tormenting Voice", 2),
            ("Ravenous Bloodseeker", 2),
            ("Souldrinker", 2),
            ("Vampire Hexmage", 2),
            ("Drudge Skeletons", 2),
            ("Regenerate", 2),
            ("Wrath of God", 1),
            ("Pariah", 2),
            ("Ajani Goldmane", 2),
            ("Jace Beleren", 2),
            ("Elspeth, Knight-Errant", 2),
            ("Garruk Wildspeaker", 2),
            ("Hero's Downfall", 2),
            ("Grizzly Bears", 3),
            ("Lightning Bolt", 2),
        ],
        // Slice 5 stress deck: copies, characteristic-defining abilities,
        // delayed triggers, modal spells, "you may".
        "copy" => &[
            ("Island", 6),
            ("Swamp", 4),
            ("Mountain", 5),
            ("Plains", 4),
            ("Forest", 4),
            ("Clone", 3),
            ("Phantasmal Image", 3),
            ("Cryptoplasm", 2),
            ("Kiki-Jiki, Mirror Breaker", 2),
            ("Tarmogoyf", 3),
            ("Lhurgoyf", 2),
            ("Nightmare", 2),
            ("Transguild Courier", 2),
            ("Flickerwisp", 3),
            ("Otherworldly Journey", 2),
            ("Abrade", 2),
            ("Boros Charm", 2),
            ("Grixis Charm", 2),
            ("Gravedigger", 3),
            ("Grizzly Bears", 2),
            ("Lightning Bolt", 2),
        ],
        _ => return None,
    };
    Some(build(pool, spec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_dsl::builtin_pool;

    #[test]
    fn all_decks_build() {
        let pool = builtin_pool();
        for name in NAMES {
            assert_eq!(by_name(&pool, name).unwrap().len(), 60, "{name}");
        }
        assert!(by_name(&pool, "nope").is_none());
    }
}
