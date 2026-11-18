//! lolalytics.com counters (the endpoint behind lolalytics.com/lol/<champ>/counters/).
//! Ranked solo queue, all regions, Emerald and above.

use anyhow::{bail, Result};
use serde::Deserialize;

use super::ddragon::{Champion, Lane};
use super::{get_json, LIVE, STATIC};

#[derive(Clone, Debug, Deserialize)]
pub struct Counter {
    /// Enemy champion key.
    pub cid: u32,
    /// Our win rate against them.
    #[serde(rename = "vsWr")]
    pub vs_wr: f64,
    /// Games played.
    pub n: u64,
    /// Win rate difference vs our average.
    pub d1: f64,
    /// Win rate difference, also adjusted for the enemy's average.
    pub d2: f64,
    /// Enemy's win rate against everyone.
    #[serde(rename = "allWr")]
    pub all_wr: f64,
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct Counters {
    pub lane: Option<Lane>,
    pub default_lane: Option<Lane>,
    pub analysed: u64,
    pub avg_wr: f64,
    pub win_rate: f64,
    pub pick_rate: f64,
    pub ban_rate: f64,
    pub counters: Vec<Counter>,
}

#[derive(Deserialize)]
struct File {
    stats: Option<Stats>,
    #[serde(default)]
    counters: Vec<Counter>,
    status: Option<u32>,
}

#[derive(Deserialize)]
struct Stats {
    #[serde(rename = "defaultLane")]
    default_lane: Option<String>,
    lane: Option<String>,
    #[serde(default)]
    analysed: u64,
    #[serde(rename = "avgWr", default)]
    avg_wr: f64,
    #[serde(default)]
    br: f64,
    #[serde(default, deserialize_with = "num_or_str")]
    pr: f64,
    #[serde(default, deserialize_with = "num_or_str")]
    wr: f64,
}

fn num_or_str<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(match v {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        serde_json::Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    })
}

/// `champ`'s counters in `lane` against the same lane on `patch` ("16.23").
pub fn counters(champ: &Champion, lane: Lane, patch: &str, live: bool) -> Result<Counters> {
    let url = format!(
        "https://a1.lolalytics.com/mega/?ep=counter&p=d&v=1&patch={patch}&c={}&lane={}&tier=emerald_plus&queue=420&region=all",
        champ.lolalytics_slug(),
        lane.lolalytics()
    );
    let f: File = get_json(&url, if live { LIVE } else { STATIC })?;
    let Some(s) = f.stats else {
        bail!("lolalytics has no data for {} on {patch} (status {:?})", champ.name, f.status);
    };
    let mut counters = f.counters;
    counters.sort_by(|a, b| a.vs_wr.total_cmp(&b.vs_wr));
    Ok(Counters {
        lane: s.lane.as_deref().and_then(Lane::from_lolalytics),
        default_lane: s.default_lane.as_deref().and_then(Lane::from_lolalytics),
        analysed: s.analysed,
        avg_wr: s.avg_wr,
        win_rate: s.wr,
        pick_rate: s.pr,
        ban_rate: s.br,
        counters,
    })
}
