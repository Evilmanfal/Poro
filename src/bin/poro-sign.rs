//! Signs a Poro release for in-app updates.
//!
//!   poro-sign [--key FILE] target/release/poro.exe
//!
//! Writes `poro.exe.sig` next to the exe. Upload both to the GitHub release.
//! The private key lives in `%APPDATA%\Poro-release\release.key` (created on
//! first use; keep it private, back it up, never commit it). Losing it means
//! installed copies can no longer be updated. It prints the public key,
//! which must match `update::RELEASE_KEY` in the app.

#[path = "../update.rs"]
mod update;

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut key = update::default_key_path();
    let mut exe = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--key" => key = args.next().map(PathBuf::from).unwrap_or(key),
            _ => exe = Some(PathBuf::from(a)),
        }
    }
    let exe = exe.ok_or_else(|| anyhow::anyhow!("usage: poro-sign [--key FILE] poro.exe"))?;
    let new_key = !key.exists();
    let (sig, public) = update::sign(&exe, &key)?;
    let mut out = exe.clone().into_os_string();
    out.push(".sig");
    std::fs::write(&out, &sig)?;
    let hex = update::hex(&public);
    if new_key {
        println!("Created a new release key at {} (keep it private, back it up, never commit it).", key.display());
    }
    println!("Signed {} -> {}", exe.display(), PathBuf::from(&out).display());
    println!("Public key: {hex}");
    if hex != update::RELEASE_KEY {
        println!("WARNING: this isn't the key built into the app (update::RELEASE_KEY); installed copies would refuse this release.");
        std::process::exit(2);
    }
    Ok(())
}
