//! Syntax tree for scenario files. See `design.md` §7.1 and `SYNTAX.md`.

use mtg_core::ids::PlayerId;
use mtg_core::state::Zone;
use mtg_core::turn::Step;

/// Players are named `A` (index 0) and `B` (index 1).
pub fn player_name(p: PlayerId) -> &'static str {
    if p.index() == 0 { "A" } else { "B" }
}

/// A point in the game: turn number and step. Ordered lexicographically.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct Position {
    pub turn: u32,
    pub step: Step,
}

/// Where to stop running.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RunTarget {
    Position(Position),
    /// Run until the game is over.
    End,
}

#[derive(Clone, Debug)]
pub struct Scenario {
    pub name: String,
    pub line: usize,
    /// Set by `skip "reason"`: the scenario is parsed but not run.
    pub skip: Option<String>,
    pub setup: SetupSpec,
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug)]
pub struct CardSpec {
    pub name: String,
    pub count: usize,
    pub tapped: bool,
    /// `on "Name"`: attach this Aura to that permanent.
    pub attached_to: Option<String>,
    /// `with N +1/+1`: counters already on the permanent.
    pub counters: Vec<(String, u32)>,
}

#[derive(Clone, Debug)]
pub struct ZoneSpec {
    pub line: usize,
    pub zone: Zone,
    pub player: PlayerId,
    pub cards: Vec<CardSpec>,
}

#[derive(Clone, Debug, Default)]
pub struct SetupSpec {
    pub life: [Option<i32>; 2],
    /// `poison B: 9`: starting poison counters.
    pub poison: [u32; 2],
    /// `energy A: 3`: starting energy counters.
    pub energy: [u32; 2],
    /// `speed A: 3`: starting speed (CR 702.179).
    pub speed: [u8; 2],
    pub zones: Vec<ZoneSpec>,
    /// `start turn N P [step]`.
    pub start: Option<(u32, PlayerId, Step)>,
    pub seed: Option<u64>,
}

#[derive(Clone, Debug)]
pub enum Statement {
    Action(ScriptedAction),
    RunTo { line: usize, target: RunTarget },
    Assert { line: usize, assertion: Assertion },
}

/// A reference to an object by name, optionally qualified by controller
/// and, where several match, by age (`oldest "X"` / `newest "X"`).
#[derive(Clone, Debug)]
pub struct ObjRef {
    pub player: Option<PlayerId>,
    pub name: String,
    pub pick: Option<Pick>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Pick {
    Oldest,
    Newest,
}

#[derive(Clone, Debug)]
pub enum TargetRef {
    Player(PlayerId),
    Object(ObjRef),
    /// `targeting auto`: every target takes the engine's first legal option
    /// (how `XMage`'s tests target when only one choice exists).
    Auto,
}

/// `targeting auto` stands for as many automatic targets as are needed.
pub fn expand_auto(targets: &[TargetRef], needed: usize) -> Vec<TargetRef> {
    if matches!(targets, [TargetRef::Auto]) {
        vec![TargetRef::Auto; needed]
    } else {
        targets.to_vec()
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    Play(String),
    Cast {
        name: String,
        /// `from graveyard`: cast with flashback (CR 702.34).
        from_graveyard: bool,
        /// `kicked`: pay the kicker (CR 702.33).
        kicked: bool,
        /// `mode N` (1-based) for a modal spell.
        mode: Option<usize>,
        /// `x N` for a spell with {X} in its cost.
        x: Option<u32>,
        /// `sacrificing "X"` / `discarding "X"`: additional costs.
        sacrifice: Option<String>,
        discard: Option<String>,
        targets: Vec<TargetRef>,
        /// `in response to "X"`: fire only while an object named X is on
        /// the stack.
        respond_to: Option<String>,
        /// `face_down`: cast face down for {3} (morph, CR 702.37c).
        face_down: bool,
        /// `evoked`: cast for the evoke cost (CR 702.74).
        evoked: bool,
        /// `dashed`: cast for the dash cost (CR 702.109).
        dashed: bool,
        /// `alternative`: cast for the alternative cost the card's text
        /// offers (CR 118.9, "rather than pay this spell's mana cost").
        alternative: bool,
        /// `bestowed`: cast for the bestow cost, as an Aura spell (CR
        /// 702.103a); its target is the creature it enchants.
        bestowed: bool,
        /// `splicing "X", "Y"`: cards in hand spliced onto the Arcane
        /// spell (CR 702.47a), in order; their targets follow the spell's.
        splice: Vec<String>,
        /// `other_cost`: pay the second of the spell's two additional costs
        /// ("sacrifice a creature or pay {3}", CR 601.2b); otherwise the
        /// first that can be paid.
        other_cost: bool,
        /// `dividing 2, 1` after the targets: how a divided spell splits
        /// its damage or counters among them, in target order (CR
        /// 601.2d).
        divide: Vec<u32>,
    },
    /// `unmorph "X"`: turn the face-down permanent that is card X face up.
    TurnFaceUp(String),
    /// `suspend "X"`: suspend card X from the hand (a special action).
    Suspend(String),
    /// `foretell "X"`: foretell card X from the hand (a special action).
    Foretell(String),
    /// `unlock "Door"`: unlock that locked door (half) of a Room the
    /// player controls (CR 709.5e, a special action).
    Unlock(String),
    /// `attack "X" [at <target>], ...`: attackers, each optionally with
    /// the player or planeswalker it attacks (CR 508.1a).
    Attack(Vec<(String, Option<TargetRef>)>),
    Block {
        attacker: String,
        blocker: String,
    },
    /// Divide `attacker`'s combat damage.
    Assign {
        attacker: String,
        parts: Vec<(TargetRef, u32)>,
    },
    Discard(String),
    /// `trigger "Source" [targeting ...]`: when the player orders several
    /// pending triggers, put this source's trigger on the stack next; when
    /// its targets are chosen, use these.
    Trigger {
        source: String,
        targets: Vec<TargetRef>,
    },
    /// `apply "Source"`: when several replacement or prevention effects
    /// apply to one event, apply this source's first (CR 616.1).
    Apply(String),
    /// `wait`: pass priority until the stack is empty before any later
    /// action of this player at the same point fires.
    Wait,
    /// `activate "Source" [sacrificing "X"] [targeting ...]`: activate an
    /// ability of a permanent the player controls (CR 602).
    Activate {
        source: String,
        /// `ability N` (1-based among the permanent's activated abilities).
        ability: Option<usize>,
        sacrifice: Option<String>,
        /// `discarding "X"` for a "discard a card" cost.
        discard: Option<String>,
        targets: Vec<TargetRef>,
    },
    /// `keep oldest|newest "Name"`: under the legend rule (CR 704.5j), keep
    /// the oldest or newest of the player's legendary permanents of that
    /// name.
    Keep {
        name: String,
        newest: bool,
    },
    /// `accept "Source"` / `decline "Source"`: the answer to that card's
    /// "you may" as it resolves.
    YouMay {
        name: String,
        accept: bool,
    },
    /// `copy "Name"` / `copy nothing`: what a Clone enters as a copy of.
    Copy(Option<ObjRef>),
    /// `choose red`, `choose Island`, `choose "Merfolk"`: a color or type
    /// chosen as a permanent enters or an effect resolves.
    Choose(String),
}

impl Action {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Action::Play(_) => "play",
            Action::Cast { .. } => "cast",
            Action::Attack(_) => "attack",
            Action::Block { .. } => "block",
            Action::Assign { .. } => "assign",
            Action::Discard(_) => "discard",
            Action::Trigger { .. } => "trigger",
            Action::Apply(_) => "apply",
            Action::Activate { .. } => "activate",
            Action::Keep { .. } => "keep",
            Action::YouMay { accept: true, .. } => "accept",
            Action::YouMay { accept: false, .. } => "decline",
            Action::Copy(_) => "copy",
            Action::Choose(_) => "choose",
            Action::Wait => "wait",
            Action::TurnFaceUp(_) => "unmorph",
            Action::Suspend(_) => "suspend",
            Action::Foretell(_) => "foretell",
            Action::Unlock(_) => "unlock",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScriptedAction {
    pub line: usize,
    pub turn: u32,
    /// `None` means any step of the turn (used for damage assignment, which
    /// may happen in the first-strike or regular damage step).
    pub step: Option<Step>,
    pub player: PlayerId,
    pub action: Action,
    /// `illegal ...`: the action must *not* be offered when its point is
    /// reached; the default choice is taken instead.
    pub illegal: bool,
}

#[derive(Clone, Debug)]
pub enum Assertion {
    Life(PlayerId, i32),
    /// `assert poison B 1`.
    Poison(PlayerId, u32),
    /// `assert energy A 2`.
    Energy(PlayerId, u32),
    /// A designation (CR 309, 701.54, 702.131, 702.179, 725, 726, 731):
    /// `assert monarch A`, `assert initiative none`, `assert daynight
    /// night`, `assert speed A 2`, `assert tempted A 1`, `assert
    /// ring_bearer A "Grizzly Bears"`, `assert blessing A yes`, `assert
    /// dungeon A "Lost Mine of Phandelver" "Goblin Lair"` (or `none`),
    /// `assert completed A 1`: the value as text.
    Designation {
        key: String,
        player: Option<PlayerId>,
        want: String,
    },
    /// `zone P contains [N] "name"`: at least one, or exactly N.
    ZoneContains {
        zone: Zone,
        player: PlayerId,
        name: String,
        count: Option<usize>,
    },
    ZoneLacks {
        zone: Zone,
        player: PlayerId,
        name: String,
    },
    ZoneCount {
        zone: Zone,
        player: PlayerId,
        count: usize,
    },
    Power(ObjRef, i32),
    Toughness(ObjRef, i32),
    Damage(ObjRef, u32),
    Tapped(ObjRef, bool),
    Winner(PlayerId),
    Draw,
    GameOver(bool),
    Turn(u32),
    Step(Step),
    StackCount(usize),
    /// `has "X" flying` / `has not "X" flying`.
    HasKeyword(ObjRef, String, bool),
    /// `is "X" creature` / `is not "X" mountain`: a type, supertype, subtype,
    /// or color.
    Is(ObjRef, String, bool),
    /// `counters "X" +1/+1 3`.
    Counters(ObjRef, String, u32),
    /// `attached "Aura" to "X"`.
    Attached(ObjRef, ObjRef),
    /// `no_abilities "X"`.
    NoAbilities(ObjRef),
    /// `stack top "X"`: the top object of the stack is named X.
    StackTop(String),
}

/// Step names accepted in scenario files.
pub const STEP_NAMES: [(&str, Step); 15] = [
    ("untap", Step::Untap),
    ("upkeep", Step::Upkeep),
    ("draw", Step::Draw),
    ("precombat_main", Step::PrecombatMain),
    ("main1", Step::PrecombatMain),
    ("begin_combat", Step::BeginningOfCombat),
    ("declare_attackers", Step::DeclareAttackers),
    ("declare_blockers", Step::DeclareBlockers),
    ("first_strike_damage", Step::FirstStrikeDamage),
    ("combat_damage", Step::CombatDamage),
    ("end_combat", Step::EndOfCombat),
    ("postcombat_main", Step::PostcombatMain),
    ("main2", Step::PostcombatMain),
    ("end", Step::End),
    ("cleanup", Step::Cleanup),
];

pub fn step_by_name(name: &str) -> Option<Step> {
    STEP_NAMES.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

pub fn step_name(step: Step) -> &'static str {
    STEP_NAMES
        .iter()
        .find(|(_, s)| *s == step)
        .map_or("?", |(n, _)| n)
}

pub fn zone_by_name(name: &str) -> Option<Zone> {
    Some(match name {
        "battlefield" => Zone::Battlefield,
        "hand" => Zone::Hand,
        "graveyard" => Zone::Graveyard,
        "library" => Zone::Library,
        "exile" => Zone::Exile,
        _ => return None,
    })
}
