//! MCP server for the engine (design §9.2): games live in a table keyed by
//! a `game_id` the server mints, every tool call names one, and the
//! caller's seat sees only what that player may see. The opponent seat is
//! played by a built-in agent unless the caller takes both seats.

#![allow(clippy::doc_markdown)] // rmcp, PettingZoo

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mtg_core::card::CardPool;
use mtg_core::decision::{Decision, StepResult};
use mtg_core::engine::{Engine, GameConfig};
use mtg_core::ids::PlayerId;
use mtg_core::record::GameRecord;
use mtg_core::state::GameResult;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{ErrorData, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One game and who plays it.
pub struct Game {
    pub engine: Engine,
    pub config: GameConfig,
    /// The caller's seat.
    pub seat: PlayerId,
    /// Plays the other seat; `None` when the caller plays both.
    pub opponent: Option<Box<dyn mtg_agents::Agent + Send>>,
    pub opponent_name: String,
    pub deck_names: (String, String),
    /// Event log cursor for `act` replies.
    pub log_cursor: usize,
}

impl Game {
    /// Let the built-in opponent act until the caller's seat must decide
    /// or the game ends.
    pub fn autoplay(&mut self) {
        while let StepResult::Decision(d) = self.engine.current() {
            if d.player == self.seat {
                break;
            }
            let Some(agent) = self.opponent.as_mut() else {
                break;
            };
            let choice = agent.choose(self.engine.state(), d);
            if self.engine.step(choice).is_err() {
                break;
            }
        }
    }

    pub fn decision(&self) -> Option<&Decision> {
        match self.engine.current() {
            StepResult::Decision(d) => Some(d),
            StepResult::Over(_) => None,
        }
    }

    /// The seat that may look at the state for a tool call: the caller's
    /// seat, or the one to act when the caller plays both.
    pub fn viewer(&self, requested: Option<u8>) -> PlayerId {
        match requested {
            Some(p) if p < 2 && (self.opponent.is_none() || PlayerId(p) == self.seat) => {
                PlayerId(p)
            }
            _ => {
                if self.opponent.is_none() {
                    self.decision().map_or(self.seat, |d| d.player)
                } else {
                    self.seat
                }
            }
        }
    }

    /// The current decision as numbered text.
    pub fn decision_text(&self) -> String {
        use std::fmt::Write as _;
        let state = self.engine.state();
        match self.engine.current() {
            StepResult::Over(r) => format!("Game over: {}.", result_text(*r, self.seat)),
            StepResult::Decision(d) => {
                let mut s = format!(
                    "P{} to act ({:?}, turn {}, {:?}). Options:\n",
                    d.player.0 + 1,
                    d.kind,
                    state.turn_number,
                    state.step
                );
                for (i, c) in d.options.iter().enumerate() {
                    let _ = writeln!(s, "  {}. {}", i + 1, c.describe(state));
                }
                s
            }
        }
    }

    /// Resolve what the caller named: a 1-based index, the exact option
    /// text, or a phrase that matches exactly one option (an `#n` label,
    /// a card name, "pass").
    pub fn resolve(&self, option: &str) -> Result<u32, String> {
        let Some(d) = self.decision() else {
            return Err("the game is over".to_string());
        };
        let state = self.engine.state();
        let text = option.trim();
        if let Ok(n) = text.parse::<usize>() {
            if n >= 1 && n <= d.options.len() {
                return Ok(u32::try_from(n - 1).unwrap_or(0));
            }
            return Err(format!(
                "option {n} is out of range; there are {} options",
                d.options.len()
            ));
        }
        let descriptions: Vec<String> = d.options.iter().map(|c| c.describe(state)).collect();
        let lower = text.to_ascii_lowercase();
        if let Some(i) = descriptions
            .iter()
            .position(|t| t.to_ascii_lowercase() == lower)
        {
            return Ok(u32::try_from(i).unwrap_or(0));
        }
        // A label names an object: match options that mention the object.
        let needle = if let Some(id) = state.by_label(d.player, text) {
            state.name_of(id).to_ascii_lowercase()
        } else {
            lower.clone()
        };
        let matches: Vec<usize> = descriptions
            .iter()
            .enumerate()
            .filter(|(_, t)| t.to_ascii_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect();
        match matches.as_slice() {
            [one] => Ok(u32::try_from(*one).unwrap_or(0)),
            [] => Err(format!(
                "no option matches {text:?}; the options are:\n{}",
                numbered(&descriptions)
            )),
            many => Err(format!(
                "{text:?} is ambiguous; it matches options {}. Give the number.",
                many.iter()
                    .map(|i| (i + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    /// Events since the cursor, as text for the viewer; advances the cursor.
    pub fn drain_events(&mut self, viewer: PlayerId) -> Vec<String> {
        let state = self.engine.state();
        let out: Vec<String> = state.log[self.log_cursor.min(state.log.len())..]
            .iter()
            .filter_map(|e| mtg_core::event_text::describe(state, e, viewer))
            .collect();
        self.log_cursor = state.log.len();
        out
    }
}

fn numbered(items: &[String]) -> String {
    items
        .iter()
        .enumerate()
        .map(|(i, t)| format!("  {}. {t}", i + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn result_text(r: GameResult, seat: PlayerId) -> String {
    match r {
        GameResult::Win { winner } if winner == seat => "you win".to_string(),
        GameResult::Win { winner } => format!("P{} wins", winner.0 + 1),
        GameResult::Draw => "a draw".to_string(),
    }
}

/// The games this server holds.
pub struct Games {
    pub pool: Arc<CardPool>,
    table: Mutex<HashMap<String, Game>>,
    next: AtomicU64,
    /// Finished games are written here as records, for `mtg-review`.
    pub record_dir: Option<std::path::PathBuf>,
}

impl Games {
    pub fn new(pool: Arc<CardPool>) -> Self {
        Games {
            pool,
            table: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            record_dir: None,
        }
    }

    #[must_use]
    pub fn with_record_dir(mut self, dir: Option<std::path::PathBuf>) -> Self {
        self.record_dir = dir;
        self
    }

    /// The record of a game, with the seat and opponent in its metadata.
    pub fn record(&self, game: &Game) -> GameRecord {
        let mut rec = GameRecord::from_engine(&game.engine, &self.pool, &game.config);
        rec.meta.extend([
            ("seat".to_string(), game.seat.0.to_string()),
            ("opponent".to_string(), game.opponent_name.clone()),
            ("deck_a".to_string(), game.deck_names.0.clone()),
            ("deck_b".to_string(), game.deck_names.1.clone()),
            ("source".to_string(), "mtg-mcp".to_string()),
        ]);
        rec
    }

    /// Write a finished game's record to `record_dir`, if set.
    fn save(&self, id: &str, game: &Game) {
        let Some(dir) = &self.record_dir else {
            return;
        };
        if let Ok(text) = serde_json::to_string(&self.record(game)) {
            let _ = std::fs::create_dir_all(dir);
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let _ = std::fs::write(dir.join(format!("mcp-{stamp}-{id}.json")), text);
        }
    }

    /// Start a game; returns its id.
    pub fn start(
        &self,
        deck_a: &str,
        deck_b: &str,
        seed: u64,
        seat: u8,
        opponent: &str,
    ) -> Result<String, String> {
        let d1 = mtg_sim::decks::by_name_seeded(&self.pool, deck_a, seed)
            .ok_or_else(|| format!("unknown deck {deck_a:?}"))?;
        let d2 = mtg_sim::decks::by_name_seeded(&self.pool, deck_b, seed ^ 0x9e37_79b9)
            .ok_or_else(|| format!("unknown deck {deck_b:?}"))?;
        let agent = if opponent == "none" {
            None
        } else {
            Some(
                mtg_agents::by_name(opponent, seed.wrapping_mul(7919)).ok_or_else(|| {
                    format!("unknown opponent {opponent:?} (greedy, random, none)")
                })?,
            )
        };
        if seat > 1 {
            return Err("seat must be 0 or 1".to_string());
        }
        let config = GameConfig {
            decks: vec![d1, d2],
            starting_life: 20,
            seed,
            logging: true,
        };
        let engine = Engine::new(self.pool.clone(), &config);
        let mut game = Game {
            engine,
            config,
            seat: PlayerId(seat),
            opponent: agent,
            opponent_name: opponent.to_string(),
            deck_names: (deck_a.to_string(), deck_b.to_string()),
            log_cursor: 0,
        };
        game.autoplay();
        let id = format!("g{}", self.next.fetch_add(1, Ordering::Relaxed));
        self.table
            .lock()
            .expect("games table")
            .insert(id.clone(), game);
        Ok(id)
    }

    /// Run `f` on a game.
    pub fn with<T>(&self, id: &str, f: impl FnOnce(&mut Game) -> T) -> Result<T, String> {
        let mut table = self.table.lock().expect("games table");
        let game = table
            .get_mut(id)
            .ok_or_else(|| format!("no game {id:?}; start one with new_game"))?;
        Ok(f(game))
    }

    pub fn remove(&self, id: &str) -> bool {
        self.table.lock().expect("games table").remove(id).is_some()
    }

    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .table
            .lock()
            .expect("games table")
            .keys()
            .cloned()
            .collect();
        ids.sort();
        ids
    }
}

// ----- tool parameters -----------------------------------------------------------

#[derive(Deserialize, JsonSchema)]
pub struct NewGameParams {
    /// Deck for seat 0: a built-in name (`mono-red`, `mono-green`,
    /// `mono-white`, `copy`, `triggers`, ... or `random:<seed>`).
    pub deck_a: String,
    /// Deck for seat 1.
    pub deck_b: String,
    /// Shuffle seed (default 0).
    #[serde(default)]
    pub seed: Option<u64>,
    /// The seat you play: 0 or 1 (default 0).
    #[serde(default)]
    pub seat: Option<u8>,
    /// Who plays the other seat: `greedy` (default), `random`, or `none`
    /// to play both seats yourself.
    #[serde(default)]
    pub opponent: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GameParams {
    pub game_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct StateParams {
    pub game_id: String,
    /// Which seat's view (only your own seat unless you play both).
    #[serde(default)]
    pub player: Option<u8>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ActParams {
    pub game_id: String,
    /// The option: its number from get_decision, its exact text, or a
    /// phrase that picks exactly one option (a `#n` label, a card name,
    /// `pass`).
    pub option: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct LogParams {
    pub game_id: String,
    /// Event cursor from a previous call (default 0: from the start).
    #[serde(default)]
    pub since: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct CardParams {
    /// A card name.
    pub name: String,
}

#[derive(Serialize)]
struct OptionInfo {
    index: usize,
    text: String,
}

#[derive(Serialize)]
struct DecisionInfo {
    game_id: String,
    over: bool,
    result: Option<String>,
    player: Option<u8>,
    kind: Option<String>,
    turn: u32,
    step: String,
    options: Vec<OptionInfo>,
}

// ----- the server ----------------------------------------------------------------

#[derive(Clone)]
pub struct MtgServer {
    games: Arc<Games>,
    tool_router: ToolRouter<Self>,
}

impl MtgServer {
    pub fn new(games: Arc<Games>) -> Self {
        MtgServer {
            games,
            tool_router: Self::tool_router(),
        }
    }

    fn text(s: String) -> CallToolResult {
        CallToolResult::success(vec![ContentBlock::text(s)])
    }

    fn fail(s: String) -> CallToolResult {
        CallToolResult::error(vec![ContentBlock::text(s)])
    }

    fn decision_info(game: &Game, id: &str) -> DecisionInfo {
        let state = game.engine.state();
        let (over, result, player, kind, options) = match game.engine.current() {
            StepResult::Over(r) => (
                true,
                Some(result_text(*r, game.seat)),
                None,
                None,
                Vec::new(),
            ),
            StepResult::Decision(d) => (
                false,
                None,
                Some(d.player.0),
                Some(format!("{:?}", d.kind)),
                d.options
                    .iter()
                    .enumerate()
                    .map(|(i, c)| OptionInfo {
                        index: i + 1,
                        text: c.describe(state),
                    })
                    .collect(),
            ),
        };
        DecisionInfo {
            game_id: id.to_string(),
            over,
            result,
            player,
            kind,
            turn: state.turn_number,
            step: format!("{:?}", state.step),
            options,
        }
    }

    /// Text and structured content for a decision.
    fn decision_result(game: &Game, id: &str, prefix: &str) -> CallToolResult {
        let text = format!("{prefix}{}", game.decision_text());
        let info = Self::decision_info(game, id);
        let mut r = CallToolResult::success(vec![ContentBlock::text(text)]);
        if let Ok(v) = serde_json::to_value(&info) {
            r.structured_content = Some(v);
        }
        r
    }
}

#[tool_router(router = tool_router)]
impl MtgServer {
    #[tool(
        name = "new_game",
        description = "Start a game of Magic between two decks. You play one seat; a built-in agent plays the other unless opponent is \"none\". Returns the game_id and your first decision. Decks: mono-red, mono-green, mono-white, mono-blue, mono-black, copy, triggers, activated, replacement, slice6, random:<seed>."
    )]
    async fn new_game(&self, p: Parameters<NewGameParams>) -> CallToolResult {
        let p = p.0;
        let seed = p.seed.unwrap_or(0);
        let seat = p.seat.unwrap_or(0);
        let opponent = p.opponent.as_deref().unwrap_or("greedy");
        match self.games.start(&p.deck_a, &p.deck_b, seed, seat, opponent) {
            Err(e) => Self::fail(e),
            Ok(id) => self
                .games
                .with(&id, |g| {
                    let viewer = g.viewer(None);
                    let events = g.drain_events(viewer);
                    let prefix = format!(
                        "Game {id} started: you are P{} with {}, {} ({}) plays {}.\n{}\n\n{}",
                        g.seat.0 + 1,
                        p.deck_a,
                        if opponent == "none" {
                            "you also"
                        } else {
                            "the opponent"
                        },
                        opponent,
                        p.deck_b,
                        events.join("\n"),
                        g.engine.state().describe(viewer)
                    );
                    Self::decision_result(g, &id, &prefix)
                })
                .unwrap_or_else(Self::fail),
        }
    }

    #[tool(
        name = "get_state",
        description = "The board as text for your seat: life, hands, battlefield with #n labels, graveyards, and the stack. Also returns the view as JSON."
    )]
    async fn get_state(&self, p: Parameters<StateParams>) -> CallToolResult {
        self.games
            .with(&p.0.game_id, |g| {
                let viewer = g.viewer(p.0.player);
                let state = g.engine.state();
                let mut r = Self::text(state.describe(viewer));
                if let Ok(v) = serde_json::to_value(state.observe(viewer)) {
                    r.structured_content = Some(v);
                }
                r
            })
            .unwrap_or_else(Self::fail)
    }

    #[tool(
        name = "get_decision",
        description = "The current decision: who acts, what kind of choice it is, and the numbered legal options. Every legal option is listed and nothing else is legal."
    )]
    async fn get_decision(&self, p: Parameters<GameParams>) -> CallToolResult {
        self.games
            .with(&p.0.game_id, |g| Self::decision_result(g, &p.0.game_id, ""))
            .unwrap_or_else(Self::fail)
    }

    #[tool(
        name = "act",
        description = "Choose an option of the current decision by number, exact text, or an unambiguous phrase (a #n label or card name, or \"pass\"). The opponent then plays until you must decide again. Returns what happened and your next decision."
    )]
    async fn act(&self, p: Parameters<ActParams>) -> CallToolResult {
        let id = p.0.game_id.clone();
        self.games
            .with(&id, |g| {
                let choice = match g.resolve(&p.0.option) {
                    Ok(c) => c,
                    Err(e) => return Self::fail(e),
                };
                let state = g.engine.state();
                let chosen = match g.decision() {
                    Some(d) => d.options[choice as usize].describe(state),
                    None => String::new(),
                };
                if let Err(e) = g.engine.step(choice) {
                    return Self::fail(e.to_string());
                }
                g.autoplay();
                if g.engine.is_over() {
                    self.games.save(&id, g);
                }
                let viewer = g.viewer(None);
                let events = g.drain_events(viewer);
                let prefix = format!(
                    "You chose: {chosen}.\n{}\n\n",
                    if events.is_empty() {
                        "(no visible events)".to_string()
                    } else {
                        events.join("\n")
                    }
                );
                Self::decision_result(g, &id, &prefix)
            })
            .unwrap_or_else(Self::fail)
    }

    #[tool(
        name = "get_log",
        description = "The game's events since a cursor, as your seat sees them, with the new cursor."
    )]
    async fn get_log(&self, p: Parameters<LogParams>) -> CallToolResult {
        self.games
            .with(&p.0.game_id, |g| {
                let viewer = g.viewer(None);
                let state = g.engine.state();
                let since = p.0.since.unwrap_or(0).min(state.log.len());
                let lines: Vec<String> = state.log[since..]
                    .iter()
                    .filter_map(|e| mtg_core::event_text::describe(state, e, viewer))
                    .collect();
                let mut r = Self::text(format!(
                    "{}\n(cursor {})",
                    lines.join("\n"),
                    state.log.len()
                ));
                r.structured_content = Some(serde_json::json!({
                    "cursor": state.log.len(),
                    "events": lines,
                }));
                r
            })
            .unwrap_or_else(Self::fail)
    }

    #[tool(
        name = "card_info",
        description = "A card's cost, type line, power and toughness, and Oracle text."
    )]
    async fn card_info(&self, p: Parameters<CardParams>) -> CallToolResult {
        use mtg_core::card::{CardType, Supertype};
        let pool = &self.games.pool;
        let Some(r) = pool.lookup(&p.0.name) else {
            return Self::fail(format!("no card named {:?} in the corpus", p.0.name));
        };
        let def = pool.get(r);
        let mut s = def.name.clone();
        let cost = def.mana_cost.to_string();
        if !cost.is_empty() {
            let _ = write!(s, "  {cost}");
        }
        let mut types: Vec<String> = Vec::new();
        for st in [
            Supertype::Basic,
            Supertype::Legendary,
            Supertype::Snow,
            Supertype::World,
        ] {
            if def.supertypes.contains(st) {
                types.push(format!("{st:?}"));
            }
        }
        for t in [
            CardType::Artifact,
            CardType::Battle,
            CardType::Creature,
            CardType::Enchantment,
            CardType::Instant,
            CardType::Kindred,
            CardType::Land,
            CardType::Planeswalker,
            CardType::Sorcery,
        ] {
            if def.types.contains(t) {
                types.push(format!("{t:?}"));
            }
        }
        let _ = write!(s, "\n{}", types.join(" "));
        if !def.subtypes.is_empty() {
            let _ = write!(s, " — {}", def.subtypes.join(" "));
        }
        if let (Some(p), Some(t)) = (def.power, def.toughness) {
            let _ = write!(s, "\n{p}/{t}");
        }
        if def.loyalty > 0 {
            let _ = write!(s, "\nLoyalty {}", def.loyalty);
        }
        if !def.oracle_text.is_empty() {
            let _ = write!(s, "\n{}", def.oracle_text);
        }
        Self::text(s)
    }

    #[tool(
        name = "legal_summary",
        description = "A short summary of what you can do now: how many options of each kind, and the notable ones."
    )]
    async fn legal_summary(&self, p: Parameters<GameParams>) -> CallToolResult {
        self.games
            .with(&p.0.game_id, |g| {
                let Some(d) = g.decision() else {
                    return Self::text(g.decision_text());
                };
                let state = g.engine.state();
                let mut kinds: Vec<(&str, Vec<String>)> = Vec::new();
                for c in &d.options {
                    let kind = kind_of(c);
                    let text = c.describe(state);
                    match kinds.iter_mut().find(|(k, _)| *k == kind) {
                        Some((_, v)) => v.push(text),
                        None => kinds.push((kind, vec![text])),
                    }
                }
                let mut s = format!(
                    "P{} to act ({:?}): {} options.",
                    d.player.0 + 1,
                    d.kind,
                    d.options.len()
                );
                for (k, v) in kinds {
                    if v.len() <= 4 {
                        let _ = write!(s, "\n- {k}: {}", v.join("; "));
                    } else {
                        let _ = write!(
                            s,
                            "\n- {k}: {} options, e.g. {}",
                            v.len(),
                            v[..3].join("; ")
                        );
                    }
                }
                Self::text(s)
            })
            .unwrap_or_else(Self::fail)
    }

    #[tool(
        name = "get_record",
        description = "The game record as JSON (decks, seed, actions so far): enough to replay the game elsewhere."
    )]
    async fn get_record(&self, p: Parameters<GameParams>) -> CallToolResult {
        self.games
            .with(&p.0.game_id, |g| {
                let rec = self.games.record(g);
                match serde_json::to_string(&rec) {
                    Ok(s) => Self::text(s),
                    Err(e) => Self::fail(e.to_string()),
                }
            })
            .unwrap_or_else(Self::fail)
    }

    #[tool(name = "end_game", description = "Forget a game.")]
    async fn end_game(&self, p: Parameters<GameParams>) -> CallToolResult {
        if self.games.remove(&p.0.game_id) {
            Self::text(format!("game {} ended", p.0.game_id))
        } else {
            Self::fail(format!("no game {:?}", p.0.game_id))
        }
    }

    #[tool(
        name = "list_games",
        description = "The ids of the games this server holds."
    )]
    async fn list_games(&self) -> CallToolResult {
        Self::text(self.games.ids().join("\n"))
    }
}

fn kind_of(c: &mtg_core::decision::Choice) -> &'static str {
    use mtg_core::decision::Choice;
    match c {
        Choice::Pass => "pass",
        Choice::PlayLand(_) => "play a land",
        Choice::Cast(_) => "cast",
        Choice::Activate { .. } => "activate",
        Choice::Target(_) | Choice::AttackTarget(_) => "target",
        Choice::Attackers(_) | Choice::AddAttacker(_) => "attack",
        Choice::Blocks(_) | Choice::AddBlock { .. } => "block",
        Choice::Done => "done",
        Choice::Keep | Choice::Mulligan => "mulligan",
        _ => "other",
    }
}

#[tool_handler(
    router = self.tool_router,
    name = "mtg-engine",
    instructions = "You are playing Magic: The Gathering on a rules engine. Start with new_game, then alternate get_decision (or read the decision returned by act) and act. Every option listed is legal and nothing else is; the opponent's turns are played for you between your decisions. Use get_state for the board, card_info for a card's text, and get_log for what happened."
)]
impl rmcp::ServerHandler for MtgServer {
    // Not an `async fn`: the macro's version is one without an `.await`,
    // which the pedantic lints flag, and attributes on this impl do not
    // survive the macro.
    fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<rmcp::model::ListToolsResult, ErrorData>> + Send
    {
        let cache_hints = context
            .protocol_version()
            .is_some_and(|v| v >= rmcp::model::ProtocolVersion::V_2026_07_28);
        std::future::ready(Ok(rmcp::model::ListToolsResult {
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            tools: self.tool_router.list_all(),
            meta: None,
            next_cursor: None,
            ttl_ms: cache_hints.then_some(0),
            cache_scope: cache_hints.then_some(rmcp::model::CacheScope::Public),
        }))
    }
}
