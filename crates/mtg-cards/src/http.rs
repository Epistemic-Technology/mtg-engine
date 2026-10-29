//! Minimal HTTP client with the headers Scryfall requires and streaming
//! download with SHA-256.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

pub const USER_AGENT: &str = concat!(
    "mtg-engine/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/Epistemic-Technology/mtg-engine)"
);

pub struct Client {
    inner: reqwest::blocking::Client,
}

/// Result of a streamed download.
#[derive(Debug, Clone)]
pub struct Downloaded {
    pub sha256: String,
    pub size: u64,
}

impl Client {
    pub fn new() -> Result<Self> {
        let inner = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .context("building HTTP client")?;
        Ok(Self { inner })
    }

    /// GET a URL and return the body as a string. Sets `Accept: application/json`.
    pub fn get_text(&self, url: &str) -> Result<String> {
        let resp = self
            .inner
            .get(url)
            .header("Accept", "application/json")
            .send()
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("GET {url}"))?;
        resp.text()
            .with_context(|| format!("reading body of {url}"))
    }

    /// GET a URL, streaming the body to `dest` (written atomically via a
    /// temporary file) while computing its SHA-256.
    pub fn download(&self, url: &str, dest: &Path) -> Result<Downloaded> {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let tmp = dest.with_extension("part");
        let mut resp = self
            .inner
            .get(url)
            .send()
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("GET {url}"))?;
        let mut file = File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1 << 16];
        let mut size: u64 = 0;
        loop {
            let n = resp
                .read(&mut buf)
                .with_context(|| format!("reading {url}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n])
                .with_context(|| format!("writing {}", tmp.display()))?;
            size += n as u64;
        }
        file.flush()?;
        drop(file);
        fs::rename(&tmp, dest)
            .with_context(|| format!("moving {} to {}", tmp.display(), dest.display()))?;
        Ok(Downloaded {
            sha256: hex::encode(hasher.finalize()),
            size,
        })
    }
}

/// SHA-256 of a file on disk, as lowercase hex, plus its size.
pub fn sha256_file(path: &Path) -> Result<Downloaded> {
    let mut file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut size: u64 = 0;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok(Downloaded {
        sha256: hex::encode(hasher.finalize()),
        size,
    })
}
