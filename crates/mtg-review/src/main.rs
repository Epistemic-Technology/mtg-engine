//! `mtg-review`: import game records into a store and serve the playback
//! UI (design §9.4).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Json};
use axum::routing::get;
use clap::{Parser, Subcommand};
use mtg_core::card::CardPool;
use mtg_core::ids::PlayerId;
use mtg_review::{SnapshotCache, Store};

#[derive(Parser)]
#[command(name = "mtg-review", about = "Game record store and playback server")]
struct Cli {
    /// The store directory (records and index.sqlite).
    #[arg(long, default_value = "data/games")]
    store: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Index the record files (`*.json`) in a directory.
    Import { dir: PathBuf },
    /// Serve the JSON API and the playback UI.
    Serve {
        #[arg(long, default_value = "127.0.0.1:8780")]
        addr: String,
    },
}

struct App {
    store: Store,
    pool: Arc<CardPool>,
    cache: SnapshotCache,
}

const INDEX_HTML: &str = include_str!("../static/index.html");

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn list(
    State(app): State<Arc<App>>,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let filter = q.get("q").cloned().unwrap_or_default();
    let limit = q
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(200usize);
    match app.store.list(&filter, limit) {
        Ok(games) => Json(serde_json::json!({ "games": games })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn game(State(app): State<Arc<App>>, Path(id): Path<String>) -> impl IntoResponse {
    match app.store.get(&id) {
        Ok(Some((meta, rec))) => {
            Json(serde_json::json!({ "meta": meta, "record": rec })).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "no such game").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn steps(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let redact = q
        .get("redact")
        .and_then(|s| s.parse::<u8>().ok())
        .filter(|p| *p < 2)
        .map(PlayerId);
    let rec = match app.store.get(&id) {
        Ok(Some((_, rec))) => rec,
        Ok(None) => return (StatusCode::NOT_FOUND, "no such game").into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let pool = app.pool.clone();
    let built = tokio::task::spawn_blocking(move || {
        app.cache
            .get_or_build(&id, redact, || mtg_review::snapshots(pool, &rec, redact))
    })
    .await;
    match built {
        Ok(Ok(snaps)) => Json(serde_json::json!({ "steps": *snaps })).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let cli = Cli::parse();
    let store = Store::open(&cli.store)?;
    match cli.command {
        Command::Import { dir } => {
            let n = store.import(&dir)?;
            println!("indexed {n} records into {}", cli.store.display());
        }
        Command::Serve { addr } => {
            // Records dropped into the store directory are indexed on start.
            let n = store.import(store.dir())?;
            tracing::info!("{n} records in {}", cli.store.display());
            let app = Arc::new(App {
                store,
                pool: Arc::new(mtg_dsl::builtin_pool()),
                cache: SnapshotCache::default(),
            });
            let router = Router::new()
                .route("/", get(index))
                .route("/favicon.ico", get(|| async { StatusCode::NO_CONTENT }))
                .route("/api/games", get(list))
                .route("/api/games/{id}", get(game))
                .route("/api/games/{id}/steps", get(steps))
                .with_state(app);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            tracing::info!("review UI at http://{addr}/");
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
        }
    }
    Ok(())
}
