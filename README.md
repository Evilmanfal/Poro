# Poro

A lightweight League of Legends companion for Windows, written in Rust with GPUI. One portable `poro.exe`:
no installer, no account.

## Features
- **Tier List** (main page): U.GG's live tier list (S+, S, A) with All / Top / Jungle / Mid / ADC / Support
  filters. Click a champion for U.GG's builds (recommended plus AP / AD / Tank / … styles): full rune trees,
  spells, starting items, core item order, 4th / 5th / 6th item options with win rates, and skill order.
- **Champion Matchup**: pick two champions for the head-to-head win rate, level 1–6 advantage and tips,
  Weak Against (from lolalytics), up to three builds for that exact matchup taken from real games (named AP,
  AD, Lethality, Bruiser, …), and the matchup's pro and high-elo games with lane and ALL / OTP / PRO filters.
- **Counters**: lolalytics' counter list for any champion and lane, sortable.
- **Patches**: the live patch and the five before it.
- **Add runes**: one click puts a build's rune page (and summoner spells, in champ select) into your League
  client. During champ select Poro fills in your champion, lane and lane opponent by itself.
- **In-game overlays** (each optional, in Settings): Quick Stats compared with your lane opponent, the next
  skills to level, your item build, and the realized gold difference under every player on the Tab
  scoreboard. They open when a game starts, close when it ends, hide when you alt-tab, and let clicks through.
  Play in Borderless or Windowed mode.
- **Hover descriptions** on every champion, ability, item, rune and summoner spell.
- **Exit into tray** (optional), and **automatic updates** from this repo's releases.

## Download
Grab `poro.exe` from the [latest release](https://github.com/Evilmanfal/Poro/releases/latest) and run it.
It's portable; keep it anywhere. Settings and a data cache live in `%LOCALAPPDATA%\Poro`.

## Data sources
- **U.GG** (tier list and champion builds), **deeplol.gg** (Champion Matchup: stats, builds, games) and **lolalytics.com**
  (counters) have no public API. Poro reads the same JSON their own websites load. These endpoints are
  unofficial and can change without notice. Responses are cached, so if a site is down Poro shows the last
  good copy.
- **Riot's Data Dragon** (champions, items, runes, spells, icons) and **CommunityDragon** (stat shard icons).
- In game, only the game's own **Live Client Data API** (`127.0.0.1:2999`). Poro never reads game memory.
  Rune and spell import uses the League client's local API.

Poro isn't affiliated with U.GG, deeplol.gg or lolalytics.com.

## Updates
At start and every 12 hours Poro checks this repo's latest release. If it's newer, Poro offers to update,
downloads `poro.exe`, checks its signature and restarts on the new version. Each release ships `poro.exe`
plus `poro.exe.sig`, an Ed25519 signature made with a private key that exists only on the developer's PC. The
matching public key is built into the app (`src/update.rs`), so a download that doesn't verify is deleted and
never run.

## Build
```powershell
./build.ps1 build -Release   # target/release/poro.exe
```
Needs Rust (stable) and the Visual Studio C++ build tools. GPUI 0.2.2 is vendored with a few small patches
(`vendor/gpui/NEO-PATCH.md`): idle CPU, and tooltips that show and hide instantly.

## Releasing
1. Bump `version` in `Cargo.toml` and commit.
2. Run `./release.ps1`. It builds, signs with `%APPDATA%\Poro-release\release.key` and puts `poro.exe` and
   `poro.exe.sig` in `dist/`.
3. Publish a GitHub release tagged `v<version>` with both files attached under exactly those names, e.g.
   `gh release create v0.1.1 dist/poro.exe dist/poro.exe.sig --title "Poro v0.1.1" --notes "..."`.

Keep a backup of the release key. Without it, installed copies can't be updated.

## License
MIT (see `LICENSE`). The vendored GPUI is Apache-2.0 (Zed Industries).

Poro isn't endorsed by Riot Games and doesn't reflect the views or opinions of Riot Games or anyone
officially involved in producing or managing Riot Games properties. Riot Games, and all associated properties
are trademarks or registered trademarks of Riot Games, Inc.
