//! A client plays a whole game through the tools, in process, over a
//! duplex transport: the same JSON-RPC exchange Claude Code would have.

use std::sync::Arc;

use mtg_mcp::{Games, MtgServer};
use rand::{Rng, SeedableRng};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use serde_json::{Value, json};

fn text_of(r: &CallToolResult) -> String {
    r.content
        .iter()
        .filter_map(|c| match c {
            ContentBlock::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn args(v: &Value) -> serde_json::Map<String, Value> {
    v.as_object().cloned().expect("object")
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn a_client_plays_a_game_through_the_tools() -> anyhow::Result<()> {
    let games = Arc::new(Games::new(Arc::new(mtg_dsl::builtin_pool())));
    let (server_io, client_io) = tokio::io::duplex(1 << 16);
    tokio::spawn(async move {
        let service = MtgServer::new(games).serve(server_io).await?;
        service.waiting().await?;
        anyhow::Ok(())
    });
    let client = ().serve(client_io).await?;

    let tools = client.list_tools(None).await?;
    let names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_ref()).collect();
    for want in [
        "new_game",
        "get_state",
        "get_decision",
        "act",
        "get_log",
        "card_info",
        "legal_summary",
    ] {
        assert!(names.contains(&want), "missing tool {want}: {names:?}");
    }

    let r = client
        .call_tool(
            CallToolRequestParams::new("new_game").with_arguments(args(&json!({
                "deck_a": "mono-red", "deck_b": "mono-green", "seed": 7, "opponent": "greedy"
            }))),
        )
        .await?;
    assert_ne!(r.is_error, Some(true), "{}", text_of(&r));
    let info = r.structured_content.clone().expect("structured decision");
    let game_id = info["game_id"].as_str().expect("game id").to_string();
    assert_eq!(info["player"], 0);
    assert!(text_of(&r).contains("You (P1)"));

    let r = client
        .call_tool(
            CallToolRequestParams::new("card_info")
                .with_arguments(args(&json!({"name": "Lightning Bolt"}))),
        )
        .await?;
    assert!(text_of(&r).contains("3 damage"));

    // Play: a random legal option each time, by number, until the game ends.
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(7);
    let mut info = info;
    let mut steps = 0;
    while info["over"] != true && steps < 3000 {
        let n = info["options"].as_array().map_or(0, Vec::len);
        assert!(n > 0, "a running game has options");
        let pick = rng.random_range(1..=n);
        let r = client
            .call_tool(
                CallToolRequestParams::new("act").with_arguments(args(&json!({
                    "game_id": game_id, "option": pick.to_string()
                }))),
            )
            .await?;
        assert_ne!(r.is_error, Some(true), "{}", text_of(&r));
        info = r.structured_content.clone().expect("structured decision");
        steps += 1;
    }
    assert_eq!(info["over"], true, "game did not end in {steps} steps");
    assert!(info["result"].is_string());

    // Textual options resolve too, and bad ones fail cleanly.
    let r = client
        .call_tool(
            CallToolRequestParams::new("new_game").with_arguments(args(&json!({
                "deck_a": "mono-red", "deck_b": "mono-red", "seed": 3, "opponent": "random"
            }))),
        )
        .await?;
    let id2 = r.structured_content.clone().unwrap()["game_id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = client
        .call_tool(
            CallToolRequestParams::new("act").with_arguments(args(&json!({
                "game_id": id2, "option": "keep"
            }))),
        )
        .await?;
    assert_ne!(r.is_error, Some(true), "{}", text_of(&r));
    let r = client
        .call_tool(
            CallToolRequestParams::new("act").with_arguments(args(&json!({
                "game_id": id2, "option": "no such option"
            }))),
        )
        .await?;
    assert_eq!(r.is_error, Some(true));
    assert!(text_of(&r).contains("no option matches"));
    let r = client
        .call_tool(
            CallToolRequestParams::new("get_log")
                .with_arguments(args(&json!({"game_id": id2, "since": 0}))),
        )
        .await?;
    assert!(text_of(&r).contains("Game start"));
    let r = client
        .call_tool(
            CallToolRequestParams::new("get_record").with_arguments(args(&json!({"game_id": id2}))),
        )
        .await?;
    let rec: Value = serde_json::from_str(&text_of(&r))?;
    assert!(rec["actions"].is_array());
    client.cancel().await?;
    Ok(())
}
