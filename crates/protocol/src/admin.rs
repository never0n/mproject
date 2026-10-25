//! Typed target identities keep player and monster ID spaces separate.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Player(i64),
    Mob(u32),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub target: Target,
    pub name: String,
    pub role: i16,
    pub level: u32,
    pub position: Option<[f32; 3]>,
    pub hp: f64,
}
/// Live self-only switches. They are not written into character stats.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Powers {
    pub immortal: bool,
    pub invisible: bool,
    pub click_teleport: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub role: i16,
    pub entries: Vec<Entry>,
    pub audit: Vec<String>,
    pub online: usize,
    #[serde(default)]
    pub powers: Powers,
}
pub fn role_name(role: i16) -> &'static str {
    match role {
        1 => "Пользователь",
        2 => "Ценсор",
        3 => "Админ",
        4 => "Супер-админ",
        5 => "Creator",
        _ => "Неизвестная роль",
    }
}
