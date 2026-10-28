//! Room, participant, and chat types shared by the store, REST, and realtime layers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ControlMode {
    /// Only the host can play, pause, seek, or skip.
    #[default]
    HostOnly,
    Everyone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum QueueMode {
    /// Anyone can add; people remove their own items; the host removes anything.
    #[default]
    Everyone,
    HostOnly,
}

impl ControlMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ControlMode::HostOnly => "host_only",
            ControlMode::Everyone => "everyone",
        }
    }
    pub fn parse(s: &str) -> Self {
        if s == "everyone" {
            ControlMode::Everyone
        } else {
            ControlMode::HostOnly
        }
    }
}

impl QueueMode {
    pub fn as_str(self) -> &'static str {
        match self {
            QueueMode::HostOnly => "host_only",
            QueueMode::Everyone => "everyone",
        }
    }
    pub fn parse(s: &str) -> Self {
        if s == "host_only" {
            QueueMode::HostOnly
        } else {
            QueueMode::Everyone
        }
    }
}

/// Missing fields fall back to their defaults, so clients may send a partial object.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct RoomSettings {
    pub control_mode: ControlMode,
    pub queue_mode: QueueMode,
    /// Kept forever (never purged for inactivity), so the link stays valid.
    pub permanent: bool,
}

/// A room as stored in the database.
#[derive(Debug, Clone)]
pub struct RoomRecord {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub host_key_hash: Vec<u8>,
    pub settings: RoomSettings,
    pub max_participants: u32,
    pub api_key_id: Option<Uuid>,
    pub created_at: i64,
    /// Argon2id PHC string of the room PIN, when set.
    pub password_hash: Option<String>,
}

/// Public room metadata.
#[derive(Debug, Clone, Serialize)]
pub struct RoomInfo {
    pub slug: String,
    pub name: String,
    pub settings: RoomSettings,
    pub max_participants: u32,
    pub created_at: i64,
    pub has_password: bool,
}

impl From<&RoomRecord> for RoomInfo {
    fn from(r: &RoomRecord) -> Self {
        Self {
            slug: r.slug.clone(),
            name: r.name.clone(),
            settings: r.settings,
            max_participants: r.max_participants,
            created_at: r.created_at,
            has_password: r.password_hash.is_some(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct VoiceState {
    pub muted: bool,
    pub camera: bool,
}

/// What everyone in the room sees about a participant.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Participant {
    pub id: Uuid,
    pub display_name: String,
    pub is_host: bool,
    /// False while the participant is inside the reconnect grace window.
    pub connected: bool,
    pub joined_at: i64,
    /// `None` when not in the call.
    pub voice: Option<VoiceState>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub id: Uuid,
    pub participant_id: Uuid,
    pub display_name: String,
    pub body: String,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<ReplyRef>,
    /// Echo of the sender's client nonce so it can reconcile its optimistic copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

/// A quoted excerpt of the message a reply answers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplyRef {
    pub id: Uuid,
    pub participant_id: Uuid,
    pub display_name: String,
    pub body: String,
}
