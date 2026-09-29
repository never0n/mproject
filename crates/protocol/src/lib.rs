//! Shared, versioned protocol. Clients submit intent; only the server sets positions and progress.
pub mod admin;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod actions;
pub mod alliance;
pub mod attributes;
pub mod chat;
pub mod crafting;
pub mod exchange;
pub mod items;
pub mod layout;
pub mod mail;
pub mod monsters;
pub mod party;
pub mod professions;
pub mod progression;
pub mod quests;
pub mod skills;
pub mod trade;
pub mod transfer;
pub mod weapons;
pub const PROTOCOL_VERSION: u32 = 42;
pub const WORLD_VERSION: &str = "mortown-10";
pub const CHARACTER_LIMIT: usize = 12;
pub const TICK_SECONDS: f32 = 0.05;
pub const RUN_SPEED: f32 = 10.;
pub const WALK_SPEED: f32 = 5.;
pub const RACES: [&str; 6] = [
    "Люди",
    "Золотые эльфы",
    "Лесные эльфы",
    "Полуорки",
    "Дварфы",
    "Полумутанты",
];
pub const CLASSES: [&str; 2] = ["Воин", "Маг"];

// Intentionally no Debug: passwords must not appear in diagnostics.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    Alliance {
        command: alliance::Command,
    },
    Party {
        command: party::Command,
    },
    Chat {
        command: chat::Command,
    },
    Exchange {
        command: exchange::Command,
    },
    Mail {
        command: mail::Command,
    },
    Login {
        version: u32,
        world: String,
        login: String,
        password: String,
    },
    Register {
        version: u32,
        world: String,
        login: String,
        password: String,
    },
    CreateCharacter {
        nickname: String,
        race: u8,
        class: u8,
        gender: u8,
    },
    SelectCharacter {
        character_id: i64,
    },
    SetEquipment {
        item: u8,
        equipped: bool,
    },
    Attack {
        target: Option<u32>,
    },
    UseSkill {
        target: u32,
    },
    Trade {
        npc: u8,
        item: u8,
        buy: bool,
        sale: Option<trade::Sale>,
    },
    UsePotion,
    Craft {
        recipe: u8,
        expected: u32,
        specialization_revision: u32,
    },
    Specialize {
        profession: professions::Profession,
        expected: u32,
    },
    SpendAttribute {
        stat: String,
        expected: u32,
    },
    Arrange {
        source: layout::Place,
        target: layout::Place,
        expected: u64,
    },
    BindHotbar {
        slot: u8,
        binding: u8,
    },
    LearnSecondary,
    LearnAbility {
        skill: u8,
    },
    UseAbility {
        skill: u8,
        target: Option<u32>,
    },
    UseSecondary {
        target: u32,
    },
    LearnSkill {
        rank: u8,
    },
    Quest {
        quest: u8,
        complete: bool,
    },
    Respawn,
    LeaveWorld,
    Move {
        seq: u64,
        target: [f32; 2],
        walking: bool,
    },
    /// Ground click while the admin click-teleport mode is enabled. The server rejects it otherwise.
    Teleport {
        target: [f32; 2],
    },
    Logout,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Character {
    pub id: i64,
    pub slot: u8,
    pub nickname: String,
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub stats: BTreeMap<String, f64>,
    pub inventory: Vec<items::Item>,
    #[serde(deserialize_with = "items::deserialize_loot")]
    pub loot: BTreeMap<u8, u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    pub id: i64,
    pub nickname: String,
    pub equipment: items::EquipmentAppearance,
    pub appearance: BodyAppearance,
    pub position: [f32; 3],
    pub yaw: f32,
    pub speed: f32,
    pub hp: f64,
    pub max_hp: f64,
    pub mana: f64,
    pub stamina: f64,
    pub respawn_seconds: u32,
    pub attack_target: Option<u32>,
    #[serde(default)]
    pub actions: Vec<actions::Motion>,
    pub combat_status: String,
    pub skill_cooldown_ms: u32,
    pub secondary_cooldown_ms: u32,
    #[serde(default)]
    pub ability_cooldowns_ms: [u32; skills::EXTRA_COOLDOWNS],
    #[serde(default)]
    pub effects: Vec<skills::Status>,
    pub progress: progression::Progress,
    #[serde(default)]
    pub powers: admin::Powers,
    pub reward_status: String,
    #[serde(deserialize_with = "items::deserialize_loot")]
    pub loot: BTreeMap<u8, u32>,
}
/// Immutable character choices exposed to observers. Inventory and stats remain separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyAppearance {
    pub race: u8,
    pub gender: u8,
}
impl Default for BodyAppearance {
    fn default() -> Self {
        Self { race: 1, gender: 1 }
    }
}
impl From<&Character> for BodyAppearance {
    fn from(character: &Character) -> Self {
        Self {
            race: character.race,
            gender: character.gender,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monster {
    #[serde(default)]
    pub kind: monsters::Kind,
    #[serde(default)]
    pub cast: Option<monsters::Cast>,
    #[serde(default)]
    pub attack: Option<monsters::AttackMotion>,
    /// Age of the current death, so late observers do not replay the fall.
    #[serde(default)]
    pub death_ms: Option<u32>,
    pub level: u32,
    pub mana: Option<(f64, f64)>,
    pub stamina: Option<(f64, f64)>,
    pub id: u32,
    pub name: String,
    pub position: [f32; 3],
    pub yaw: f32,
    pub hp: f64,
    pub max_hp: f64,
    pub slow_ms: u32,
    #[serde(default)]
    pub effects: Vec<skills::Status>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
// Welcome is sent once per entry; keep its small owned payload without another allocation.
#[allow(clippy::large_enum_variant)]
pub enum ServerMessage {
    Alliance {
        reply: alliance::Reply,
    },
    Party {
        reply: party::Reply,
    },
    Chat {
        reply: chat::Reply,
    },
    Exchange {
        reply: exchange::Reply,
    },
    Mail {
        reply: mail::Reply,
    },
    EconomyUpdated {
        character: Character,
        actor: Actor,
    },
    CharacterUpdated {
        character: Character,
    },
    Registered,
    Characters {
        characters: Vec<Character>,
        selected: Option<i64>,
        limit: usize,
    },
    Welcome {
        version: u32,
        world: String,
        character: Character,
        actor: Actor,
    },
    Snapshot {
        tick: u64,
        actors: Vec<Actor>,
        monsters: Vec<Monster>,
        residents: Vec<admin::NpcState>,
        #[serde(default)]
        areas: Vec<skills::Area>,
    },
    Error {
        message: String,
        fatal: bool,
    },
    Saved,
}

pub fn validate_credentials(login: &str, password: &str) -> Result<(), &'static str> {
    if !(3..=16).contains(&login.len())
        || !login
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err("Логин: 3–16 символов, латиница, цифры, _ или -");
    }
    if !(8..=128).contains(&password.len()) || password.chars().any(char::is_control) {
        return Err("Пароль: от 8 до 128 байт, без управляющих символов");
    }
    Ok(())
}
pub fn validate_character(
    nickname: &str,
    race: u8,
    class: u8,
    gender: u8,
) -> Result<(), &'static str> {
    if !(2..=16).contains(&nickname.chars().count())
        || !nickname
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Имя: 2–16 букв, цифр, _ или -");
    }
    if !(1..=6).contains(&race) || !(1..=2).contains(&class) || !(1..=2).contains(&gender) {
        return Err("Выберите расу, класс и пол");
    }
    Ok(())
}

/// Port of server/services/formulas.js::calculateStartingStats, using the original config.
pub fn starting_stats(race: u8, class: u8) -> Option<BTreeMap<String, f64>> {
    starting_stats_with_primary(race, class, &BTreeMap::new())
}

pub fn starting_stats_with_primary(
    race: u8,
    class: u8,
    overrides: &BTreeMap<String, f64>,
) -> Option<BTreeMap<String, f64>> {
    let rules: serde_json::Value =
        serde_json::from_str(include_str!("../data/character_rules.json")).expect("bundled rules");
    let mut primary = rules["baseStats"]
        .get(format!("{race}_{class}"))?
        .as_object()?
        .clone();
    for key in ["STR", "DEX", "CON", "INT_", "WIT", "MEN", "LUC"] {
        if let Some(value) = overrides.get(key) {
            primary.insert(
                if key == "INT_" { "INT" } else { key }.into(),
                serde_json::json!(value),
            );
        }
    }
    let registration = &rules["registration"];
    let mut out = BTreeMap::new();
    for key in [
        "f_attack",
        "f_defence",
        "f_crit_power",
        "f_crit_chance",
        "f_accuracy",
        "f_evasion",
        "f_attack_speed",
        "m_attack",
        "m_defence",
        "m_crit_power",
        "m_crit_chance",
        "m_accuracy",
        "m_evasion",
        "m_attack_speed",
        "speed",
        "initiative",
        "hp",
        "mana",
        "stamina",
    ] {
        out.insert(key.to_owned(), registration[key].as_f64()?);
    }
    for (stat, influences) in rules["statInfluence"].as_object()? {
        let diff = primary[stat].as_f64()? - rules["statNeutral"].as_f64()?;
        for (derived, coefficients) in influences.as_object()? {
            if let Some(value) = out.get_mut(derived) {
                *value += diff
                    * coefficients[if diff > 0. {
                        "bonusPerUnit"
                    } else {
                        "penaltyPerUnit"
                    }]
                    .as_f64()?;
            }
        }
    }
    for (key, value) in &mut out {
        if key.contains("crit_chance") {
            *value = value.clamp(
                rules["crit"]["minChance"].as_f64()?,
                rules["crit"]["maxChance"].as_f64()?,
            );
        }
        let scale = if key.contains("crit_") { 100. } else { 1. };
        *value = (*value * scale + 0.5).floor() / scale;
    }
    for (key, value) in &primary {
        out.insert(
            if key == "INT" {
                "INT_".into()
            } else {
                key.clone()
            },
            value.as_f64()?,
        );
    }
    for key in ["hp", "mana", "stamina"] {
        out.insert(format!("current_{key}"), out[key]);
    }
    for (key, source) in [
        ("current_energy", "energy"),
        ("energy", "energy"),
        ("level", "level"),
        ("current_exp", "currentExp"),
        ("exp", "exp"),
        ("coins", "coins"),
        ("gold", "gold"),
        ("sp", "sp"),
        ("stat_point", "statPoint"),
        ("location", "location"),
        ("sublocation", "sublocation"),
    ] {
        out.insert(key.into(), registration[source].as_f64()?);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_all_twelve_original_js_results() {
        let fixtures: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../data/legacy_starting_stats.json")).unwrap();
        for f in fixtures {
            let actual = starting_stats(
                f["race"].as_u64().unwrap() as u8,
                f["class"].as_u64().unwrap() as u8,
            )
            .unwrap();
            let expected: BTreeMap<String, f64> =
                serde_json::from_value(f["stats"].clone()).unwrap();
            assert_eq!(actual, expected);
        }
        assert!(starting_stats(7, 1).is_none());
    }
    #[test]
    fn wire_rejects_injected_identity_and_invalid_credentials() {
        assert!(
            serde_json::from_str::<ClientMessage>(
                r#"{"type":"move","seq":1,"target":[0,0],"walking":false,"id":9}"#
            )
            .is_err()
        );
        assert!(validate_credentials("abc", "short").is_err());
        assert!(validate_character("Герой", 6, 2, 1).is_ok());
        assert!(validate_character("Герой", 7, 2, 1).is_err());
    }
}
