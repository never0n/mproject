//! Character mail. Sender identity always comes from the authenticated world session.
use serde::{Deserialize, Serialize};

pub const MAX_PRICE: u32 = 1_000_000;

pub const PAGE_SIZE: usize = 6;
pub const SUBJECT_LIMIT: usize = 128;
pub const BODY_LIMIT: usize = 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Folder {
    #[default]
    Inbox,
    Sent,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    List {
        folder: Folder,
        page: u32,
    },
    Read {
        id: i64,
    },
    Send {
        recipient: String,
        subject: String,
        body: String,
        draft: String,
        #[serde(default)]
        attachment: Option<AttachmentRequest>,
    },
    Delete {
        id: i64,
    },
    Return {
        id: i64,
    },
    Claim {
        id: i64,
        expected_price: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub return_of: Option<i64>,
    pub id: i64,
    pub sender: String,
    pub recipient: String,
    pub subject: String,
    pub sent_at: String,
    pub read: bool,
    pub attachment: Option<Attachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Letter {
    pub header: Header,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub folder: Folder,
    pub page: u32,
    pub total: u64,
    pub unread: u64,
    pub entries: Vec<Header>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Reply {
    List {
        page: Page,
    },
    Read {
        letter: Letter,
    },
    Sent {
        id: i64,
        draft: String,
        update: Option<Box<InventoryUpdate>>,
    },
    Returned {
        id: i64,
    },
    Claimed {
        id: i64,
        update: Box<InventoryUpdate>,
    },
    Deleted {
        id: i64,
    },
    Unread {
        count: u64,
    },
    Error {
        message: String,
    },
}

pub fn validate(recipient: &str, subject: &str, body: &str) -> Result<(), &'static str> {
    if !(2..=16).contains(&recipient.chars().count())
        || !recipient
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Укажите имя персонажа получателя");
    }
    if subject.trim().is_empty()
        || subject.chars().count() > SUBJECT_LIMIT
        || subject.chars().any(char::is_control)
    {
        return Err("Тема письма: от 1 до 128 символов без переноса строки");
    }
    if body.trim().is_empty()
        || body.chars().count() > BODY_LIMIT
        || body.chars().any(|c| c.is_control() && c != '\n')
    {
        return Err("Текст письма: от 1 до 1024 символов");
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentRequest {
    pub price: u32,
    pub token: u64,
    pub quantity: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub price: u32,
    pub kind: u8,
    pub quantity: u32,
    pub claimed: bool,
    pub returned: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryUpdate {
    pub character: crate::Character,
    pub actor: crate::Actor,
}
impl Reply {
    pub fn inventory_update(&self) -> Option<&InventoryUpdate> {
        match self {
            Self::Sent { update, .. } => update.as_deref(),
            Self::Claimed { update, .. } => Some(update),
            _ => None,
        }
    }
}
impl Command {
    pub fn changes_inventory(&self) -> bool {
        matches!(
            self,
            Self::Send {
                attachment: Some(_),
                ..
            } | Self::Claim { .. }
        )
    }
}
