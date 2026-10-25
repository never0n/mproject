//! Chat wire types: channels, client `Command`s (send, whisper, slash commands, ignore, report)
//! and server `Reply`s (messages, statuses, admin dialogs). Slash-command parsing is in `commands`.
//! Client: `client/src/chat.rs`; server: `server/src/chat.rs` and `console.rs`.
use serde::{Deserialize, Serialize};
pub mod commands;
/// Max message length (chars); `INPUT_LIMIT` is the max raw input including slash commands.
pub const LIMIT: usize = 280;
pub const INPUT_LIMIT: usize = 512;
/// "Local" channel reach in meters.
pub const LOCAL_RADIUS: f32 = 60.;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    #[default]
    General,
    Local,
    Group,
    Alliance,
    Private,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Send {
        channel: Channel,
        text: String,
    },
    Ignore {
        nickname: String,
        blocked: bool,
    },
    Report {
        id: i64,
    },
    Execute {
        text: String,
    },
    Admin {
        text: String,
        target: Option<crate::admin::Target>,
    },
    Confirm {
        token: String,
    },
    Whisper {
        nickname: String,
        text: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub recipient: Option<(i64, String)>,
    pub alliance: Option<i64>,
    pub party: Option<u64>,
    pub id: i64,
    pub sender: i64,
    pub nickname: String,
    pub channel: Channel,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Reply {
    Message {
        message: Message,
    },
    Status {
        message: String,
    },
    Notice {
        message: String,
    },
    Dialog {
        dialog: Dialog,
    },
    Announcement {
        message: String,
    },
    Logout,
    Acknowledged,
    Preferences {
        ignored: Vec<i64>,
        ignore_all: bool,
        silence: bool,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DialogKind {
    Help,
    IgnoreList,
    Confirm,
    GameSettings,
    ChatSettings,
    Admin,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Dialog {
    pub kind: DialogKind,
    pub title: String,
    pub text: String,
    pub entries: Vec<(i64, String)>,
    pub token: Option<String>,
    pub admin: Option<Box<crate::admin::View>>,
}
pub fn validate(text: &str) -> Result<&str, &'static str> {
    let text = text.trim();
    if text.is_empty()
        || text.chars().count() > LIMIT
        || text
            .chars()
            .any(|c| c.is_control() || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'))
    {
        return Err("Сообщение: 1–280 символов, без переноса строки");
    }
    Ok(text)
}
