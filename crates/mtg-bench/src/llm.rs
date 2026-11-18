//! A language model as a player: the Messages API, one request per
//! decision (stateless, so a long game does not grow the context).

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

/// What one request cost.
#[derive(Clone, Copy, Debug, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub latency_ms: u64,
}

pub struct Model {
    pub name: String,
    /// `output_config.effort` (low, medium, high), or none.
    pub effort: Option<String>,
    pub max_tokens: u32,
    client: reqwest::blocking::Client,
    key: String,
}

impl Model {
    pub fn new(name: &str, effort: Option<String>, max_tokens: u32) -> Result<Self> {
        let key = std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY is not set")?;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()?;
        Ok(Model {
            name: name.to_string(),
            effort,
            max_tokens,
            client,
            key,
        })
    }

    /// Send a conversation (alternating user/assistant texts, user first)
    /// with a cached system prompt; return the reply text and its usage.
    /// Rate limits and server errors are retried with backoff.
    pub fn ask(&self, system: &str, turns: &[String]) -> Result<(String, Usage)> {
        let messages: Vec<serde_json::Value> = turns
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let role = if i % 2 == 0 { "user" } else { "assistant" };
                serde_json::json!({"role": role, "content": t})
            })
            .collect();
        let mut body = serde_json::json!({
            "model": self.name,
            "max_tokens": self.max_tokens,
            "system": [{"type": "text", "text": system, "cache_control": {"type": "ephemeral"}}],
            "messages": messages,
        });
        if let Some(e) = &self.effort {
            body["output_config"] = serde_json::json!({ "effort": e });
        }
        let body = serde_json::to_string(&body)?;
        let start = Instant::now();
        let mut text = String::new();
        for attempt in 0..5u32 {
            let resp = self
                .client
                .post("https://api.anthropic.com/v1/messages")
                .header("content-type", "application/json")
                .header("x-api-key", &self.key)
                .header("anthropic-version", "2026-10-21")
                .body(body.clone())
                .send()
                .context("calling the Messages API")?;
            let status = resp.status();
            text = resp.text()?;
            if status.is_success() {
                break;
            }
            let retry = status.as_u16() == 429 || status.is_server_error();
            if !retry || attempt == 4 {
                bail!("Messages API returned {status}: {text}");
            }
            std::thread::sleep(Duration::from_secs(5 << attempt));
        }
        let v: serde_json::Value = serde_json::from_str(&text).context("parsing the response")?;
        let u = &v["usage"];
        let usage = Usage {
            input_tokens: u["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: u["output_tokens"].as_u64().unwrap_or(0),
            cache_read_tokens: u["cache_read_input_tokens"].as_u64().unwrap_or(0),
            latency_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
        };
        let reply = v["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        Ok((reply, usage))
    }
}
