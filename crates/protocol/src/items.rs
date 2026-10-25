//! Item catalogue and physical stacks. Every stack is one persistent instance with a server id
//! that sits in exactly one place: a bag cell, an equipment slot, the coin purse, a container cell
//! or the ground. Moving a stack changes its place; it never copies it.
use serde::{Deserialize, Serialize};

/// Static catalog entry: `slot` is a display/category string ("Правая рука", "Материал"…),
/// `stat`/`bonus` is the stat it raises when worn, `weight` is in kilograms per unit.
pub struct Definition {
    pub name: &'static str,
    pub slot: &'static str,
    pub stat: &'static str,
    pub bonus: f64,
    pub weight: f32,
}
pub fn definition(kind: u8) -> Option<Definition> {
    // [PROPOSAL] weights; only the carry limit relation matters until balance work.
    let (name, slot, stat, bonus, weight) = match kind {
        1 => ("Деревянный меч", "Правая рука", "f_attack", 6., 1.5),
        2 => ("Деревянный посох", "Правая рука", "m_attack", 6., 1.5),
        3 => ("Кожаная кираса", "Доспех", "f_defence", 4., 5.),
        4 => (
            "Железный меч разбойника",
            "Правая рука",
            "f_attack",
            10.,
            2.5,
        ),
        5 => ("Резной посох", "Правая рука", "m_attack", 10., 2.),
        6 => ("Усиленная кожаная кираса", "Доспех", "f_defence", 7., 7.),
        7 => ("Обрывки ткани", "Материал", "", 0., 0.1),
        8 => ("Малое зелье здоровья", "Зелье", "", 0., 0.3),
        9 => ("Куски кожи", "Материал", "", 0., 0.5),
        10 => ("Рецепт усиленной кожаной кирасы", "Рецепт", "", 0., 0.05),
        COPPER => ("Медная монета", "Монеты", "", 0., 0.),
        GOLD => ("Золотая монета", "Монеты", "", 0., 0.),
        IRON => ("Железные заготовки", "Материал", "", 0., 1.),
        14 => ("Осколки рунного камня", "Материал", "", 0., 0.2),
        15 => ("Чертёж стального меча", "Рецепт", "", 0., 0.05),
        16 => ("Рецепт рунного посоха", "Рецепт", "", 0., 0.05),
        17 => ("Стальной меч", "Правая рука", "f_attack", 16., 3.),
        18 => ("Рунный посох", "Правая рука", "m_attack", 16., 2.5),
        19 => ("Простой топор", "Правая рука", "f_attack", 7., 2.5),
        20 => ("Простой молот", "Правая рука", "f_attack", 8., 3.),
        21 => ("Простой двуручный меч", "Правая рука", "f_attack", 9., 4.),
        22 => (
            "Простой двуручный топор",
            "Правая рука",
            "f_attack",
            10.,
            4.5,
        ),
        23 => ("Тяжёлая дубина", "Правая рука", "f_attack", 11., 4.),
        24 => ("Простая пика", "Правая рука", "f_attack", 8., 3.5),
        25 => ("Простой лук", "Правая рука", "f_attack", 6., 1.),
        26 => ("Простой кинжал", "Правая рука", "f_attack", 4., 0.5),
        SILVER => ("Серебряная монета", "Монеты", "", 0., 0.),
        CHEST => ("Сундук", "Хранилище", "", 0., 15.),
        KEY => ("Ключ", "Ключ", "", 0., 0.05),
        30 => ("Древесина", "Материал", "", 0., 1.),
        31 => ("Зерно", "Материал", "", 0., 0.2),
        32 => ("Фляга воды", "Питьё", "", 0., 0.5),
        33 => ("Хлеб", "Еда", "", 0., 0.4),
        34 => ("Соль", "Материал", "", 0., 0.2),
        35 => ("Солёный хлеб", "Еда", "", 0., 0.4),
        36 => ("Золотая руда", "Материал", "", 0., 2.),
        37 => ("Прилавок", "Лавка", "", 0., 4.),
        38 => ("Чертёж клинка", "Чертёж", "", 0., 0.05),
        39 => ("Негодный клинок", "Правая рука", "f_attack", 1., 1.5),
        40 => ("Грубый клинок", "Правая рука", "f_attack", 5., 2.),
        41 => ("Добротный клинок", "Правая рука", "f_attack", 12., 2.5),
        42 => ("Строительный брус", "Материал", "", 0., 4.),
        43 => ("Письмо", "Документ", "", 0., 0.1),
        CART => ("Телега", "Перевозка", "", 0., 12.),
        BRONZE => ("Бронза C90700", "Материал", "", 0., 0.1),
        CASTING => ("Отливка", "Заготовка", "", 0., 0.),
        _ => return None,
    };
    Some(Definition {
        name,
        slot,
        stat,
        bonus,
        weight,
    })
}

pub const IRON: u8 = 13;
pub const COPPER: u8 = 11;
pub const GOLD: u8 = 12;
pub const SILVER: u8 = 27;
pub const CHEST: u8 = 28;
pub const KEY: u8 = 29;
pub const STALL: u8 = 37;
pub const CART: u8 = 44;
/// Tin bronze stock. One unit is [`crate::casting::BRONZE_MG`] milligrams, not gold ore.
pub const BRONZE: u8 = 45;
/// A poured blank. Its mass and shape live on the instance, and it is not a weapon.
pub const CASTING: u8 = 46;
pub const POTION: u8 = 8;
pub const POTION_HEAL: f64 = 50.;
/// Kinds every new human starts with: one melee weapon, one staff and light armour.
pub const STARTER: [u8; 3] = [1, 2, 3];
/// Largest quantity in one stack of a stackable kind.
pub const MAX_STACK: u32 = 9999;
/// [PROPOSAL] Weight a character can carry in the bag and on the body, kilograms.
/// 30 kg was below one outing of armour and materials, so loot fell at the feet
/// and [E] then refused every piece.
pub const CARRY_LIMIT: f32 = 160.;
/// [PROPOSAL] Coins a purse holds regardless of weight (canon: separate count limit).
pub const PURSE_LIMIT: u64 = 1000;

/// Canonical exchange rate: 1 gold = 100 silver, 1 silver = 100 copper. Values are in copper.
pub fn coin_value(kind: u8) -> Option<u64> {
    match kind {
        COPPER => Some(1),
        SILVER => Some(100),
        GOLD => Some(10_000),
        _ => None,
    }
}
/// Coins from the largest denomination down; a purse or pile always holds this split.
pub fn denominate(value: u64) -> [(u8, u64); 3] {
    [
        (GOLD, value / 10_000),
        (SILVER, value / 100 % 100),
        (COPPER, value % 100),
    ]
}
/// Number of physical coins needed to hold `value`.
pub fn coin_count(value: u64) -> u64 {
    denominate(value).iter().map(|(_, n)| n).sum()
}
pub fn format_coins(value: u64) -> String {
    let parts: Vec<String> = denominate(value)
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(kind, n)| {
            let short = match *kind {
                GOLD => "з",
                SILVER => "с",
                _ => "м",
            };
            format!("{n} {short}")
        })
        .collect();
    if parts.is_empty() {
        "0 м".into()
    } else {
        parts.join(" ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Weapon,
    Armor,
}
pub fn slot(kind: u8) -> Option<Slot> {
    match definition(kind)?.slot {
        "Правая рука" => Some(Slot::Weapon),
        "Доспех" => Some(Slot::Armor),
        _ => None,
    }
}
pub fn equippable(kind: u8) -> bool {
    slot(kind).is_some()
}
/// [PROPOSAL] Attribute a worn item expects for its full bonus. Below it the item still fits
/// and `effective_bonus` scales the bonus by have/need. `MEN` is the internal key of the
/// unnamed attribute that governs mana and magic defense.
pub fn requirement(kind: u8) -> Option<(&'static str, f64)> {
    let def = definition(kind)?;
    let attr = match def.stat {
        "f_attack" => "STR",
        "m_attack" => "INT_",
        "f_defence" => "CON",
        "m_defence" => "MEN",
        _ => return None,
    };
    Some((attr, 10. + def.bonus))
}
pub fn effective_bonus(kind: u8, stats: &std::collections::BTreeMap<String, f64>) -> f64 {
    let Some(def) = definition(kind) else {
        return 0.;
    };
    let Some((attr, need)) = requirement(kind) else {
        return def.bonus;
    };
    let have = stats.get(attr).copied().unwrap_or(0.).max(0.);
    if need <= 0. || have >= need {
        def.bonus
    } else {
        def.bonus * have / need
    }
}
/// Equipment, chests and keys are unique objects; everything else stacks.
pub fn stackable(kind: u8) -> bool {
    !equippable(kind)
        && !matches!(
            kind,
            CHEST | KEY | STALL | CART | CASTING | 38 | 33 | 35 | 43
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "at", rename_all = "snake_case", deny_unknown_fields)]
pub enum Location {
    Bag { cell: u16 },
    Worn { slot: Slot },
    Purse,
    Container { container: i64, cell: u16 },
    Ground { x: f32, z: f32 },
}
/// One physical stack. `id` 0 marks a stack that is not stored yet; `lock` is the lock code a
/// key opens.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stack {
    pub id: i64,
    pub kind: u8,
    pub quantity: u32,
    pub location: Location,
    pub lock: Option<i64>,
    /// Instance mass in milligrams. Empty means the catalogue weight.
    #[serde(default)]
    pub mass_mg: Option<u64>,
}
impl Stack {
    pub fn new(kind: u8, quantity: u32, location: Location) -> Self {
        Self {
            id: 0,
            kind,
            quantity,
            location,
            lock: None,
            mass_mg: None,
        }
    }
    pub fn carried(&self) -> bool {
        matches!(self.location, Location::Bag { .. } | Location::Worn { .. })
    }
    pub fn weight(&self) -> f32 {
        if let Some(mg) = self.mass_mg {
            return mg as f32 / 1_000_000. * self.quantity as f32;
        }
        definition(self.kind).map_or(0., |d| d.weight) * self.quantity as f32
    }
}
/// Weight in the bag and on the body; the purse does not count.
pub fn carried_weight(stacks: &[Stack]) -> f32 {
    stacks
        .iter()
        .filter(|s| s.carried())
        .map(Stack::weight)
        .sum()
}
pub fn purse_value(stacks: &[Stack]) -> u64 {
    stacks
        .iter()
        .filter(|s| s.location == Location::Purse)
        .filter_map(|s| coin_value(s.kind).map(|v| v * u64::from(s.quantity)))
        .sum()
}
pub fn worn(stacks: &[Stack], slot: Slot) -> Option<&Stack> {
    stacks
        .iter()
        .find(|s| s.location == Location::Worn { slot })
}

/// Public appearance contains only the worn catalogue IDs, never the owner's bag.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EquipmentAppearance {
    pub weapon: Option<u8>,
    pub armor: Option<u8>,
}
impl EquipmentAppearance {
    pub fn from_items(stacks: &[Stack]) -> Self {
        Self {
            weapon: worn(stacks, Slot::Weapon).map(|s| s.kind),
            armor: worn(stacks, Slot::Armor).map(|s| s.kind),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coins_follow_the_canonical_ratio() {
        assert_eq!(coin_value(GOLD), Some(100 * coin_value(SILVER).unwrap()));
        assert_eq!(coin_value(SILVER), Some(100 * coin_value(COPPER).unwrap()));
        assert_eq!(
            denominate(1_020_304),
            [(GOLD, 102), (SILVER, 3), (COPPER, 4)]
        );
        assert_eq!(coin_count(1_020_304), 109);
        assert_eq!(format_coins(10_005), "1 з 5 м");
        assert_eq!(format_coins(0), "0 м");
    }
    #[test]
    fn only_bag_and_body_weigh_and_unique_objects_do_not_stack() {
        let stacks = [
            Stack::new(3, 1, Location::Worn { slot: Slot::Armor }),
            Stack::new(9, 4, Location::Bag { cell: 0 }),
            Stack::new(GOLD, 900, Location::Purse),
            Stack::new(13, 50, Location::Ground { x: 0., z: 0. }),
        ];
        assert!((carried_weight(&stacks) - 7.).abs() < 1e-4);
        assert_eq!(purse_value(&stacks), 9_000_000);
        assert!(!stackable(1) && !stackable(CHEST) && !stackable(KEY));
        assert!(!stackable(CASTING));
        assert!(stackable(9) && stackable(COPPER) && stackable(BRONZE));
        let old = Stack::new(BRONZE, 3, Location::Bag { cell: 1 });
        assert!(old.mass_mg.is_none());
        assert!((old.weight() - 0.3).abs() < 1e-4);
        let blank = Stack {
            id: 9,
            mass_mg: Some(1_400_000),
            ..Stack::new(CASTING, 1, Location::Bag { cell: 2 })
        };
        assert!((blank.weight() - 1.4).abs() < 1e-3);
        assert_eq!(slot(17), Some(Slot::Weapon));
        assert_eq!(slot(6), Some(Slot::Armor));
        assert_eq!(
            EquipmentAppearance::from_items(&stacks),
            EquipmentAppearance {
                weapon: None,
                armor: Some(3)
            }
        );
    }
}
