//! Starter crafting balance. Recipes are retained; ingredients are consumed on success.
//! Anyone holding a recipe may craft it: there is no exclusive profession.

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
}
pub const RECIPES: &[Recipe] = &[
    Recipe {
        item: CUIRASS_RECIPE,
        result: 6,
        ingredients: &[(LEATHER, 6), (7, 4)],
    },
    Recipe {
        item: SWORD_RECIPE,
        result: STEEL_SWORD,
        ingredients: &[(IRON, 6), (LEATHER, 3)],
    },
    Recipe {
        item: STAFF_RECIPE,
        result: RUNE_STAFF,
        ingredients: &[(RUNE_SHARD, 6), (7, 3)],
    },
];
pub fn definition(item: u8) -> Option<&'static Recipe> {
    RECIPES.iter().find(|r| r.item == item)
}
impl Recipe {
    /// The recipe and the ingredients must lie in the bag.
    pub fn validate(&self, character: &crate::Character) -> Result<(), &'static str> {
        if in_bag(character, self.item) == 0 {
            return Err("Нужен соответствующий рецепт");
        }
        if self
            .ingredients
            .iter()
            .any(|(kind, count)| in_bag(character, *kind) < *count)
        {
            return Err("Недостаточно материалов");
        }
        Ok(())
    }
}
pub fn in_bag(character: &crate::Character, kind: u8) -> u32 {
    character
        .items
        .iter()
        .filter(|s| s.kind == kind && matches!(s.location, crate::items::Location::Bag { .. }))
        .map(|s| s.quantity)
        .fold(0, u32::saturating_add)
}
pub fn count(character: &crate::Character) -> u32 {
    character.stats.get(COUNTER).copied().unwrap_or(0.) as u32
}
