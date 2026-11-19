//! Riot's Data Dragon: the official static data for every patch (champion
//! list and IDs, rune trees, summoner spells, icons).

use std::collections::HashMap;

use anyhow::Result;
use serde::Deserialize;

use super::{get_json, LIVE, STATIC};

const CDN: &str = "https://ddragon.leagueoflegends.com/cdn";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lane {
    Top,
    Jungle,
    Middle,
    Bottom,
    Support,
}

impl Lane {
    pub const ALL: [Lane; 5] = [Lane::Top, Lane::Jungle, Lane::Middle, Lane::Bottom, Lane::Support];

    pub fn label(self) -> &'static str {
        match self {
            Lane::Top => "TOP",
            Lane::Jungle => "JUNGLE",
            Lane::Middle => "MID",
            Lane::Bottom => "BOT",
            Lane::Support => "SUPPORT",
        }
    }

    /// Short label for filters ("JUG").
    pub fn short(self) -> &'static str {
        match self {
            Lane::Top => "TOP",
            Lane::Jungle => "JUG",
            Lane::Middle => "MID",
            Lane::Bottom => "BOT",
            Lane::Support => "SUP",
        }
    }

    /// The League client's lane icon (via CommunityDragon).
    pub fn icon(self) -> String {
        let name = match self {
            Lane::Top => "top",
            Lane::Jungle => "jungle",
            Lane::Middle => "middle",
            Lane::Bottom => "bottom",
            Lane::Support => "utility",
        };
        format!("https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-clash/global/default/assets/images/position-selector/positions/icon-position-{name}.png")
    }

    /// deeplol's lane keys.
    pub fn deeplol(self) -> &'static str {
        match self {
            Lane::Top => "Top",
            Lane::Jungle => "Jungle",
            Lane::Middle => "Middle",
            Lane::Bottom => "Bot",
            Lane::Support => "Supporter",
        }
    }

    pub fn from_deeplol(s: &str) -> Option<Lane> {
        Lane::ALL.into_iter().find(|l| l.deeplol() == s)
    }

    /// lolalytics' lane names.
    pub fn lolalytics(self) -> &'static str {
        match self {
            Lane::Top => "top",
            Lane::Jungle => "jungle",
            Lane::Middle => "middle",
            Lane::Bottom => "bottom",
            Lane::Support => "support",
        }
    }

    pub fn from_lolalytics(s: &str) -> Option<Lane> {
        Lane::ALL.into_iter().find(|l| l.lolalytics() == s)
    }

    /// The League client's `assignedPosition` names.
    pub fn from_client(s: &str) -> Option<Lane> {
        match s {
            "top" => Some(Lane::Top),
            "jungle" => Some(Lane::Jungle),
            "middle" => Some(Lane::Middle),
            "bottom" => Some(Lane::Bottom),
            "utility" => Some(Lane::Support),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Champion {
    /// Numeric key (Orianna = 61): what deeplol and the League client use.
    pub key: u32,
    /// Data Dragon id ("MonkeyKing"): used in icon URLs.
    pub id: String,
    /// Display name ("Wukong").
    pub name: String,
    /// "the Lady of Clockwork".
    pub title: String,
    /// Classes: "Mage", "Support", ...
    pub tags: Vec<String>,
}

impl Champion {
    /// lolalytics' URL name: the display name, lowercase letters only.
    pub fn lolalytics_slug(&self) -> String {
        match self.id.as_str() {
            "Nunu" => "nunu".into(),
            "Renata" => "renata".into(),
            _ => self.name.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase(),
        }
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Rune {
    pub id: u32,
    pub name: String,
    pub icon: String,
    pub desc: String,
}

#[derive(Clone, Debug, Default)]
pub struct SummonerSpell {
    /// Data Dragon id ("SummonerFlash").
    pub id: String,
    pub name: String,
    pub desc: String,
    pub cooldown: String,
}

/// A champion ability (passive, Q, W, E, R).
#[derive(Clone, Debug, Default)]
pub struct Skill {
    pub icon: String,
    /// "Passive", "Q", "W", "E" or "R".
    pub slot: &'static str,
    pub name: String,
    pub desc: String,
    pub cooldown: Option<String>,
    pub cost: Option<String>,
}

/// Text for a hover tooltip.
#[derive(Clone, Debug, Default)]
pub struct Tip {
    pub title: String,
    pub subtitle: Option<String>,
    pub body: String,
}

/// Turns Riot's tooltip markup into plain text: <br> becomes a line break,
/// other tags are dropped.
pub fn clean_html(s: &str) -> String {
    let s = s
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</li>", "\n")
        .replace("<li>", "• ");
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"");
    // Tidy spaces and drop repeated blank lines.
    let mut tidy: Vec<String> = Vec::new();
    for l in out.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")) {
        if l.is_empty() && tidy.last().is_none_or(|p| p.is_empty()) {
            continue;
        }
        tidy.push(l);
    }
    while tidy.last().is_some_and(|l| l.is_empty()) {
        tidy.pop();
    }
    tidy.join("\n")
}

#[derive(Clone, Debug, Default)]
pub struct StaticData {
    /// Newest first, e.g. "16.23.1".
    pub versions: Vec<String>,
    pub champions: Vec<Champion>,
    by_key: HashMap<u32, usize>,
    /// Rune trees and runes, by id.
    pub runes: HashMap<u32, Rune>,
    /// Summoner spells by numeric key.
    pub spells: HashMap<u32, SummonerSpell>,
    /// Items by id, with what kind of stats they give.
    pub items: HashMap<u32, ItemInfo>,
}

/// What an item is, for naming builds (AP, AD, Lethality, …).
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct ItemInfo {
    pub name: String,
    pub gold: u32,
    /// One-line summary ("Increases attack speed").
    pub plaintext: String,
    /// Full stats and passives as plain text.
    pub desc: String,
    /// A finished item (not a component, consumable or boots).
    pub legendary: bool,
    /// Finished boots (tier 2 or upgraded).
    pub boots: bool,
    pub ap: bool,
    pub ad: bool,
    pub lethality: bool,
    pub crit: bool,
    pub on_hit: bool,
    pub health: bool,
    /// Armor or magic resist without damage.
    pub tank: bool,
}

impl StaticData {
    pub fn latest(&self) -> &str {
        self.versions.first().map(String::as_str).unwrap_or("16.23.1")
    }

    pub fn champion(&self, key: u32) -> Option<&Champion> {
        self.by_key.get(&key).map(|&i| &self.champions[i])
    }

    pub fn champion_name(&self, key: u32) -> String {
        self.champion(key).map(|c| c.name.clone()).unwrap_or_else(|| format!("#{key}"))
    }

    pub fn champion_icon(&self, key: u32) -> Option<String> {
        self.champion(key).map(|c| format!("{CDN}/{}/img/champion/{}.png", self.latest(), c.id))
    }

    pub fn item_icon(&self, item: u32) -> String {
        format!("{CDN}/{}/img/item/{item}.png", self.latest())
    }

    /// Icon URL for a rune tree, rune or stat shard.
    pub fn rune_icon(&self, id: u32) -> Option<String> {
        self.runes.get(&id).map(|r| r.icon.clone())
    }

    pub fn rune_name(&self, id: u32) -> String {
        self.runes.get(&id).map(|r| r.name.clone()).unwrap_or_else(|| shard_name(id).to_string())
    }

    pub fn spell_icon(&self, key: u32) -> Option<String> {
        self.spells.get(&key).map(|s| format!("{CDN}/{}/img/spell/{}.png", self.latest(), s.id))
    }

    pub fn item_tip(&self, id: u32) -> Option<Tip> {
        let it = self.items.get(&id)?;
        let body = if it.desc.is_empty() { it.plaintext.clone() } else { it.desc.clone() };
        Some(Tip { title: it.name.clone(), subtitle: (it.gold > 0).then(|| format!("{} gold", it.gold)), body })
    }

    pub fn rune_tip(&self, id: u32) -> Option<Tip> {
        let r = self.runes.get(&id)?;
        Some(Tip { title: r.name.clone(), subtitle: None, body: r.desc.clone() })
    }

    pub fn spell_tip(&self, key: u32) -> Option<Tip> {
        let s = self.spells.get(&key)?;
        Some(Tip {
            title: s.name.clone(),
            subtitle: (!s.cooldown.is_empty()).then(|| format!("Cooldown {}s", s.cooldown)),
            body: s.desc.clone(),
        })
    }

    pub fn champion_tip(&self, key: u32) -> Option<Tip> {
        let c = self.champion(key)?;
        Some(Tip { title: c.name.clone(), subtitle: Some(c.tags.join(" · ")), body: capitalize(&c.title) })
    }

    /// Champions whose name starts with (then contains) `query`, ignoring
    /// case, spaces and punctuation.
    pub fn search(&self, query: &str) -> Vec<&Champion> {
        let q: String = query.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let norm = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
        let mut starts: Vec<&Champion> = Vec::new();
        let mut contains: Vec<&Champion> = Vec::new();
        for c in &self.champions {
            let n = norm(&c.name);
            if n.starts_with(&q) {
                starts.push(c);
            } else if n.contains(&q) {
                contains.push(c);
            }
        }
        starts.extend(contains);
        starts
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

impl Skill {
    pub fn tip(&self) -> Tip {
        let mut sub = vec![self.slot.to_string()];
        if let Some(cd) = &self.cooldown {
            sub.push(format!("Cooldown {cd}s"));
        }
        if let Some(cost) = &self.cost {
            sub.push(format!("Cost {cost}"));
        }
        Tip { title: self.name.clone(), subtitle: Some(sub.join(" · ")), body: self.desc.clone() }
    }
}

/// Stat shards aren't in Data Dragon's rune data.
pub fn shard_name(id: u32) -> &'static str {
    match id {
        5001 => "Scaling Health",
        5002 => "Armor",
        5003 => "Magic Resist",
        5005 => "Attack Speed",
        5007 => "Ability Haste",
        5008 => "Adaptive Force",
        5010 => "Move Speed",
        5011 => "Health",
        5013 => "Tenacity & Slow Resist",
        _ => "Shard",
    }
}

#[derive(Deserialize)]
struct ChampionFile {
    data: HashMap<String, ChampionEntry>,
}
#[derive(Deserialize)]
struct ChampionEntry {
    id: String,
    key: String,
    name: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct RuneStyle {
    id: u32,
    name: String,
    icon: String,
    slots: Vec<RuneSlot>,
}
#[derive(Deserialize)]
struct RuneSlot {
    runes: Vec<RuneEntry>,
}
#[derive(Deserialize)]
struct RuneEntry {
    id: u32,
    name: String,
    icon: String,
    #[serde(rename = "longDesc", default)]
    long_desc: String,
}

const CDRAGON: &str = "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default";
const CDRAGON_PERKS: &str =
    "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/perks.json";

#[derive(Deserialize)]
struct CdragonPerk {
    id: u32,
    name: String,
    #[serde(rename = "shortDesc", default)]
    short_desc: String,
    #[serde(rename = "iconPath")]
    icon_path: String,
}

#[derive(Deserialize)]
struct SummonerFile {
    data: HashMap<String, SummonerEntry>,
}
#[derive(Deserialize)]
struct SummonerEntry {
    id: String,
    key: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(rename = "cooldownBurn", default)]
    cooldown_burn: String,
}

pub fn load() -> Result<StaticData> {
    let versions: Vec<String> = get_json("https://ddragon.leagueoflegends.com/api/versions.json", LIVE)?;
    let v = versions.first().cloned().unwrap_or_default();

    let champs: ChampionFile = get_json(&format!("{CDN}/{v}/data/en_US/champion.json"), STATIC)?;
    let mut champions: Vec<Champion> = champs
        .data
        .into_values()
        .filter_map(|c| Some(Champion { key: c.key.parse().ok()?, id: c.id, name: c.name, title: c.title, tags: c.tags }))
        .collect();
    champions.sort_by(|a, b| a.name.cmp(&b.name));
    let by_key = champions.iter().enumerate().map(|(i, c)| (c.key, i)).collect();

    let styles: Vec<RuneStyle> = get_json(&format!("{CDN}/{v}/data/en_US/runesReforged.json"), STATIC)?;
    let mut runes = HashMap::new();
    for s in styles {
        let desc = format!("{} rune tree", s.name);
        runes.insert(s.id, Rune { id: s.id, name: s.name, icon: format!("{CDN}/img/{}", s.icon), desc });
        for slot in s.slots {
            for r in slot.runes {
                runes.insert(r.id, Rune { id: r.id, name: r.name, icon: format!("{CDN}/img/{}", r.icon), desc: clean_html(&r.long_desc) });
            }
        }
    }
    // Stat shard icons only exist in CommunityDragon's copy of the client data.
    if let Ok(perks) = get_json::<Vec<CdragonPerk>>(CDRAGON_PERKS, STATIC) {
        for p in perks.into_iter().filter(|p| (5000..5100).contains(&p.id)) {
            let path = p.icon_path.trim_start_matches("/lol-game-data/assets/").to_lowercase();
            runes.insert(p.id, Rune { id: p.id, name: p.name, icon: format!("{CDRAGON}/{path}"), desc: clean_html(&p.short_desc) });
        }
    }

    let sums: SummonerFile = get_json(&format!("{CDN}/{v}/data/en_US/summoner.json"), STATIC)?;
    let spells = sums
        .data
        .into_values()
        .filter_map(|s| {
            Some((
                s.key.parse().ok()?,
                SummonerSpell { id: s.id, name: s.name, desc: clean_html(&s.description), cooldown: s.cooldown_burn },
            ))
        })
        .collect();

    let items = load_items(&v).unwrap_or_default();

    Ok(StaticData { versions, champions, by_key, runes, spells, items })
}

fn load_items(v: &str) -> Result<HashMap<u32, ItemInfo>> {
    #[derive(Deserialize)]
    struct File {
        data: HashMap<String, Item>,
    }
    #[derive(Deserialize)]
    struct Item {
        name: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        plaintext: String,
        #[serde(default)]
        tags: Vec<String>,
        #[serde(default)]
        stats: HashMap<String, f64>,
        #[serde(default)]
        into: Vec<String>,
        gold: Gold,
        #[serde(default)]
        maps: HashMap<String, bool>,
    }
    #[derive(Deserialize)]
    struct Gold {
        total: u32,
    }
    let file: File = get_json(&format!("{CDN}/{v}/data/en_US/item.json"), STATIC)?;
    let mut out = HashMap::new();
    for (id, it) in file.data {
        let Ok(id) = id.parse::<u32>() else { continue };
        let has = |s: &str| it.stats.get(s).is_some_and(|v| *v > 0.0);
        let tag = |t: &str| it.tags.iter().any(|x| x == t);
        let on_rift = it.maps.get("11").copied().unwrap_or(true);
        let is_boots = tag("Boots");
        let ap = has("FlatMagicDamageMod");
        let ad = has("FlatPhysicalDamageMod");
        out.insert(
            id,
            ItemInfo {
                legendary: on_rift && !is_boots && it.into.is_empty() && it.gold.total >= 2200 && !tag("Consumable"),
                boots: on_rift && is_boots && it.gold.total >= 900,
                ap,
                ad,
                lethality: it.description.contains("Lethality"),
                crit: has("FlatCritChanceMod"),
                on_hit: tag("OnHit") && has("PercentAttackSpeedMod"),
                health: has("FlatHPPoolMod"),
                tank: (has("FlatArmorMod") || has("FlatSpellBlockMod")) && !ap && !ad,
                gold: it.gold.total,
                plaintext: it.plaintext.clone(),
                desc: clean_html(&it.description),
                name: it.name,
            },
        );
    }
    Ok(out)
}

/// A champion's passive and Q/W/E/R, with icons and descriptions.
pub fn skills(data: &StaticData, champ: &Champion) -> Result<Vec<Skill>> {
    #[derive(Deserialize)]
    struct File {
        data: HashMap<String, Detail>,
    }
    #[derive(Deserialize)]
    struct Detail {
        passive: Passive,
        spells: Vec<Spell>,
    }
    #[derive(Deserialize)]
    struct Passive {
        name: String,
        #[serde(default)]
        description: String,
        image: ImgFile,
    }
    #[derive(Deserialize)]
    struct Spell {
        name: String,
        #[serde(default)]
        description: String,
        #[serde(rename = "cooldownBurn", default)]
        cooldown_burn: String,
        #[serde(rename = "costBurn", default)]
        cost_burn: String,
        #[serde(rename = "costType", default)]
        cost_type: String,
        image: ImgFile,
    }
    #[derive(Deserialize)]
    struct ImgFile {
        full: String,
    }
    let v = data.latest();
    let file: File = get_json(&format!("{CDN}/{v}/data/en_US/champion/{}.json", champ.id), STATIC)?;
    let d = file.data.into_values().next().ok_or_else(|| anyhow::anyhow!("no champion data"))?;
    let mut out = vec![Skill {
        icon: format!("{CDN}/{v}/img/passive/{}", d.passive.image.full),
        slot: "Passive",
        name: d.passive.name,
        desc: clean_html(&d.passive.description),
        cooldown: None,
        cost: None,
    }];
    for (spell, slot) in d.spells.into_iter().zip(["Q", "W", "E", "R"]) {
        let cost_type = clean_html(&spell.cost_type);
        let cost = (!spell.cost_burn.is_empty() && spell.cost_burn != "0").then(|| {
            // Data Dragon leaves "{{ abilityresourcename }}" for mana/energy.
            let unit = if cost_type.contains("{{") || cost_type.trim().is_empty() { "Mana" } else { cost_type.trim() };
            format!("{} {unit}", spell.cost_burn)
        });
        out.push(Skill {
            icon: format!("{CDN}/{v}/img/spell/{}", spell.image.full),
            slot,
            name: spell.name,
            desc: clean_html(&spell.description),
            cooldown: (!spell.cooldown_burn.is_empty() && spell.cooldown_burn != "0").then_some(spell.cooldown_burn),
            cost,
        });
    }
    Ok(out)
}
