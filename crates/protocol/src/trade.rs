//! A sale identifies the bag copy that the player inspected, including its observed quantity.
use crate::{Character, layout};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Sale {
    pub slot: u16,
    pub token: u64,
    pub owned: u32,
}
impl Sale {
    pub fn validate(self, character: &Character, kind: u8) -> Result<(), &'static str> {
        if layout::kind(self.token) != Some(kind)
            || layout::quantity(character, kind) != self.owned
            || self.owned == 0
            || layout::bag(character).get(&self.slot).copied() != Some(self.token)
        {
            return Err("Вещь в рюкзаке изменилась. Выберите её заново");
        }
        Ok(())
    }
}
