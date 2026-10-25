//! Direct player exchange: one stack (by id) and coins (copper value) on each side.
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Invite {
        player: i64,
    },
    Accept {
        session: u64,
        revision: u64,
    },
    Offer {
        session: u64,
        revision: u64,
        item: Option<crate::transfer::ItemRequest>,
        coins: u64,
    },
    Confirm {
        session: u64,
        revision: u64,
    },
    Cancel {
        session: u64,
    },
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Offer {
    pub item: Option<crate::transfer::ItemRequest>,
    pub kind: u8,
    pub coins: u64,
    pub confirmed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct View {
    pub session: u64,
    pub revision: u64,
    pub peer: String,
    pub invited: bool,
    pub accepted: bool,
    pub own: Offer,
    pub other: Offer,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    pub view: Option<View>,
    pub message: String,
    pub update: Option<Box<crate::transfer::InventoryUpdate>>,
}
