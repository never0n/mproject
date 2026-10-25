//! Level, attribute points, talent points and skill practice.
//! Amounts come from `character_rules.json` `progression` and are [PROPOSAL], not canon.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
type Stats = BTreeMap<String, f64>;
pub const KILL_EXPERIENCE: u32 = 25;

/// [PROPOSAL] Directions a talent point can raise. The exact set is OPEN in canon;
/// these keys are internal. `arcane` stands in for magic until its attributes are named.
pub const TALENTS: [(&str, &str); 3] = [
    ("arms", "Оружие"),
    ("arcane", "Магия"),
    ("craft", "Ремесло"),
];

struct Tuning {
    attribute_points: f64,
    talent_points: f64,
    craft_experience: u32,
    practice_per_rank: u32,
    talent_bonus: f64,
    level_growth: f64,
}
fn tuning() -> Tuning {
    let rules: serde_json::Value =
        serde_json::from_str(include_str!("../data/character_rules.json")).expect("bundled rules");
    let p = &rules["progression"];
    Tuning {
        attribute_points: p["attributePointsPerLevel"].as_f64().unwrap_or(1.),
        talent_points: p["talentPointsPerLevel"].as_f64().unwrap_or(1.),
        craft_experience: p["craftExperience"].as_u64().unwrap_or(15) as u32,
        practice_per_rank: p["practicePerRank"].as_u64().unwrap_or(8) as u32,
        talent_bonus: p["talentBonusPerRank"].as_f64().unwrap_or(0.02),
        level_growth: p["levelGrowth"].as_f64().unwrap_or(1.15),
    }
}
pub fn craft_experience() -> u32 {
    tuning().craft_experience
}
/// Talent points a character of this level holds before spending any, under the current rule.
pub fn talent_points_for_level(level: u32) -> f64 {
    f64::from(level.saturating_sub(1)) * tuning().talent_points
}
pub fn practice_per_rank() -> u32 {
    tuning().practice_per_rank.max(1)
}
/// [PROPOSAL] Rank starts at 1 and rises by one step per `practicePerRank` uses. No purchase.
pub fn rank_of(uses: u32) -> u8 {
    1 + (uses / practice_per_rank()) as u8
}
pub fn talent_key(talent: &str) -> Option<String> {
    TALENTS
        .iter()
        .find(|(key, _)| *key == talent)
        .map(|(key, _)| format!("talent_{key}"))
}
pub fn talent_rank(stats: &Stats, talent: &str) -> u32 {
    talent_key(talent)
        .and_then(|key| stats.get(&key).copied())
        .unwrap_or(0.) as u32
}
/// [PROPOSAL] A rank of the weapon or magic talent scales that activity. Crafting does not.
pub fn activity_scale(stats: &Stats, magical: bool) -> f64 {
    let talent = if magical { "arcane" } else { "arms" };
    1. + tuning().talent_bonus * f64::from(talent_rank(stats, talent))
}

/// Typed view of the progress part of `Character.stats`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Progress {
    pub level: u32,
    pub experience: f64,
    pub stat_points: f64,
    #[serde(default)]
    pub talent_points: f64,
    pub kills: u64,
    pub skill_rank: u8,
    #[serde(default)]
    pub practice: [u32; 5],
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
        let practice = std::array::from_fn(|i| v(&format!("practice_{}", i + 1)) as u32);
        Self {
            level: (v("level") as u32).max(1),
            experience: v("current_exp"),
            stat_points: v("stat_point"),
            talent_points: v("talent_point"),
            kills: v("kills") as u64,
            skill_rank: (v("skill_rank") as u8).max(rank_of(practice[0])).max(1),
            practice,
            primary_disabled: v("primary_disabled") == 1.,
            admin_revision: v("admin_revision") as u32,
            secondary_learned: v("secondary_learned") == 1.,
            extra_skills: v("extra_skills") as u8 & 0b111,
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
        s.insert("talent_point".into(), self.talent_points);
        if self.skill_rank > 1 {
            s.insert("skill_rank".into(), self.skill_rank as f64);
        }
        for (i, uses) in self.practice.iter().enumerate() {
            if *uses > 0 {
                s.insert(format!("practice_{}", i + 1), f64::from(*uses));
            }
        }
        for (k, v) in [
            ("level", self.level as f64),
            ("current_exp", self.experience),
            ("exp", threshold(self.level)),
            ("stat_point", self.stat_points),
            ("kills", self.kills as f64),
        ] {
            if k != "kills" || self.kills > 0 {
                s.insert(k.into(), v);
            }
        }
    }
    /// Adds general experience and grants separate attribute and talent points per level.
    /// Does not touch derived combat stats: the caller applies `grow` to a derived sheet.
    pub fn add_experience(&mut self, amount: u32) -> u32 {
        let before = self.level;
        let points = tuning();
        self.experience += f64::from(amount);
        while self.experience >= threshold(self.level) {
            self.experience -= threshold(self.level);
            self.level += 1;
            self.stat_points += points.attribute_points;
            self.talent_points += points.talent_points;
        }
        self.level - before
    }
    pub fn reward(&mut self, stats: &mut Stats) -> u32 {
        self.reward_share(stats, KILL_EXPERIENCE)
    }
    /// One eligible kill credit, with this character's share of the common experience pool.
    pub fn reward_share(&mut self, stats: &mut Stats, experience: u32) -> u32 {
        self.kills += 1;
        let gained = self.add_experience(experience);
        if gained > 0 {
            grow(stats, gained);
        }
        self.write_stats(stats);
        gained
    }
    /// One confirmed use of skill `id` (1–5). Raises that skill's rank from practice and can
    /// open the next skill. Talent points are not involved.
    pub fn practice(&mut self, id: u8) -> bool {
        let Some(slot) = id
            .checked_sub(1)
            .and_then(|i| self.practice.get_mut(usize::from(i)))
        else {
            return false;
        };
        *slot += 1;
        let uses = *slot;
        if id == 1 {
            self.skill_rank = self.skill_rank.max(rank_of(uses));
        }
        uses == practice_per_rank()
    }
    pub fn known(&self, id: u8) -> bool {
        let step = practice_per_rank();
        match id {
            1 => !self.primary_disabled,
            2 => self.secondary_learned || self.practice[0] >= step,
            3..=5 => {
                self.extra_skills & (1 << (id - 3)) != 0
                    || self.practice[usize::from(id - 2)] >= step
            }
            _ => false,
        }
    }
}
pub fn threshold(level: u32) -> f64 {
    let mut exp: f64 = 100.;
    let last = level.min(99);
    for l in 2..=last {
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
    // [PROPOSAL] Past the old curve the same growth factor continues. There is no level cap.
    let growth = tuning().level_growth;
    for _ in 100..=level {
        exp = (exp * growth).round();
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
    fn levels_grant_separate_points_and_do_not_stop() {
        let mut stats = crate::starting_stats().unwrap();
        let mut p = Progress::from_stats(&stats);
        for _ in 0..3 {
            assert_eq!(p.reward(&mut stats), 0);
        }
        assert_eq!(p.reward(&mut stats), 1);
        assert_eq!(
            (p.level, p.experience, p.stat_points, p.talent_points),
            (2, 0., 6., 1.)
        );
        assert!(!stats.contains_key("sp"));
        assert_eq!(stats["hp"], 110.);
        assert_eq!(stats["f_attack"], 20.);
        assert_eq!(threshold(2), 115.);
        assert_eq!(threshold(5), 228.);
        let mut loaded = crate::starting_stats().unwrap();
        grow(&mut loaded, p.level - 1);
        p.write_stats(&mut loaded);
        assert_eq!(stats["hp"], loaded["hp"]);
        assert!(threshold(100) > threshold(99));
        p.level = 100;
        p.experience = 0.;
        assert_eq!(p.reward(&mut stats), 0);
        assert!(p.experience > 0.);
    }
    #[test]
    fn practice_ranks_one_skill_and_opens_the_next_without_talent() {
        let mut stats = crate::starting_stats().unwrap();
        let mut p = Progress::from_stats(&stats);
        p.talent_points = 3.;
        assert!(p.known(1) && !p.known(2));
        for _ in 0..practice_per_rank() - 1 {
            assert!(!p.practice(1));
        }
        assert!(p.practice(1));
        assert!(p.known(2));
        assert_eq!(p.skill_rank, 2);
        assert_eq!(p.talent_points, 3.);
        assert!(!p.known(3));
        p.write_stats(&mut stats);
        let loaded = Progress::from_stats(&stats);
        assert_eq!(loaded.practice[0], practice_per_rank());
        assert_eq!(loaded.skill_rank, 2);
    }
}
