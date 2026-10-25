//! Alliance (guild) wire types: roles, commands, member list and invitations.
//! Rules and DB work live in `server/src/alliance.rs`; UI in `client/src/alliance.rs`.
use serde::{Deserialize, Serialize};

/// Max members, creation price (coins), invitation lifetime (s).
pub const LIMIT: usize = 20;
pub const CREATE_COST: u64 = 1000;
pub const INVITE_SECONDS: u32 = 120;

/// [CANON] one membership per type. The full classifier is still open; these three are named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Faction,
    #[default]
    Guild,
    Order,
}
impl Kind {
    pub fn code(self) -> i16 {
        match self {
            Self::Faction => 1,
            Self::Guild => 2,
            Self::Order => 3,
        }
    }
    pub fn parse(code: i16) -> Self {
        match code {
            1 => Self::Faction,
            3 => Self::Order,
            _ => Self::Guild,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Faction => "Фракция",
            Self::Guild => "Гильдия",
            Self::Order => "Орден",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Leader,
    Officer,
    Member,
}
impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Self::Leader => "Лидер",
            Self::Officer => "Офицер",
            Self::Member => "Участник",
        }
    }
    pub fn can_invite(self) -> bool {
        self != Self::Member
    }
    pub fn can_kick(self, target: Self) -> bool {
        self == Self::Leader && target != Self::Leader
            || self == Self::Officer && target == Self::Member
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Refresh,
    Create {
        name: String,
        #[serde(default)]
        kind: Kind,
    },
    Invite {
        alliance: i64,
        nickname: String,
    },
    Accept {
        invitation: i64,
    },
    Decline {
        invitation: i64,
    },
    Leave {
        alliance: i64,
    },
    Kick {
        alliance: i64,
        player: i64,
    },
    SetOfficer {
        alliance: i64,
        player: i64,
        officer: bool,
    },
    Transfer {
        alliance: i64,
        player: i64,
    },
    Dissolve {
        alliance: i64,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub id: i64,
    pub nickname: String,
    pub level: u32,
    pub role: Role,
    pub online: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct View {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub kind: Kind,
    pub members: Vec<Member>,
}
impl View {
    pub fn role(&self, owner: i64) -> Option<Role> {
        self.members.iter().find(|m| m.id == owner).map(|m| m.role)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invitation {
    pub id: i64,
    pub alliance: i64,
    pub name: String,
    pub inviter: String,
    pub seconds_left: u32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Reply {
    pub response: bool,
    pub alliance: Option<View>,
    /// Every current membership. A combat party is not one of these.
    #[serde(default)]
    pub organizations: Vec<View>,
    pub invitation: Option<Invitation>,
    pub message: String,
}

pub fn validate_name(name: &str) -> Result<&str, &'static str> {
    let name = name.trim();
    if !(2..=24).contains(&name.chars().count())
        || !name.chars().any(char::is_alphabetic)
        || name
            .chars()
            .any(|c| !c.is_alphanumeric() && !matches!(c, ' ' | '-' | '\''))
    {
        return Err("Название: 2–24 символа, буквы, цифры, пробел, дефис или апостроф");
    }
    Ok(name)
}
