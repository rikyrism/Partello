//! The hub owns every live room on this instance and the sessions that may
//! connect to them. It turns [`Effect`]s from the room state machine into
//! WebSocket frames and database writes.
//!
//! Locking: one `std::sync::Mutex` per room, held only for synchronous work
//! (state transition + `try_send` to bounded channels). Nothing awaits while a
//! room lock is held. Slow consumers whose channel is full are disconnected
//! instead of buffering without bound.

use std::{
    collections::{HashMap, VecDeque},
    net::IpAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU16, AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::extract::ws::{Message, Utf8Bytes};
use tokio::sync::{Notify, mpsc};
use uuid::Uuid;

use super::protocol::{ClientEvent, IceServer, ServerEvent};
use crate::{
    config::Config,
    database::{Store, now_ms},
    error::AppError,
    rate_limit::KeyedLimiter,
    room::{
        model::{ChatMessage, ControlMode, QueueMode, RoomInfo, RoomRecord, RoomSettings, VoiceState},
        queue::QueueItem,
        state::{Effect, Effects, RoomLimits, RoomState},
    },
    security, validation,
    youtube::{self, YoutubeClient},
};

/// Seats reserved by a REST join but never connected are released after this.
const RESERVATION_TTL_MS: i64 = 60_000;
pub const CHANNEL_CAPACITY: usize = 256;

#[derive(Clone, Debug)]
pub struct Session {
    pub slug: String,
    pub participant_id: Uuid,
    pub display_name: String,
    pub is_creator: bool,
    pub expires_at: i64,
    /// Address the session joined from (rate-limit form), used for kick bans.
    pub ip: IpAddr,
}

/// Wrong PINs a room tolerates before its PIN stops working until the host
/// sets a new one. With 10,000 possible PINs this caps a guesser's odds at 0.2%.
pub const PIN_LOCK_AFTER: u32 = 20;
/// How long a kicked person's address may not rejoin that room.
pub const KICK_BAN_MS: i64 = 15 * 60 * 1000;

/// Tabs one session may have open at once; more would multiply its rate limits.
pub const MAX_CONNECTIONS_PER_PARTICIPANT: usize = 3;

/// Asks a connection task to close with a specific WebSocket close code.
#[derive(Default)]
pub struct KillSwitch {
    notify: Notify,
    code: AtomicU16,
}

impl KillSwitch {
    pub fn trigger(&self, code: u16) {
        self.code.store(code, Ordering::SeqCst);
        self.notify.notify_one();
    }

    pub async fn triggered(&self) -> u16 {
        self.notify.notified().await;
        self.code.load(Ordering::SeqCst)
    }
}

struct Conn {
    id: u64,
    tx: mpsc::Sender<Message>,
    kill: Arc<KillSwitch>,
}

struct LiveInner {
    state: RoomState,
    conns: HashMap<Uuid, Vec<Conn>>,
    empty_since: Option<i64>,
    /// Set (under this lock) when maintenance drops the room from the map, so
    /// anyone still holding the Arc retries with a fresh load.
    unloaded: bool,
    /// Wrong PIN attempts since the PIN was last set.
    pin_failures: u32,
    /// Addresses of kicked people and when their ban ends (ms).
    bans: HashMap<IpAddr, i64>,
    /// Recent queue_add requests by (participant, nonce), so a request the
    /// client re-sends after a dropped connection is not added twice.
    adds: VecDeque<((Uuid, String), AddState)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AddState {
    InFlight,
    Done,
}

const RECENT_ADDS: usize = 256;

pub struct LiveRoom {
    pub slug: String,
    pub id: Uuid,
    inner: Mutex<LiveInner>,
}

pub struct JoinOutcome {
    pub token: String,
    pub participant_id: Uuid,
    pub is_host_key_valid: bool,
    pub room: RoomInfo,
}

pub struct Hub {
    pub store: Store,
    config: Arc<Config>,
    youtube: YoutubeClient,
    rooms: Mutex<HashMap<String, Arc<LiveRoom>>>,
    sessions: Mutex<HashMap<Vec<u8>, Session>>,
    load_lock: tokio::sync::Mutex<()>,
    next_conn_id: AtomicU64,
    /// Argon2 needs ~19 MiB per run; cap how many run at once.
    argon: Arc<tokio::sync::Semaphore>,
    /// PIN guesses per room and address (only rooms that really have a PIN).
    pin_limiter: KeyedLimiter<(Uuid, IpAddr)>,
}

fn encode(event: &ServerEvent) -> Message {
    let json = serde_json::to_string(event).expect("server events always serialize");
    Message::Text(Utf8Bytes::from(json))
}

impl LiveInner {
    fn send_to(&mut self, pid: Uuid, msg: &Message) {
        if let Some(conns) = self.conns.get_mut(&pid) {
            conns.retain(|c| match c.tx.try_send(msg.clone()) {
                Ok(()) => true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    tracing::warn!(participant = %pid, conn = c.id, "slow consumer disconnected");
                    c.kill.trigger(super::ws::close::TOO_SLOW);
                    false
                }
                Err(mpsc::error::TrySendError::Closed(_)) => false,
            });
        }
    }

    /// Delivers effects and returns chat messages that still need persisting.
    fn dispatch(&mut self, effects: Effects) -> Vec<ChatMessage> {
        let mut persist = Vec::new();
        for effect in effects {
            match effect {
                Effect::Broadcast(ev) => {
                    let msg = encode(&ev);
                    let ids: Vec<Uuid> = self.conns.keys().copied().collect();
                    for pid in ids {
                        self.send_to(pid, &msg);
                    }
                }
                Effect::BroadcastExcept(skip, ev) => {
                    let msg = encode(&ev);
                    let ids: Vec<Uuid> = self.conns.keys().copied().filter(|p| *p != skip).collect();
                    for pid in ids {
                        self.send_to(pid, &msg);
                    }
                }
                Effect::To(pid, ev) => self.send_to(pid, &encode(&ev)),
                Effect::PersistChat(m) => persist.push(m),
            }
        }
        persist
    }
}

impl Hub {
    pub fn new(store: Store, config: Arc<Config>) -> Self {
        Self {
            store,
            youtube: YoutubeClient::new(config.youtube_oembed),
            config,
            rooms: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            load_lock: tokio::sync::Mutex::new(()),
            next_conn_id: AtomicU64::new(1),
            argon: Arc::new(tokio::sync::Semaphore::new(4)),
            pin_limiter: KeyedLimiter::new(10, Duration::from_secs(300)),
        }
    }

    fn limits(&self) -> RoomLimits {
        let l = &self.config.limits;
        RoomLimits {
            max_voice: l.max_voice_participants,
            max_queue: l.max_queue_len,
            chat_history: l.chat_history_len,
            chat_max_chars: l.chat_max_chars,
            search_enabled: self.config.youtube_api_key.is_some(),
        }
    }

    pub fn live_room_count(&self) -> usize {
        self.rooms.lock().unwrap().len()
    }

    pub fn is_live(&self, slug: &str) -> bool {
        self.rooms.lock().unwrap().contains_key(slug)
    }

    // ------------------------------------------------------------ rooms

    /// Creates and persists a room. Returns the record and the plaintext host key
    /// (shown to the creator once, stored only as a hash).
    pub async fn create_room(
        &self,
        name: &str,
        settings: RoomSettings,
        api_key_id: Option<Uuid>,
        pin: Option<&str>,
    ) -> Result<(RoomRecord, String), AppError> {
        let name = validation::room_name(name)?;
        let password_hash = match pin {
            Some(p) => Some(self.hash_pin(validation::room_pin(p)?).await?),
            None => None,
        };
        let host_key = security::new_secret("phk_");
        for _ in 0..5 {
            let record = RoomRecord {
                id: Uuid::new_v4(),
                slug: security::new_room_slug(),
                name: name.clone(),
                host_key_hash: security::hash_secret(&host_key),
                settings,
                max_participants: self.config.limits.max_participants,
                api_key_id,
                created_at: now_ms(),
                password_hash: password_hash.clone(),
            };
            if self.store.insert_room(&record).await? {
                tracing::info!(room = %record.slug, "room created");
                return Ok((record, host_key));
            }
        }
        Err(AppError::Internal(anyhow::anyhow!("could not allocate a unique room slug")))
    }

    /// Returns the live room, loading it from the database on first use.
    pub async fn load(&self, slug: &str) -> Result<Arc<LiveRoom>, AppError> {
        if !security::is_valid_slug(slug) {
            return Err(AppError::RoomNotFound);
        }
        if let Some(room) = self.rooms.lock().unwrap().get(slug) {
            return Ok(room.clone());
        }
        let _guard = self.load_lock.lock().await;
        if let Some(room) = self.rooms.lock().unwrap().get(slug) {
            return Ok(room.clone());
        }
        let record = self.store.room_by_slug(slug).await?.ok_or(AppError::RoomNotFound)?;
        let history = self.store.recent_chat(record.id, self.config.limits.chat_history_len).await?;
        let room = Arc::new(LiveRoom {
            slug: record.slug.clone(),
            id: record.id,
            inner: Mutex::new(LiveInner {
                state: RoomState::new(&record, self.limits(), history, now_ms()),
                conns: HashMap::new(),
                empty_since: Some(now_ms()),
                unloaded: false,
                pin_failures: 0,
                bans: HashMap::new(),
                adds: VecDeque::new(),
            }),
        });
        self.rooms.lock().unwrap().insert(slug.to_string(), room.clone());
        tracing::debug!(room = %slug, "room loaded into memory");
        Ok(room)
    }

    /// Metadata plus how many people are in the room right now.
    /// Metadata, how many people are visibly in the room, and whether every
    /// seat (reservations included) is taken.
    pub async fn room_summary(&self, slug: &str) -> Result<(RoomInfo, usize, bool), AppError> {
        if !security::is_valid_slug(slug) {
            return Err(AppError::RoomNotFound);
        }
        let live = self.rooms.lock().unwrap().get(slug).cloned();
        if let Some(room) = live {
            let inner = room.inner.lock().unwrap();
            let full = inner.state.occupancy() >= inner.state.max_participants as usize;
            return Ok((inner.state.info(), inner.state.participants().len(), full));
        }
        let record = self.store.room_by_slug(slug).await?.ok_or(AppError::RoomNotFound)?;
        Ok((RoomInfo::from(&record), 0, false))
    }

    // ------------------------------------------------------------ sessions

    pub async fn join(
        &self,
        slug: &str,
        display_name: &str,
        host_key: Option<&str>,
        pin: Option<&str>,
        ip: IpAddr,
    ) -> Result<JoinOutcome, AppError> {
        let display_name = validation::display_name(display_name)?;
        let room = self.load(slug).await?;
        // PIN rooms: the host key bypasses the PIN, everyone else must match it.
        let (stored_hash, holds_host_key) = {
            let mut inner = room.inner.lock().unwrap();
            let holds = host_key.is_some_and(|k| security::digest_eq(&security::hash_secret(k), &inner.state.host_key_hash));
            let now = now_ms();
            inner.bans.retain(|_, until| *until > now);
            if !holds && inner.bans.contains_key(&ip) {
                return Err(AppError::forbidden("The host removed you from this room. Try again later."));
            }
            if !holds && inner.state.password_hash.is_some() && inner.pin_failures >= PIN_LOCK_AFTER {
                return Err(AppError::PinLocked);
            }
            (inner.state.password_hash.clone(), holds)
        };
        if let (Some(hash), false) = (stored_hash, holds_host_key) {
            let attempt = pin.ok_or(AppError::PinRequired)?.to_string();
            if !self.pin_limiter.check(&(room.id, ip)) {
                return Err(AppError::RateLimited);
            }
            let _permit = self.argon.clone().acquire_owned().await.map_err(|e| AppError::Internal(e.into()))?;
            let ok = tokio::task::spawn_blocking(move || security::verify_password(&attempt, &hash))
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            if !ok {
                room.inner.lock().unwrap().pin_failures += 1;
                return Err(AppError::WrongPin);
            }
        }
        drop(room);
        let participant_id = Uuid::new_v4();
        let (info, is_creator) = loop {
            let room = self.load(slug).await?;
            let mut inner = room.inner.lock().unwrap();
            if inner.unloaded {
                continue; // raced with maintenance; load a fresh copy
            }
            let is_creator = host_key
                .map(|k| security::digest_eq(&security::hash_secret(k), &inner.state.host_key_hash))
                .unwrap_or(false);
            inner.state.reserve(participant_id, display_name.clone(), is_creator, now_ms())?;
            inner.empty_since = None;
            break (inner.state.info(), is_creator);
        };
        let token = security::new_secret("pts_");
        let session = Session {
            slug: slug.to_string(),
            participant_id,
            display_name,
            is_creator,
            expires_at: now_ms() + self.config.limits.session_ttl.as_millis() as i64,
            ip,
        };
        self.sessions.lock().unwrap().insert(security::hash_secret(&token), session);
        Ok(JoinOutcome {
            token,
            participant_id,
            is_host_key_valid: is_creator,
            room: info,
        })
    }

    /// Argon2 is deliberately slow and memory-hungry: run it on the blocking
    /// pool, a few at a time.
    async fn hash_pin(&self, pin: String) -> Result<String, AppError> {
        let _permit = self.argon.clone().acquire_owned().await.map_err(|e| AppError::Internal(e.into()))?;
        tokio::task::spawn_blocking(move || security::hash_password(&pin))
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .map_err(AppError::Internal)
    }

    pub fn authenticate(&self, slug: &str, token: &str) -> Result<Session, AppError> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions.get(&security::hash_secret(token)).ok_or(AppError::Unauthorized)?;
        if session.slug != slug || session.expires_at < now_ms() {
            return Err(AppError::Unauthorized);
        }
        Ok(session.clone())
    }

    fn revoke(&self, participant_id: Uuid) {
        self.sessions.lock().unwrap().retain(|_, s| s.participant_id != participant_id);
    }

    fn ice_servers(&self, participant_id: Uuid) -> Vec<IceServer> {
        let ice = &self.config.ice;
        let mut out = Vec::new();
        if !ice.stun_urls.is_empty() {
            out.push(IceServer {
                urls: ice.stun_urls.clone(),
                username: None,
                credential: None,
            });
        }
        if let (false, Some(secret)) = (ice.turn_urls.is_empty(), &ice.turn_shared_secret) {
            let expires = now_ms() / 1000 + ice.turn_credential_ttl.as_secs() as i64;
            let tag = participant_id.simple().to_string();
            let (username, credential) = security::turn_credentials(secret, &tag[..12], expires);
            out.push(IceServer {
                urls: ice.turn_urls.clone(),
                username: Some(username),
                credential: Some(credential),
            });
        }
        out
    }

    // ------------------------------------------------------------ connections

    /// Registers an authenticated connection and sends it the full room state.
    pub async fn attach(
        &self,
        session: &Session,
        tx: mpsc::Sender<Message>,
        kill: Arc<KillSwitch>,
    ) -> Result<(Arc<LiveRoom>, u64), AppError> {
        let pid = session.participant_id;
        let conn_id = self.next_conn_id.fetch_add(1, Ordering::Relaxed);
        let ice = self.ice_servers(pid);
        loop {
            let room = self.load(&session.slug).await?;
            let mut inner = room.inner.lock().unwrap();
            if inner.unloaded {
                continue; // raced with maintenance; load a fresh copy
            }
            // At the cap, the oldest connection goes: on flaky mobile networks
            // it is usually a dead socket the server has not noticed yet.
            if let Some(conns) = inner.conns.get_mut(&pid) {
                while conns.len() >= MAX_CONNECTIONS_PER_PARTICIPANT {
                    let oldest = conns.remove(0);
                    oldest.kill.trigger(super::ws::close::TOO_MANY_CONNECTIONS);
                }
            }
            let now = now_ms();
            if !inner.state.is_member(pid) {
                // Session outlived the seat (long disconnect, room reloaded): rejoin transparently.
                inner.state.reserve(pid, session.display_name.clone(), session.is_creator, now)?;
            }
            let effects = inner.state.connect(pid, now)?;
            inner.empty_since = None;
            let snapshot = encode(&inner.state.snapshot(pid, ice, now));
            // The channel is fresh, so this cannot fail for capacity.
            let _ = tx.try_send(snapshot);
            inner.conns.entry(pid).or_default().push(Conn { id: conn_id, tx, kill });
            inner.dispatch(effects);
            drop(inner);
            return Ok((room, conn_id));
        }
    }

    pub fn detach(self: &Arc<Self>, room: &Arc<LiveRoom>, pid: Uuid, conn_id: u64) {
        let mut inner = room.inner.lock().unwrap();
        if let Some(conns) = inner.conns.get_mut(&pid) {
            conns.retain(|c| c.id != conn_id);
            if conns.is_empty() {
                inner.conns.remove(&pid);
            }
        }
        let effects = inner.state.disconnect(pid, now_ms());
        inner.dispatch(effects);
        drop(inner);

        // Remove the participant if they do not come back within the grace window.
        let hub = self.clone();
        let room = room.clone();
        let grace = self.config.limits.reconnect_grace;
        tokio::spawn(async move {
            tokio::time::sleep(grace + Duration::from_millis(50)).await;
            hub.sweep_room(&room, now_ms());
        });
    }

    fn sweep_room(&self, room: &LiveRoom, now: i64) {
        let mut inner = room.inner.lock().unwrap();
        let grace = self.config.limits.reconnect_grace.as_millis() as i64;
        let effects = inner.state.sweep(now, grace, RESERVATION_TTL_MS);
        inner.dispatch(effects);
        if inner.state.is_empty() && inner.empty_since.is_none() {
            inner.empty_since = Some(now);
        }
    }

    pub fn leave(&self, room: &LiveRoom, pid: Uuid) {
        let mut inner = room.inner.lock().unwrap();
        let effects = inner.state.leave(pid);
        inner.dispatch(effects);
        // Close any other tabs of this participant, telling them they left.
        if let Some(conns) = inner.conns.remove(&pid) {
            for c in conns {
                c.kill.trigger(super::ws::close::LEFT);
            }
        }
        if inner.state.is_empty() {
            inner.empty_since = Some(now_ms());
        }
        drop(inner);
        self.revoke(pid);
    }

    /// Host removes `target`: everyone is told, their tabs close, their sessions end.
    pub fn kick(&self, room: &LiveRoom, actor: Uuid, target: Uuid) -> Result<(), AppError> {
        let mut inner = room.inner.lock().unwrap();
        let effects = inner.state.kick(actor, target)?;
        inner.dispatch(effects);
        // Revoke while still holding the room lock, so a connection that is
        // authenticating right now cannot re-reserve the seat; and keep the
        // address out of this room for a while.
        let ips: Vec<IpAddr> = {
            let mut sessions = self.sessions.lock().unwrap();
            let ips = sessions.values().filter(|s| s.participant_id == target).map(|s| s.ip).collect();
            sessions.retain(|_, s| s.participant_id != target);
            ips
        };
        let until = now_ms() + KICK_BAN_MS;
        for ip in ips {
            inner.bans.insert(ip, until);
        }
        if let Some(conns) = inner.conns.remove(&target) {
            for c in conns {
                let _ = c.tx.try_send(encode(&ServerEvent::Error {
                    code: "kicked".into(),
                    message: "The host removed you from this room".into(),
                    nonce: None,
                }));
                c.kill.trigger(super::ws::close::KICKED);
            }
        }
        drop(inner);
        tracing::info!(room = %room.slug, %actor, %target, "participant kicked");
        Ok(())
    }

    /// Applies one client event. Errors are reported back to the sender only.
    pub async fn handle(&self, room: &LiveRoom, pid: Uuid, event: ClientEvent) -> Result<(), AppError> {
        if let ClientEvent::QueueAdd { url, nonce } = &event {
            return self.queue_add(room, pid, url, nonce.clone()).await;
        }
        if let ClientEvent::Kick { participant_id } = event {
            return self.kick(room, pid, participant_id);
        }
        let now = now_ms();
        let persist = {
            let mut inner = room.inner.lock().unwrap();
            let s = &mut inner.state;
            let effects = match event {
                ClientEvent::ChatSend { body, nonce } => s.chat(pid, &body, nonce, now)?,
                ClientEvent::PlaybackControl { action, position } => s.playback_control(pid, action, position, now)?,
                ClientEvent::PlaybackDuration { item_id, duration } => s.playback_duration(pid, item_id, duration)?,
                ClientEvent::PlaybackEnded { item_id } => s.playback_ended(pid, item_id, now)?,
                ClientEvent::QueueRemove { item_id } => s.queue_remove(pid, item_id)?,
                ClientEvent::QueuePlay { item_id } => s.queue_play(pid, item_id, now)?,
                ClientEvent::QueueMove { item_id, to_index } => s.queue_move(pid, item_id, to_index)?,
                ClientEvent::QueueSkip => s.queue_skip(pid, now)?,
                ClientEvent::VoiceJoin { muted, camera } => s.voice_join(pid, VoiceState { muted, camera })?,
                ClientEvent::VoiceUpdate { muted, camera } => s.voice_update(pid, VoiceState { muted, camera })?,
                ClientEvent::VoiceLeave => s.voice_leave(pid)?,
                // Signals racing a peer leaving the call are dropped quietly:
                // the sender learns about the departure from presence events.
                ClientEvent::VoiceSignal { to, data } => s.voice_signal(pid, to, data).unwrap_or_default(),
                ClientEvent::HostTransfer { to } => s.host_transfer(pid, to)?,
                // Handled by the connection loop.
                ClientEvent::Auth { .. }
                | ClientEvent::Ping { .. }
                | ClientEvent::Leave
                | ClientEvent::QueueAdd { .. }
                | ClientEvent::Kick { .. } => Vec::new(),
            };
            inner.dispatch(effects)
        };
        for msg in persist {
            if let Err(err) = self.store.insert_chat(room.id, &msg).await {
                // The message was delivered live; losing it from history is acceptable.
                tracing::warn!(room = %room.slug, error = %err, "failed to persist chat message");
            }
        }
        Ok(())
    }

    pub async fn resolve_video(&self, url: &str, added_by: Uuid, added_by_name: String) -> Result<QueueItem, AppError> {
        let parsed = youtube::parse_video_url(url)?;
        let meta = self.youtube.metadata(&parsed.video_id).await?;
        Ok(QueueItem {
            id: Uuid::new_v4(),
            video_id: parsed.video_id,
            title: meta.title,
            author: meta.author,
            thumbnail_url: meta.thumbnail_url,
            start: parsed.start,
            added_by,
            added_by_name,
        })
    }

    async fn queue_add(&self, room: &LiveRoom, pid: Uuid, url: &str, nonce: Option<String>) -> Result<(), AppError> {
        let nonce = nonce.filter(|n| n.len() <= super::protocol::NONCE_MAX);
        let key = nonce.clone().map(|n| (pid, n));
        // Check permission before the network round trip, and again after it.
        let name = {
            let mut inner = room.inner.lock().unwrap();
            if let Some(key) = &key {
                match inner.adds.iter().find(|(k, _)| k == key).map(|(_, s)| *s) {
                    // A re-send of a request still being processed: the original answers.
                    Some(AddState::InFlight) => return Ok(()),
                    // Already added; the reply was lost with the old connection.
                    Some(AddState::Done) => {
                        let event = ServerEvent::QueueUpdated {
                            queue: inner.state.queue.items(),
                            actor_id: Some(pid),
                            nonce,
                        };
                        inner.dispatch(vec![Effect::To(pid, event)]);
                        return Ok(());
                    }
                    None => {}
                }
            }
            inner.state.can_add_to_queue(pid)?;
            if let Some(key) = &key {
                if inner.adds.len() >= RECENT_ADDS {
                    inner.adds.pop_front();
                }
                inner.adds.push_back((key.clone(), AddState::InFlight));
            }
            inner.state.participant(pid).map(|p| p.display_name.clone()).unwrap_or_default()
        };
        let finish = |room: &LiveRoom, done: bool| {
            if let Some(key) = &key {
                let mut inner = room.inner.lock().unwrap();
                if let Some(pos) = inner.adds.iter().position(|(k, _)| k == key) {
                    if done {
                        inner.adds[pos].1 = AddState::Done;
                    } else {
                        inner.adds.remove(pos);
                    }
                }
            }
        };
        let item = match self.resolve_video(url, pid, name).await {
            Ok(item) => item,
            Err(err) => {
                finish(room, false);
                return Err(err);
            }
        };
        let mut inner = room.inner.lock().unwrap();
        let mut effects = match inner.state.queue_add(pid, item, now_ms()) {
            Ok(fx) => fx,
            Err(err) => {
                drop(inner);
                finish(room, false);
                return Err(err);
            }
        };
        // Echo the nonce so the sender can match this result to its request.
        for effect in &mut effects {
            if let Effect::Broadcast(ServerEvent::PlaybackChanged { nonce: n, .. } | ServerEvent::QueueUpdated { nonce: n, .. }) = effect {
                *n = nonce.clone();
            }
        }
        inner.dispatch(effects);
        drop(inner);
        finish(room, true);
        Ok(())
    }

    // ------------------------------------------------------------ host actions over REST

    pub async fn update_settings(
        &self,
        slug: &str,
        token: &str,
        name: Option<String>,
        control_mode: Option<ControlMode>,
        queue_mode: Option<QueueMode>,
        // Some("") removes the PIN, Some(p) sets it.
        pin: Option<String>,
    ) -> Result<RoomInfo, AppError> {
        let session = self.authenticate(slug, token)?;
        let room = self.load(slug).await?;
        let is_host = room
            .inner
            .lock()
            .unwrap()
            .state
            .participant(session.participant_id)
            .is_some_and(|p| p.is_host);
        if !is_host {
            return Err(AppError::forbidden("Only the host can change room settings"));
        }
        let password_hash = match pin.as_deref() {
            None => None,
            Some("") => Some(None),
            Some(p) => Some(Some(self.hash_pin(validation::room_pin(p)?).await?)),
        };
        let pin_changed = password_hash.is_some();
        let (info, stored_hash) = {
            let mut inner = room.inner.lock().unwrap();
            let effects = inner
                .state
                .update_settings(session.participant_id, name, control_mode, queue_mode, password_hash)?;
            inner.dispatch(effects);
            if pin_changed {
                inner.pin_failures = 0;
            }
            (inner.state.info(), inner.state.password_hash.clone())
        };
        self.store
            .update_room(room.id, &info.name, info.settings, stored_hash.as_deref())
            .await?;
        Ok(info)
    }

    // ------------------------------------------------------------ developer API

    async fn owned_room(&self, slug: &str, api_key_id: Uuid) -> Result<Arc<LiveRoom>, AppError> {
        let room = self.load(slug).await?;
        let owner = room.inner.lock().unwrap().state.api_key_id;
        if owner != Some(api_key_id) {
            // Same answer as a missing room: do not reveal rooms owned by others.
            return Err(AppError::RoomNotFound);
        }
        Ok(room)
    }

    pub async fn api_enqueue(&self, slug: &str, api_key_id: Uuid, urls: &[String], play_now: bool) -> Result<ApiRoomStatus, AppError> {
        let room = self.owned_room(slug, api_key_id).await?;
        let mut items = Vec::with_capacity(urls.len());
        for url in urls {
            items.push(self.resolve_video(url, Uuid::nil(), "API".into()).await?);
        }
        {
            let mut inner = room.inner.lock().unwrap();
            let now = now_ms();
            for (i, item) in items.into_iter().enumerate() {
                let effects = if play_now && i == 0 {
                    inner.state.play_now(None, item, now)
                } else {
                    inner.state.enqueue(None, item, now)
                };
                inner.dispatch(effects);
            }
        }
        self.api_status(slug, api_key_id).await
    }

    /// Starts a freshly created room with an already resolved video.
    pub async fn seed_item(&self, slug: &str, item: QueueItem) -> Result<(), AppError> {
        let room = self.load(slug).await?;
        let mut inner = room.inner.lock().unwrap();
        let effects = inner.state.enqueue(None, item, now_ms());
        inner.dispatch(effects);
        Ok(())
    }

    pub async fn api_status(&self, slug: &str, api_key_id: Uuid) -> Result<ApiRoomStatus, AppError> {
        let room = self.owned_room(slug, api_key_id).await?;
        let inner = room.inner.lock().unwrap();
        let now = now_ms();
        Ok(ApiRoomStatus {
            room: inner.state.info(),
            participant_count: inner.state.participants().len(),
            now_playing: inner.state.playback.item.clone(),
            status: inner.state.playback.status,
            position: inner.state.playback.position_at(now),
            queue: inner.state.queue.items(),
        })
    }

    // ------------------------------------------------------------ housekeeping

    /// Periodic maintenance: sweep presence, unload idle rooms, expire sessions,
    /// mark active rooms in the database.
    pub async fn maintenance(self: &Arc<Self>) {
        let now = now_ms();
        let idle_ms = self.config.limits.idle_room_ttl.as_millis() as i64;
        let rooms: Vec<Arc<LiveRoom>> = self.rooms.lock().unwrap().values().cloned().collect();
        let mut active = Vec::new();
        let mut unload = Vec::new();
        for room in rooms {
            self.sweep_room(&room, now);
            let inner = room.inner.lock().unwrap();
            if !inner.state.is_empty() {
                active.push(room.id);
            } else if inner.empty_since.is_some_and(|t| now - t >= idle_ms) {
                unload.push(room.slug.clone());
            }
        }
        if !unload.is_empty() {
            let mut map = self.rooms.lock().unwrap();
            for slug in &unload {
                // Re-check under the map lock: someone may have joined meanwhile.
                let removable = map.get(slug).is_some_and(|r| {
                    let mut inner = r.inner.lock().unwrap();
                    let empty = inner.state.is_empty();
                    if empty {
                        inner.unloaded = true;
                    }
                    empty
                });
                if removable {
                    map.remove(slug);
                    tracing::debug!(room = %slug, "idle room unloaded");
                }
            }
        }
        self.sessions.lock().unwrap().retain(|_, s| s.expires_at > now);
        if let Err(err) = self.store.touch_rooms(&active).await {
            tracing::warn!(error = %err, "failed to mark rooms active");
        }
    }

    /// Deletes rooms nobody used within the retention window.
    pub async fn purge_inactive_rooms(&self) -> Result<u64, AppError> {
        let cutoff = now_ms() - self.config.limits.room_retention.as_millis() as i64;
        let deleted = self.store.delete_inactive_rooms(cutoff).await?;
        if deleted > 0 {
            tracing::info!(deleted, "purged inactive rooms");
        }
        Ok(deleted)
    }
}

#[derive(serde::Serialize)]
pub struct ApiRoomStatus {
    pub room: RoomInfo,
    pub participant_count: usize,
    pub now_playing: Option<QueueItem>,
    pub status: crate::room::playback::PlayStatus,
    pub position: f64,
    pub queue: Vec<QueueItem>,
}
