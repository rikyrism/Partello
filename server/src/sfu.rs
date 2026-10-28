//! LiveKit SFU for calls with more people than a peer-to-peer mesh carries.
//!
//! Partello stays in charge of who is in the call: a participant gets a
//! LiveKit access token only after the room accepted their `voice_join`, the
//! token is valid only for that room's call and only for publishing a camera
//! and a microphone, and people who are kicked or leave are removed from the
//! LiveKit room through its admin API. The API secret never leaves the server.

use std::time::Duration;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha2::Sha256;
use uuid::Uuid;

use crate::{config::LiveKitConfig, database::now_ms};

/// How long a join token may be used to connect. LiveKit refreshes the
/// session itself afterwards, so this only bounds a leaked, unused token.
const JOIN_TOKEN_TTL_SECS: i64 = 10 * 60;

pub struct Sfu {
    config: LiveKitConfig,
    http: reqwest::Client,
}

impl Sfu {
    pub fn new(config: LiveKitConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("reqwest client"),
        }
    }

    pub fn url(&self) -> &str {
        &self.config.url
    }

    /// LiveKit room name for a Partello room: its id, which is never shown
    /// to anyone, so knowing a room's link is not enough to find its call.
    fn room_name(room_id: Uuid) -> String {
        format!("partello-{}", room_id.simple())
    }

    pub fn join_token(&self, room_id: Uuid, participant_id: Uuid, display_name: &str) -> String {
        let now = now_ms() / 1000;
        self.sign(&json!({
            "iss": self.config.api_key,
            "sub": participant_id.to_string(),
            "name": display_name,
            "nbf": now - 10,
            "exp": now + JOIN_TOKEN_TTL_SECS,
            "video": {
                "room": Self::room_name(room_id),
                "roomJoin": true,
                "canPublish": true,
                "canSubscribe": true,
                "canPublishData": false,
                "canPublishSources": ["camera", "microphone"],
            },
        }))
    }

    fn admin_token(&self, room_id: Uuid) -> String {
        let now = now_ms() / 1000;
        self.sign(&json!({
            "iss": self.config.api_key,
            "nbf": now - 10,
            "exp": now + 60,
            "video": { "room": Self::room_name(room_id), "roomAdmin": true },
        }))
    }

    /// HS256 JWT, the format LiveKit expects.
    fn sign(&self, claims: &serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
        let input = format!("{header}.{payload}");
        let mut mac = Hmac::<Sha256>::new_from_slice(self.config.api_secret.as_bytes()).expect("HMAC accepts any key length");
        mac.update(input.as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        format!("{input}.{sig}")
    }

    /// Disconnects someone from the call (kicked, left the room). Best effort:
    /// a participant who is not connected to LiveKit answers "not found".
    pub async fn remove(&self, room_id: Uuid, participant_id: Uuid) {
        let url = format!("{}/twirp/livekit.RoomService/RemoveParticipant", self.config.api_url);
        let body = json!({ "room": Self::room_name(room_id), "identity": participant_id.to_string() }).to_string();
        let res = self
            .http
            .post(url)
            .bearer_auth(self.admin_token(room_id))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await;
        match res {
            Ok(r) if r.status().is_success() || r.status() == reqwest::StatusCode::NOT_FOUND => {}
            Ok(r) => tracing::warn!(status = %r.status(), "livekit remove participant failed"),
            Err(err) => tracing::warn!(error = %err, "livekit unreachable"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sfu() -> Sfu {
        Sfu::new(LiveKitConfig {
            url: "wss://lk.example.org".into(),
            api_url: "https://lk.example.org".into(),
            api_key: "APIkey".into(),
            api_secret: "secret-that-is-long-enough-for-hs256".into(),
        })
    }

    fn claims(token: &str) -> serde_json::Value {
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap()
    }

    #[test]
    fn join_tokens_are_scoped_to_one_call() {
        let room = Uuid::new_v4();
        let pid = Uuid::new_v4();
        let token = sfu().join_token(room, pid, "Nita");
        let c = claims(&token);
        assert_eq!(c["iss"], "APIkey");
        assert_eq!(c["sub"], pid.to_string());
        assert_eq!(c["video"]["room"], format!("partello-{}", room.simple()));
        assert_eq!(c["video"]["roomJoin"], true);
        assert_eq!(c["video"]["canPublishSources"], json!(["camera", "microphone"]));
        assert!(c["video"].get("roomAdmin").is_none());
        assert!(c["exp"].as_i64().unwrap() - c["nbf"].as_i64().unwrap() <= JOIN_TOKEN_TTL_SECS + 10);
    }

    #[test]
    fn signature_matches_hs256() {
        let token = sfu().join_token(Uuid::nil(), Uuid::nil(), "x");
        let (input, sig) = token.rsplit_once('.').unwrap();
        let mut mac = Hmac::<Sha256>::new_from_slice(b"secret-that-is-long-enough-for-hs256").unwrap();
        mac.update(input.as_bytes());
        mac.verify_slice(&URL_SAFE_NO_PAD.decode(sig).unwrap()).unwrap();
    }
}
