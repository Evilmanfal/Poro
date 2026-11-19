//! Data sources: Riot's Data Dragon (champion list, icons, runes), deeplol.gg
//! (matchups, builds, pro/high-elo games) and lolalytics.com (counters).
//!
//! deeplol and lolalytics have no public API; these are the JSON endpoints
//! their own pages call. Every response is cached on disk so the app keeps
//! showing the last good data if a site is down or changes its format.

pub mod ddragon;
pub mod deeplol;
pub mod lolalytics;
pub mod matchup_builds;
pub mod ugg;

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{bail, Context as _, Result};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

pub fn client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(15))
            .build()
            .expect("http client")
    })
}

pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let poro = base.join("Poro");
    // Keep the cache from when the app was called Matchup.
    let old = base.join("Matchup");
    if !poro.exists() && old.exists() {
        let _ = std::fs::rename(&old, &poro);
    }
    poro.join("cache")
}

fn cache_path(url: &str) -> PathBuf {
    // FNV-1a: stable across runs, unlike the std hasher.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in url.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    cache_dir().join(format!("{h:016x}"))
}

fn read_cache(url: &str, max_age: Option<Duration>) -> Option<Vec<u8>> {
    let path = cache_path(url);
    if let Some(max_age) = max_age {
        let age = std::fs::metadata(&path).ok()?.modified().ok()?.elapsed().unwrap_or(Duration::MAX);
        if age > max_age {
            return None;
        }
    }
    std::fs::read(path).ok()
}

fn write_cache(url: &str, bytes: &[u8]) {
    let path = cache_path(url);
    let _ = std::fs::create_dir_all(cache_dir());
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(tmp, path);
    }
}

/// GET `url`. A cached copy younger than `fresh_for` is used without asking
/// the network; if the request fails, any cached copy is used instead.
pub fn get_bytes(url: &str, fresh_for: Duration) -> Result<Vec<u8>> {
    if let Some(bytes) = read_cache(url, Some(fresh_for)) {
        return Ok(bytes);
    }
    match fetch(url) {
        Ok(bytes) => {
            write_cache(url, &bytes);
            Ok(bytes)
        }
        Err(e) => read_cache(url, None).ok_or(e),
    }
}

pub(crate) fn fetch(url: &str) -> Result<Vec<u8>> {
    let resp = client().get(url).send().with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        bail!("GET {url}: HTTP {status}");
    }
    Ok(resp.bytes()?.to_vec())
}

pub fn get_json<T: serde::de::DeserializeOwned>(url: &str, fresh_for: Duration) -> Result<T> {
    let bytes = get_bytes(url, fresh_for)?;
    serde_json::from_slice(&bytes).with_context(|| format!("unexpected data from {url}"))
}

/// Live stats: refresh every 30 minutes (the sites update a few times a day).
pub const LIVE: Duration = Duration::from_secs(30 * 60);
/// Old patches and static game data don't change.
pub const STATIC: Duration = Duration::from_secs(30 * 24 * 3600);
