//! One-off conversion of characters saved before every character became a human without class.
//! Identity, items, coins, level and experience stay; race/class starting bonuses are dropped,
//! attribute points the player spent are kept on top of the human base.
use crate::items::{self, Location, Stack};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Pre-stage-4 inventory entry: one per owned kind, quantities in a separate `loot` map.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub kind: u8,
    pub equipped: bool,
}
/// One entry per owned kind. Kinds 1–3 are starter gear: kept unless loot records 0 for them.
pub fn include_loot(items: &mut Vec<Item>, loot: &BTreeMap<u8, u32>) {
    items.retain(|i| {
        loot.get(&i.kind)
            .copied()
            .unwrap_or(if i.kind < 4 { 1 } else { 0 })
            > 0
    });
    for (&kind, &quantity) in loot {
        if quantity > 0
            && items::definition(kind).is_some()
            && !items.iter().any(|i| i.kind == kind)
        {
            items.push(Item {
                kind,
                equipped: false,
            });
        }
    }
}
/// Extra free attribute points every converted character receives, like a new human.
const CREATION_POINTS: f64 = 5.;
/// The removed racial skill was extra skill 6: bit `1 << 3`, learned for 3 SP.
const RACIAL_BIT: u8 = 1 << 3;
const RACIAL_SP: f64 = 3.;
const RACIAL_SKILL: f64 = 6.;
const STARTER_WEAPONS: [u8; 2] = [1, 2];
/// Exclusive-profession state removed in redesign stage 3 and archived by migration 0017.
/// Characters archived by 0016 but converted later must not get it back.
pub const PROFESSION_STATS: [&str; 5] = [
    "craft_specialization",
    "craft_specialization_revision",
    "craft_xp_leather",
    "craft_xp_smithing",
    "craft_xp_runes",
];

pub struct Converted {
    pub stats: BTreeMap<String, f64>,
    pub inventory: Vec<Item>,
    pub loot: BTreeMap<u8, u32>,
}

fn legacy_primary(race: u8, class: u8) -> BTreeMap<String, f64> {
    let fixtures: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../data/legacy_starting_stats.json"))
            .expect("bundled legacy stats");
    fixtures
        .iter()
        .find(|f| {
            f["race"].as_u64() == Some(race.into()) && f["class"].as_u64() == Some(class.into())
        })
        .and_then(|f| serde_json::from_value(f["stats"].clone()).ok())
        .unwrap_or_default()
}

pub fn humanize(
    race: u8,
    class: u8,
    mut stats: BTreeMap<String, f64>,
    mut inventory: Vec<Item>,
    mut loot: BTreeMap<u8, u32>,
) -> Result<Converted, &'static str> {
    let old_base = legacy_primary(race, class);
    let mut primary = BTreeMap::new();
    for (key, _) in crate::attributes::ATTRIBUTES {
        let spent = match (stats.get(key), old_base.get(key)) {
            (Some(now), Some(base)) => (now - base).max(0.),
            _ => 0.,
        };
        primary.insert(key.to_owned(), 10. + spent);
    }
    primary.insert("LUC".into(), 10.);
    crate::attributes::rebase(&mut stats, &primary)?;
    for key in PROFESSION_STATS {
        stats.remove(key);
    }
    *stats.entry("stat_point".into()).or_default() += CREATION_POINTS;

    let extra = stats.get("extra_skills").copied().unwrap_or(0.) as u8;
    if extra & RACIAL_BIT != 0 {
        stats.insert("extra_skills".into(), f64::from(extra & !RACIAL_BIT));
        *stats.entry("sp".into()).or_default() += RACIAL_SP;
    }
    for (key, value) in stats.iter_mut() {
        if key.starts_with("hotbar_") && *value == RACIAL_SKILL {
            *value = 0.;
        }
    }

    // New humans own both starter weapons; give the one the old class did not receive.
    let mut owned = inventory.clone();
    include_loot(&mut owned, &loot);
    for kind in STARTER_WEAPONS {
        if !owned.iter().any(|i| i.kind == kind) {
            if loot.get(&kind) == Some(&0) {
                loot.remove(&kind);
            }
            if !inventory.iter().any(|i| i.kind == kind) {
                inventory.push(Item {
                    kind,
                    equipped: false,
                });
            }
        }
    }
    Ok(Converted {
        stats,
        inventory,
        loot,
    })
}

/// Pre-stage-4 copy token: kind × 2^32 + copy number.
const STRIDE: u64 = 1 << 32;
fn old_token(kind: u8, copy: u32) -> u64 {
    u64::from(kind) * STRIDE + u64::from(copy)
}
/// Stats that only described the old kind/count inventory (coins/gold become purse coins).
pub fn old_inventory_stat(key: &str) -> bool {
    key.starts_with("bag_") || key.starts_with("gear_copy_") || matches!(key, "coins" | "gold")
}
fn old_quantity(inventory: &[Item], loot: &BTreeMap<u8, u32>, k: u8) -> u32 {
    if !inventory.iter().any(|i| i.kind == k) {
        0
    } else {
        loot.get(&k).copied().unwrap_or(if k < 4 { 1 } else { 0 })
    }
}
fn old_equipped(
    stats: &BTreeMap<String, f64>,
    inventory: &[Item],
    loot: &BTreeMap<u8, u32>,
    weapon: bool,
) -> u64 {
    inventory
        .iter()
        .find(|i| {
            i.equipped && items::slot(i.kind).is_some_and(|s| (s == items::Slot::Weapon) == weapon)
        })
        .map_or(0, |i| {
            let copy = stats
                .get(&format!("gear_copy_{}", i.kind))
                .copied()
                .unwrap_or(0.) as u32;
            old_token(
                i.kind,
                copy.min(old_quantity(inventory, loot, i.kind).saturating_sub(1)),
            )
        })
}
/// Old bag cells: saved `bag_N` tokens that still name an owned copy.
fn old_bag(stats: &BTreeMap<String, f64>, available: &BTreeSet<u64>) -> BTreeMap<u64, u16> {
    let mut left = available.clone();
    let mut out = BTreeMap::new();
    for (k, v) in stats {
        if let Some(slot) = k.strip_prefix("bag_").and_then(|s| s.parse::<u16>().ok()) {
            let t = *v as u64;
            if left.remove(&t) {
                out.insert(t, slot);
            }
        }
    }
    out
}
/// Physical form of one old character inventory.
#[derive(Debug, Clone, PartialEq)]
pub struct Physical {
    pub stats: BTreeMap<String, f64>,
    /// New stacks (id 0) and the old token each replaces (0 for coins and extra stack parts).
    pub stacks: Vec<(u64, Stack)>,
    /// Hotbar keys that held an old equipment copy token, rebound once stacks have ids.
    pub copies: Vec<(String, u64)>,
    /// Copper value of the old `coins` and `gold` (1 gold = 10 000 copper).
    pub coins: u64,
}
/// Converts kind/count inventory, copy tokens and stat currencies into stacks, keeping bag
/// cells, the worn copy and hotbar bindings.
pub fn physical(
    mut stats: BTreeMap<String, f64>,
    mut inventory: Vec<Item>,
    loot: &BTreeMap<u8, u32>,
) -> Physical {
    include_loot(&mut inventory, loot);
    let amount = |key: &str| stats.get(key).copied().unwrap_or(0.).max(0.).floor() as u64;
    let coins = amount("coins").saturating_add(amount("gold").saturating_mul(10_000));
    let worn = [true, false].map(|w| old_equipped(&stats, &inventory, loot, w));
    let mut planned: Vec<(u64, Stack)> = Vec::new();
    for item in &inventory {
        let k = item.kind;
        if items::definition(k).is_none() || items::coin_value(k).is_some() {
            continue;
        }
        let count = old_quantity(&inventory, loot, k);
        if items::equippable(k) {
            for n in 0..count {
                let t = old_token(k, n);
                let location = match items::slot(k) {
                    Some(slot) if worn.contains(&t) => Location::Worn { slot },
                    _ => Location::Bag { cell: u16::MAX },
                };
                planned.push((t, Stack::new(k, 1, location)));
            }
        } else {
            let mut left = count;
            let mut first = true;
            while left > 0 {
                let part = left.min(items::MAX_STACK);
                let t = if first { old_token(k, 0) } else { 0 };
                planned.push((t, Stack::new(k, part, Location::Bag { cell: u16::MAX })));
                left -= part;
                first = false;
            }
        }
    }
    let available: BTreeSet<u64> = planned
        .iter()
        .filter(|(t, s)| *t != 0 && !matches!(s.location, Location::Worn { .. }))
        .map(|(t, _)| *t)
        .collect();
    let cells = old_bag(&stats, &available);
    let mut used: BTreeSet<u16> = cells.values().copied().collect();
    for (t, s) in &mut planned {
        if s.location == (Location::Bag { cell: u16::MAX }) {
            let cell = match cells.get(t) {
                Some(cell) => *cell,
                None => {
                    let cell = (0..=u16::MAX)
                        .find(|n| !used.contains(n))
                        .unwrap_or(u16::MAX);
                    used.insert(cell);
                    cell
                }
            };
            s.location = Location::Bag { cell };
        }
    }
    for (kind, n) in items::denominate(coins) {
        if n > 0 {
            let n = u32::try_from(n).unwrap_or(u32::MAX);
            planned.push((0, Stack::new(kind, n, Location::Purse)));
        }
    }
    let mut copies = Vec::new();
    for (key, value) in stats.iter_mut() {
        if !key.starts_with("hotbar_") {
            continue;
        }
        let v = *value as u64;
        let t = if (101..=108).contains(&v) {
            old_token((v - 100) as u8, 0)
        } else {
            v
        };
        if t < STRIDE {
            continue;
        }
        let k = u8::try_from(t / STRIDE).unwrap_or(0);
        if items::equippable(k) {
            copies.push((key.clone(), t));
            *value = 0.;
        } else if items::stackable(k) && items::coin_value(k).is_none() && t % STRIDE == 0 {
            *value = crate::layout::kind_token(k) as f64;
        } else {
            *value = 0.;
        }
    }
    stats.retain(|k, _| !old_inventory_stat(k));
    Physical {
        stats,
        stacks: planned,
        copies,
        coins,
    }
}
/// Points hotbar keys that held old copy tokens at the stored stacks that replaced them.
pub fn rebind(stats: &mut BTreeMap<String, f64>, copies: &[(String, u64)], ids: &[(u64, i64)]) {
    for (key, old) in copies {
        let token = ids
            .iter()
            .find(|(t, _)| t == old)
            .map_or(0, |(_, id)| crate::layout::item_token(*id));
        stats.insert(key.clone(), token as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_inventory_becomes_stacks_in_the_same_cells_with_the_same_worn_copy() {
        let mut stats = crate::starting_stats().unwrap();
        stats.insert("coins".into(), 1234.);
        stats.insert("gold".into(), 2.);
        stats.insert("gear_copy_4".into(), 1.);
        stats.insert("bag_7".into(), old_token(4, 0) as f64);
        stats.insert("bag_3".into(), old_token(9, 0) as f64);
        stats.insert("bag_0".into(), old_token(11, 0) as f64);
        stats.insert("hotbar_0".into(), old_token(4, 0) as f64);
        stats.insert("hotbar_1".into(), 108.);
        stats.insert("hotbar_2".into(), 2.);
        stats.insert("hotbar_3".into(), old_token(4, 1) as f64);
        let inventory = vec![
            Item {
                kind: 4,
                equipped: true,
            },
            Item {
                kind: 1,
                equipped: false,
            },
        ];
        let loot = BTreeMap::from([(4, 2), (9, 12_000), (8, 3), (2, 0), (3, 0)]);
        let p = physical(stats, inventory, &loot);
        let count = |k: u8| -> u32 {
            p.stacks
                .iter()
                .filter(|(_, s)| s.kind == k)
                .map(|(_, s)| s.quantity)
                .sum()
        };
        assert_eq!(
            (count(4), count(9), count(8), count(1), count(2), count(3)),
            (2, 12_000, 3, 1, 0, 0)
        );
        assert_eq!(p.coins, 1234 + 20_000);
        assert_eq!(
            (
                count(items::GOLD),
                count(items::SILVER),
                count(items::COPPER)
            ),
            (2, 12, 34)
        );
        let at = |t: u64| p.stacks.iter().find(|(o, _)| *o == t).unwrap().1.location;
        assert_eq!(
            at(old_token(4, 1)),
            Location::Worn {
                slot: items::Slot::Weapon
            }
        );
        assert_eq!(at(old_token(4, 0)), Location::Bag { cell: 7 });
        assert_eq!(at(old_token(9, 0)), Location::Bag { cell: 3 });
        let cells: BTreeSet<_> = p
            .stacks
            .iter()
            .filter_map(|(_, s)| match s.location {
                Location::Bag { cell } => Some(cell),
                _ => None,
            })
            .collect();
        assert_eq!(
            cells.len(),
            p.stacks
                .iter()
                .filter(|(_, s)| matches!(s.location, Location::Bag { .. }))
                .count()
        );
        assert!(p.stats.keys().all(|k| !old_inventory_stat(k)));
        assert_eq!(p.stats["hotbar_1"], crate::layout::kind_token(8) as f64);
        assert_eq!(p.stats["hotbar_2"], 2.);
        let mut stats = p.stats.clone();
        rebind(
            &mut stats,
            &p.copies,
            &[(old_token(4, 0), 41), (old_token(4, 1), 42)],
        );
        assert_eq!(stats["hotbar_0"], crate::layout::item_token(41) as f64);
        assert_eq!(stats["hotbar_3"], crate::layout::item_token(42) as f64);
    }
    fn legacy(race: u8, class: u8) -> BTreeMap<String, f64> {
        let mut stats = legacy_primary(race, class);
        assert!(!stats.is_empty());
        stats.insert("level".into(), 7.);
        stats.insert("coins".into(), 1234.);
        stats.insert("sp".into(), 2.);
        stats.insert("stat_point".into(), 1.);
        stats
    }
    #[test]
    fn keeps_spent_points_progress_and_items_on_the_human_base() {
        let mut stats = legacy(4, 1);
        *stats.get_mut("CON").unwrap() += 2.;
        stats.insert("extra_skills".into(), f64::from(0b1001));
        stats.insert("hotbar_2".into(), 6.);
        stats.insert("hotbar_3".into(), 3.);
        stats.insert("craft_specialization".into(), 2.);
        stats.insert("craft_xp_smithing".into(), 75.);
        stats.insert("craft_count".into(), 4.);
        let inventory = vec![
            Item {
                kind: 1,
                equipped: true,
            },
            Item {
                kind: 3,
                equipped: false,
            },
        ];
        let loot = BTreeMap::from([(4, 2)]);
        let c = humanize(4, 1, stats, inventory, loot).unwrap();
        let mut expected_primary = BTreeMap::new();
        expected_primary.insert("CON".to_owned(), 12.);
        let expected = crate::starting_stats_with_primary(&expected_primary).unwrap();
        for key in [
            "STR", "DEX", "CON", "INT_", "WIT", "MEN", "LUC", "hp", "f_attack", "m_attack",
        ] {
            assert_eq!(c.stats[key], expected[key], "{key}");
        }
        assert_eq!(c.stats["CON"], 12.);
        assert_eq!(c.stats["level"], 7.);
        assert_eq!(c.stats["coins"], 1234.);
        assert_eq!(c.stats["stat_point"], 6.);
        assert_eq!(c.stats["sp"], 5.);
        assert_eq!(c.stats["extra_skills"], 1.);
        assert_eq!(c.stats["hotbar_2"], 0.);
        assert_eq!(c.stats["hotbar_3"], 3.);
        assert!(PROFESSION_STATS.iter().all(|k| !c.stats.contains_key(*k)));
        assert_eq!(c.stats["craft_count"], 4.);
        assert!(c.inventory.iter().any(|i| i.kind == 1 && i.equipped));
        assert!(c.inventory.iter().any(|i| i.kind == 2 && !i.equipped));
        assert_eq!(c.loot, BTreeMap::from([(4, 2)]));
    }
    #[test]
    fn every_legacy_start_converts_and_lost_starter_weapons_return() {
        for race in 1..=6 {
            for class in 1..=2 {
                let (own, other) = if class == 1 { (1, 2) } else { (2, 1) };
                let c = humanize(
                    race,
                    class,
                    legacy(race, class),
                    vec![Item {
                        kind: own,
                        equipped: false,
                    }],
                    BTreeMap::from([(3, 0), (other, 0)]),
                )
                .unwrap();
                for (key, _) in crate::attributes::ATTRIBUTES {
                    assert_eq!(c.stats[key], 10.);
                }
                assert_eq!(c.stats["stat_point"], 6.);
                let mut owned = c.inventory.clone();
                include_loot(&mut owned, &c.loot);
                for kind in STARTER_WEAPONS {
                    assert_eq!(owned.iter().filter(|i| i.kind == kind).count(), 1);
                }
                // Armour the player already received and gave away is not duplicated.
                assert!(!owned.iter().any(|i| i.kind == 3));
            }
        }
        let unknown = humanize(9, 9, BTreeMap::new(), Vec::new(), BTreeMap::new()).unwrap();
        assert_eq!(unknown.stats["STR"], 10.);
        assert_eq!(unknown.stats["stat_point"], 5.);
    }
}
