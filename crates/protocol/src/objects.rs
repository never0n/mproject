//! Physical objects in the world: stacks lying on the ground and placed containers.
//! A container is shown to everyone; its contents only to someone who opened it with a key.
use crate::{items::Stack, transfer::ItemRequest};
use serde::{Deserialize, Serialize};

/// [PROPOSAL] Reach for picking up, opening and placing, metres.
/// Wide enough that a sack at the character's feet is usable from the normal camera.
pub const REACH: f32 = 6.;
/// Radius of the `Objects` list a client receives, metres.
pub const VIEW_RANGE: f32 = 80.;
/// [PROPOSAL] A chest holds one bag page of stacks and at most this many kilograms.
pub const CHEST_CELLS: u16 = 30;
pub const CHEST_WEIGHT: f32 = 300.;

#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// Lay part or all of a bag stack on the ground at the character's feet.
    Drop {
        item: i64,
        quantity: u32,
    },
    PickUp {
        item: i64,
    },
    /// Set a chest from the bag down at the character's feet. The server cuts a new lock and
    /// puts its only key into the bag.
    Place {
        item: i64,
    },
    Open {
        container: i64,
    },
    /// Move a bag stack and/or coins into the open container.
    Put {
        container: i64,
        revision: u64,
        item: Option<ItemRequest>,
        coins: u64,
    },
    Take {
        container: i64,
        revision: u64,
        item: Option<ItemRequest>,
        coins: u64,
    },
    /// Pick an empty chest back up as an item.
    Lift {
        container: i64,
        revision: u64,
    },
}
/// Public look of a placed container.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Container {
    pub id: i64,
    pub kind: u8,
    pub position: [f32; 2],
    pub yaw: f32,
    pub locked: bool,
    /// Set on a player stall: the price of the single offered stack, in copper.
    #[serde(default)]
    pub price: Option<u64>,
    /// Kind of the stack standing on that stall. Empty when nothing is offered.
    #[serde(default)]
    pub offer: Option<u8>,
    /// Character who placed the object, when it has one.
    #[serde(default)]
    pub owner: Option<i64>,
}
/// A stack lying on the ground.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Loose {
    pub id: i64,
    pub kind: u8,
    pub quantity: u32,
    pub position: [f32; 2],
}
/// The server's stored cells for one casting on the ground.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CastingBody {
    pub item: i64,
    pub spans: Vec<crate::casting::Span>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct View {
    pub container: Container,
    /// Bumped on every change of the contents; stale `Put`/`Take` requests are refused.
    pub revision: u64,
    pub cells: u16,
    pub items: Vec<Stack>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    pub view: Option<View>,
    pub message: String,
    pub update: Option<Box<crate::transfer::InventoryUpdate>>,
}
