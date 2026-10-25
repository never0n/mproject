//! Knockdown, death and the offline body. Amounts are [PROPOSAL] and live in
//! `character_rules.json` under `life`. Two magic-attribute names stay unnamed.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

type Stats = BTreeMap<String, f64>;

struct Tuning {
    lives: u32,
    knock_seconds: f64,
    rise_fraction: f64,
    loss: f64,
    return_seconds: f64,
    return_radius: f32,
    offline_seconds: f64,
}
fn tuning() -> Tuning {
    let rules: serde_json::Value =
        serde_json::from_str(include_str!("../data/character_rules.json")).expect("bundled rules");
    let life = &rules["life"];
    Tuning {
        lives: life["lives"].as_u64().unwrap_or(10) as u32,
        knock_seconds: life["knockSeconds"].as_f64().unwrap_or(5.),
        rise_fraction: life["riseFraction"].as_f64().unwrap_or(0.05),
        loss: life["loss"].as_f64().unwrap_or(0.2),
        return_seconds: life["returnSeconds"].as_f64().unwrap_or(30.),
        return_radius: life["returnRadius"].as_f64().unwrap_or(12.) as f32,
        offline_seconds: life["offlineSeconds"].as_f64().unwrap_or(1800.),
    }
}
pub fn lives() -> u32 {
    tuning().lives.max(1)
}
pub fn knock_ticks() -> u64 {
    (tuning().knock_seconds * 20.).round() as u64
}
pub fn rise_hp(max_hp: f64) -> f64 {
    (max_hp * tuning().rise_fraction).max(1.)
}
pub fn return_millis() -> u64 {
    (tuning().return_seconds * 1000.).round() as u64
}
pub fn return_radius() -> f32 {
    tuning().return_radius
}
pub fn offline_ticks() -> u64 {
    (tuning().offline_seconds * 20.).round() as u64
}
/// Combinable offline orders. Absent keys mean the default: return and guard, not work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Orders {
    pub returns: bool,
    pub guards: bool,
    pub works: bool,
}
impl Default for Orders {
    fn default() -> Self {
        Self {
            returns: true,
            guards: true,
            works: false,
        }
    }
}
impl Orders {
    pub fn from_stats(stats: &Stats) -> Self {
        let flag = |key: &str, default: bool| stats.get(key).map(|v| *v == 1.).unwrap_or(default);
        Self {
            returns: flag("offline_return", true),
            guards: flag("offline_guard", true),
            works: flag("offline_work", false),
        }
    }
    pub fn write_stats(self, stats: &mut Stats) {
        stats.insert("offline_return".into(), f64::from(u8::from(self.returns)));
        stats.insert("offline_guard".into(), f64::from(u8::from(self.guards)));
        stats.insert("offline_work".into(), f64::from(u8::from(self.works)));
    }
}

/// One damaging hit against a standing or downed body.
#[derive(Debug, PartialEq)]
pub enum Blow {
    /// Still standing, with the remaining health.
    Standing(f64),
    /// Health reached zero. `rise_at` is the tick when the body gets up if nobody finishes it.
    Down { rise_at: u64 },
    /// A later hit on an already downed body. The caller records the death once.
    Finished,
}
pub fn blow(hp: f64, downed: bool, hit: f64, tick: u64) -> Blow {
    if downed {
        return if hit > 0. {
            Blow::Finished
        } else {
            Blow::Down {
                rise_at: tick + knock_ticks(),
            }
        };
    }
    let left = (hp - hit.max(0.)).max(0.);
    if left > 0. {
        Blow::Standing(left)
    } else {
        Blow::Down {
            rise_at: tick + knock_ticks(),
        }
    }
}

/// [PROPOSAL] Numerical progress above the human base drops by `loss`, rounded.
/// Learned skill flags are not touched. Unspent points drop by the same fraction.
pub fn reduce_numbers(stats: &mut Stats) {
    let keep = 1. - tuning().loss;
    for key in ["STR", "DEX", "CON", "INT_", "WIT", "MEN", "LUC"] {
        if let Some(value) = stats.get_mut(key) {
            let above = (*value - 10.).max(0.);
            *value = 10. + (above * keep).round();
        }
    }
    let ranks: Vec<String> = stats
        .keys()
        .filter(|key| {
            key.starts_with("talent_") && *key != "talent_point" || key.starts_with("practice_")
        })
        .cloned()
        .collect();
    for key in ranks {
        let value = stats[&key];
        stats.insert(key, (value * keep).round());
    }
    if let Some(rank) = stats.get("skill_rank").copied() {
        let dropped = 1. + ((rank - 1.).max(0.) * keep).round();
        if dropped <= 1. {
            stats.remove("skill_rank");
        } else {
            stats.insert("skill_rank".into(), dropped);
        }
    }
    if let Some(level) = stats.get("level").copied() {
        stats.insert("level".into(), 1. + ((level - 1.).max(0.) * keep).round());
    }
    for key in ["stat_point", "talent_point", "current_exp"] {
        if let Some(value) = stats.get_mut(key) {
            *value = (*value * keep).round();
        }
    }
}

pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_second_blow_finishes_once_and_a_miss_does_not() {
        assert!(matches!(blow(40., false, 10., 0), Blow::Standing(30.)));
        let Blow::Down { rise_at } = blow(4., false, 10., 0) else {
            panic!("knockdown");
        };
        assert_eq!(rise_at, knock_ticks());
        assert_eq!(
            blow(0., true, 0., rise_at),
            Blow::Down {
                rise_at: rise_at + knock_ticks()
            }
        );
        assert_eq!(blow(0., true, 8., rise_at), Blow::Finished);
        assert_eq!(blow(0., true, 8., rise_at), Blow::Finished);
    }
    #[test]
    fn numbers_drop_and_learned_flags_stay() {
        let mut stats = BTreeMap::from([
            ("STR".into(), 15.),
            ("DEX".into(), 10.),
            ("level".into(), 6.),
            ("stat_point".into(), 5.),
            ("talent_arms".into(), 5.),
            ("talent_point".into(), 2.),
            ("practice_1".into(), 10.),
            ("skill_rank".into(), 3.),
            ("extra_skills".into(), 1.),
            ("secondary_learned".into(), 1.),
        ]);
        reduce_numbers(&mut stats);
        assert_eq!(stats["STR"], 14.);
        assert_eq!(stats["DEX"], 10.);
        assert_eq!(stats["level"], 5.);
        assert_eq!(stats["talent_arms"], 4.);
        assert_eq!(stats["extra_skills"], 1.);
        assert_eq!(stats["secondary_learned"], 1.);
    }
}
