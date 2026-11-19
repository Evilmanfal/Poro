# Poro

A lightweight Rust + GPUI desktop app for League of Legends:

- **Champion Matchup**: deeplol.gg's matchup view (deeplol.gg/llm). It shows the selected champion's tier, rank,
  win/pick/ban rate and games; head-to-head win rate and sample size; Weak Against; skills; level 1–6
  advantage and tips; popular builds; and real games of the matchup by pros and high-elo players, each with its full
  rune page, spells, items, skill order and item timeline.
- **Counters**: lolalytics.com's counters list (win rate vs, all-champs win rate, Δ1, Δ2, games), sortable.
  Click a row to open that matchup.
- **Patches**: the live patch plus the 5 before it. deeplol is usually one patch behind; the app says when it
  falls back to deeplol's newest patch.
- **Add runes**: sends a build's rune page (and summoner spells, in champ select) to the League client running on
  this PC through its local API. Pages made by the app are named `Poro: …`, and each new import reuses that slot.
  The app follows champ select by itself: it fills in your champion, lane and lane opponent (click the status
  pill to turn that off).

Icons come from Riot's Data Dragon (champions, items, spells, runes, skills) and CommunityDragon (stat shards).

## Data sources
deeplol.gg and lolalytics.com have no public API. The app calls the same JSON endpoints their own pages use
(`b2c-api-cdn.deeplol.gg`, `a1.lolalytics.com/mega`). These are unofficial and can change without notice.
Every response is cached in `%LOCALAPPDATA%\Poro\cache`. Live data refreshes every 30 minutes; if a site is
unreachable, the app shows the last good copy.

## Icon
`assets/poro.ico` (embedded in the exe) and `assets/poro.png` (the in-app logo) are made by
`./assets/make_icon.ps1 <transparent png>`.

## Build
```powershell
./build.ps1 build -Release   # target/release/poro.exe
```
GPUI 0.2.2 is vendored with Neo's idle-CPU patch (`vendor/gpui/NEO-PATCH.md`).

## Updates
Poro updates itself from this repo's GitHub releases. At start and every 12 hours it checks the latest
release. If that release is newer, Poro offers to update, downloads `poro.exe`, checks its signature and
restarts on the new version. Each release ships `poro.exe` plus `poro.exe.sig`, an Ed25519 signature made
with a private key that exists only on the developer's PC. The matching public key is built into the app
(`src/update.rs`), so a download that doesn't verify is deleted and never run.

## Releasing
1. Bump `version` in `Cargo.toml` and commit.
2. Run `./release.ps1`. It builds, signs with `%APPDATA%\Poro-release\release.key` and puts `poro.exe` and
   `poro.exe.sig` in `dist/`.
3. Publish a GitHub release tagged `v<version>` with both files attached under exactly those names, e.g.
   `gh release create v0.1.1 dist/poro.exe dist/poro.exe.sig --title "Poro v0.1.1" --notes "..."`.

Keep a backup of the release key. Without it, installed copies can't be updated (they would need a manual
download of a build with a new key).
