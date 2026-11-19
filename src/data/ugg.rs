//! U.GG's tier list: the same data file U.GG's tier list page loads
//! (stats2.u.gg …/champion_ranking/…), Emerald+ ranked solo, all regions,
//! with U.GG's own tier formula (from its site code): an "effective win
//! rate" from win, pick and ban rate, scored in standard deviations within
//! each role, then S+ ≥ 2, S ≥ 0.75, A ≥ 0, B ≥ −0.5, C ≥ −0.75, else D.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::ddragon::Lane;
use super::{get_json, LIVE, STATIC};

/// Champions under this pick rate (%) are left out, as on U.GG.
pub const MIN_PICK_RATE: f64 = 0.5;

#[derive(Clone, Debug)]
pub struct TierEntry {
    pub champion: u32,
    pub lane: Lane,
    /// Percentages.
    pub win_rate: f64,
    pub pick_rate: f64,
    pub ban_rate: f64,
    pub games: u64,
    /// Standard deviations above the role average.
    pub stdevs: f64,
}

impl TierEntry {
    pub fn tier(&self) -> &'static str {
        tier_of(self.stdevs)
    }
}

pub fn tier_of(stdevs: f64) -> &'static str {
    if stdevs >= 2.0 {
        "S+"
    } else if stdevs >= 0.75 {
        "S"
    } else if stdevs >= 0.0 {
        "A"
    } else if stdevs >= -0.5 {
        "B"
    } else if stdevs >= -0.75 {
        "C"
    } else {
        "D"
    }
}

#[derive(Clone, Debug, Default)]
pub struct TierList {
    /// Every champion in every role it's played in, best first.
    pub entries: Vec<TierEntry>,
    /// U.GG's patch name for the data ("16.23").
    pub patch: String,
}

fn lane_of(role: &str) -> Option<Lane> {
    match role {
        "top" => Some(Lane::Top),
        "jungle" => Some(Lane::Jungle),
        "mid" => Some(Lane::Middle),
        "adc" => Some(Lane::Bottom),
        "supp" => Some(Lane::Support),
        _ => None,
    }
}

/// U.GG's tier list for `patch` ("16.23"), falling back to its newest patch.
/// U.GG's key for `patch` ("16.23" -> "16_23", or its newest patch if it
/// has no data for that one) and the data-format version of `api` there.
fn patch_key(patch: &str, api: &str) -> Result<(String, String)> {
    let versions: Value =
        get_json("https://static.bigbrain.gg/assets/lol/riot_patch_update/prod/ugg/ugg-api-versions.json", LIVE)?;
    let wanted = patch.replace('.', "_");
    let key = if versions.get(&wanted).is_some() {
        wanted
    } else {
        // Newest patch U.GG has.
        let mut keys: Vec<&String> = versions.as_object().map(|o| o.keys().collect()).unwrap_or_default();
        keys.sort_by_key(|k| k.split('_').map(|x| x.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>());
        keys.last().map(|k| k.to_string()).ok_or_else(|| anyhow!("U.GG lists no patches"))?
    };
    let version = versions[&key][api].as_str().unwrap_or("1.5.0").to_string();
    Ok((key, version))
}

pub fn tier_list(patch: &str, live: bool) -> Result<TierList> {
    let (key, api) = patch_key(patch, "champion_ranking")?;
    let url = format!("https://stats2.u.gg/lol/1.5/champion_ranking/world/{key}/ranked_solo_5x5/emerald_plus/{api}.json");
    let d: Value = get_json(&url, if live { LIVE } else { STATIC })?;

    let roles = d.get(0).and_then(Value::as_object).ok_or_else(|| anyhow!("unexpected U.GG data"))?;
    let bans = d.get(1).and_then(Value::as_object).ok_or_else(|| anyhow!("unexpected U.GG data"))?;
    let total = d.get(3).and_then(Value::as_f64).unwrap_or(0.0).round().max(1.0);
    let ban_total = bans.get("total_matches").and_then(Value::as_f64).unwrap_or(1.0).max(1.0);

    let mut by_role: Vec<(Lane, Vec<TierEntry>)> = Vec::new();
    for (role, rows) in roles {
        let Some(lane) = lane_of(role) else { continue };
        let mut list = Vec::new();
        for r in rows.as_array().into_iter().flatten() {
            let champion = r.get(0).and_then(|v| v.as_str()?.parse().ok()).unwrap_or(0);
            let wins = r.get(2).and_then(Value::as_f64).unwrap_or(0.0);
            let games = r.get(3).and_then(Value::as_f64).unwrap_or(0.0);
            if champion == 0 || games <= 0.0 {
                continue;
            }
            let bans = bans.get(&champion.to_string()).and_then(Value::as_f64).unwrap_or(0.0);
            list.push(TierEntry {
                champion,
                lane,
                win_rate: wins / games * 100.0,
                pick_rate: games / total * 100.0,
                ban_rate: bans / ban_total * 100.0,
                games: games as u64,
                stdevs: 0.0,
            });
        }
        by_role.push((lane, list));
    }

    // A champion's ban rate is shared across its roles by pick rate.
    let mut pick_sum: HashMap<u32, f64> = HashMap::new();
    for (_, list) in &by_role {
        for e in list {
            *pick_sum.entry(e.champion).or_default() += e.pick_rate;
        }
    }
    for (_, list) in &mut by_role {
        let effective: Vec<f64> = list
            .iter()
            .map(|e| {
                let (n, l) = (e.pick_rate, e.win_rate);
                let i = n / pick_sum[&e.champion] * e.ban_rate;
                let ban_value = if l >= 50.0 { (l / 100.0 + 0.53) / 2.0 } else { (l / 100.0 + 0.505) / 2.0 };
                (i / 100.0 * ban_value + n / 100.0 * (l / 100.0) + (1.0 - i / 100.0 - n / 100.0) * 0.5) * 100.0
            })
            .collect();
        let mean = effective.iter().sum::<f64>() / effective.len().max(1) as f64;
        let var = effective.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / effective.len().max(1) as f64;
        let sd = var.sqrt().max(1e-9);
        for (e, x) in list.iter_mut().zip(effective) {
            e.stdevs = (x - mean) / sd;
        }
    }

    let mut entries: Vec<TierEntry> = by_role.into_iter().flat_map(|(_, l)| l).collect();
    entries.sort_by(|a, b| b.stdevs.total_cmp(&a.stdevs));
    Ok(TierList { entries, patch: key.replace('_', ".") })
}

#[cfg(test)]
mod tests {
    #[test]
    fn live_champion_builds() {
        let b = super::champion_builds(56, super::Lane::Jungle, "16.23", true).unwrap();
        for x in &b {
            println!(
                "{} {} games, core {:?}, 4th {:?}, 5th {:?}, 6th {:?}",
                x.label,
                x.games,
                x.core,
                x.item_options[0].iter().map(|o| o.item).collect::<Vec<_>>(),
                x.item_options[1].iter().map(|o| o.item).collect::<Vec<_>>(),
                x.item_options[2].iter().map(|o| o.item).collect::<Vec<_>>()
            );
        }
        assert!(!b.is_empty());
    }

    #[test]
    fn live_tier_list() {
        let t = super::tier_list("16.23", true).unwrap();
        let top: Vec<String> = t
            .entries
            .iter()
            .filter(|e| e.pick_rate >= super::MIN_PICK_RATE)
            .take(12)
            .map(|e| format!("{}:{:?} {} {:.1}% {}", e.champion, e.lane, e.tier(), e.win_rate, e.games))
            .collect();
        println!("patch {} {} entries\n{}", t.patch, t.entries.len(), top.join("\n"));
        assert!(t.entries.len() > 100);
    }
}

// ---- Champion builds (U.GG's build page: overview files) ----

/// One option for the 4th/5th/6th item: (item, wins, games).
#[derive(Clone, Debug)]
pub struct ItemOption {
    pub item: u32,
    pub wins: u64,
    pub games: u64,
}

/// A build from U.GG's champion page.
#[derive(Clone, Debug)]
pub struct UggBuild {
    /// "Recommended", or the build style of a style-specific page ("AP", "Tank", …).
    pub label: String,
    pub games: u64,
    pub wins: u64,
    pub primary_style: u32,
    pub sub_style: u32,
    /// The six runes as U.GG lists them (any order) and the three shards.
    pub perks: Vec<u32>,
    pub shards: Vec<u32>,
    pub spells: Vec<u32>,
    pub starter: Vec<u32>,
    pub core: Vec<u32>,
    /// Options for the 4th, 5th and 6th item, most played first.
    pub item_options: [Vec<ItemOption>; 3],
    /// Level 1..18 skill points (1 = Q .. 4 = R) and the max order.
    pub skills: Vec<u32>,
    pub max_order: Vec<u32>,
}

/// U.GG's role numbers.
fn role_id(lane: Lane) -> &'static str {
    match lane {
        Lane::Top => "4",
        Lane::Jungle => "1",
        Lane::Middle => "5",
        Lane::Bottom => "3",
        Lane::Support => "2",
    }
}

/// World (region 12), Emerald+ (rank 17), as the tier list uses.
const WORLD: &str = "12";
const EMERALD_PLUS: &str = "17";

fn skill_id(letter: &str) -> u32 {
    match letter {
        "Q" => 1,
        "W" => 2,
        "E" => 3,
        "R" => 4,
        _ => 0,
    }
}

fn ids(v: &Value) -> Vec<u32> {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| x.as_u64().map(|n| n as u32).or_else(|| x.as_str()?.parse().ok())).collect())
        .unwrap_or_default()
}

/// Parses one champion/role entry of an overview file.
fn parse_build(entry: &Value, label: &str) -> Option<UggBuild> {
    let d = entry.get(0)?;
    let runes = d.get(0)?;
    let opts = d.get(5);
    let option_list = |i: usize| -> Vec<ItemOption> {
        let mut v: Vec<ItemOption> = opts
            .and_then(|o| o.get(i))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        Some(ItemOption {
                            item: x.get(0)?.as_u64()? as u32,
                            wins: x.get(1)?.as_u64()?,
                            games: x.get(2)?.as_u64()?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| b.games.cmp(&a.games));
        v
    };
    let skills = d.get(4)?;
    let totals = d.get(6)?;
    Some(UggBuild {
        label: label.to_string(),
        wins: totals.get(0)?.as_u64()?,
        games: totals.get(1)?.as_u64()?,
        primary_style: runes.get(2)?.as_u64()? as u32,
        sub_style: runes.get(3)?.as_u64()? as u32,
        perks: ids(runes.get(4)?),
        shards: d.get(8).and_then(|x| x.get(2)).map(ids).unwrap_or_default(),
        spells: ids(d.get(1)?.get(2)?),
        starter: ids(d.get(2)?.get(2)?),
        core: ids(d.get(3)?.get(2)?),
        item_options: [option_list(0), option_list(1), option_list(2)],
        skills: skills.get(2)?.as_array()?.iter().filter_map(|x| x.as_str()).map(skill_id).collect(),
        max_order: skills.get(3)?.as_str()?.chars().map(|c| skill_id(&c.to_string())).collect(),
    })
}

/// U.GG's builds for `champ` in `lane`: its recommended build plus each
/// build style it has enough games of (AP, AD, Tank, …), most played first.
pub fn champion_builds(champ: u32, lane: Lane, patch: &str, live: bool) -> Result<Vec<UggBuild>> {
    let (key, api) = patch_key(patch, "overview")?;
    let fresh = if live { LIVE } else { STATIC };
    let load = |page: &str| -> Option<Value> {
        get_json(&format!("https://stats2.u.gg/lol/1.5/{page}/{key}/ranked_solo_5x5/{champ}/{api}.json"), fresh).ok()
    };
    let entry = |v: &Value| v.get(WORLD)?.get(EMERALD_PLUS)?.get(role_id(lane)).cloned();
    let main = load("overview").as_ref().and_then(entry).and_then(|e| parse_build(&e, "Recommended"));
    let Some(main) = main else {
        return Err(anyhow!("U.GG has no {} build for this champion", lane.label()));
    };
    let min_games = (main.games / 20).max(50);
    let pages = [
        ("ad-overview", "AD"),
        ("ap-overview", "AP"),
        ("tank-overview", "Tank"),
        ("lethality-overview", "Lethality"),
        ("crit-overview", "Crit"),
        ("onhit-overview", "On-hit"),
    ];
    let mut styles: Vec<UggBuild> = std::thread::scope(|s| {
        let handles: Vec<_> = pages
            .iter()
            .map(|(page, label)| s.spawn(move || load(page).as_ref().and_then(entry).and_then(|e| parse_build(&e, label))))
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok().flatten()).collect()
    });
    styles.retain(|b| b.games >= min_games && b.core != main.core);
    styles.sort_by(|a, b| b.games.cmp(&a.games));
    let mut out = vec![main];
    out.extend(styles.into_iter().take(3));
    Ok(out)
}
