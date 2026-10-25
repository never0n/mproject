//! Bag cells, worn equipment and hotbar bindings over the character's physical stacks.
//! A token names what a cell or hotbar slot shows: a skill (1..=LAST_SKILL), a stackable kind
//! (`kind << 32`, so the binding survives stacks being split or used up) or one stored stack
//! (`ITEM_BASE + id`). Hotbar tokens live in `hotbar_N` stats.
use crate::{
    Character,
    items::{self, Location, Slot, Stack},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ITEM_BASE: u64 = 1 << 40;
const KIND_SHIFT: u32 = 32;
pub const HOTBAR: u8 = 12;
/// Where an item/skill sits: bag cell, hotbar slot, equipment (true = weapon hand, false = armor),
/// skill list, the character model (drop = equip), or nowhere (`Clear` removes a hotbar binding).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Place {
    Bag(u16),
    Bar(u8),
    Equipment(bool),
    Skill(u8),
    Model,
    Clear,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Skill(u8),
    Kind(u8),
    Item(i64),
}
pub fn item_token(id: i64) -> u64 {
    ITEM_BASE + id as u64
}
pub fn kind_token(kind: u8) -> u64 {
    u64::from(kind) << KIND_SHIFT
}
pub fn decode(t: u64) -> Option<Token> {
    if (1..=u64::from(crate::skills::LAST_SKILL)).contains(&t) {
        Some(Token::Skill(t as u8))
    } else if t >= ITEM_BASE {
        i64::try_from(t - ITEM_BASE)
            .ok()
            .filter(|id| *id > 0)
            .map(Token::Item)
    } else if t.trailing_zeros() >= KIND_SHIFT {
        u8::try_from(t >> KIND_SHIFT)
            .ok()
            .filter(|k| items::stackable(*k))
            .map(Token::Kind)
    } else {
        None
    }
}
fn slot_of(weapon: bool) -> Slot {
    if weapon { Slot::Weapon } else { Slot::Armor }
}
/// A carried stack (bag or body) by id.
pub fn stack(c: &Character, id: i64) -> Option<&Stack> {
    c.items.iter().find(|s| s.id == id && s.carried())
}
/// Item kind a token shows, if it is an item the character carries.
pub fn kind(c: &Character, t: u64) -> Option<u8> {
    match decode(t)? {
        Token::Item(id) => stack(c, id).map(|s| s.kind),
        Token::Kind(k) => Some(k),
        Token::Skill(_) => None,
    }
}
/// Carried quantity of a kind, counting the purse for coins.
pub fn quantity(c: &Character, kind: u8) -> u32 {
    c.items
        .iter()
        .filter(|s| s.kind == kind && (s.carried() || s.location == Location::Purse))
        .map(|s| s.quantity)
        .fold(0, u32::saturating_add)
}
pub fn valid(c: &Character, t: u64) -> bool {
    match decode(t) {
        Some(Token::Skill(n)) => {
            crate::skills::learned(&crate::progression::Progress::from_stats(&c.stats), n)
        }
        Some(Token::Kind(k)) => quantity(c, k) > 0,
        Some(Token::Item(id)) => stack(c, id).is_some(),
        None => false,
    }
}
pub fn equipped(c: &Character, weapon: bool) -> u64 {
    items::worn(&c.items, slot_of(weapon)).map_or(0, |s| item_token(s.id))
}
pub fn bag(c: &Character) -> BTreeMap<u16, u64> {
    c.items
        .iter()
        .filter_map(|s| match s.location {
            Location::Bag { cell } => Some((cell, item_token(s.id))),
            _ => None,
        })
        .collect()
}
pub fn free_cell(stacks: &[Stack]) -> Option<u16> {
    (0..=u16::MAX).find(|n| {
        !stacks
            .iter()
            .any(|s| s.location == Location::Bag { cell: *n })
    })
}
pub fn at(c: &Character, p: Place) -> u64 {
    match p {
        Place::Bag(n) => bag(c).get(&n).copied().unwrap_or(0),
        Place::Bar(n) => {
            let t = c.stats.get(&format!("hotbar_{n}")).copied().unwrap_or(0.) as u64;
            if valid(c, t) { t } else { 0 }
        }
        Place::Skill(n) if valid(c, u64::from(n)) => u64::from(n),
        Place::Equipment(w) => equipped(c, w),
        _ => 0,
    }
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
fn set_location(c: &mut Character, id: i64, location: Location) {
    if let Some(s) = c.items.iter_mut().find(|s| s.id == id) {
        s.location = location;
    }
}
fn occupant(c: &Character, location: Location) -> Option<i64> {
    c.items
        .iter()
        .find(|s| s.location == location)
        .map(|s| s.id)
}
fn apply(
    c: &mut Character,
    source: Place,
    target: Place,
    expected: u64,
) -> Result<(), &'static str> {
    for p in [source, target] {
        if matches!(p,Place::Bar(n) if n>=HOTBAR)
            || matches!(p,Place::Skill(n) if !(1..=crate::skills::LAST_SKILL).contains(&n))
        {
            return Err("Недопустимая ячейка");
        }
    }
    if source == target {
        return Ok(());
    }
    let t = at(c, source);
    if t == 0 || t != expected {
        return Err("Предмет или навык уже изменился");
    }
    if let Place::Bar(to) = target {
        let shown = match decode(t) {
            // Supplies are bound by kind so the slot keeps working as stacks change.
            Some(Token::Item(id)) => match stack(c, id) {
                Some(s) if items::stackable(s.kind) => kind_token(s.kind),
                _ => t,
            },
            _ => t,
        };
        let other = at(c, target);
        c.stats.insert(format!("hotbar_{to}"), shown as f64);
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
    let id = match decode(t) {
        Some(Token::Item(id)) => id,
        Some(Token::Kind(k)) => c
            .items
            .iter()
            .find(|s| s.kind == k && matches!(s.location, Location::Bag { .. }))
            .map(|s| s.id)
            .ok_or("Предмета нет в рюкзаке")?,
        _ => {
            if let Place::Bar(n) = source
                && matches!(target, Place::Skill(_))
            {
                c.stats.insert(format!("hotbar_{n}"), 0.);
                return Ok(());
            }
            return Err("Навык нельзя положить в рюкзак или надеть");
        }
    };
    let item = *stack(c, id).ok_or("Предмет или навык уже изменился")?;
    match target {
        Place::Equipment(_) | Place::Model => {
            let slot = items::slot(item.kind).ok_or("Этот предмет нельзя надеть")?;
            if matches!(target, Place::Equipment(w) if slot_of(w) != slot) {
                return Err("Предмет не подходит этому слоту");
            }
            let worn = Location::Worn { slot };
            if item.location == worn {
                return Ok(());
            }
            let cell = match item.location {
                Location::Bag { cell } => cell,
                _ => return Err("Предмета нет в рюкзаке"),
            };
            if let Some(current) = occupant(c, worn) {
                set_location(c, current, Location::Bag { cell });
            }
            set_location(c, id, worn);
        }
        Place::Bag(to) => {
            let destination = Location::Bag { cell: to };
            match (item.location, occupant(c, destination)) {
                (Location::Bag { cell }, Some(other)) => {
                    set_location(c, other, Location::Bag { cell });
                }
                (Location::Worn { .. }, Some(other)) => {
                    let free = free_cell(&c.items).ok_or("Рюкзак переполнен")?;
                    set_location(c, other, Location::Bag { cell: free });
                }
                _ => {}
            }
            set_location(c, id, destination);
            if let Place::Bar(n) = source {
                c.stats.insert(format!("hotbar_{n}"), 0.);
            }
        }
        _ => return Err("Сюда нельзя переместить предмет"),
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::items::Stack;
    pub fn hero() -> Character {
        let mut items = Vec::new();
        for (n, (kind, quantity)) in [(1, 1), (2, 1), (3, 1), (4, 1), (4, 1), (4, 1), (6, 1)]
            .into_iter()
            .chain([(8, 5)])
            .enumerate()
        {
            let mut s = Stack::new(kind, quantity, Location::Bag { cell: n as u16 });
            s.id = n as i64 + 1;
            items.push(s);
        }
        let mut coins = Stack::new(items::SILVER, 5, Location::Purse);
        coins.id = 20;
        items.push(coins);
        Character {
            id: 1,
            slot: 1,
            nickname: "Test".into(),
            gender: 1,
            stats: crate::starting_stats().unwrap(),
            items,
        }
    }
    fn location(c: &Character, t: u64) -> Place {
        Place::Bag(*bag(c).iter().find(|(_, v)| **v == t).unwrap().0)
    }
    #[test]
    fn copies_equip_swap_unequip_and_empty_cells_preserve_every_item() {
        let mut c = hero();
        let original = bag(&c).len();
        // The purse is not a bag cell.
        assert_eq!(original, 8);
        let sword = item_token(5);
        let origin = location(&c, sword);
        arrange(&mut c, origin, Place::Bag(29), sword).unwrap();
        assert_eq!(at(&c, origin), 0);
        assert_eq!(at(&c, Place::Bag(29)), sword);
        arrange(&mut c, Place::Bag(29), Place::Model, sword).unwrap();
        assert_eq!(bag(&c).len(), original - 1);
        assert_eq!(at(&c, Place::Bag(29)), 0);
        assert_eq!(equipped(&c, true), sword);
        let replacement = item_token(6);
        let pos = location(&c, replacement);
        arrange(&mut c, pos, Place::Equipment(true), replacement).unwrap();
        assert_eq!(at(&c, pos), sword);
        assert_eq!(equipped(&c, true), replacement);
        arrange(&mut c, Place::Equipment(true), Place::Bag(0), replacement).unwrap();
        assert_eq!(equipped(&c, true), 0);
        assert_eq!(at(&c, Place::Bag(0)), replacement);
        assert_eq!(bag(&c).len(), original);
        assert!(arrange(&mut c, Place::Bag(2), Place::Equipment(true), item_token(3)).is_err());
        let saved = serde_json::to_string(&c).unwrap();
        let restored: Character = serde_json::from_str(&saved).unwrap();
        assert_eq!(bag(&c), bag(&restored));
        let mut ids: Vec<i64> = c.items.iter().map(|s| s.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), 9);
    }
    #[test]
    fn bars_bind_supplies_by_kind_and_return_items_without_duplication() {
        let mut c = hero();
        assert!((0..HOTBAR).all(|n| at(&c, Place::Bar(n)) == 0));
        assert_eq!(at(&c, Place::Skill(2)), 0);
        let potion = item_token(8);
        let pos = location(&c, potion);
        let count = bag(&c).len();
        arrange(&mut c, pos, Place::Bar(11), potion).unwrap();
        assert_eq!(at(&c, Place::Bar(11)), kind_token(8));
        arrange(&mut c, Place::Skill(1), Place::Bar(0), 1).unwrap();
        arrange(&mut c, Place::Bar(0), Place::Bar(11), 1).unwrap();
        assert_eq!(at(&c, Place::Bar(0)), kind_token(8));
        assert_eq!(at(&c, Place::Bar(11)), 1);
        arrange(&mut c, Place::Bar(0), Place::Bag(22), kind_token(8)).unwrap();
        assert_eq!(at(&c, Place::Bar(0)), 0);
        assert_eq!(at(&c, Place::Bag(22)), potion);
        assert_eq!(bag(&c).len(), count);
        assert_eq!(quantity(&c, 8), 5);
        let old = c.clone();
        assert!(arrange(&mut c, Place::Bag(22), Place::Model, potion).is_err());
        assert_eq!(c, old);
        assert!(arrange(&mut c, Place::Skill(2), Place::Bar(1), 2).is_err());
        assert!(arrange(&mut c, Place::Bag(22), Place::Bar(12), potion).is_err());
        assert!(arrange(&mut c, Place::Bag(22), Place::Bag(5), item_token(5)).is_err());
        c.items.retain(|s| s.kind != 8);
        assert_eq!(at(&c, Place::Bag(22)), 0);
        assert!(decode(kind_token(1)).is_none());
        assert_eq!(decode(item_token(7)), Some(Token::Item(7)));
    }
}
