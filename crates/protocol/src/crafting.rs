//! Starter crafting balance. Recipes are retained; ingredients are consumed on success.
use crate::professions::Profession;
use std::collections::BTreeMap;

pub const LEATHER: u8 = 9;
pub const CUIRASS_RECIPE: u8 = 10;
pub const IRON: u8 = 13;
pub const RUNE_SHARD: u8 = 14;
pub const SWORD_RECIPE: u8 = 15;
pub const STAFF_RECIPE: u8 = 16;
pub const STEEL_SWORD: u8 = 17;
pub const RUNE_STAFF: u8 = 18;
pub const COUNTER: &str = "craft_count";

pub struct Recipe {
    pub item: u8,
    pub result: u8,
    pub ingredients: &'static [(u8, u32)],
    pub profession: Profession,
}
pub const RECIPES: &[Recipe] = &[
    Recipe {
        item: CUIRASS_RECIPE,
        result: 6,
        ingredients: &[(LEATHER, 6), (7, 4)],
        profession: Profession::Leatherworking,
    },
    Recipe {
        item: SWORD_RECIPE,
        result: STEEL_SWORD,
        ingredients: &[(IRON, 6), (LEATHER, 3)],
        profession: Profession::Smithing,
    },
    Recipe {
        item: STAFF_RECIPE,
        result: RUNE_STAFF,
        ingredients: &[(RUNE_SHARD, 6), (7, 3)],
        profession: Profession::Runecraft,
    },
];
pub fn definition(item: u8) -> Option<&'static Recipe> {
    RECIPES.iter().find(|r| r.item == item)
}
impl Recipe {
    pub fn requirements(&self, stats: &BTreeMap<String, f64>) -> Vec<(u8, u32)> {
        let (primary, secondary) = self.profession.savings(stats);
        self.ingredients
            .iter()
            .enumerate()
            .map(|(index, &(kind, count))| {
                (
                    kind,
                    count
                        .saturating_sub(if index == 0 { primary } else { secondary })
                        .max(1),
                )
            })
            .collect()
    }
    pub fn validate(&self, loot: &BTreeMap<u8, u32>) -> Result<(), &'static str> {
        self.validate_with(loot, &BTreeMap::new())
    }
    pub fn validate_with(
        &self,
        loot: &BTreeMap<u8, u32>,
        stats: &BTreeMap<String, f64>,
    ) -> Result<(), &'static str> {
        if loot.get(&self.item).copied().unwrap_or(0) == 0 {
            return Err("Нужен соответствующий рецепт из лавки у площади");
        }
        if self
            .requirements(stats)
            .iter()
            .any(|(kind, count)| loot.get(kind).copied().unwrap_or(0) < *count)
        {
            return Err("Недостаточно материалов");
        }
        if loot.get(&self.result) == Some(&u32::MAX) {
            return Err("Достигнут предел количества предметов");
        }
        Ok(())
    }
}
pub fn count(character: &crate::Character) -> u32 {
    character.stats.get(COUNTER).copied().unwrap_or(0.) as u32
}
