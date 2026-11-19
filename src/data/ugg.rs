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
pub fn tier_list(patch: &str, live: bool) -> Result<TierList> {
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
    let api = versions[&key]["champion_ranking"].as_str().unwrap_or("1.5.0").to_string();
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
