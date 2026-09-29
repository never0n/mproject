use serde::{Deserialize, Serialize};
pub const LIMIT: usize = 5;
pub const REWARD_RANGE: f32 = 60.;
pub const RECONNECT_SECONDS: u32 = 180;
#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Invite { player: i64 },
    Accept { invitation: u64 },
    Decline { invitation: u64 },
    Leave { party: u64 },
    Kick { party: u64, player: i64 },
    Promote { party: u64, player: i64 },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub id: i64,
    pub nickname: String,
    pub level: u32,
    pub online: bool,
    pub reconnect_seconds: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct View {
    pub id: u64,
    pub leader: i64,
    pub members: Vec<Member>,
    pub last_loot: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invitation {
    pub id: u64,
    pub inviter: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Reply {
    pub response: bool,
    pub group: Option<View>,
    pub invitation: Option<Invitation>,
    pub message: String,
}
