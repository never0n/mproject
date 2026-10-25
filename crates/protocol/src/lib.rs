//! Shared, versioned protocol. Clients submit intent; only the server sets positions and progress.
//! Messages are JSON over WebSocket: `ClientMessage` (client → server) and `ServerMessage` (back).
//! Submodules hold game rules both sides must agree on (items, skills, layout, chat…).
//! Changing a message means updating client `network.rs`, server `lib.rs` and usually `PROTOCOL_VERSION`.
pub mod admin;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod actions;
pub mod alliance;
pub mod attributes;
pub mod casting;
pub mod chat;
pub mod crafting;
pub mod exchange;
pub mod items;
pub mod layout;
pub mod legacy;
pub mod life;
pub mod monsters;
pub mod objects;
pub mod party;
pub mod progression;
pub mod skills;
pub mod society;
pub mod transfer;
pub mod weapons;
pub mod work;
/// Bump on any incompatible message change; client and server refuse to play on mismatch.
pub const PROTOCOL_VERSION: u32 = 58;
/// Bump when world geometry/collision changes, so old clients can't desync movement.
pub const WORLD_VERSION: &str = "mortown-10";
/// One sentence for a protocol or world mismatch, on the login refusal and in the client.
pub const VERSION_MISMATCH: &str = "Обновите клиент: версия мира не совпадает";
/// Max characters per account.
pub const CHARACTER_LIMIT: usize = 12;
/// Server simulation step (20 Hz). Must match the 50 ms interval in server `Server::new`.
pub const TICK_SECONDS: f32 = 0.05;
/// Authoritative movement speeds in m/s, applied by server `simulation::advance`.
pub const RUN_SPEED: f32 = 10.;
pub const WALK_SPEED: f32 = 5.;
/// Everything a client may ask. JSON form: `{"type":"move","seq":1,...}`.
/// `deny_unknown_fields` rejects injected extras such as a forged player id.
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
    /// Claimed name, private note, covered face. None of these certify identity.
    Society {
        command: society::Command,
    },
    /// Proximity-voice prototype. The server relays it only inside earshot and a clear segment.
    Voice {
        speaking: bool,
    },
    Exchange {
        command: exchange::Command,
    },
    /// Ground stacks and containers: drop, pick up, place a chest, open, put, take.
    Objects {
        command: objects::Command,
    },
    /// First message of a connection (as is `Register`); versions are checked before credentials.
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
        gender: u8,
    },
    SelectCharacter {
        character_id: i64,
    },
    // Handled by the server, but the current client equips through `Arrange` instead.
    SetEquipment {
        item: u8,
        equipped: bool,
    },
    /// Start auto-attack on a monster id; `None` stops attacking.
    Attack {
        target: Option<u32>,
    },
    UseSkill {
        target: u32,
    },
    UsePotion,
    // `expected`/revision fields carry the client's view of state; the server rejects the
    // request if that state changed meanwhile (stale UI, double click).
    Craft {
        recipe: u8,
        expected: u32,
    },
    SpendAttribute {
        stat: String,
        expected: u32,
    },
    /// Spend one talent point on a direction. `expected` is the rank the client saw.
    SpendTalent {
        talent: String,
        expected: u32,
    },
    /// Replace the offline orders. Any combination of the three is allowed.
    OfflineOrders {
        returns: bool,
        guards: bool,
        works: bool,
    },
    /// Gathering, a blade plan, a plot, food or a player stall.
    Work {
        command: work::Command,
    },
    /// Draw a cavity, charge bronze, heat and pour. The reply is `ServerMessage::Cast`.
    Cast {
        command: casting::Command,
    },
    /// Move an item between bag/hotbar/equipment/character model; also how equipping works.
    Arrange {
        source: layout::Place,
        target: layout::Place,
        expected: u64,
    },
    BindHotbar {
        slot: u8,
        binding: u8,
    },
    UseAbility {
        skill: u8,
        target: Option<u32>,
    },
    UseSecondary {
        target: u32,
    },
    Respawn,
    LeaveWorld,
    /// "Walk toward `target` [x, z]". `seq` must increase; the server moves you and stops you
    /// if no `Move` arrives for 10 ticks.
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
/// Persistent hero sheet stored in the DB. `stats` is a free-form map ("level", "hp"…);
/// `items` are the stacks the character carries: bag, body and purse, ordered by id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Character {
    pub id: i64,
    pub slot: u8,
    pub nickname: String,
    pub gender: u8,
    pub stats: BTreeMap<String, f64>,
    pub items: Vec<items::Stack>,
}
impl Character {
    /// Whether the equipped weapon makes attacks and skills magical (see `weapons::magical`).
    pub fn magical(&self) -> bool {
        weapons::magical(items::EquipmentAppearance::from_items(&self.items).weapon)
    }
}
/// Live in-world state of one player, produced by the server simulation and sent every tick.
/// Positions are meters, `[x, y, z]` with Y up; `yaw` in radians.
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
    /// Server's last combat result text, shown in the combat panel.
    pub combat_status: String,
    pub skill_cooldown_ms: u32,
    pub secondary_cooldown_ms: u32,
    #[serde(default)]
    pub ability_cooldowns_ms: [u32; skills::EXTRA_COOLDOWNS],
    #[serde(default)]
    pub effects: Vec<skills::Status>,
    pub progress: progression::Progress,
    /// Admin toggles (invisible, click teleport…); the server blanks them for other players.
    #[serde(default)]
    pub powers: admin::Powers,
    pub reward_status: String,
    /// Knocked down at 0 HP. A further hit is a finishing blow; time raises the body.
    #[serde(default)]
    pub downed: bool,
    /// Bound body: no actions, and the offline bot stays still. Login does not clear it.
    #[serde(default)]
    pub bound: bool,
    /// The player is gone; this is the same body under offline orders.
    #[serde(default)]
    pub offline: bool,
    #[serde(default = "default_lives")]
    pub lives: u32,
    /// Seconds until a downed body rises, or until a new identity may leave the return area.
    #[serde(default)]
    pub return_seconds: u32,
    /// Incremented once per finishing blow. The database applies each value one time.
    #[serde(default)]
    pub death_mark: u64,
    #[serde(default)]
    pub pending_death: bool,
    #[serde(default)]
    pub orders: life::Orders,
    /// Where an offline "work" order stands.
    #[serde(default)]
    pub post: [f32; 2],
    /// Unix milliseconds. While this is in the future the body cannot leave the return area.
    #[serde(default)]
    pub return_until: u64,
    /// 0 is sated, 100 is starving. [PROPOSAL] rate; hunger and thirst can kill.
    #[serde(default)]
    pub hunger: f64,
    #[serde(default)]
    pub thirst: f64,
    /// Name the character claims. Empty until they introduce themselves.
    #[serde(default)]
    pub presented: String,
    /// Covered face: observers lose the automatic recognition hint.
    #[serde(default)]
    pub face_hidden: bool,
    /// Current identity counter. Not sent: a stranger does not need it.
    #[serde(skip)]
    pub identity: f64,
}
/// Drop hidden numbers and replace the name before a stranger's snapshot is sent.
pub fn conceal(actor: &mut Actor, label: String) {
    actor.nickname = label;
    actor.mana = 0.;
    actor.stamina = 0.;
    actor.combat_status.clear();
    actor.reward_status.clear();
    actor.hunger = 0.;
    actor.thirst = 0.;
    actor.lives = 0;
    actor.orders = Default::default();
    actor.death_mark = 0;
    actor.return_until = 0;
    actor.pending_death = false;
    actor.progress.experience = 0.;
    actor.progress.stat_points = 0.;
    actor.progress.talent_points = 0.;
    actor.progress.kills = 0;
    actor.progress.skill_rank = 0;
    actor.progress.practice = [0; 5];
    actor.progress.primary_disabled = false;
    actor.progress.admin_revision = 0;
    actor.progress.secondary_learned = false;
    actor.progress.extra_skills = 0;
    actor.powers = Default::default();
}
fn default_lives() -> u32 {
    life::lives()
}
/// Immutable character choices exposed to observers. Inventory and stats remain separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyAppearance {
    pub gender: u8,
}
impl Default for BodyAppearance {
    fn default() -> Self {
        Self { gender: 1 }
    }
}
impl From<&Character> for BodyAppearance {
    fn from(character: &Character) -> Self {
        Self {
            gender: character.gender,
        }
    }
}
/// What clients see of a monster; the full server-side state is `server::combat::Mob`.
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
/// Everything the server sends. Handled on the client in `network::receive`.
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
    /// Someone nearby is speaking. `label` is not the account nickname.
    Hearing {
        speaker: i64,
        label: String,
    },
    Exchange {
        reply: exchange::Reply,
    },
    /// Containers and ground stacks near the character, resent when anything nearby changes.
    Objects {
        containers: Vec<objects::Container>,
        ground: Vec<objects::Loose>,
        #[serde(default)]
        nodes: Vec<work::Node>,
        #[serde(default)]
        plots: Vec<work::Plot>,
        #[serde(default)]
        blocks: Vec<work::Block>,
        /// Filled cells of castings lying nearby. Empty spans are absent from the mesh.
        #[serde(default)]
        castings: Vec<objects::CastingBody>,
    },
    /// Result of an `Objects` command: the open container (if any) and the committed inventory.
    Object {
        reply: objects::Reply,
    },
    /// The foundry session after a `Cast` command.
    Cast {
        view: casting::View,
    },
    /// Result of a committed inventory/stat transaction (craft, potion, arrange, admin edit).
    EconomyUpdated {
        character: Character,
        actor: Actor,
    },
    /// Reply to `SetEquipment`.
    CharacterUpdated {
        character: Character,
    },
    /// Account created; the server then closes the connection and the player logs in.
    Registered,
    /// Character selection list (after login or `LeaveWorld`).
    Characters {
        characters: Vec<Character>,
        selected: Option<i64>,
        limit: usize,
    },
    /// Entered the world with this character; snapshots follow.
    Welcome {
        version: u32,
        world: String,
        character: Character,
        actor: Actor,
    },
    /// Full world state, published every tick (each connection only gets the latest one).
    Snapshot {
        tick: u64,
        actors: Vec<Actor>,
        monsters: Vec<Monster>,
        #[serde(default)]
        areas: Vec<skills::Area>,
    },
    /// Rejected request; `fatal` makes the client disconnect.
    Error {
        message: String,
        fatal: bool,
    },
    /// Character saved after logout; the connection ends.
    Saved,
}

/// Shared validation: the client rejects bad input early, the server checks again.
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
pub fn validate_character(nickname: &str, gender: u8) -> Result<(), &'static str> {
    if !(2..=16).contains(&nickname.chars().count())
        || !nickname
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Имя: 2–16 букв, цифр, _ или -");
    }
    if !(1..=2).contains(&gender) {
        return Err("Выберите пол");
    }
    Ok(())
}

/// Level-one sheet of a new human: one physiological base plus unspent creation points
/// (`registration.statPoint`), using the formulas of server/services/formulas.js.
pub fn starting_stats() -> Option<BTreeMap<String, f64>> {
    starting_stats_with_primary(&BTreeMap::new())
}

/// Copper value of the coins a new human starts with, placed in the purse at creation.
pub fn starting_coins() -> u64 {
    let rules: serde_json::Value =
        serde_json::from_str(include_str!("../data/character_rules.json")).expect("bundled rules");
    let value = |key: &str| rules["registration"][key].as_u64().unwrap_or(0);
    value("coins") + value("gold") * 10_000
}

/// Like `starting_stats`, but `overrides` replaces primary attributes (STR…LUC) before
/// derived combat stats are computed. Used when attribute points are spent.
pub fn starting_stats_with_primary(
    overrides: &BTreeMap<String, f64>,
) -> Option<BTreeMap<String, f64>> {
    let rules: serde_json::Value =
        serde_json::from_str(include_str!("../data/character_rules.json")).expect("bundled rules");
    let mut primary = rules["humanBase"].as_object()?.clone();
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
    // Each primary attribute above/below `statNeutral` adds a bonus/penalty per point to derived stats.
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
        // Round half up: crit values to 0.01, everything else to whole numbers (matches the JS original).
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
        ("stat_point", "statPoint"),
        ("talent_point", "talentPoint"),
        ("lives", "lives"),
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
    fn every_human_starts_from_one_base_with_free_points() {
        let stats = starting_stats().unwrap();
        for (key, _) in attributes::ATTRIBUTES {
            assert_eq!(stats[key], 10.);
        }
        assert_eq!(stats["LUC"], 10.);
        assert_eq!(stats["stat_point"], 5.);
        // Coins are physical purse items, not a stat.
        assert!(
            !stats.contains_key("coins")
                && !stats.contains_key("gold")
                && !stats.contains_key("sp")
        );
        assert_eq!(starting_coins(), 500);
        for (key, value) in [("hp", 100.), ("f_attack", 20.), ("m_attack", 20.)] {
            assert_eq!(stats[key], value);
        }
        let mut stronger = BTreeMap::new();
        stronger.insert("STR".into(), 12.);
        assert_eq!(
            starting_stats_with_primary(&stronger).unwrap()["f_attack"],
            24.
        );
        assert!(validate_character("Мирэль", 2).is_ok());
        assert!(validate_character("Мирэль", 3).is_err());
    }
    #[test]
    fn wire_rejects_injected_identity_and_invalid_credentials() {
        assert!(
            serde_json::from_str::<ClientMessage>(
                r#"{"type":"move","seq":1,"target":[0,0],"walking":false,"id":9}"#
            )
            .is_err()
        );
        for removed in [
            r#"{"type":"trade","npc":1,"item":8,"buy":true,"sale":null}"#,
            r#"{"type":"quest","quest":1,"complete":false}"#,
            r#"{"type":"create_character","nickname":"Эльф","race":2,"class":2,"gender":1}"#,
            r#"{"type":"mail","command":{"action":"list","folder":"inbox","page":0}}"#,
            r#"{"type":"specialize","profession":"smithing","expected":0}"#,
            r#"{"type":"craft","recipe":10,"expected":0,"specialization_revision":0}"#,
            r#"{"type":"alliance","command":{"action":"withdraw","alliance":1,"revision":0,"entry":1,"quantity":1}}"#,
            r#"{"type":"exchange","command":{"action":"offer","session":1,"revision":0,"item":{"price":5,"token":4294967300,"quantity":1},"coins":0}}"#,
            r#"{"type":"exchange","command":{"action":"offer","session":1,"revision":0,"item":{"token":4294967300,"quantity":1},"coins":0}}"#,
            r#"{"type":"objects","command":{"action":"take","container":1,"revision":0,"item":null,"coins":0,"owner":2}}"#,
            r#"{"type":"learn_skill","rank":2}"#,
            r#"{"type":"learn_secondary"}"#,
            r#"{"type":"learn_ability","skill":3}"#,
        ] {
            assert!(serde_json::from_str::<ClientMessage>(removed).is_err());
        }
        assert!(
            serde_json::from_str::<ClientMessage>(
                r#"{"type":"create_character","nickname":"Герой","gender":1}"#
            )
            .is_ok()
        );
        assert!(
            serde_json::from_str::<ClientMessage>(
                r#"{"type":"objects","command":{"action":"take","container":1,"revision":0,"item":{"item":5,"quantity":1},"coins":0}}"#
            )
            .is_ok()
        );
        assert!(validate_credentials("abc", "short").is_err());
    }
}
