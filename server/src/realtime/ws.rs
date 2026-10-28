//! One task per WebSocket connection: authenticate, then pump events between
//! the socket and the hub with per-connection rate limits and heartbeats.

use std::{sync::Arc, time::Duration};

use axum::{
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use tokio::{
    sync::mpsc,
    time::{Instant, timeout},
};

use super::{
    hub::KillSwitch,
    protocol::{ClientEvent, ServerEvent},
};
use crate::{AppState, database::now_ms, error::AppError, rate_limit::TokenBucket, security};

const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const HEARTBEAT_EVERY: Duration = Duration::from_secs(20);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_MALFORMED: u32 = 20;

/// Application close codes (documented in docs/WEBSOCKET.md).
pub mod close {
    pub const BAD_REQUEST: u16 = 4400;
    pub const UNAUTHORIZED: u16 = 4401;
    pub const NOT_FOUND: u16 = 4404;
    pub const ROOM_FULL: u16 = 4409;
    pub const LEFT: u16 = 4000;
    pub const KICKED: u16 = 4003;
    pub const TOO_MANY_CONNECTIONS: u16 = 4429;
    pub const TOO_SLOW: u16 = 4008;
}

pub async fn ws_handler(State(app): State<AppState>, Path(slug): Path<String>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    // Browsers always send Origin on WebSocket handshakes; enforce it to stop
    // other sites from opening sockets with a visitor's session.
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok()) {
        if !app.config.allowed_origins.iter().any(|a| a == origin) {
            tracing::warn!(%origin, "websocket origin rejected");
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    if !security::is_valid_slug(&slug) {
        return StatusCode::NOT_FOUND.into_response();
    }
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| run(app, slug, socket))
}

fn event(ev: &ServerEvent) -> Message {
    Message::Text(serde_json::to_string(ev).expect("serializable").into())
}

fn error_event(err: &AppError, nonce: Option<String>) -> Message {
    event(&ServerEvent::Error {
        code: err.code().into(),
        message: err.to_string(),
        nonce,
    })
}

fn close_code(err: &AppError) -> u16 {
    match err {
        AppError::Unauthorized => close::UNAUTHORIZED,
        AppError::RoomNotFound => close::NOT_FOUND,
        AppError::RoomFull => close::ROOM_FULL,
        AppError::TooManyConnections => close::TOO_MANY_CONNECTIONS,
        _ => close::BAD_REQUEST,
    }
}

async fn reject(mut socket: WebSocket, err: AppError) {
    let _ = socket.send(error_event(&err, None)).await;
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code: close_code(&err),
            reason: err.code().into(),
        })))
        .await;
}

/// Per-connection budgets. Generous for humans, tight for scripts.
struct Limits {
    any: TokenBucket,
    chat: TokenBucket,
    queue: TokenBucket,
    playback: TokenBucket,
    signal: TokenBucket,
    react: TokenBucket,
}

impl Limits {
    fn new() -> Self {
        Self {
            any: TokenBucket::new(60, 30.0),
            chat: TokenBucket::new(6, 1.0),
            queue: TokenBucket::new(5, 0.5),
            playback: TokenBucket::new(12, 4.0),
            signal: TokenBucket::new(300, 60.0),
            react: TokenBucket::new(10, 3.0),
        }
    }

    fn allow(&mut self, ev: &ClientEvent) -> bool {
        // Call setup bursts ICE candidates; they have their own budget and
        // must not starve chat, pings and playback.
        if let ClientEvent::VoiceSignal { .. } = ev {
            return self.signal.try_take();
        }
        if !self.any.try_take() {
            return false;
        }
        match ev {
            ClientEvent::ChatSend { .. } => self.chat.try_take(),
            ClientEvent::QueueAdd { .. }
            | ClientEvent::QueueAddPlaylist { .. }
            | ClientEvent::QueueRemove { .. }
            | ClientEvent::QueueMove { .. } => self.queue.try_take(),
            ClientEvent::React { .. } => self.react.try_take(),
            ClientEvent::SkipVote { .. } => self.playback.try_take(),
            ClientEvent::PlaybackControl { .. } | ClientEvent::QueuePlay { .. } | ClientEvent::QueueSkip => self.playback.try_take(),
            _ => true,
        }
    }
}

fn nonce_of(ev: &ClientEvent) -> Option<String> {
    match ev {
        ClientEvent::ChatSend { nonce, .. } | ClientEvent::QueueAdd { nonce, .. } | ClientEvent::QueueAddPlaylist { nonce, .. } => {
            nonce.clone()
        }
        _ => None,
    }
}

async fn run(app: AppState, slug: String, mut socket: WebSocket) {
    // 1. The first frame must be `auth`. Tokens never travel in the URL, so they
    //    do not end up in proxy or access logs.
    let token = match timeout(AUTH_TIMEOUT, socket.recv()).await {
        Ok(Some(Ok(Message::Text(text)))) => match serde_json::from_str::<ClientEvent>(&text) {
            Ok(ClientEvent::Auth { token }) => token,
            _ => return reject(socket, AppError::Unauthorized).await,
        },
        _ => return reject(socket, AppError::Unauthorized).await,
    };
    let session = match app.hub.authenticate(&slug, &token) {
        Ok(s) => s,
        Err(err) => return reject(socket, err).await,
    };

    let (tx, mut rx) = mpsc::channel::<Message>(super::hub::CHANNEL_CAPACITY);
    let kill = Arc::new(KillSwitch::default());
    let (room, conn_id) = match app.hub.attach(&session, tx.clone(), kill.clone()).await {
        Ok(v) => v,
        Err(err) => return reject(socket, err).await,
    };
    let pid = session.participant_id;
    tracing::info!(room = %slug, participant = %pid, conn = conn_id, "connected");

    let (mut sink, mut stream) = socket.split();

    // Writer: forwards queued frames and sends protocol-level pings.
    let writer = tokio::spawn(async move {
        let mut heartbeat = tokio::time::interval(HEARTBEAT_EVERY);
        heartbeat.tick().await;
        loop {
            tokio::select! {
                msg = rx.recv() => match msg {
                    Some(msg) => {
                        let closing = matches!(msg, Message::Close(_));
                        if sink.send(msg).await.is_err() || closing { break; }
                    }
                    None => break,
                },
                _ = heartbeat.tick() => {
                    if sink.send(Message::Ping(Vec::new().into())).await.is_err() { break; }
                }
            }
        }
        let _ = sink.close().await;
    });

    let mut limits = Limits::new();
    let mut malformed = 0u32;
    let mut last_seen = Instant::now();
    let mut close_with: Option<u16> = None;
    let mut left = false;

    loop {
        let idle_left = IDLE_TIMEOUT.saturating_sub(last_seen.elapsed());
        let frame = tokio::select! {
            f = stream.next() => f,
            code = kill.triggered() => {
                close_with = Some(code);
                // Left or kicked: the hub already removed the participant.
                left = code == close::LEFT || code == close::KICKED;
                break;
            }
            _ = tokio::time::sleep(idle_left) => { tracing::debug!(participant = %pid, "idle timeout"); break; }
        };
        let text = match frame {
            Some(Ok(Message::Text(text))) => text,
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
            Some(Ok(_)) => {
                // Ping/Pong (answered by the library) and binary frames.
                last_seen = Instant::now();
                continue;
            }
        };
        last_seen = Instant::now();

        let ev = match serde_json::from_str::<ClientEvent>(&text) {
            Ok(ev) => ev,
            Err(_) => {
                malformed += 1;
                let _ = tx.try_send(error_event(&AppError::validation("Malformed event"), None));
                if malformed > MAX_MALFORMED {
                    close_with = Some(close::BAD_REQUEST);
                    break;
                }
                continue;
            }
        };
        if !limits.allow(&ev) {
            let _ = tx.try_send(error_event(&AppError::RateLimited, nonce_of(&ev)));
            continue;
        }
        match ev {
            ClientEvent::Ping { t } => {
                let _ = tx.try_send(event(&ServerEvent::Pong { t, server_time: now_ms() }));
            }
            ClientEvent::Leave => {
                app.hub.leave(&room, pid);
                left = true;
                close_with = Some(close::LEFT);
                break;
            }
            ClientEvent::Auth { .. } => {}
            ev => {
                let nonce = nonce_of(&ev);
                let kind = ev.kind();
                if let Err(err) = app.hub.handle(&room, pid, ev).await {
                    if matches!(err, AppError::Internal(_)) {
                        tracing::error!(participant = %pid, kind, error = %err, "event failed");
                    } else {
                        tracing::debug!(participant = %pid, kind, code = err.code(), "event rejected");
                    }
                    let _ = tx.try_send(error_event(&err, nonce));
                }
            }
        }
    }

    if let Some(code) = close_with {
        let _ = tx.try_send(Message::Close(Some(CloseFrame { code, reason: "".into() })));
    }
    drop(tx);
    if !left {
        app.hub.detach(&room, pid, conn_id);
    }
    // Give the writer a moment to flush the close frame.
    let _ = timeout(Duration::from_secs(2), writer).await;
    tracing::info!(room = %slug, participant = %pid, conn = conn_id, "disconnected");
}
