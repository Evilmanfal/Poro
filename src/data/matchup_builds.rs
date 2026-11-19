//! Builds for one specific matchup, made from deeplol's matchup history
//! (real games of champion A vs champion B): games are grouped by build
//! style (AP, AD, Lethality, …) and keystone, and each group's most common
//! runes, spells, starter, items and skill order become one build.

use std::collections::HashMap;
use std::hash::Hash;

use anyhow::Result;

use super::ddragon::{ItemInfo, StaticData};
use super::deeplol::{self, Build, Game, RunePage};

/// deeplol keeps up to ~80 recent games per matchup (10 per page).
const MAX_PAGES: u32 = 10;

#[derive(Clone, Debug)]
pub struct MatchupBuild {
    /// "Lethality", "AP", "Bruiser", …
    pub style: String,
    pub keystone: u32,
    /// Games using this build (0 for a general deeplol build).
    pub games: usize,
    pub wins: usize,
    pub runes: RunePage,
    pub spells: (u32, u32),
    pub starter: Vec<u32>,
    /// Finished items in buy order.
    pub items: Vec<u32>,
    pub boots: Option<u32>,
    /// Levels 1..15 (1 = Q .. 4 = R).
    pub skills: Vec<u32>,
    /// Filled in from deeplol's general builds (not this matchup's games).
    pub general: Option<GeneralStats>,
}

#[derive(Clone, Debug)]
pub struct GeneralStats {
    pub win_rate: f64,
    pub pick_rate: f64,
    pub games: u64,
}

#[derive(Clone, Debug, Default)]
pub struct MatchupBuilds {
    /// How many matchup games were looked at.
    pub sample: usize,
    pub builds: Vec<MatchupBuild>,
}

/// One of deeplol's game lists, up to MAX_PAGES pages.
fn game_list(champion: u32, enemy: u32, player_type: &'static str) -> Result<Vec<Game>> {
    let pages: Vec<Result<Vec<Game>>> = std::thread::scope(|s| {
        let handles: Vec<_> =
            (1..=MAX_PAGES).map(|p| s.spawn(move || deeplol::games(champion, enemy, p, player_type))).collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Ok(Vec::new()))).collect()
    });
    let mut out = Vec::new();
    for (i, page) in pages.into_iter().enumerate() {
        match page {
            Ok(list) if list.is_empty() => break,
            Ok(list) => out.extend(list),
            // A later page failing just means less data.
            Err(e) if i == 0 => return Err(e),
            Err(_) => break,
        }
    }
    Ok(out)
}

/// Every game deeplol has of `champion` vs `enemy`: its normal list
/// (high-elo players and one-tricks) plus its separate pro players' list,
/// newest first.
pub fn all_games(champion: u32, enemy: u32) -> Result<Vec<Game>> {
    let (normal, pro) = std::thread::scope(|s| {
        let pro = s.spawn(move || game_list(champion, enemy, "Pro"));
        (game_list(champion, enemy, "ALL"), pro.join().unwrap_or_else(|_| Ok(Vec::new())))
    });
    let mut out: Vec<Game> = Vec::new();
    for g in normal?.into_iter().chain(pro.unwrap_or_default()) {
        if !out.iter().any(|o| o.match_id == g.match_id) {
            out.push(g);
        }
    }
    out.sort_by(|a, b| b.game_end_time.cmp(&a.game_end_time));
    // Look up each game's patch, 10 at a time.
    for chunk in out.chunks_mut(10) {
        std::thread::scope(|s| {
            for g in chunk.iter_mut() {
                s.spawn(move || g.patch = deeplol::game_patch(&g.match_id).ok());
            }
        });
    }
    Ok(out)
}

/// "1 on 16.23, 26 on 16.22, 16 on 16.21", newest patch first.
pub fn patch_breakdown(games: &[Game]) -> String {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for g in games {
        let p = g.patch.clone().unwrap_or_else(|| "unknown patch".into());
        match counts.iter_mut().find(|(k, _)| *k == p) {
            Some((_, n)) => *n += 1,
            None => counts.push((p, 1)),
        }
    }
    let key = |p: &str| -> (u32, u32) {
        let mut it = p.split('.').map(|x| x.parse().unwrap_or(0));
        (it.next().unwrap_or(0), it.next().unwrap_or(0))
    };
    counts.sort_by(|a, b| key(&b.0).cmp(&key(&a.0)));
    counts.into_iter().map(|(p, n)| format!("{n} on {p}")).collect::<Vec<_>>().join(", ")
}

/// The finished items of a game in the order they were bought.
fn finished_items(g: &Game, items: &HashMap<u32, ItemInfo>) -> (Vec<u32>, Option<u32>) {
    let mut legendaries = Vec::new();
    let mut boots = None;
    let bought: Vec<u32> = if g.item_seq.is_empty() {
        g.item_final.clone()
    } else {
        g.item_seq.iter().filter(|e| e.kind == "ITEM_PURCHASED").map(|e| e.item_id).collect()
    };
    for id in bought {
        let Some(info) = items.get(&id) else { continue };
        if info.legendary && !legendaries.contains(&id) {
            legendaries.push(id);
        } else if info.boots {
            boots = Some(id);
        }
    }
    (legendaries, boots)
}

/// Names a build from its first few finished items.
pub fn build_style(items: &[u32], data: &HashMap<u32, ItemInfo>) -> String {
    let core: Vec<&ItemInfo> = items.iter().take(4).filter_map(|i| data.get(i)).collect();
    if core.is_empty() {
        return "Unknown".into();
    }
    let n = |f: fn(&ItemInfo) -> bool| core.iter().filter(|i| f(i)).count();
    let (ap, ad, leth, crit, on_hit, health, tank) = (
        n(|i| i.ap),
        n(|i| i.ad),
        n(|i| i.lethality),
        n(|i| i.crit),
        n(|i| i.on_hit),
        n(|i| i.health),
        n(|i| i.tank),
    );
    let half = core.len().div_ceil(2);
    if tank >= half && ap + ad <= 1 {
        "Tank".into()
    } else if ap > 0 && ad > 0 && ap.abs_diff(ad) <= 1 && ap + ad >= 3 {
        "Hybrid".into()
    } else if ap >= ad && ap > 0 {
        if health >= half {
            "AP Bruiser".into()
        } else {
            "AP".into()
        }
    } else if ad > 0 {
        if leth >= 2 {
            "Lethality".into()
        } else if crit >= 2 {
            "Crit".into()
        } else if on_hit >= 2 {
            "On-hit".into()
        } else if health >= half {
            "Bruiser".into()
        } else {
            "AD".into()
        }
    } else if tank > 0 || health > 0 {
        "Tank".into()
    } else {
        "Utility".into()
    }
}

fn most_common<T: Eq + Hash + Clone>(values: impl IntoIterator<Item = T>) -> Option<T> {
    let mut counts: HashMap<T, (usize, usize)> = HashMap::new();
    for (i, v) in values.into_iter().enumerate() {
        let e = counts.entry(v).or_insert((0, i));
        e.0 += 1;
    }
    // Most often; ties go to the one seen first (the newest game).
    counts.into_iter().max_by(|a, b| a.1 .0.cmp(&b.1 .0).then(b.1 .1.cmp(&a.1 .1))).map(|(v, _)| v)
}

/// Groups the games into at most `max` builds, most played first. If there
/// are fewer distinct builds than `max`, deeplol's general builds for the
/// champion fill the remaining slots.
pub fn summarize(games: &[Game], data: &StaticData, general: &[Build], max: usize) -> MatchupBuilds {
    struct Entry<'a> {
        game: &'a Game,
        items: Vec<u32>,
        boots: Option<u32>,
    }
    let mut groups: Vec<((String, u32), Vec<Entry>)> = Vec::new();
    for g in games {
        let (items, boots) = finished_items(g, &data.items);
        // No finished item (a remake or an early surrender): nothing to
        // learn from, and the build can't be named.
        if items.is_empty() {
            continue;
        }
        let key = (build_style(&items, &data.items), g.rune.perk_0);
        let entry = Entry { game: g, items, boots };
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(entry),
            None => groups.push((key, vec![entry])),
        }
    }
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    let mut builds: Vec<MatchupBuild> = groups
        .into_iter()
        .take(max)
        .map(|((style, keystone), entries)| {
            let runes = most_common(entries.iter().map(|e| e.game.runes())).unwrap();
            let spells = most_common(entries.iter().map(|e| {
                let (a, b) = (e.game.spell.spell_1, e.game.spell.spell_2);
                (a.min(b), a.max(b))
            }))
            .unwrap();
            // Keep the more common Flash side.
            let spells = most_common(
                entries
                    .iter()
                    .map(|e| (e.game.spell.spell_1, e.game.spell.spell_2))
                    .filter(|&(a, b)| (a.min(b), a.max(b)) == spells),
            )
            .unwrap_or(spells);
            let starter = most_common(entries.iter().filter(|e| !e.game.item_start.is_empty()).map(|e| {
                let mut s = e.game.item_start.clone();
                s.sort();
                s
            }))
            .unwrap_or_default();
            let boots = most_common(entries.iter().filter_map(|e| e.boots));
            // Items: the most bought finished items, in their usual buy order.
            let mut item_stats: HashMap<u32, (usize, usize)> = HashMap::new();
            for e in &entries {
                for (pos, &it) in e.items.iter().enumerate() {
                    let s = item_stats.entry(it).or_insert((0, 0));
                    s.0 += 1;
                    s.1 += pos;
                }
            }
            let mut ranked: Vec<(u32, usize, f64)> =
                item_stats.into_iter().map(|(it, (n, pos))| (it, n, pos as f64 / n as f64)).collect();
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.total_cmp(&b.2)));
            let mut items: Vec<(u32, f64)> = ranked.iter().take(5).map(|(it, _, pos)| (*it, *pos)).collect();
            items.sort_by(|a, b| a.1.total_cmp(&b.1));
            let skills = most_common(entries.iter().map(|e| e.game.skill_order().into_iter().take(15).collect::<Vec<_>>()))
                .unwrap_or_default();
            MatchupBuild {
                style,
                keystone,
                games: entries.len(),
                wins: entries.iter().filter(|e| e.game.win == 1).count(),
                runes,
                spells,
                starter,
                items: items.into_iter().map(|(it, _)| it).collect(),
                boots,
                skills,
                general: None,
            }
        })
        .collect();

    // Not enough distinct builds in this matchup: fill up with deeplol's
    // general builds for the champion, skipping ones already shown.
    for b in general {
        if builds.len() >= max {
            break;
        }
        let Some(runes) = b.runes() else { continue };
        let all_items: Vec<u32> = b.item.build.clone();
        let boots = all_items.iter().copied().find(|i| data.items.get(i).is_some_and(|x| x.boots));
        let items: Vec<u32> = all_items.iter().copied().filter(|i| data.items.get(i).is_some_and(|x| x.legendary)).collect();
        let style = build_style(&items, &data.items);
        if items.is_empty() || style == "Unknown" || builds.iter().any(|m| m.style == style && m.keystone == runes.perks[0]) {
            continue;
        }
        let spells = match b.spell.build.as_slice() {
            [a, c, ..] => (*a, *c),
            _ => (4, 14),
        };
        builds.push(MatchupBuild {
            style,
            keystone: runes.perks[0],
            games: 0,
            wins: 0,
            runes,
            spells,
            starter: b.start_item.build.clone(),
            items,
            boots,
            skills: b.skill.detail.iter().take(15).copied().collect(),
            general: Some(GeneralStats { win_rate: b.win_rate, pick_rate: b.pick_rate, games: b.games }),
        });
    }

    MatchupBuilds { sample: games.len(), builds }
}

#[cfg(test)]
mod tests {
    #[test]
    fn games_without_finished_items_make_no_build() {
        let data = super::super::ddragon::load().unwrap();
        let games = super::all_games(56, 7).unwrap();
        let s = super::summarize(&games, &data, &[], 3);
        assert!(s.builds.iter().all(|b| !b.items.is_empty() && b.style != "Unknown"));
    }

    #[test]
    fn live_matchup_builds() {
        let data = super::super::ddragon::load().unwrap();
        for (a, b, lane) in [(56, 64, super::super::ddragon::Lane::Jungle), (61, 7, super::super::ddragon::Lane::Middle)] {
            let games = super::all_games(a, b).unwrap();
            let general = super::deeplol::champion(a, "16.22", true).unwrap().remove(&lane).unwrap().build_lst;
            let s = super::summarize(&games, &data, &general, 3);
            println!("{} vs {}: {} games", data.champion_name(a), data.champion_name(b), s.sample);
            for m in &s.builds {
                let names: Vec<String> = m.items.iter().map(|i| data.items[i].name.clone()).collect();
                println!(
                    "  {} · {} — {}/{} games {:?} items {:?}",
                    m.style,
                    data.rune_name(m.keystone),
                    m.wins,
                    m.games,
                    m.general.as_ref().map(|g| g.games),
                    names
                );
            }
        }
    }
}
