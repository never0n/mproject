//! Per-character mastery. Existing recipes remain usable without a specialization.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
type Stats = BTreeMap<String, f64>;

pub const MAX_LEVEL: u8 = 5;
pub const CRAFT_XP: u32 = 25;
pub const SWITCH_COST: u32 = 100;
const SPECIALIZATION: &str = "craft_specialization";
const REVISION: &str = "craft_specialization_revision";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profession {
    Leatherworking,
    Smithing,
    Runecraft,
}
pub const ALL: [Profession; 3] = [
    Profession::Leatherworking,
    Profession::Smithing,
    Profession::Runecraft,
];

pub struct Mastery {
    pub level: u8,
    pub experience: u32,
    pub within_level: u32,
    pub needed: u32,
}
pub struct Gain {
    pub experience: u32,
    pub before: u8,
    pub after: u8,
}
/// Total experience at the beginning of a level: 0, 50, 150, 300, 500.
pub fn threshold(level: u8) -> u32 {
    let level = u32::from(level.clamp(1, MAX_LEVEL));
    25 * level * (level - 1)
}
impl Profession {
    pub fn id(self) -> u8 {
        match self {
            Self::Leatherworking => 1,
            Self::Smithing => 2,
            Self::Runecraft => 3,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Leatherworking => "Кожевничество",
            Self::Smithing => "Кузнечное дело",
            Self::Runecraft => "Рунопись",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Leatherworking => "craft_xp_leather",
            Self::Smithing => "craft_xp_smithing",
            Self::Runecraft => "craft_xp_runes",
        }
    }
    pub fn mastery(self, stats: &Stats) -> Mastery {
        let experience =
            (stats.get(self.key()).copied().unwrap_or(0.) as u32).min(threshold(MAX_LEVEL));
        let level = (1..=MAX_LEVEL)
            .rev()
            .find(|&l| experience >= threshold(l))
            .unwrap_or(1);
        Mastery {
            level,
            experience,
            within_level: experience - threshold(level),
            needed: if level == MAX_LEVEL {
                0
            } else {
                threshold(level + 1) - threshold(level)
            },
        }
    }
    pub fn gain(self, stats: &mut Stats) -> Gain {
        let old = self.mastery(stats);
        let next = (old.experience + CRAFT_XP).min(threshold(MAX_LEVEL));
        stats.insert(self.key().into(), f64::from(next));
        Gain {
            experience: next - old.experience,
            before: old.level,
            after: self.mastery(stats).level,
        }
    }
    pub fn savings(self, stats: &Stats) -> (u32, u32) {
        let level = self.mastery(stats).level;
        let primary = if active(stats) == Some(self) && level >= 2 {
            1 + u32::from(level >= 4)
        } else {
            0
        };
        let secondary = u32::from((level - 1) / 2);
        (primary, secondary)
    }
    pub fn summary(self, stats: &Stats) -> String {
        let mastery = self.mastery(stats);
        if mastery.level == MAX_LEVEL {
            format!("{} · уровень {} · мастер", self.name(), mastery.level)
        } else {
            format!(
                "{} · уровень {} · {} / {} опыта",
                self.name(),
                mastery.level,
                mastery.within_level,
                mastery.needed
            )
        }
    }
}
pub fn active(stats: &Stats) -> Option<Profession> {
    ALL.into_iter()
        .find(|p| stats.get(SPECIALIZATION).copied() == Some(f64::from(p.id())))
}
pub fn revision(stats: &Stats) -> u32 {
    stats.get(REVISION).copied().unwrap_or(0.) as u32
}
pub fn selection_cost(stats: &Stats) -> u32 {
    if active(stats).is_some() {
        SWITCH_COST
    } else {
        0
    }
}
pub fn eligible(stats: &Stats, profession: Profession) -> Result<(), &'static str> {
    if active(stats) == Some(profession) {
        return Err("Эта специализация уже выбрана");
    }
    if profession.mastery(stats).level < 2 {
        return Err("Для специализации нужен 2 уровень этого ремесла");
    }
    Ok(())
}
/// Returns the coin cost. The caller persists these stats and the debit in one transaction.
pub fn choose(
    stats: &mut Stats,
    profession: Profession,
    expected: u32,
    coins: f64,
) -> Result<u32, &'static str> {
    if revision(stats) != expected {
        return Err("Специализация уже изменилась. Повторите выбор");
    }
    eligible(stats, profession)?;
    let cost = selection_cost(stats);
    if coins < f64::from(cost) {
        return Err("Недостаточно монет для смены специализации");
    }
    let next = expected
        .checked_add(1)
        .ok_or("Достигнут предел смен специализации")?;
    stats.insert(SPECIALIZATION.into(), f64::from(profession.id()));
    stats.insert(REVISION.into(), f64::from(next));
    Ok(cost)
}
