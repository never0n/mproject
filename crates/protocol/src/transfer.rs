//! Pure moves between holders of physical stacks: a character (bag, body, purse) or a container.
//! A moved stack keeps its id; only the part split off a larger stack is a new stack (id 0).
//! Callers load every involved holder under row locks, apply these functions to copies and
//! persist the difference in one database transaction.
use crate::items::{self, Location, Stack};
use serde::{Deserialize, Serialize};

/// One stack (by id) and how much of it to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemRequest {
    pub item: i64,
    pub quantity: u32,
}
/// Committed inventory of one participant, sent back after a transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryUpdate {
    pub character: crate::Character,
    pub actor: crate::Actor,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    Character,
    Container { id: i64, cells: u16 },
}
impl Holder {
    fn cell(self, s: &Stack) -> Option<u16> {
        match (self, s.location) {
            (Self::Character, Location::Bag { cell }) => Some(cell),
            (Self::Container { id, .. }, Location::Container { container, cell })
                if container == id =>
            {
                Some(cell)
            }
            _ => None,
        }
    }
    fn place(self, cell: u16) -> Location {
        match self {
            Self::Character => Location::Bag { cell },
            Self::Container { id, .. } => Location::Container {
                container: id,
                cell,
            },
        }
    }
    pub fn free(self, stacks: &[Stack]) -> Option<u16> {
        let limit = match self {
            Self::Character => u16::MAX,
            Self::Container { cells, .. } => cells.checked_sub(1)?,
        };
        (0..=limit).find(|n| !stacks.iter().any(|s| self.cell(s) == Some(*n)))
    }
    fn coin(self, s: &Stack) -> bool {
        items::coin_value(s.kind).is_some()
            && match self {
                Self::Character => s.location == Location::Purse,
                Self::Container { .. } => self.cell(s).is_some(),
            }
    }
}
/// Bag (or container) stacks that may be handed over: not worn, not coins.
pub fn eligible(stacks: &[Stack], holder: Holder) -> Vec<i64> {
    stacks
        .iter()
        .filter(|s| holder.cell(s).is_some() && items::coin_value(s.kind).is_none())
        .map(|s| s.id)
        .collect()
}
/// Removes `quantity` of stack `id` from the holder's cells and returns it, keeping the id when
/// the whole stack moves.
pub fn split(
    stacks: &mut Vec<Stack>,
    holder: Holder,
    id: i64,
    quantity: u32,
) -> Result<Stack, &'static str> {
    let index = stacks
        .iter()
        .position(|s| s.id == id && id != 0)
        .ok_or("Вещь уже переместили")?;
    let s = stacks[index];
    if items::coin_value(s.kind).is_some() {
        return Err("Монеты передаются отдельно от предметов");
    }
    if holder.cell(&s).is_none() {
        return Err(match s.location {
            Location::Worn { .. } => "Сначала снимите предмет",
            _ => "Вещь лежит в другом месте",
        });
    }
    if quantity == 0 || quantity > s.quantity {
        return Err("Недостаточно вещей");
    }
    if quantity == s.quantity {
        return Ok(stacks.remove(index));
    }
    stacks[index].quantity -= quantity;
    Ok(Stack {
        id: 0,
        quantity,
        ..s
    })
}
/// Places a stack into the holder: coins join the purse/pile, supplies merge into stacks of the
/// same kind, the rest takes the first free cell.
pub fn put(stacks: &mut Vec<Stack>, holder: Holder, mut s: Stack) -> Result<(), &'static str> {
    if let Some(value) = items::coin_value(s.kind) {
        let total = coins(stacks, holder)
            .checked_add(value * u64::from(s.quantity))
            .ok_or("Слишком много монет")?;
        return set_coins(stacks, holder, total);
    }
    if items::stackable(s.kind) && s.lock.is_none() && s.mass_mg.is_none() {
        for other in stacks.iter_mut() {
            if s.quantity == 0 {
                break;
            }
            if other.kind == s.kind
                && other.lock.is_none()
                && other.mass_mg.is_none()
                && holder.cell(other).is_some()
            {
                let room = items::MAX_STACK.saturating_sub(other.quantity);
                let moved = room.min(s.quantity);
                other.quantity += moved;
                s.quantity -= moved;
            }
        }
        if s.quantity == 0 {
            return Ok(());
        }
    }
    let cell = holder.free(stacks).ok_or("Нет свободного места")?;
    s.location = holder.place(cell);
    stacks.push(s);
    Ok(())
}
/// Value in copper of the holder's coins.
pub fn coins(stacks: &[Stack], holder: Holder) -> u64 {
    stacks
        .iter()
        .filter(|s| holder.coin(s))
        .filter_map(|s| items::coin_value(s.kind).map(|v| v * u64::from(s.quantity)))
        .sum()
}
/// Replaces the holder's coins with the fewest coins worth `value`, reusing stack ids.
pub fn set_coins(stacks: &mut Vec<Stack>, holder: Holder, value: u64) -> Result<(), &'static str> {
    for (kind, count) in items::denominate(value) {
        let count = u32::try_from(count).map_err(|_| "Слишком много монет")?;
        match stacks.iter().position(|s| s.kind == kind && holder.coin(s)) {
            Some(i) if count == 0 => {
                stacks.remove(i);
            }
            Some(i) => stacks[i].quantity = count,
            None if count == 0 => {}
            None => {
                let location = match holder {
                    Holder::Character => Location::Purse,
                    _ => holder.place(holder.free(stacks).ok_or("Нет места для монет")?),
                };
                stacks.push(Stack::new(kind, count, location));
            }
        }
    }
    Ok(())
}
pub fn pay(stacks: &mut Vec<Stack>, holder: Holder, value: u64) -> Result<(), &'static str> {
    let have = coins(stacks, holder);
    if have < value {
        return Err("Недостаточно монет");
    }
    set_coins(stacks, holder, have - value)
}
/// Uses up `quantity` of a supply kind from the bag, smallest stacks first.
pub fn consume(stacks: &mut Vec<Stack>, kind: u8, mut quantity: u32) -> Result<(), &'static str> {
    let mut held: Vec<usize> = (0..stacks.len())
        .filter(|i| stacks[*i].kind == kind && Holder::Character.cell(&stacks[*i]).is_some())
        .collect();
    let total: u64 = held.iter().map(|i| u64::from(stacks[*i].quantity)).sum();
    if total < u64::from(quantity) {
        return Err("Недостаточно материалов");
    }
    held.sort_by_key(|i| stacks[*i].quantity);
    for i in &held {
        let used = stacks[*i].quantity.min(quantity);
        stacks[*i].quantity -= used;
        quantity -= used;
    }
    stacks.retain(|s| s.quantity > 0);
    Ok(())
}
/// A character may keep a load acquired earlier, but nothing may make an over-limit load heavier
/// or a full purse fuller.
pub fn check_carry(before: &[Stack], after: &[Stack]) -> Result<(), &'static str> {
    let (old, new) = (items::carried_weight(before), items::carried_weight(after));
    if new > items::CARRY_LIMIT + 1e-3 && new > old + 1e-3 {
        return Err("Слишком тяжело: освободите рюкзак");
    }
    let purse = |s: &[Stack]| items::coin_count(items::purse_value(s));
    if purse(after) > items::PURSE_LIMIT && purse(after) > purse(before) {
        return Err("Кошелёк полон");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::{COPPER, GOLD, SILVER};
    fn bag(kind: u8, quantity: u32, cell: u16, id: i64) -> Stack {
        Stack {
            id,
            ..Stack::new(kind, quantity, Location::Bag { cell })
        }
    }
    #[test]
    fn moving_keeps_identity_and_splitting_creates_one_new_stack() {
        let mut a = vec![bag(4, 1, 0, 1), bag(9, 10, 1, 2)];
        let chest = Holder::Container { id: 7, cells: 2 };
        let mut b = vec![];
        let sword = split(&mut a, Holder::Character, 1, 1).unwrap();
        put(&mut b, chest, sword).unwrap();
        assert_eq!(b[0].id, 1);
        assert_eq!(
            b[0].location,
            Location::Container {
                container: 7,
                cell: 0
            }
        );
        let part = split(&mut a, Holder::Character, 2, 4).unwrap();
        assert_eq!((part.id, a[0].quantity), (0, 6));
        put(&mut b, chest, part).unwrap();
        let more = split(&mut a, Holder::Character, 2, 6).unwrap();
        put(&mut b, chest, more).unwrap();
        // Merged into the stored stack; the emptied stack id disappears.
        assert!(a.is_empty());
        assert_eq!(b.iter().map(|s| s.quantity).sum::<u32>(), 11);
        assert_eq!(b.len(), 2);
        let mut castings = vec![Stack {
            id: 8,
            mass_mg: Some(1_400_000),
            ..Stack::new(crate::items::CASTING, 1, Location::Bag { cell: 0 })
        }];
        put(
            &mut castings,
            Holder::Character,
            Stack {
                id: 9,
                mass_mg: Some(900_000),
                ..Stack::new(crate::items::CASTING, 1, Location::Ground { x: 0., z: 0. })
            },
        )
        .unwrap();
        assert_eq!(castings.len(), 2);
        assert_eq!(castings[0].quantity, 1);
        assert_eq!(castings[1].mass_mg, Some(900_000));
        assert!(put(&mut b, chest, Stack::new(1, 1, Location::Purse)).is_err());
        assert!(split(&mut a, Holder::Character, 1, 1).is_err());
        assert!(split(&mut b, chest, 1, 2).is_err());
        let mut worn = vec![Stack {
            id: 3,
            ..Stack::new(
                3,
                1,
                Location::Worn {
                    slot: items::Slot::Armor,
                },
            )
        }];
        assert!(split(&mut worn, Holder::Character, 3, 1).is_err());
    }
    #[test]
    fn coins_pay_with_change_and_respect_count_limit() {
        let mut purse = vec![];
        set_coins(&mut purse, Holder::Character, 10_250).unwrap();
        assert_eq!(purse.len(), 3);
        pay(&mut purse, Holder::Character, 251).unwrap();
        assert_eq!(coins(&purse, Holder::Character), 9_999);
        let counts: Vec<_> = purse.iter().map(|s| (s.kind, s.quantity)).collect();
        assert!(counts.contains(&(SILVER, 99)) && counts.contains(&(COPPER, 99)));
        assert!(!counts.iter().any(|(k, _)| *k == GOLD));
        assert!(pay(&mut purse, Holder::Character, 10_000).is_err());
        let before = purse.clone();
        set_coins(&mut purse, Holder::Character, 1_001 * 10_000).unwrap();
        assert!(check_carry(&before, &purse).is_err());
        let mut heavy = vec![bag(13, 200, 0, 1)];
        let lighter = {
            let mut s = heavy.clone();
            split(&mut s, Holder::Character, 1, 1).unwrap();
            s
        };
        // Over the limit already: dropping is allowed, adding is not.
        assert!(check_carry(&heavy, &lighter).is_ok());
        put(&mut heavy, Holder::Character, bag(13, 1, 5, 0)).unwrap();
        assert!(check_carry(&lighter, &heavy).is_err());
    }
    #[test]
    fn consume_uses_smallest_stacks_and_rejects_shortage() {
        let mut s = vec![bag(9, 3, 0, 1), bag(9, 9999, 1, 2), bag(9, 1, 2, 3)];
        assert!(consume(&mut s, 9, 10_004).is_err());
        assert_eq!(s.len(), 3);
        consume(&mut s, 9, 5).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].id, s[0].quantity), (2, 9998));
        let mut full = vec![bag(9, 9999, 0, 1)];
        put(&mut full, Holder::Character, bag(9, 2, 4, 0)).unwrap();
        assert_eq!(full[1].location, Location::Bag { cell: 1 });
    }
}
