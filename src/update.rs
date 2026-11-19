//! In-app updates from GitHub releases, signed with the developer's key
//! (the same design as Neo's updater).
//!
//! - **Check:** one small HTTPS request to the latest-release API at start and
//!   every 12 hours (`PORO_UPDATE_URL` overrides it for tests). Failures
//!   (offline, rate-limited) are silent.
//! - **Trust:** every release ships `poro.exe` and `poro.exe.sig`, an Ed25519
//!   signature over `"poro-release-v1" || BLAKE3(poro.exe)`. The public key is
//!   built into the app; the private key exists only on the developer's PC
//!   (`%APPDATA%\Poro-release\release.key`, made by `poro-sign`). A download
//!   that doesn't verify is deleted and never run, so a hijacked GitHub
//!   account or a corrupted download can't push a bad build.
//! - **Install:** Windows can rename a running exe but not overwrite it:
//!   `poro.exe` becomes `poro.old.exe`, the verified file takes its place, the
//!   new one starts and the old one quits. The next start deletes
//!   `poro.old.exe`.
//!
//! This file is also compiled into the `poro-sign` tool (src/bin/poro-sign.rs),
//! so it only uses external crates.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use serde_json::Value;

/// Where releases are published.
pub const LATEST: &str = "https://api.github.com/repos/Evilmanfal/Poro/releases/latest";
/// The release signing key (public half), hex.
pub const RELEASE_KEY: &str = "34f8fbe4248a0952f77d899122d36540a9187ea83469ab09774e3172ff613a30";
const CONTEXT: &[u8] = b"poro-release-v1";
const MAX_EXE: u64 = 256 << 20;
pub const EVERY: Duration = Duration::from_secs(12 * 3600);
/// Release asset names.
pub const EXE_ASSET: &str = "poro.exe";
pub const SIG_ASSET: &str = "poro.exe.sig";

/// This build's version.
pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// `v1.2.3` → (1, 2, 3). Pre-releases (`-beta`) and anything odd → None.
pub fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    if v.contains('-') || v.contains('+') {
        return None;
    }
    let mut it = v.split('.').map(|p| p.parse::<u64>().ok());
    let (a, b, c) = (it.next()??, it.next().unwrap_or(Some(0))?, it.next().unwrap_or(Some(0))?);
    if it.next().is_some() {
        return None;
    }
    Some((a, b, c))
}

/// True if `tag` is a release newer than `current`.
pub fn newer(tag: &str, current: &str) -> bool {
    matches!((parse(tag), parse(current)), (Some(t), Some(c)) if t > c)
}

/// A newer release found on GitHub.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    /// e.g. "0.2.0".
    pub version: String,
    /// The first line of the release notes worth showing (may be empty).
    pub notes: String,
    pub exe_url: String,
    pub sig_url: String,
}

fn http() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(concat!("Poro/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(None)
        .build()?)
}

/// The release API URL (overridable for tests).
pub fn endpoint() -> String {
    std::env::var("PORO_UPDATE_URL").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| LATEST.into())
}

/// One check: Some if a newer release with both files exists.
pub fn check(url: &str) -> Result<Option<Release>> {
    let resp = http()?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20))
        .send()?;
    if !resp.status().is_success() {
        bail!("update check: HTTP {}", resp.status());
    }
    let v: Value = resp.json()?;
    let tag = v["tag_name"].as_str().unwrap_or_default();
    if !newer(tag, current()) || v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) {
        return Ok(None);
    }
    let asset = |name: &str| {
        v["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(name))?["browser_download_url"]
            .as_str()
            .map(str::to_string)
    };
    let (Some(exe_url), Some(sig_url)) = (asset(EXE_ASSET), asset(SIG_ASSET)) else {
        return Ok(None); // not a complete, signed release
    };
    Ok(Some(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        notes: summary(v["body"].as_str().unwrap_or_default()),
        exe_url,
        sig_url,
    }))
}

/// A short line from release notes: the first non-heading line, unmarked.
fn summary(body: &str) -> String {
    let line = body
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("---"))
        .unwrap_or_default();
    let line = line.trim_start_matches(['-', '*', ' ']).replace("**", "").replace('`', "");
    if line.chars().count() > 140 {
        format!("{}…", line.chars().take(139).collect::<String>())
    } else {
        line
    }
}

fn message(file: &Path) -> Result<Vec<u8>> {
    let mut h = blake3::Hasher::new();
    h.update_reader(std::fs::File::open(file)?)?;
    let mut m = CONTEXT.to_vec();
    m.extend_from_slice(h.finalize().as_bytes());
    Ok(m)
}

/// Checks `sig` over `file` against a public key (32 bytes).
pub fn verify(file: &Path, sig: &[u8], public_key: &[u8]) -> Result<()> {
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(&message(file)?, sig)
        .map_err(|_| anyhow!("signature doesn't match"))
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The built-in release key (`PORO_UPDATE_KEY` overrides it for tests).
pub fn release_key() -> Vec<u8> {
    let hex = std::env::var("PORO_UPDATE_KEY").unwrap_or_else(|_| RELEASE_KEY.into());
    unhex(&hex).unwrap_or_default()
}

/// The default place for the private release key.
pub fn default_key_path() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("Poro-release").join("release.key")
}

/// Signs `file` with the key at `key_path`, creating the key if needed.
/// Returns (signature, public key).
pub fn sign(file: &Path, key_path: &Path) -> Result<(Vec<u8>, Vec<u8>)> {
    if !key_path.exists() {
        let pkcs8 =
            Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).map_err(|_| anyhow!("can't make a key"))?;
        if let Some(dir) = key_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(key_path, pkcs8.as_ref())?;
    }
    let key = Ed25519KeyPair::from_pkcs8(&std::fs::read(key_path)?)
        .map_err(|_| anyhow!("{} isn't a release key", key_path.display()))?;
    let sig = key.sign(&message(file)?);
    Ok((sig.as_ref().to_vec(), key.public_key().as_ref().to_vec()))
}

fn part_path(exe: &Path) -> PathBuf {
    exe.with_file_name("poro.update.part")
}

/// Downloads and verifies a release next to `exe`; returns the verified file.
/// `progress(done, total)` is called as it goes.
pub fn download(rel: &Release, exe: &Path, public_key: &[u8], progress: impl Fn(u64, u64)) -> Result<PathBuf> {
    let dir = exe.parent().ok_or_else(|| anyhow!("no folder for the update"))?;
    let part = part_path(exe);
    let client = http()?;
    let result: Result<()> = (|| {
        let sig = client.get(&rel.sig_url).timeout(Duration::from_secs(60)).send()?.error_for_status()?.bytes()?;
        if sig.len() != 64 {
            bail!("bad signature file");
        }
        let mut resp = client.get(&rel.exe_url).send()?.error_for_status()?;
        let total = resp.content_length().unwrap_or(0);
        if total > MAX_EXE {
            bail!("the download is unexpectedly large");
        }
        let mut file = std::fs::File::create(&part).with_context(|| format!("can't write to {}", dir.display()))?;
        let mut done = 0u64;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = resp.read(&mut buf)?;
            if n == 0 {
                break;
            }
            done += n as u64;
            if done > MAX_EXE {
                bail!("the download is unexpectedly large");
            }
            file.write_all(&buf[..n])?;
            progress(done, total.max(done));
        }
        file.flush()?;
        drop(file);
        verify(&part, &sig, public_key)
            .map_err(|_| anyhow!("the download didn't pass Poro's signature check, so it wasn't installed"))?;
        Ok(())
    })();
    match result {
        Ok(()) => Ok(part),
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

fn old_path(exe: &Path) -> PathBuf {
    let stem = exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "poro".into());
    exe.with_file_name(format!("{stem}.old.exe"))
}

/// Puts the verified `new` file in place of the running `exe` (which moves
/// to `poro.old.exe`). On failure everything is put back.
pub fn install(new: &Path, exe: &Path) -> Result<()> {
    let old = old_path(exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).with_context(|| format!("can't move {}", exe.display()))?;
    if let Err(e) = std::fs::rename(new, exe) {
        let _ = std::fs::rename(&old, exe);
        return Err(e).context("can't put the update in place");
    }
    Ok(())
}

/// At startup: removes `poro.old.exe` and a stale partial download. The old
/// process may still be exiting, so this retries for a few seconds.
pub fn cleanup(exe: &Path) {
    let (old, part) = (old_path(exe), part_path(exe));
    let _ = std::fs::remove_file(part);
    if !old.exists() {
        return;
    }
    std::thread::spawn(move || {
        for _ in 0..15 {
            if std::fs::remove_file(&old).is_ok() || !old.exists() {
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(newer("v0.2.0", "0.1.0"));
        assert!(newer("v0.10.0", "0.9.9"));
        assert!(newer("1.0", "0.9.12"));
        assert!(!newer("v0.2.0", "0.2.0"));
        assert!(!newer("v0.1.9", "0.2.0"));
        assert!(!newer("v0.3.0-beta", "0.2.0"), "pre-releases aren't offered");
        assert!(!newer("garbage", "0.2.0"));
        assert_eq!(summary("## What's new\n\n- **Faster** builds with `cache`\n- more"), "Faster builds with cache");
    }

    #[test]
    fn signatures_verify_and_tampering_is_caught() {
        let dir = std::env::temp_dir().join(format!("poro-sign-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("poro.exe");
        std::fs::write(&exe, b"pretend this is poro.exe").unwrap();
        let (sig, public) = sign(&exe, &dir.join("key/release.key")).unwrap();
        verify(&exe, &sig, &public).unwrap();
        let (_, again) = sign(&exe, &dir.join("key/release.key")).unwrap();
        assert_eq!(public, again, "the same key is reused");
        std::fs::write(&exe, b"pretend this is malware").unwrap();
        assert!(verify(&exe, &sig, &public).is_err(), "a changed file is refused");
        let (_, other) = sign(&exe, &dir.join("other.key")).unwrap();
        std::fs::write(&exe, b"pretend this is poro.exe").unwrap();
        assert!(verify(&exe, &sig, &other).is_err(), "another key is refused");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_swaps_files_and_cleanup_removes_the_old_one() {
        let dir = std::env::temp_dir().join(format!("poro-swap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (exe, new) = (dir.join("poro.exe"), dir.join("poro.update.part"));
        std::fs::write(&exe, b"v1").unwrap();
        std::fs::write(&new, b"v2").unwrap();
        install(&new, &exe).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"v2");
        assert_eq!(std::fs::read(dir.join("poro.old.exe")).unwrap(), b"v1");
        assert!(!new.exists());
        assert!(install(&dir.join("missing"), &exe).is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), b"v2", "a failed swap puts the original back");
        cleanup(&exe);
        for _ in 0..30 {
            if !dir.join("poro.old.exe").exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(!dir.join("poro.old.exe").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn built_in_key_is_valid() {
        assert_eq!(unhex(RELEASE_KEY).map(|k| k.len()), Some(32));
    }
}
