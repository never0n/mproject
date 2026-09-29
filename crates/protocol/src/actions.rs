//! Short-lived presentation events for confirmed character actions, not damage commands.
use crate::weapons::{self, Family};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Weapon,
    Spell,
    Gesture,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    pub sequence: u64,
    pub started_tick: u64,
    pub elapsed_ms: u32,
    pub duration_ms: u32,
    pub kind: Kind,
    pub weapon: Option<u8>,
    pub skill: Option<u8>,
    pub origin: [f32; 3],
    pub target: Option<[f32; 3]>,
    pub yaw: f32,
}
pub const MAX_RECENT: usize = 8;
pub fn duration(kind: Kind, weapon: Option<u8>) -> u32 {
    match kind {
        Kind::Spell => 850,
        Kind::Gesture => 750,
        Kind::Weapon => match weapon.and_then(weapons::definition).map(|w| w.family) {
            Some(Family::Greatsword | Family::Greataxe | Family::Greatclub) => 1050,
            Some(Family::Bow) => 950,
            Some(Family::Hammer | Family::Pike) => 850,
            Some(Family::Dagger) | None => 600,
            _ => 750,
        },
    }
}
