//! Persistent positions for individual equipment copies and stackable supplies.
use crate::{Character, items};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
const STRIDE: u64 = 1 << 32;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Place {
    Bag(u16),
    Bar(u8),
    Equipment(bool),
    Skill(u8),
    Model,
    Clear,
}
pub fn token(kind: u8, copy: u32) -> u64 {
    u64::from(kind) * STRIDE + u64::from(copy)
}
pub fn kind(value: u64) -> Option<u8> {
    let k = if (101..=108).contains(&value) {
        (value - 100) as u8
    } else {
        (value / STRIDE) as u8
    };
    if value / STRIDE > u64::from(u8::MAX) || items::definition(k).is_none() {
        None
    } else {
        Some(k)
    }
}
pub fn quantity(c: &Character, k: u8) -> u32 {
    if let Some(key) = items::currency(k) {
        c.stats.get(key).copied().unwrap_or(0.).max(0.) as u32
    } else if !c.inventory.iter().any(|i| i.kind == k) {
        0
    } else if k < 4 {
        c.loot.get(&k).copied().unwrap_or(1)
    } else {
        c.loot.get(&k).copied().unwrap_or(0)
    }
}
pub fn valid(c: &Character, t: u64) -> bool {
    if (1..=u64::from(crate::skills::LAST_SKILL)).contains(&t) {
        return crate::skills::learned(
            &crate::progression::Progress::from_stats(&c.stats),
            t as u8,
        );
    }
    let Some(k) = kind(t) else { return false };
    if (101..=108).contains(&t) {
        return quantity(c, k) > 0;
    }
    let n = t % STRIDE;
    quantity(c, k) > 0
        && if items::equippable(k) {
            n < u64::from(quantity(c, k))
        } else {
            n == 0
        }
}
pub fn equipped(c: &Character, weapon: bool) -> u64 {
    c.inventory
        .iter()
        .find(|i| {
            i.equipped
                && items::definition(i.kind).is_some_and(|d| (d.slot == "Правая рука") == weapon)
        })
        .map_or(0, |i| {
            token(
                i.kind,
                (c.stats
                    .get(&format!("gear_copy_{}", i.kind))
                    .copied()
                    .unwrap_or(0.) as u32)
                    .min(quantity(c, i.kind).saturating_sub(1)),
            )
        })
}
pub fn canonical(c: &Character, t: u64) -> u64 {
    if (101..=108).contains(&t) {
        token((t - 100) as u8, 0)
    } else if valid(c, t) {
        t
    } else {
        0
    }
}
pub fn bag(c: &Character) -> BTreeMap<u16, u64> {
    let mut available = BTreeSet::new();
    for i in &c.inventory {
        let count = quantity(c, i.kind);
        for n in 0..if items::equippable(i.kind) {
            count
        } else {
            count.min(1)
        } {
            available.insert(token(i.kind, n));
        }
    }
    for k in [11, 12] {
        if quantity(c, k) > 0 {
            available.insert(token(k, 0));
        }
    }
    available.remove(&equipped(c, true));
    available.remove(&equipped(c, false));
    let mut out = BTreeMap::new();
    for (k, v) in &c.stats {
        if let Some(slot) = k.strip_prefix("bag_").and_then(|s| s.parse::<u16>().ok()) {
            let t = *v as u64;
            if available.remove(&t) {
                out.insert(slot, t);
            }
        }
    }
    let mut index = 0_u16;
    for t in available {
        while out.contains_key(&index) {
            index = index.checked_add(1).expect("inventory exceeds 65536 cells");
        }
        out.insert(index, t);
    }
    out
}
pub fn at(c: &Character, p: Place) -> u64 {
    match p {
        Place::Bag(n) => bag(c).get(&n).copied().unwrap_or(0),
        Place::Bar(n) => {
            let t = c.stats.get(&format!("hotbar_{n}")).copied().unwrap_or(0.) as u64;
            if valid(c, t) { canonical(c, t) } else { 0 }
        }
        Place::Skill(n) => {
            if valid(c, u64::from(n)) {
                u64::from(n)
            } else {
                0
            }
        }
        Place::Equipment(w) => equipped(c, w),
        _ => 0,
    }
}
fn save_bag(c: &mut Character, b: &BTreeMap<u16, u64>) {
    c.stats.retain(|k, _| !k.starts_with("bag_"));
    for (n, t) in b {
        c.stats.insert(format!("bag_{n}"), *t as f64);
    }
}
fn put(b: &mut BTreeMap<u16, u64>, t: u64, preferred: Option<u16>) {
    if t == 0 {
        return;
    }
    if let Some(n) = preferred
        && !b.contains_key(&n)
    {
        b.insert(n, t);
        return;
    }
    let n = (0..=u16::MAX)
        .find(|n| !b.contains_key(n))
        .expect("inventory full");
    b.insert(n, t);
}
/// Validate first, then mutate a clone: rejected drags never partly move items.
pub fn arrange(
    c: &mut Character,
    source: Place,
    target: Place,
    expected: u64,
) -> Result<(), &'static str> {
    let mut next = c.clone();
    apply(&mut next, source, target, expected)?;
    *c = next;
    Ok(())
}
fn apply(
    c: &mut Character,
    source: Place,
    target: Place,
    expected: u64,
) -> Result<(), &'static str> {
    for p in [source, target] {
        if matches!(p,Place::Bar(n) if n>=12)
            || matches!(p,Place::Skill(n) if !(1..=crate::skills::LAST_SKILL).contains(&n))
        {
            return Err("Недопустимая ячейка");
        }
    }
    if source == target {
        return Ok(());
    }
    let t = at(c, source);
    if t == 0 || t != expected || !valid(c, t) {
        return Err("Предмет или навык уже изменился");
    }
    let mut b = bag(c);
    if let Place::Bar(to) = target {
        if kind(t).is_some_and(|k| items::currency(k).is_some()) {
            return Err("Валюта хранится в рюкзаке и расходуется при покупке");
        }
        if t == u64::from(crate::skills::TRADER_SKILL) {
            return Err("Пассивный навык работает постоянно и не требует ячейки панели");
        }
        let other = at(c, target);
        c.stats.insert(format!("hotbar_{to}"), t as f64);
        if let Place::Bar(from) = source {
            c.stats.insert(format!("hotbar_{from}"), other as f64);
        }
        return Ok(());
    }
    if target == Place::Clear {
        if let Place::Bar(n) = source {
            c.stats.insert(format!("hotbar_{n}"), 0.);
            return Ok(());
        }
        return Err("Здесь нельзя удалить предмет");
    }
    let Some(k) = kind(t) else {
        if let Place::Bar(n) = source
            && matches!(target, Place::Skill(_))
        {
            c.stats.insert(format!("hotbar_{n}"), 0.);
            return Ok(());
        }
        return Err("Навык нельзя положить в рюкзак или надеть");
    };
    let weapon = items::definition(k).unwrap().slot == "Правая рука";
    let current = equipped(c, weapon);
    let source_cell = b.iter().find(|(_, v)| **v == t).map(|(n, _)| *n);
    match target {
        Place::Equipment(w) if w != weapon => return Err("Предмет не подходит этому слоту"),
        Place::Equipment(_) | Place::Model => {
            if !items::equippable(k) {
                return Err("Этот предмет нельзя надеть");
            }
            items::equip(&mut c.inventory, c.class, k, true)?;
            c.stats
                .insert(format!("gear_copy_{k}"), (t % STRIDE) as f64);
            if let Some(n) = source_cell {
                b.remove(&n);
            }
            if current != t {
                put(&mut b, current, source_cell);
            }
        }
        Place::Bag(to) => {
            if current == t && k <= 6 {
                items::equip(&mut c.inventory, c.class, k, false)?;
                let other = b.insert(to, t);
                if let Some(other) = other {
                    put(&mut b, other, None);
                }
            } else {
                let from = source_cell.ok_or("Предмета нет в рюкзаке")?;
                b.remove(&from);
                if let Some(other) = b.insert(to, t) {
                    b.insert(from, other);
                }
            }
            if let Place::Bar(n) = source {
                c.stats.insert(format!("hotbar_{n}"), 0.);
            }
        }
        _ => return Err("Сюда нельзя переместить предмет"),
    }
    save_bag(c, &b);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hero() -> Character {
        let mut c = Character {
            id: 1,
            slot: 1,
            nickname: "Test".into(),
            race: 1,
            class: 1,
            gender: 1,
            stats: crate::starting_stats(1, 1).unwrap(),
            inventory: items::starter(1),
            loot: BTreeMap::from([(4, 3), (6, 2), (8, 5)]),
        };
        items::include_loot(&mut c.inventory, &c.loot);
        c
    }
    fn location(c: &Character, t: u64) -> Place {
        Place::Bag(*bag(c).iter().find(|(_, v)| **v == t).unwrap().0)
    }
    #[test]
    fn copies_equip_swap_unequip_and_empty_cells_preserve_every_item() {
        let mut c = hero();
        let original = bag(&c).len();
        // Eight item cells plus the character's persistent coin cell.
        assert_eq!(original, 9);
        assert!(bag(&c).values().any(|t| kind(*t) == Some(11)));
        let sword = token(4, 1);
        let origin = location(&c, sword);
        arrange(&mut c, origin, Place::Bag(29), sword).unwrap();
        assert_eq!(at(&c, origin), 0);
        assert_eq!(at(&c, Place::Bag(29)), sword);
        arrange(&mut c, Place::Bag(29), Place::Model, sword).unwrap();
        assert_eq!(bag(&c).len(), original - 1);
        assert_eq!(at(&c, Place::Bag(29)), 0);
        assert_eq!(equipped(&c, true), sword);
        let replacement = token(4, 2);
        let pos = location(&c, replacement);
        arrange(&mut c, pos, Place::Equipment(true), replacement).unwrap();
        assert_eq!(at(&c, pos), sword);
        assert_eq!(equipped(&c, true), replacement);
        arrange(&mut c, Place::Equipment(true), Place::Bag(0), replacement).unwrap();
        assert_eq!(equipped(&c, true), 0);
        assert_eq!(at(&c, Place::Bag(0)), replacement);
        assert_eq!(bag(&c).len(), original);
        let saved = serde_json::to_string(&c).unwrap();
        let restored: Character = serde_json::from_str(&saved).unwrap();
        assert_eq!(bag(&c), bag(&restored));
    }
    #[test]
    fn bars_start_empty_swap_and_return_items_without_duplication() {
        let mut c = hero();
        assert!((0..12).all(|n| at(&c, Place::Bar(n)) == 0));
        assert_eq!(at(&c, Place::Skill(2)), 0);
        let potion = token(8, 0);
        let pos = location(&c, potion);
        let count = bag(&c).len();
        arrange(&mut c, pos, Place::Bar(11), potion).unwrap();
        arrange(&mut c, Place::Skill(1), Place::Bar(0), 1).unwrap();
        arrange(&mut c, Place::Bar(0), Place::Bar(11), 1).unwrap();
        assert_eq!(at(&c, Place::Bar(0)), potion);
        assert_eq!(at(&c, Place::Bar(11)), 1);
        arrange(&mut c, Place::Bar(0), Place::Bag(22), potion).unwrap();
        assert_eq!(at(&c, Place::Bar(0)), 0);
        assert_eq!(at(&c, Place::Bag(22)), potion);
        assert_eq!(bag(&c).len(), count);
        assert_eq!(c.loot[&8], 5);
        let old = c.clone();
        assert!(arrange(&mut c, Place::Bag(22), Place::Model, potion).is_err());
        assert_eq!(c, old);
        assert!(arrange(&mut c, Place::Skill(2), Place::Bar(1), 2).is_err());
        assert!(arrange(&mut c, Place::Bag(22), Place::Bar(12), potion).is_err());
        assert!(arrange(&mut c, Place::Bag(22), Place::Bag(5), token(4, 0)).is_err());
        c.loot.insert(8, 0);
        items::include_loot(&mut c.inventory, &c.loot);
        assert_eq!(at(&c, Place::Bag(22)), 0);
    }
}
