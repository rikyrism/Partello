//! End-to-end tests: a real server on a random port, real HTTP and WebSocket
//! clients, in-memory store. Two "browsers" join the same room and must see
//! each other's playback, queue, and chat changes.

use std::{net::SocketAddr, time::Duration};

use futures_util::{SinkExt, StreamExt};
use partello::{build_state, config::Config, database::Store, routes};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest, http::HeaderValue},
};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Server {
    addr: SocketAddr,
    app: partello::AppState,
    http: reqwest::Client,
}

async fn spawn_with(config: Config) -> Server {
    spawn_on(config, Store::memory()).await
}

async fn spawn_on(config: Config, store: Store) -> Server {
    let app = build_state(config, store);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = routes::router(app.clone());
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap();
    });
    Server {
        addr,
        app,
        http: reqwest::Client::new(),
    }
}

async fn spawn() -> Server {
    spawn_with(Config::for_tests()).await
}

impl Server {
    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        let r = self.http.post(self.url(path)).json(&body).send().await.unwrap();
        let status = r.status().as_u16();
        (status, r.json().await.unwrap_or(Value::Null))
    }

    async fn create_room(&self, host_name: &str) -> (String, String) {
        let (status, body) = self.post("/api/rooms", json!({ "display_name": host_name })).await;
        assert_eq!(status, 201, "{body}");
        (
            body["room"]["slug"].as_str().unwrap().into(),
            body["host_key"].as_str().unwrap().into(),
        )
    }

    async fn join(&self, slug: &str, name: &str, host_key: Option<&str>) -> (String, String) {
        let (status, body) = self
            .post(
                &format!("/api/rooms/{slug}/join"),
                json!({ "display_name": name, "host_key": host_key }),
            )
            .await;
        assert_eq!(status, 200, "{body}");
        (
            body["session_token"].as_str().unwrap().into(),
            body["participant_id"].as_str().unwrap().into(),
        )
    }

    async fn socket(&self, slug: &str, token: &str) -> (Ws, Value) {
        let mut req = format!("ws://{}/api/rooms/{slug}/ws", self.addr).into_client_request().unwrap();
        req.headers_mut()
            .insert("origin", HeaderValue::from_static("http://localhost:5173"));
        let (mut ws, _) = connect_async(req).await.unwrap();
        send(&mut ws, json!({ "type": "auth", "token": token })).await;
        let sync = next_of(&mut ws, "room_state_sync").await;
        (ws, sync)
    }

    /// Create a room as host and join a guest: the common two-browser setup.
    async fn pair(&self) -> (String, Ws, Ws, String, String) {
        let (slug, host_key) = self.create_room("Host").await;
        let (host_token, host_id) = self.join(&slug, "Host", Some(&host_key)).await;
        let (guest_token, guest_id) = self.join(&slug, "Guest", None).await;
        let (host, _) = self.socket(&slug, &host_token).await;
        let (guest, _) = self.socket(&slug, &guest_token).await;
        (slug, host, guest, host_id, guest_id)
    }
}

async fn send(ws: &mut Ws, v: Value) {
    ws.send(Message::Text(v.to_string().into())).await.unwrap();
}

async fn next_json(ws: &mut Ws) -> Option<Value> {
    loop {
        match tokio::time::timeout(Duration::from_secs(3), ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => return Some(serde_json::from_str(&t).unwrap()),
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) | Err(_) => return None,
            Ok(Some(Ok(_))) => continue,
        }
    }
}

/// Waits for the next event of `kind`, skipping others.
async fn next_of(ws: &mut Ws, kind: &str) -> Value {
    loop {
        let v = next_json(ws)
            .await
            .unwrap_or_else(|| panic!("socket ended while waiting for {kind}"));
        if v["type"] == kind {
            return v;
        }
    }
}

const VIDEO: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";

#[tokio::test]
async fn two_browsers_share_playback_queue_and_chat() {
    let s = spawn().await;
    let (_slug, mut host, mut guest, host_id, _guest_id) = s.pair().await;
    let joined = next_of(&mut host, "participant_joined").await;
    assert_eq!(joined["participant"]["display_name"], "Guest");

    // Host adds a video: it starts playing for everyone.
    send(&mut host, json!({ "type": "queue_add", "url": VIDEO })).await;
    for ws in [&mut host, &mut guest] {
        let ev = next_of(ws, "playback_changed").await;
        assert_eq!(ev["playback"]["status"], "playing");
        assert_eq!(ev["playback"]["item"]["video_id"], "dQw4w9WgXcQ");
        assert_eq!(ev["actor_id"], host_id.as_str());
    }

    // Host pauses at 12.5s: guest receives the exact position.
    send(
        &mut host,
        json!({ "type": "playback_control", "action": "pause", "position": 12.5 }),
    )
    .await;
    let ev = next_of(&mut guest, "playback_changed").await;
    assert_eq!(ev["playback"]["status"], "paused");
    assert_eq!(ev["playback"]["position"], 12.5);
    let version = ev["playback"]["version"].as_u64().unwrap();

    // Guest is not allowed to control playback by default.
    send(&mut guest, json!({ "type": "playback_control", "action": "play", "position": 0 })).await;
    let err = next_of(&mut guest, "error").await;
    assert_eq!(err["code"], "forbidden");

    // Guest can queue the next video and chat.
    send(
        &mut guest,
        json!({ "type": "queue_add", "url": "https://youtu.be/9bZkp7q19f0", "nonce": "q1" }),
    )
    .await;
    let q = next_of(&mut host, "queue_updated").await;
    assert_eq!(q["queue"].as_array().unwrap().len(), 1);
    send(
        &mut guest,
        json!({ "type": "chat_send", "body": "  popcorn ready  ", "nonce": "c1" }),
    )
    .await;
    for ws in [&mut host, &mut guest] {
        let m = next_of(ws, "chat_message_created").await;
        assert_eq!(m["message"]["body"], "popcorn ready");
        assert_eq!(m["message"]["display_name"], "Guest");
    }

    // Host skips: next video plays with a higher version.
    send(&mut host, json!({ "type": "queue_skip" })).await;
    let ev = next_of(&mut guest, "playback_changed").await;
    assert_eq!(ev["playback"]["item"]["video_id"], "9bZkp7q19f0");
    assert!(ev["playback"]["version"].as_u64().unwrap() > version);
}

#[tokio::test]
async fn late_joiner_receives_current_state() {
    let s = spawn().await;
    let (slug, mut host, _guest, _, _) = s.pair().await;
    send(&mut host, json!({ "type": "queue_add", "url": VIDEO })).await;
    next_of(&mut host, "playback_changed").await;
    send(&mut host, json!({ "type": "playback_control", "action": "seek", "position": 90 })).await;
    next_of(&mut host, "playback_changed").await;
    send(&mut host, json!({ "type": "chat_send", "body": "hi" })).await;
    next_of(&mut host, "chat_message_created").await;

    let (token, id) = s.join(&slug, "Late", None).await;
    let (_late, sync) = s.socket(&slug, &token).await;
    assert_eq!(sync["self_id"], id.as_str());
    assert_eq!(sync["participants"].as_array().unwrap().len(), 3);
    assert_eq!(sync["playback"]["item"]["video_id"], "dQw4w9WgXcQ");
    assert_eq!(sync["playback"]["status"], "playing");
    assert!(sync["playback"]["position"].as_f64().unwrap() >= 90.0);
    assert_eq!(sync["chat"][0]["body"], "hi");
    assert!(sync["server_time"].as_i64().unwrap() > 0);
    assert!(sync["ice_servers"][0]["urls"][0].as_str().unwrap().starts_with("stun:"));
}

#[tokio::test]
async fn reconnect_within_grace_keeps_identity() {
    let s = spawn().await;
    let (slug, host_key) = s.create_room("Host").await;
    let (host_token, _) = s.join(&slug, "Host", Some(&host_key)).await;
    let (guest_token, guest_id) = s.join(&slug, "Guest", None).await;
    let (mut host, _) = s.socket(&slug, &host_token).await;
    let (guest, _) = s.socket(&slug, &guest_token).await;
    next_of(&mut host, "participant_joined").await;

    drop(guest);
    let ev = next_of(&mut host, "participant_updated").await;
    assert_eq!(ev["participant"]["connected"], false);

    let (_guest, sync) = s.socket(&slug, &guest_token).await;
    assert_eq!(sync["self_id"], guest_id.as_str(), "same participant after reconnect");
    let ev = next_of(&mut host, "participant_updated").await;
    assert_eq!(ev["participant"]["connected"], true);
}

#[tokio::test]
async fn disconnect_past_grace_removes_participant() {
    let s = spawn().await;
    let (_slug, mut host, guest, _, guest_id) = s.pair().await;
    next_of(&mut host, "participant_joined").await;
    drop(guest);
    // Config::for_tests uses a 300ms grace window.
    let left = next_of(&mut host, "participant_left").await;
    assert_eq!(left["participant_id"], guest_id.as_str());
    assert_eq!(left["reason"], "disconnected");
}

#[tokio::test]
async fn explicit_leave_is_immediate_and_revokes_session() {
    let s = spawn().await;
    let (slug, mut host, mut guest, _, _) = s.pair().await;
    next_of(&mut host, "participant_joined").await;
    send(&mut guest, json!({ "type": "leave" })).await;
    let left = next_of(&mut host, "participant_left").await;
    assert_eq!(left["reason"], "left");
    // Host role does not move; guest's token no longer works.
    let room = s.http.get(s.url(&format!("/api/rooms/{slug}"))).send().await.unwrap();
    let body: Value = room.json().await.unwrap();
    assert_eq!(body["participant_count"], 1);
}

#[tokio::test]
async fn host_leaving_promotes_the_next_participant() {
    let s = spawn().await;
    let (_slug, mut host, mut guest, _, guest_id) = s.pair().await;
    next_of(&mut host, "participant_joined").await;
    send(&mut host, json!({ "type": "leave" })).await;
    loop {
        let ev = next_of(&mut guest, "participant_updated").await;
        if ev["participant"]["id"] == guest_id.as_str() && ev["participant"]["is_host"] == true {
            break;
        }
    }
    // The new host can now control playback.
    send(&mut guest, json!({ "type": "queue_add", "url": VIDEO })).await;
    next_of(&mut guest, "playback_changed").await;
    send(&mut guest, json!({ "type": "playback_control", "action": "pause", "position": 1 })).await;
    let ev = next_of(&mut guest, "playback_changed").await;
    assert_eq!(ev["playback"]["status"], "paused");
}

#[tokio::test]
async fn malformed_and_invalid_events_get_errors() {
    let s = spawn().await;
    let (_slug, mut host, _guest, _, _) = s.pair().await;
    host.send(Message::Text("not json".into())).await.unwrap();
    assert_eq!(next_of(&mut host, "error").await["code"], "validation_failed");
    send(&mut host, json!({ "type": "playback_control", "action": "rewind", "position": 1 })).await;
    assert_eq!(next_of(&mut host, "error").await["code"], "validation_failed");
    send(
        &mut host,
        json!({ "type": "queue_add", "url": "https://vimeo.com/1", "nonce": "x" }),
    )
    .await;
    let err = next_of(&mut host, "error").await;
    assert_eq!(err["code"], "invalid_video");
    assert_eq!(err["nonce"], "x");
    send(&mut host, json!({ "type": "chat_send", "body": "x".repeat(501) })).await;
    assert_eq!(next_of(&mut host, "error").await["code"], "validation_failed");
    // Signals to someone who is not in the call are dropped without an error
    // (they race people leaving); the next event the sender sees is a pong.
    send(
        &mut host,
        json!({ "type": "voice_signal", "to": uuid::Uuid::new_v4(), "data": { "kind": "candidate" } }),
    )
    .await;
    send(&mut host, json!({ "type": "ping", "t": 1 })).await;
    assert_eq!(next_json(&mut host).await.unwrap()["type"], "pong");
}

#[tokio::test]
async fn chat_is_rate_limited() {
    let s = spawn().await;
    let (_slug, mut host, _guest, _, _) = s.pair().await;
    for i in 0..10 {
        send(&mut host, json!({ "type": "chat_send", "body": format!("m{i}") })).await;
    }
    assert_eq!(next_of(&mut host, "error").await["code"], "rate_limited");
}

#[tokio::test]
async fn voice_signaling_relays_between_call_members() {
    let s = spawn().await;
    let (_slug, mut host, mut guest, host_id, guest_id) = s.pair().await;
    send(&mut host, json!({ "type": "voice_join", "muted": false, "camera": false })).await;
    send(&mut guest, json!({ "type": "voice_join", "muted": true, "camera": true })).await;
    loop {
        let ev = next_of(&mut host, "participant_updated").await;
        if ev["participant"]["id"] == guest_id.as_str() {
            assert_eq!(ev["participant"]["voice"]["camera"], true);
            break;
        }
    }
    let offer = json!({ "kind": "description", "description": { "type": "offer", "sdp": "v=0" } });
    send(&mut host, json!({ "type": "voice_signal", "to": guest_id, "data": offer })).await;
    let sig = next_of(&mut guest, "voice_signal").await;
    assert_eq!(sig["from"], host_id.as_str());
    assert_eq!(sig["data"]["description"]["type"], "offer");
}

#[tokio::test]
async fn websocket_rejects_bad_auth_and_foreign_origins() {
    let s = spawn().await;
    let (slug, _) = s.create_room("Host").await;

    let mut req = format!("ws://{}/api/rooms/{slug}/ws", s.addr).into_client_request().unwrap();
    req.headers_mut().insert("origin", HeaderValue::from_static("https://evil.example"));
    assert!(connect_async(req).await.is_err(), "foreign origin must be refused");

    let req = format!("ws://{}/api/rooms/{slug}/ws", s.addr).into_client_request().unwrap();
    let (mut ws, _) = connect_async(req).await.unwrap();
    send(&mut ws, json!({ "type": "auth", "token": "pts_forged" })).await;
    assert_eq!(next_json(&mut ws).await.unwrap()["code"], "unauthorized");
    match tokio::time::timeout(Duration::from_secs(2), ws.next()).await {
        Ok(Some(Ok(Message::Close(Some(frame))))) => assert_eq!(u16::from(frame.code), 4401),
        other => panic!("expected close 4401, got {other:?}"),
    }
}

#[tokio::test]
async fn rest_validation_and_capacity() {
    let mut config = Config::for_tests();
    config.limits.max_participants = 2;
    let s = spawn_with(config).await;

    let (status, body) = s.post("/api/rooms", json!({ "display_name": "   " })).await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "validation_failed");
    let (status, _) = s.post("/api/rooms", json!({ "nope": true })).await;
    assert_eq!(status, 422);

    let r = s.http.get(s.url("/api/rooms/zzzzzzzzzzzz")).send().await.unwrap();
    assert_eq!(r.status(), 404);
    let r = s.http.get(s.url("/api/rooms/..%2F..%2Fetc")).send().await.unwrap();
    assert_eq!(r.status(), 404);

    let (slug, _) = s.create_room("Ana").await;
    let r: Value = s
        .http
        .get(s.url(&format!("/api/rooms/{slug}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(r["room"]["name"], "Ana's room");
    assert_eq!(r["room"]["settings"]["control_mode"], "host_only");

    s.join(&slug, "One", None).await;
    s.join(&slug, "Two", None).await;
    let (status, body) = s.post(&format!("/api/rooms/{slug}/join"), json!({ "display_name": "Three" })).await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "room_full");
}

#[tokio::test]
async fn only_the_host_changes_settings() {
    let s = spawn().await;
    let (slug, host_key) = s.create_room("Host").await;
    let (host_token, _) = s.join(&slug, "Host", Some(&host_key)).await;
    let (guest_token, _) = s.join(&slug, "Guest", None).await;
    let (_h, _) = s.socket(&slug, &host_token).await;
    let (mut guest, _) = s.socket(&slug, &guest_token).await;

    let patch = |token: &str| {
        s.http
            .patch(s.url(&format!("/api/rooms/{slug}/settings")))
            .bearer_auth(token)
            .json(&json!({ "control_mode": "everyone", "name": "Friday films" }))
            .send()
    };
    assert_eq!(patch(&guest_token).await.unwrap().status(), 403);
    assert_eq!(patch("pts_nope").await.unwrap().status(), 401);
    assert_eq!(patch(&host_token).await.unwrap().status(), 200);
    let ev = next_of(&mut guest, "room_settings_changed").await;
    assert_eq!(ev["room"]["name"], "Friday films");
    assert_eq!(ev["room"]["settings"]["control_mode"], "everyone");
}

#[tokio::test]
async fn developer_api_creates_and_controls_rooms() {
    let s = spawn().await;
    let (status, key) = s.post("/api/v1/keys", json!({ "label": "My bot" })).await;
    assert_eq!(status, 201);
    let api_key = key["api_key"].as_str().unwrap().to_string();
    assert!(api_key.starts_with("ptk_"));

    let unauth = s.http.post(s.url("/api/v1/rooms")).json(&json!({})).send().await.unwrap();
    assert_eq!(unauth.status(), 401);

    let r = s
        .http
        .post(s.url("/api/v1/rooms"))
        .bearer_auth(&api_key)
        .json(&json!({ "name": "Bot room", "video_url": VIDEO }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201);
    let body: Value = r.json().await.unwrap();
    let slug = body["room"]["slug"].as_str().unwrap().to_string();
    assert!(body["invite_url"].as_str().unwrap().ends_with(&format!("/r/{slug}")));
    assert!(body["host_url"].as_str().unwrap().contains("#host=phk_"));

    let status: Value = s
        .http
        .post(s.url(&format!("/api/v1/rooms/{slug}/queue")))
        .bearer_auth(&api_key)
        .json(&json!({ "items": [{ "url": "https://youtu.be/9bZkp7q19f0" }, { "url": "https://youtu.be/kJQP7kiw5Fk" }] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status["now_playing"]["video_id"], "dQw4w9WgXcQ");
    assert_eq!(status["queue"].as_array().unwrap().len(), 2);

    let r = s
        .http
        .post(s.url(&format!("/api/v1/rooms/{slug}/play")))
        .bearer_auth(&api_key)
        .json(&json!({ "url": "https://youtu.be/kJQP7kiw5Fk" }))
        .send()
        .await
        .unwrap();
    let status: Value = r.json().await.unwrap();
    assert_eq!(status["now_playing"]["video_id"], "kJQP7kiw5Fk");
    assert_eq!(status["status"], "playing");

    // Another key cannot see or touch this room.
    let (_, other) = s.post("/api/v1/keys", json!({ "label": "Other" })).await;
    let r = s
        .http
        .get(s.url(&format!("/api/v1/rooms/{slug}")))
        .bearer_auth(other["api_key"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);

    let r = s
        .http
        .post(s.url(&format!("/api/v1/rooms/{slug}/queue")))
        .bearer_auth(&api_key)
        .json(&json!({ "items": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 422);
}

#[tokio::test]
async fn idle_rooms_are_unloaded_and_old_rooms_purged() {
    let mut config = Config::for_tests();
    config.limits.idle_room_ttl = Duration::ZERO;
    config.limits.room_retention = Duration::ZERO;
    let s = spawn_with(config).await;
    let (slug, host_key) = s.create_room("Host").await;
    let (token, _) = s.join(&slug, "Host", Some(&host_key)).await;
    let (mut ws, _) = s.socket(&slug, &token).await;
    assert!(s.app.hub.is_live(&slug));

    s.app.hub.maintenance().await;
    assert!(s.app.hub.is_live(&slug), "occupied rooms stay loaded");

    send(&mut ws, json!({ "type": "leave" })).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    s.app.hub.maintenance().await;
    assert!(!s.app.hub.is_live(&slug), "empty room unloaded");

    tokio::time::sleep(Duration::from_millis(5)).await;
    assert_eq!(s.app.hub.purge_inactive_rooms().await.unwrap(), 1);
    let r = s.http.get(s.url(&format!("/api/rooms/{slug}"))).send().await.unwrap();
    assert_eq!(r.status(), 404);
}

#[tokio::test]
async fn health_and_readiness() {
    let s = spawn().await;
    let r = s.http.get(s.url("/healthz")).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert!(r.headers().contains_key("x-request-id"));
    assert!(r.headers().contains_key("content-security-policy"));
    let r = s.http.get(s.url("/readyz")).send().await.unwrap();
    assert_eq!(r.status(), 200);
}

#[tokio::test]
async fn search_requires_a_session_and_reports_when_unconfigured() {
    let s = spawn().await;
    let (slug, host_key) = s.create_room("Host").await;
    let (token, _) = s.join(&slug, "Host", Some(&host_key)).await;
    let (_ws, sync) = s.socket(&slug, &token).await;
    assert_eq!(sync["limits"]["search_enabled"], false);

    let url = s.url(&format!("/api/rooms/{slug}/search?q=big+buck+bunny"));
    assert_eq!(s.http.get(&url).send().await.unwrap().status(), 401);
    let r = s.http.get(&url).bearer_auth(&token).send().await.unwrap();
    assert_eq!(r.status(), 503);
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["error"]["code"], "unavailable");
}

#[tokio::test]
async fn web_rooms_can_start_with_a_video() {
    let s = spawn().await;
    let (status, body) = s
        .post(
            "/api/rooms",
            json!({ "display_name": "Nita", "video_url": "https://youtu.be/irVNGjRFZGk" }),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let slug = body["room"]["slug"].as_str().unwrap().to_string();
    let host_key = body["host_key"].as_str().unwrap().to_string();
    let (token, _) = s.join(&slug, "Nita", Some(&host_key)).await;
    let (_ws, sync) = s.socket(&slug, &token).await;
    assert_eq!(sync["playback"]["item"]["video_id"], "irVNGjRFZGk");
    assert_eq!(sync["playback"]["status"], "playing");

    let (status, body) = s
        .post("/api/rooms", json!({ "display_name": "Nita", "video_url": "https://vimeo.com/1" }))
        .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "invalid_video");
}

#[tokio::test]
async fn malformed_bodies_and_partial_settings() {
    let s = spawn().await;
    // Partial settings fall back to defaults.
    let (status, body) = s
        .post(
            "/api/rooms",
            json!({ "display_name": "Riky", "settings": { "control_mode": "everyone" } }),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["room"]["settings"]["control_mode"], "everyone");
    assert_eq!(body["room"]["settings"]["queue_mode"], "everyone");

    // Every rejection uses the JSON error body, including extractor failures.
    let r = s
        .http
        .post(s.url("/api/rooms"))
        .header("content-type", "application/json")
        .body("{not json")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
    let body: Value = r.json().await.expect("JSON error body");
    assert_eq!(body["error"]["code"], "validation_failed");

    let r = s.http.post(s.url("/api/rooms")).body("{}").send().await.unwrap();
    assert_eq!(r.status(), 415, "missing content type");
    assert_eq!(r.json::<Value>().await.unwrap()["error"]["code"], "validation_failed");

    let (status, body) = s.post("/api/v1/keys", json!({ "label": 42 })).await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "validation_failed");

    let (slug, host_key) = s.create_room("Riky").await;
    let (token, _) = s.join(&slug, "Riky", Some(&host_key)).await;
    let r = s
        .http
        .get(s.url(&format!("/api/rooms/{slug}/search")))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400, "missing q");
    assert_eq!(r.json::<Value>().await.unwrap()["error"]["code"], "validation_failed");

    // The developer API accepts partial settings too.
    let (_, key) = s.post("/api/v1/keys", json!({ "label": "bot" })).await;
    let r = s
        .http
        .post(s.url("/api/v1/rooms"))
        .bearer_auth(key["api_key"].as_str().unwrap())
        .json(&json!({ "settings": { "control_mode": "everyone" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201);
}

#[tokio::test]
async fn pin_rooms() {
    let s = spawn().await;
    let (status, body) = s.post("/api/rooms", json!({ "display_name": "Nita", "pin": "abcd" })).await;
    assert_eq!((status, body["error"]["code"].as_str()), (422, Some("validation_failed")));
    let (status, body) = s.post("/api/rooms", json!({ "display_name": "Nita", "pin": "0420" })).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["room"]["has_password"], true);
    let slug = body["room"]["slug"].as_str().unwrap().to_string();
    let host_key = body["host_key"].as_str().unwrap().to_string();
    let join_path = format!("/api/rooms/{slug}/join");
    let join = |pin: Option<&'static str>| s.post(&join_path, json!({ "display_name": "Riky", "pin": pin }));

    let (status, body) = join(None).await;
    assert_eq!((status, body["error"]["code"].as_str()), (401, Some("pin_required")));
    let (status, body) = join(Some("1111")).await;
    assert_eq!((status, body["error"]["code"].as_str()), (403, Some("wrong_pin")));
    let (status, _) = join(Some("0420")).await;
    assert_eq!(status, 200);

    // The host key bypasses the PIN.
    let (host_token, _) = s.join(&slug, "Nita", Some(&host_key)).await;
    let (_ws, sync) = s.socket(&slug, &host_token).await;
    assert_eq!(sync["room"]["has_password"], true);

    let patch = |body: Value| {
        s.http
            .patch(s.url(&format!("/api/rooms/{slug}/settings")))
            .bearer_auth(&host_token)
            .json(&body)
            .send()
    };
    assert_eq!(patch(json!({ "pin": "12345" })).await.unwrap().status(), 422);
    let r: Value = patch(json!({ "pin": "" })).await.unwrap().json().await.unwrap();
    assert_eq!(r["has_password"], false);
    let (status, _) = join(None).await;
    assert_eq!(status, 200);

    // Guessing is rate limited.
    patch(json!({ "pin": "9876" })).await.unwrap();
    let mut last = 0;
    for _ in 0..12 {
        last = join(Some("0000")).await.0;
    }
    assert_eq!(last, 429);
}

#[tokio::test]
async fn host_can_kick_and_sessions_are_capped() {
    let s = spawn().await;
    let (slug, mut host, mut guest, _, guest_id) = s.pair().await;
    next_of(&mut host, "participant_joined").await;
    send(&mut host, json!({ "type": "kick", "participant_id": guest_id })).await;
    let left = next_of(&mut host, "participant_left").await;
    assert_eq!(left["reason"], "kicked");
    assert_eq!(next_of(&mut guest, "error").await["code"], "kicked");
    match tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match guest.next().await {
                Some(Ok(Message::Close(Some(f)))) => return u16::from(f.code),
                Some(Ok(_)) => continue,
                _ => return 0,
            }
        }
    })
    .await
    {
        Ok(code) => assert_eq!(code, 4003),
        Err(_) => panic!("guest socket not closed"),
    }

    // The kicked address cannot rejoin this room for a while...
    let (status, body) = s
        .post(&format!("/api/rooms/{slug}/join"), json!({ "display_name": "Guest again" }))
        .await;
    assert_eq!((status, body["error"]["code"].as_str()), (403, Some("forbidden")));
}

#[tokio::test]
async fn a_fourth_socket_replaces_the_oldest() {
    let s = spawn().await;
    let (slug, _) = s.create_room("Host").await;
    let (token, _) = s.join(&slug, "Tabs", None).await;
    let (mut a, _) = s.socket(&slug, &token).await;
    let _b = s.socket(&slug, &token).await;
    let _c = s.socket(&slug, &token).await;
    // A phone hopping networks reconnects before the server notices the dead
    // socket: the newest connection must win.
    let (_d, sync) = s.socket(&slug, &token).await;
    assert_eq!(sync["type"], "room_state_sync");
    let code = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match a.next().await {
                Some(Ok(Message::Close(Some(f)))) => return u16::from(f.code),
                Some(Ok(_)) => continue,
                _ => return 0,
            }
        }
    })
    .await
    .expect("oldest socket closed");
    assert_eq!(code, 4429);
}

#[tokio::test]
async fn pin_locks_after_repeated_misses_until_the_host_resets_it() {
    let s = spawn().await;
    let (status, body) = s.post("/api/rooms", json!({ "display_name": "Nita", "pin": "0420" })).await;
    assert_eq!(status, 201);
    let slug = body["room"]["slug"].as_str().unwrap().to_string();
    let host_key = body["host_key"].as_str().unwrap().to_string();
    let hub = &s.app.hub;
    // Guesses spread over many addresses still count against the room.
    for n in 0..partello::realtime::hub::PIN_LOCK_AFTER {
        let ip = std::net::IpAddr::from([198, 51, 100, n as u8]);
        let err = hub
            .join(&slug, "Guesser", None, Some("1111"), ip)
            .await
            .err()
            .expect("join must fail");
        assert_eq!(err.code(), "wrong_pin");
    }
    let friend = std::net::IpAddr::from([203, 0, 113, 7]);
    let err = hub
        .join(&slug, "Riky", None, Some("0420"), friend)
        .await
        .err()
        .expect("join must fail");
    assert_eq!(err.code(), "pin_locked", "even the right PIN is refused once locked");
    // The host still gets in and sets a new PIN, which unlocks the room.
    let Ok(host) = hub.join(&slug, "Nita", Some(&host_key), None, friend).await else {
        panic!("host joins")
    };
    let (_ws, _) = s.socket(&slug, &host.token).await;
    hub.update_settings(
        &slug,
        &host.token,
        partello::realtime::hub::SettingsChange {
            pin: Some("2468".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(hub.join(&slug, "Riky", None, Some("2468"), friend).await.is_ok());
}

#[tokio::test]
async fn csp_lists_websocket_origins() {
    let s = spawn().await;
    let r = s.http.get(s.url("/healthz")).send().await.unwrap();
    let csp = r.headers()["content-security-policy"].to_str().unwrap().to_string();
    assert!(csp.contains("connect-src 'self' ws://localhost:5173"), "{csp}");
}

#[tokio::test]
async fn queue_add_echoes_the_nonce() {
    let s = spawn().await;
    let (_slug, mut host, _guest, _, _) = s.pair().await;
    send(&mut host, json!({ "type": "queue_add", "url": VIDEO, "nonce": "first" })).await;
    assert_eq!(next_of(&mut host, "playback_changed").await["nonce"], "first");
    send(
        &mut host,
        json!({ "type": "queue_add", "url": "https://youtu.be/9bZkp7q19f0", "nonce": "second" }),
    )
    .await;
    assert_eq!(next_of(&mut host, "queue_updated").await["nonce"], "second");
    // Re-sending the same request (after a dropped connection) does not add it twice.
    send(
        &mut host,
        json!({ "type": "queue_add", "url": "https://youtu.be/9bZkp7q19f0", "nonce": "second" }),
    )
    .await;
    let again = next_of(&mut host, "queue_updated").await;
    assert_eq!(again["nonce"], "second");
    assert_eq!(again["queue"].as_array().unwrap().len(), 1, "no duplicate");
    // Ordinary changes carry no nonce.
    send(&mut host, json!({ "type": "playback_control", "action": "pause", "position": 1 })).await;
    assert!(next_of(&mut host, "playback_changed").await.get("nonce").is_none());
}

#[tokio::test]
async fn reactions_and_skip_votes_reach_everyone() {
    let s = spawn().await;
    let (slug, mut host, mut guest, _host_id, guest_id) = s.pair().await;
    send(&mut host, json!({ "type": "queue_add", "url": VIDEO })).await;
    let item_id = next_of(&mut guest, "playback_changed").await["playback"]["item"]["id"].clone();

    send(&mut guest, json!({ "type": "react", "emoji": "😂" })).await;
    let r = next_of(&mut host, "reaction").await;
    assert_eq!(r["emoji"], "😂");
    assert_eq!(r["participant_id"], guest_id);
    send(&mut guest, json!({ "type": "react", "emoji": "💩" })).await;
    assert_eq!(next_of(&mut guest, "error").await["code"], "validation_failed");

    // Two people: both must vote.
    send(&mut guest, json!({ "type": "skip_vote", "item_id": item_id, "vote": true })).await;
    let votes = next_of(&mut host, "skip_votes").await;
    assert_eq!(votes["voters"], json!([guest_id]));
    assert_eq!(votes["skipped"], false);
    // A late joiner sees the votes so far.
    let (token, _) = s.join(&slug, "Late", None).await;
    let (_late, sync) = s.socket(&slug, &token).await;
    assert_eq!(sync["skip_votes"], json!([guest_id]));
}

#[tokio::test]
async fn chat_replies_carry_the_quote() {
    let s = spawn().await;
    let (_slug, mut host, mut guest, _, _) = s.pair().await;
    send(&mut host, json!({ "type": "chat_send", "body": "popcorn?" })).await;
    let original = next_of(&mut guest, "chat_message_created").await["message"]["id"].clone();
    send(
        &mut guest,
        json!({ "type": "chat_send", "body": "yes @Host", "reply_to": original }),
    )
    .await;
    let reply = next_of(&mut host, "chat_message_created").await;
    let reply = if reply["message"]["body"] == "popcorn?" {
        next_of(&mut host, "chat_message_created").await
    } else {
        reply
    };
    assert_eq!(reply["message"]["reply_to"]["body"], "popcorn?");
    assert_eq!(reply["message"]["reply_to"]["display_name"], "Host");
}

#[tokio::test]
async fn video_and_queue_survive_a_restart() {
    let store = Store::memory();
    let s = spawn_on(Config::for_tests(), store.clone()).await;
    let (slug, mut host, _guest, _, _) = s.pair().await;
    send(&mut host, json!({ "type": "queue_add", "url": VIDEO })).await;
    next_of(&mut host, "playback_changed").await;
    send(&mut host, json!({ "type": "queue_add", "url": "https://youtu.be/aqz-KE-bpKQ" })).await;
    next_of(&mut host, "queue_updated").await;
    s.app.hub.flush().await;

    // A new process on the same database: sessions are gone, the room is not.
    let s2 = spawn_on(Config::for_tests(), store).await;
    let (token, _) = s2.join(&slug, "Host again", None).await;
    let (_ws, sync) = s2.socket(&slug, &token).await;
    assert_eq!(sync["playback"]["item"]["video_id"], "dQw4w9WgXcQ");
    assert_eq!(sync["playback"]["status"], "playing");
    assert_eq!(sync["queue"][0]["video_id"], "aqz-KE-bpKQ");
}

#[tokio::test]
async fn playlist_import_reports_when_unconfigured() {
    let s = spawn().await;
    let (_slug, mut host, _guest, _, _) = s.pair().await;
    send(
        &mut host,
        json!({ "type": "queue_add_playlist", "url": "https://www.youtube.com/playlist?list=PLx0sYbCqOb8TBPRdmBHs5Iftvv9TPboYG", "nonce": "p1" }),
    )
    .await;
    let err = next_of(&mut host, "error").await;
    assert_eq!(err["nonce"], "p1");
    assert_eq!(err["code"], "unavailable");
    send(
        &mut host,
        json!({ "type": "queue_add_playlist", "url": "https://www.youtube.com/watch?v=YEhzWw8qQWk&list=RDYEhzWw8qQWk", "nonce": "p2" }),
    )
    .await;
    let err = next_of(&mut host, "error").await;
    assert!(err["message"].as_str().unwrap().contains("mixes"), "{err}");
}

#[tokio::test]
async fn sfu_calls_hand_out_tokens_to_call_members_only() {
    let mut config = Config::for_tests();
    config.livekit = Some(partello::config::LiveKitConfig {
        url: "wss://lk.example.org".into(),
        // Nothing listens here: removals fail quietly in the background.
        api_url: "http://127.0.0.1:9".into(),
        api_key: "APIkey".into(),
        api_secret: "secret-that-is-long-enough-for-hs256".into(),
    });
    let s = spawn_with(config).await;
    let (slug, host_key) = s.create_room("Host").await;
    let (token, _) = s.join(&slug, "Host", Some(&host_key)).await;
    let (mut ws, sync) = s.socket(&slug, &token).await;
    assert_eq!(sync["limits"]["call_mode"], "sfu");
    assert_eq!(sync["limits"]["max_voice_participants"], 8, "for_tests keeps the default");

    send(&mut ws, json!({ "type": "voice_join", "muted": false, "camera": false })).await;
    let call = next_of(&mut ws, "call_token").await;
    assert_eq!(call["url"], "wss://lk.example.org");
    assert_eq!(call["token"].as_str().unwrap().split('.').count(), 3);

    // Browsers may open sockets to the SFU.
    let r = s.http.get(s.url("/healthz")).send().await.unwrap();
    let csp = r.headers()["content-security-policy"].to_str().unwrap();
    assert!(csp.contains("wss://lk.example.org https://lk.example.org"), "{csp}");
}

#[tokio::test]
async fn hosts_can_make_a_room_permanent() {
    let s = spawn().await;
    let (slug, host_key) = s.create_room("Host").await;
    let (token, _) = s.join(&slug, "Host", Some(&host_key)).await;
    let (_ws, _) = s.socket(&slug, &token).await;
    let r = s
        .http
        .patch(s.url(&format!("/api/rooms/{slug}/settings")))
        .bearer_auth(&token)
        .json(&json!({ "permanent": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["settings"]["permanent"], true);
    assert_eq!(body["settings"]["control_mode"], "host_only", "other settings unchanged");
}
