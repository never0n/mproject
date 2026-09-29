//! Level formulas ported from TOP_Project/server/services/formulas.js and gameConfig.js.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
type Stats = BTreeMap<String, f64>;
pub const KILL_EXPERIENCE: u32 = 25;
pub const KILL_COINS: u32 = 12;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Progress {
    pub level: u32,
    pub experience: f64,
    pub coins: f64,
    pub sp: f64,
    pub stat_points: f64,
    pub kills: u64,
    pub quest_stage: u8,
    pub quest_kills: u32,
    #[serde(default)]
    pub quest_crafted: bool,
    pub skill_rank: u8,
    #[serde(default)]
    pub primary_disabled: bool,
    #[serde(default)]
    pub admin_revision: u32,
    pub secondary_learned: bool,
    #[serde(default)]
    pub extra_skills: u8,
}
impl Progress {
    pub fn from_stats(s: &Stats) -> Self {
        let v = |k: &str| s.get(k).copied().unwrap_or(0.);
        Self {
            level: (v("level") as u32).clamp(1, 100),
            experience: v("current_exp"),
            coins: v("coins"),
            sp: v("sp"),
            stat_points: v("stat_point"),
            kills: v("kills") as u64,
            quest_stage: v("quest_stage") as u8,
            quest_kills: v("quest_kills") as u32,
            quest_crafted: v("quest_crafted") == 1.,
            skill_rank: (v("skill_rank") as u8).clamp(1, 3),
            primary_disabled: v("primary_disabled") == 1.,
            admin_revision: v("admin_revision") as u32,
            secondary_learned: v("secondary_learned") == 1.,
            extra_skills: v("extra_skills") as u8 & 0b11111,
        }
    }
    pub fn write_stats(&self, s: &mut Stats) {
        s.insert("admin_revision".into(), f64::from(self.admin_revision));
        s.insert(
            "primary_disabled".into(),
            f64::from(u8::from(self.primary_disabled)),
        );
        s.insert(
            "secondary_learned".into(),
            f64::from(u8::from(self.secondary_learned)),
        );
        s.insert("extra_skills".into(), f64::from(self.extra_skills));
        if self.skill_rank > 1 {
            s.insert("skill_rank".into(), self.skill_rank as f64);
        }
        if self.quest_stage > 0 {
            s.insert("quest_stage".into(), self.quest_stage as f64);
            s.insert("quest_kills".into(), self.quest_kills as f64);
            s.insert("quest_crafted".into(), u8::from(self.quest_crafted) as f64);
        }
        for (k, v) in [
            ("level", self.level as f64),
            ("current_exp", self.experience),
            ("exp", threshold(self.level)),
            ("coins", self.coins),
            ("sp", self.sp),
            ("stat_point", self.stat_points),
            ("kills", self.kills as f64),
        ] {
            if k != "kills" || self.kills > 0 {
                s.insert(k.into(), v);
            }
        }
    }
    pub fn quest_description(&self) -> String {
        crate::quests::current(self).map_or_else(
            || "Поручения торговца завершены".into(),
            |q| format!("{} · подробности в журнале", q.title),
        )
    }
    pub fn reward(&mut self, stats: &mut Stats) -> u32 {
        self.reward_share(stats, KILL_EXPERIENCE, KILL_COINS)
    }
    /// One eligible kill credit, with this character's share of the common reward pool.
    pub fn reward_share(&mut self, stats: &mut Stats, experience: u32, coins: u32) -> u32 {
        self.reward_enemy(stats, experience, coins, true)
    }
    pub fn reward_enemy(
        &mut self,
        stats: &mut Stats,
        experience: u32,
        coins: u32,
        bandit: bool,
    ) -> u32 {
        let before = self.level;
        self.coins += f64::from(coins);
        self.kills += 1;
        if bandit && self.quest_stage == 1 {
            self.quest_kills = (self.quest_kills + 1).min(3);
        }
        if self.level < 100 {
            self.experience += f64::from(experience);
        }
        while self.level < 100 && self.experience >= threshold(self.level) {
            self.experience -= threshold(self.level);
            self.level += 1;
            self.sp += (5
                + 2 * (self.level - 2)
                + (self.level / 6) * 5
                + if self.level >= 96 { 5 } else { 0 }) as f64;
            self.stat_points += if self.level.is_multiple_of(12) {
                2.
            } else if self.level.is_multiple_of(6) {
                1.
            } else {
                0.
            };
            grow(stats, 1);
        }
        if self.level == 100 {
            self.experience = 0.;
        }
        self.write_stats(stats);
        self.level - before
    }
}
pub fn threshold(level: u32) -> f64 {
    if level >= 100 {
        return 0.;
    }
    let mut exp: f64 = 100.;
    for l in 2..=level {
        exp = (exp
            * if l == 99 {
                3.
            } else if l >= 96 {
                2.
            } else if l >= 5 && (l - 5) % 6 == 0 {
                1.5
            } else {
                1.15
            })
        .round();
    }
    exp
}
/// Stored combat stats are level-one bases. Apply growth to derived views only.
pub fn grow(stats: &mut Stats, levels: u32) {
    for (key, primary, base, multiplier) in [
        ("energy", "", 5., 0.),
        ("hp", "CON", 10., 2.),
        ("mana", "MEN", 10., 2.),
        ("stamina", "DEX", 10., 2.),
        ("f_attack", "STR", 0., 1.),
        ("f_defence", "CON", 0., 1.),
        ("m_attack", "INT_", 0., 1.),
        ("m_defence", "MEN", 0., 1.),
    ] {
        let bonus = (stats.get(primary).copied().unwrap_or(10.) - 10.).max(0.);
        *stats.entry(key.into()).or_default() += (base + bonus * multiplier) * levels as f64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_kills_level_up_with_legacy_growth_and_no_duplicate_bonuses() {
        let mut stats = crate::starting_stats(1, 1).unwrap();
        let mut p = Progress::from_stats(&stats);
        for _ in 0..3 {
            assert_eq!(p.reward(&mut stats), 0);
        }
        assert_eq!(p.reward(&mut stats), 1);
        assert_eq!((p.level, p.experience, p.coins, p.sp), (2, 0., 548., 5.));
        assert_eq!(stats["hp"], 134.);
        assert_eq!(stats["f_attack"], 32.);
        assert_eq!(threshold(2), 115.);
        assert_eq!(threshold(5), 228.);
        let mut loaded = crate::starting_stats(1, 1).unwrap();
        grow(&mut loaded, p.level - 1);
        p.write_stats(&mut loaded);
        assert_eq!(stats, loaded);
        p.level = 100;
        assert_eq!(p.reward(&mut stats), 0);
        assert_eq!(p.experience, 0.);
        assert_eq!(threshold(100), 0.);
    }
}
