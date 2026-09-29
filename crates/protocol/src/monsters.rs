//! Encounter balance shared by server simulation and client presentation.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Bandit,
    Marauder,
    Apostate,
}
pub struct Definition {
    pub name: &'static str,
    pub level: u32,
    pub hp: f64,
    pub physical_defence: f64,
    pub magical_defence: f64,
    pub evasion: f64,
    pub attack: f64,
    pub magical: bool,
    pub accuracy: f64,
    pub range: f32,
    pub speed: f32,
    pub interval: u64,
    pub cast_ticks: u64,
    pub respawn_ticks: u64,
    pub experience: u32,
    pub coins: u32,
}
impl Kind {
    /// Windup and full presentation duration, in 50 ms simulation ticks.
    pub fn attack_timing(self) -> (u64, u64) {
        match self {
            Self::Bandit => (8, 19),
            Self::Marauder => (12, 27),
            Self::Apostate => {
                let cast = self.definition().cast_ticks;
                (cast, cast + 12)
            }
        }
    }
    pub fn definition(self) -> Definition {
        match self {
            Self::Bandit => Definition {
                name: "Разбойник",
                level: 1,
                hp: 90.,
                physical_defence: 10.,
                magical_defence: 10.,
                evasion: 40.,
                attack: 18.,
                magical: false,
                accuracy: 48.,
                range: 2.5,
                speed: 4.,
                interval: 40,
                cast_ticks: 0,
                respawn_ticks: 600,
                experience: 25,
                coins: 12,
            },
            Self::Marauder => Definition {
                name: "Мародёр тракта",
                level: 3,
                hp: 155.,
                physical_defence: 22.,
                magical_defence: 8.,
                evasion: 38.,
                attack: 27.,
                magical: false,
                accuracy: 51.,
                range: 2.8,
                speed: 3.2,
                interval: 48,
                cast_ticks: 0,
                respawn_ticks: 900,
                experience: 40,
                coins: 18,
            },
            Self::Apostate => Definition {
                name: "Пепельный отступник",
                level: 5,
                hp: 125.,
                physical_defence: 6.,
                magical_defence: 22.,
                evasion: 43.,
                attack: 35.,
                magical: true,
                accuracy: 53.,
                range: 11.,
                speed: 3.6,
                interval: 60,
                cast_ticks: 24,
                respawn_ticks: 1200,
                experience: 60,
                coins: 24,
            },
        }
    }
    pub fn index(self) -> usize {
        match self {
            Self::Bandit => 0,
            Self::Marauder => 1,
            Self::Apostate => 2,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cast {
    pub remaining_ms: u32,
    pub duration_ms: u32,
}

/// One server-owned attack: preparation, impact (including a miss), recovery.
/// The sequence is scoped to a monster and survives its respawns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackMotion {
    pub sequence: u32,
    pub elapsed_ms: u32,
    pub impact_ms: u32,
    pub duration_ms: u32,
    pub released: bool,
}
