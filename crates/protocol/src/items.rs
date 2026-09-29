//! Initial equipment catalogue, inspired by the original seed-items.js grade 1.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub kind: u8,
    pub equipped: bool,
}
pub struct Definition {
    pub name: &'static str,
    pub slot: &'static str,
    pub class: Option<u8>,
    pub stat: &'static str,
    pub bonus: f64,
}
pub fn definition(kind: u8) -> Option<Definition> {
    let (name, slot, class, stat, bonus) = match kind {
        1 => ("Деревянный меч", "Правая рука", Some(1), "f_attack", 6.),
        2 => ("Деревянный посох", "Правая рука", Some(2), "m_attack", 6.),
        3 => ("Кожаная кираса", "Доспех", None, "f_defence", 4.),
        4 => (
            "Железный меч разбойника",
            "Правая рука",
            Some(1),
            "f_attack",
            10.,
        ),
        5 => ("Резной посох", "Правая рука", Some(2), "m_attack", 10.),
        6 => ("Усиленная кожаная кираса", "Доспех", None, "f_defence", 7.),
        7 => ("Обрывки ткани", "Материал", None, "", 0.),
        8 => ("Малое зелье здоровья", "Зелье", None, "", 0.),
        9 => ("Куски кожи", "Материал", None, "", 0.),
        10 => ("Рецепт усиленной кожаной кирасы", "Рецепт", None, "", 0.),
        11 => ("Монеты", "Валюта", None, "", 0.),
        12 => ("Золото", "Валюта", None, "", 0.),
        13 => ("Железные заготовки", "Материал", None, "", 0.),
        14 => ("Осколки рунного камня", "Материал", None, "", 0.),
        15 => ("Чертёж стального меча", "Рецепт", None, "", 0.),
        16 => ("Рецепт рунного посоха", "Рецепт", None, "", 0.),
        17 => ("Стальной меч", "Правая рука", Some(1), "f_attack", 16.),
        18 => ("Рунный посох", "Правая рука", Some(2), "m_attack", 16.),
        19 => ("Простой топор", "Правая рука", Some(1), "f_attack", 7.),
        20 => ("Простой молот", "Правая рука", Some(1), "f_attack", 8.),
        21 => (
            "Простой двуручный меч",
            "Правая рука",
            Some(1),
            "f_attack",
            9.,
        ),
        22 => (
            "Простой двуручный топор",
            "Правая рука",
            Some(1),
            "f_attack",
            10.,
        ),
        23 => ("Тяжёлая дубина", "Правая рука", Some(1), "f_attack", 11.),
        24 => ("Простая пика", "Правая рука", Some(1), "f_attack", 8.),
        25 => ("Простой лук", "Правая рука", Some(1), "f_attack", 6.),
        26 => ("Простой кинжал", "Правая рука", Some(1), "f_attack", 4.),
        _ => return None,
    };
    Some(Definition {
        name,
        slot,
        class,
        stat,
        bonus,
    })
}
pub fn starter(class: u8) -> Vec<Item> {
    vec![
        Item {
            kind: if class == 1 { 1 } else { 2 },
            equipped: false,
        },
        Item {
            kind: 3,
            equipped: false,
        },
    ]
}
pub fn equip(items: &mut [Item], class: u8, kind: u8, equipped: bool) -> Result<(), &'static str> {
    let def = definition(kind).ok_or("Неизвестный предмет")?;
    if !matches!(def.slot, "Правая рука" | "Доспех") {
        return Err("Этот предмет нельзя надеть");
    }
    if !items.iter().any(|i| i.kind == kind) {
        return Err("У персонажа нет этого предмета");
    }
    if equipped && def.class.is_some_and(|c| c != class) {
        return Err("Предмет не подходит вашему классу");
    }
    for item in items {
        if item.kind == kind {
            item.equipped = equipped;
        } else if equipped && definition(item.kind).is_some_and(|d| d.slot == def.slot) {
            item.equipped = false;
        }
    }
    Ok(())
}

/// One equipped instance per kind. Quantities live separately from equipment state.
pub fn include_loot(items: &mut Vec<Item>, loot: &std::collections::BTreeMap<u8, u32>) {
    items.retain(|i| {
        loot.get(&i.kind)
            .copied()
            .unwrap_or(if i.kind < 4 { 1 } else { 0 })
            > 0
    });
    for (&kind, &quantity) in loot {
        if quantity > 0 && definition(kind).is_some() && !items.iter().any(|i| i.kind == kind) {
            items.push(Item {
                kind,
                equipped: false,
            });
        }
    }
}

// Internally tagged messages buffer JSON object keys as strings before deserializing fields.
pub fn deserialize_loot<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<std::collections::BTreeMap<u8, u32>, D::Error> {
    let strings = std::collections::BTreeMap::<String, u32>::deserialize(deserializer)?;
    strings
        .into_iter()
        .map(|(key, count)| {
            key.parse::<u8>()
                .map(|kind| (kind, count))
                .map_err(serde::de::Error::custom)
        })
        .collect()
}

pub fn sell_price(kind: u8) -> Option<u32> {
    match kind {
        1 | 2 | 19..=26 => Some(4),
        4 | 5 => Some(8),
        6 => Some(6),
        7 => Some(2),
        13 | 14 => Some(4),
        17 | 18 => Some(18),
        _ => None,
    }
}
pub const POTION_PRICE: u32 = 15;
pub const POTION_HEAL: f64 = 50.;
pub const MERCHANT_STOCK: &[u8] = &[8, 10, 15, 16];
pub const MERCHANT_BUYS: &[u8] = &[
    1, 2, 19, 20, 21, 22, 23, 24, 25, 26, 4, 5, 6, 7, 13, 14, 17, 18,
];
pub fn buy_price(kind: u8) -> Option<u32> {
    match kind {
        1 | 2 | 19..=26 => Some(12),
        8 => Some(POTION_PRICE),
        crate::crafting::CUIRASS_RECIPE => Some(25),
        crate::crafting::SWORD_RECIPE | crate::crafting::STAFF_RECIPE => Some(80),
        _ => None,
    }
}
/// Round up to whole coins, using the same price in the shop and the server transaction.
pub fn purchase_price(kind: u8, progress: &crate::progression::Progress) -> Option<u32> {
    buy_price(kind).map(|price| {
        if crate::skills::learned(progress, crate::skills::TRADER_SKILL) {
            (u64::from(price) * 90).div_ceil(100) as u32
        } else {
            price
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_ownership_class_and_idempotent_equipment() {
        let mut items = starter(1);
        assert!(equip(&mut items, 1, 2, true).is_err());
        items.push(Item {
            kind: 2,
            equipped: false,
        });
        assert!(equip(&mut items, 1, 2, true).is_err());
        equip(&mut items, 1, 1, true).unwrap();
        equip(&mut items, 1, 1, true).unwrap();
        assert_eq!(items.iter().filter(|i| i.equipped).count(), 1);
        equip(&mut items, 1, 1, false).unwrap();
        assert!(items.iter().all(|i| !i.equipped));
        assert!(equip(&mut items, 1, 255, true).is_err());
    }
}

/// Equipment always occupies one cell per owned copy, including future catalogue entries.
pub fn equippable(kind: u8) -> bool {
    definition(kind).is_some_and(|d| matches!(d.slot, "Правая рука" | "Доспех"))
}

/// Public appearance contains only the equipped catalogue IDs, never the owner's bag.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EquipmentAppearance {
    pub weapon: Option<u8>,
    pub armor: Option<u8>,
}
impl EquipmentAppearance {
    pub fn from_inventory(items: &[Item]) -> Self {
        let mut result = Self::default();
        for item in items.iter().filter(|item| item.equipped) {
            match definition(item.kind).map(|d| d.slot) {
                Some("Правая рука") => result.weapon = Some(item.kind),
                Some("Доспех") => result.armor = Some(item.kind),
                _ => {}
            }
        }
        result
    }
}
pub fn currency(kind: u8) -> Option<&'static str> {
    match kind {
        11 => Some("coins"),
        12 => Some("gold"),
        _ => None,
    }
}
