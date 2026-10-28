//! WebSocket wire protocol. Every frame is a JSON object with a `type` field.
//! The schema is documented in docs/WEBSOCKET.md and mirrored in
//! web/src/realtime/protocol.ts; keep the three in sync.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::room::{
    model::{ChatMessage, Participant, RoomInfo},
    playback::Playback,
    queue::QueueItem,
};

/// Max length of client-chosen correlation ids.
pub const NONCE_MAX: usize = 64;
/// Max serialized size of one WebRTC signal (SDP with many candidates fits easily).
pub const SIGNAL_MAX_BYTES: usize = 24 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackAction {
    Play,
    Pause,
    Seek,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientEvent {
    Auth { token: String },
    Ping { t: f64 },
    ChatSend { body: String, nonce: Option<String> },
    PlaybackControl { action: PlaybackAction, position: f64 },
    PlaybackDuration { item_id: Uuid, duration: f64 },
    PlaybackEnded { item_id: Uuid },
    QueueAdd { url: String, nonce: Option<String> },
    QueueRemove { item_id: Uuid },
    QueuePlay { item_id: Uuid },
    QueueMove { item_id: Uuid, to_index: usize },
    QueueSkip,
    VoiceJoin { muted: bool, camera: bool },
    VoiceLeave,
    VoiceUpdate { muted: bool, camera: bool },
    VoiceSignal { to: Uuid, data: serde_json::Value },
    HostTransfer { to: Uuid },
    Kick { participant_id: Uuid },
    Leave,
}

impl ClientEvent {
    /// Name used for logging and rate-limit buckets.
    pub fn kind(&self) -> &'static str {
        match self {
            ClientEvent::Auth { .. } => "auth",
            ClientEvent::Ping { .. } => "ping",
            ClientEvent::ChatSend { .. } => "chat_send",
            ClientEvent::PlaybackControl { .. } => "playback_control",
            ClientEvent::PlaybackDuration { .. } => "playback_duration",
            ClientEvent::PlaybackEnded { .. } => "playback_ended",
            ClientEvent::QueueAdd { .. } => "queue_add",
            ClientEvent::QueueRemove { .. } => "queue_remove",
            ClientEvent::QueuePlay { .. } => "queue_play",
            ClientEvent::QueueMove { .. } => "queue_move",
            ClientEvent::QueueSkip => "queue_skip",
            ClientEvent::VoiceJoin { .. } => "voice_join",
            ClientEvent::VoiceLeave => "voice_leave",
            ClientEvent::VoiceUpdate { .. } => "voice_update",
            ClientEvent::VoiceSignal { .. } => "voice_signal",
            ClientEvent::HostTransfer { .. } => "host_transfer",
            ClientEvent::Kick { .. } => "kick",
            ClientEvent::Leave => "leave",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LimitsView {
    pub chat_max_chars: usize,
    pub max_voice_participants: u32,
    pub max_queue_len: usize,
    /// Whether `GET /api/rooms/{slug}/search` is configured.
    pub search_enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaveReason {
    Left,
    Disconnected,
    Kicked,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    RoomStateSync {
        self_id: Uuid,
        room: RoomInfo,
        participants: Vec<Participant>,
        playback: Playback,
        queue: Vec<QueueItem>,
        chat: Vec<ChatMessage>,
        ice_servers: Vec<IceServer>,
        limits: LimitsView,
        server_time: i64,
    },
    ParticipantJoined {
        participant: Participant,
    },
    ParticipantLeft {
        participant_id: Uuid,
        display_name: String,
        reason: LeaveReason,
    },
    ParticipantUpdated {
        participant: Participant,
    },
    PlaybackChanged {
        playback: Playback,
        actor_id: Option<Uuid>,
        server_time: i64,
        /// Echo of the actor's `queue_add` nonce when this change completes it.
        #[serde(skip_serializing_if = "Option::is_none")]
        nonce: Option<String>,
    },
    QueueUpdated {
        queue: Vec<QueueItem>,
        actor_id: Option<Uuid>,
        #[serde(skip_serializing_if = "Option::is_none")]
        nonce: Option<String>,
    },
    ChatMessageCreated {
        message: ChatMessage,
    },
    VoiceSignal {
        from: Uuid,
        data: serde_json::Value,
    },
    RoomSettingsChanged {
        room: RoomInfo,
    },
    Pong {
        t: f64,
        server_time: i64,
    },
    Error {
        code: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        nonce: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_events() {
        let e: ClientEvent = serde_json::from_str(r#"{"type":"chat_send","body":"hi","nonce":"n1"}"#).unwrap();
        assert!(matches!(e, ClientEvent::ChatSend { ref body, .. } if body == "hi"));
        let e: ClientEvent = serde_json::from_str(r#"{"type":"queue_skip"}"#).unwrap();
        assert!(matches!(e, ClientEvent::QueueSkip));
        let e: ClientEvent = serde_json::from_str(r#"{"type":"playback_control","action":"seek","position":12.5}"#).unwrap();
        assert!(matches!(
            e,
            ClientEvent::PlaybackControl {
                action: PlaybackAction::Seek,
                ..
            }
        ));
    }

    #[test]
    fn rejects_malformed_events() {
        for raw in [
            r#"{}"#,
            r#"{"type":"nope"}"#,
            r#"{"type":"chat_send"}"#,
            r#"{"type":"chat_send","body":42}"#,
            r#"{"type":"playback_control","action":"rewind","position":1}"#,
            r#"{"type":"playback_control","action":"play","position":"1"}"#,
            r#"{"type":"voice_signal","to":"not-a-uuid","data":{}}"#,
            r#"[1,2,3]"#,
            r#"not json"#,
        ] {
            assert!(serde_json::from_str::<ClientEvent>(raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn server_events_are_tagged() {
        let json = serde_json::to_value(ServerEvent::Pong { t: 1.0, server_time: 2 }).unwrap();
        assert_eq!(json["type"], "pong");
        assert_eq!(json["server_time"], 2);
    }
}
