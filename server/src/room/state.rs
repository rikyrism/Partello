//! The in-memory state of one live room and every rule that changes it.
//!
//! This module does no I/O. Each operation validates permissions, mutates the
//! state, and returns a list of [`Effect`]s (who should receive which event,
//! what to persist). The realtime hub applies those effects. Keeping the rules
//! pure makes them easy to test and is the seam for moving room state to a
//! shared store later (see docs/ARCHITECTURE.md).

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    model::{ChatMessage, ControlMode, Participant, QueueMode, ReplyRef, RoomInfo, RoomRecord, RoomSettings, VoiceState},
    playback::{PlayStatus, Playback},
    queue::{Queue, QueueItem},
};
use crate::{
    error::AppError,
    realtime::protocol::{CallMode, IceServer, LeaveReason, LimitsView, PlaybackAction, REACTIONS, SIGNAL_MAX_BYTES, ServerEvent},
    validation,
};

#[derive(Debug)]
pub enum Effect {
    Broadcast(ServerEvent),
    BroadcastExcept(Uuid, ServerEvent),
    To(Uuid, ServerEvent),
    PersistChat(ChatMessage),
}

pub type Effects = Vec<Effect>;

#[derive(Debug, Clone, Copy)]
pub struct RoomLimits {
    pub max_voice: u32,
    pub max_queue: usize,
    pub chat_history: usize,
    pub chat_max_chars: usize,
    pub search_enabled: bool,
    pub call_mode: CallMode,
}

/// A host who has been disconnected this long hands host controls to someone
/// who is still here, so the room is never stuck without anyone to press play.
pub const HOST_HANDOFF_MS: i64 = 10_000;
/// After a restart, a video that was playing keeps playing if the server was
/// back within this window (everyone's players kept running meanwhile).
pub const RESUME_WINDOW_MS: i64 = 2 * 60 * 1000;
/// Length of the 3-2-1 before a synchronized start.
pub const COUNTDOWN_MS: i64 = 3_000;
/// Longest quoted excerpt in a reply.
const REPLY_EXCERPT_CHARS: usize = 140;

/// What survives a restart or an unload: the current video and the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedState {
    pub playback: Playback,
    pub queue: Vec<QueueItem>,
    pub saved_at: i64,
}

#[derive(Debug)]
struct Member {
    info: Participant,
    is_creator: bool,
    connections: u32,
    /// Becomes true on the first WebSocket connection; reserved seats are invisible.
    announced: bool,
    disconnected_at: Option<i64>,
    reserved_at: i64,
}

#[derive(Debug)]
pub struct RoomState {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub settings: RoomSettings,
    pub max_participants: u32,
    pub created_at: i64,
    pub host_key_hash: Vec<u8>,
    pub api_key_id: Option<Uuid>,
    pub password_hash: Option<String>,
    pub playback: Playback,
    pub queue: Queue,
    members: Vec<Member>,
    chat: VecDeque<ChatMessage>,
    limits: RoomLimits,
    /// Voters to skip `skip_item`; stale once another video plays.
    skip_votes: Vec<Uuid>,
    skip_item: Option<Uuid>,
    /// The host that host controls were taken from while they were away;
    /// they get them back when they reconnect.
    handed_off_from: Option<Uuid>,
}

const ONLY_HOST_PLAYBACK: &str = "Only the host can control playback in this room";

impl RoomState {
    pub fn new(record: &RoomRecord, limits: RoomLimits, history: Vec<ChatMessage>, now: i64) -> Self {
        Self {
            id: record.id,
            slug: record.slug.clone(),
            name: record.name.clone(),
            settings: record.settings,
            max_participants: record.max_participants,
            created_at: record.created_at,
            host_key_hash: record.host_key_hash.clone(),
            api_key_id: record.api_key_id,
            password_hash: record.password_hash.clone(),
            playback: Playback::new(now),
            queue: Queue::default(),
            members: Vec::new(),
            chat: history.into_iter().collect(),
            limits,
            skip_votes: Vec::new(),
            skip_item: None,
            handed_off_from: None,
        }
    }

    pub fn save(&self, now: i64) -> SavedState {
        SavedState {
            playback: self.playback.clone(),
            queue: self.queue.items(),
            saved_at: now,
        }
    }

    /// Puts back the video and queue from before a restart or unload.
    pub fn restore(&mut self, saved: SavedState, now: i64) {
        let mut playback = saved.playback;
        if playback.status == PlayStatus::Playing && now - saved.saved_at > RESUME_WINDOW_MS {
            // Long gone: wait at the spot where everyone stopped watching.
            playback.position = playback.position_at(saved.saved_at);
            playback.status = PlayStatus::Paused;
            playback.updated_at = now;
            playback.version += 1;
        }
        self.playback = playback;
        self.queue = Queue::from_items(saved.queue);
    }

    pub fn info(&self) -> RoomInfo {
        RoomInfo {
            slug: self.slug.clone(),
            name: self.name.clone(),
            settings: self.settings,
            max_participants: self.max_participants,
            created_at: self.created_at,
            has_password: self.password_hash.is_some(),
        }
    }

    pub fn participants(&self) -> Vec<Participant> {
        self.members.iter().filter(|m| m.announced).map(|m| m.info.clone()).collect()
    }

    pub fn participant(&self, id: Uuid) -> Option<&Participant> {
        self.member(id).map(|m| &m.info)
    }

    /// Seats taken, including reservations that have not connected yet.
    pub fn occupancy(&self) -> usize {
        self.members.len()
    }

    /// People here with a live connection right now.
    pub fn connected_count(&self) -> usize {
        self.members.iter().filter(|m| m.announced && m.info.connected).count()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn is_member(&self, id: Uuid) -> bool {
        self.member(id).is_some()
    }

    pub fn chat_history(&self) -> Vec<ChatMessage> {
        self.chat.iter().cloned().collect()
    }

    fn member(&self, id: Uuid) -> Option<&Member> {
        self.members.iter().find(|m| m.info.id == id)
    }

    fn member_mut(&mut self, id: Uuid) -> Option<&mut Member> {
        self.members.iter_mut().find(|m| m.info.id == id)
    }

    fn require(&self, id: Uuid) -> Result<&Member, AppError> {
        self.member(id).filter(|m| m.announced).ok_or(AppError::Unauthorized)
    }

    pub fn snapshot(&self, self_id: Uuid, ice_servers: Vec<IceServer>, now: i64) -> ServerEvent {
        ServerEvent::RoomStateSync {
            self_id,
            room: self.info(),
            participants: self.participants(),
            playback: self.playback.clone(),
            queue: self.queue.items(),
            chat: self.chat_history(),
            ice_servers,
            limits: LimitsView {
                chat_max_chars: self.limits.chat_max_chars,
                max_voice_participants: self.limits.max_voice,
                max_queue_len: self.limits.max_queue,
                search_enabled: self.limits.search_enabled,
                playlist_import: self.limits.search_enabled,
                call_mode: self.limits.call_mode,
            },
            skip_votes: self.current_skip_votes(),
            server_time: now,
        }
    }

    // ---------------------------------------------------------------- presence

    /// Holds a seat for someone who passed the REST join. Idempotent.
    pub fn reserve(&mut self, id: Uuid, display_name: String, is_creator: bool, now: i64) -> Result<(), AppError> {
        if let Some(m) = self.member_mut(id) {
            m.is_creator |= is_creator;
            return Ok(());
        }
        if self.members.len() >= self.max_participants as usize {
            return Err(AppError::RoomFull);
        }
        self.members.push(Member {
            info: Participant {
                id,
                display_name,
                is_host: false,
                connected: false,
                joined_at: now,
                voice: None,
            },
            is_creator,
            connections: 0,
            announced: false,
            disconnected_at: None,
            reserved_at: now,
        });
        Ok(())
    }

    /// A WebSocket for `id` authenticated.
    pub fn connect(&mut self, id: Uuid, now: i64) -> Result<Effects, AppError> {
        let mut fx = Vec::new();
        let member = self.member_mut(id).ok_or(AppError::Unauthorized)?;
        member.connections += 1;
        if !member.announced {
            member.announced = true;
            member.info.connected = true;
            member.info.joined_at = now;
            let is_creator = member.is_creator;
            if is_creator || self.host_id().is_none() {
                self.set_host(id, &mut fx);
            }
            let info = self.member(id).unwrap().info.clone();
            fx.push(Effect::BroadcastExcept(id, ServerEvent::ParticipantJoined { participant: info }));
        } else if member.disconnected_at.take().is_some() || !member.info.connected {
            member.info.connected = true;
            if self.handed_off_from == Some(id) {
                self.handed_off_from = None;
                self.set_host(id, &mut fx);
            }
            let info = self.member(id).unwrap().info.clone();
            fx.push(Effect::Broadcast(ServerEvent::ParticipantUpdated { participant: info }));
        }
        Ok(fx)
    }

    /// A WebSocket for `id` closed. The member stays for the grace window.
    pub fn disconnect(&mut self, id: Uuid, now: i64) -> Effects {
        let Some(member) = self.member_mut(id) else {
            return Vec::new();
        };
        member.connections = member.connections.saturating_sub(1);
        if member.connections == 0 && member.announced && member.info.connected {
            member.info.connected = false;
            // Without a connection nobody can signal them: free the call seat now.
            member.info.voice = None;
            member.disconnected_at = Some(now);
            let info = member.info.clone();
            return vec![Effect::BroadcastExcept(id, ServerEvent::ParticipantUpdated { participant: info })];
        }
        Vec::new()
    }

    /// Removes members whose grace window or reservation expired.
    pub fn sweep(&mut self, now: i64, grace_ms: i64, reservation_ttl_ms: i64) -> Effects {
        let expired: Vec<Uuid> = self
            .members
            .iter()
            .filter(|m| {
                if m.announced {
                    m.connections == 0 && m.disconnected_at.is_some_and(|t| now - t >= grace_ms)
                } else {
                    m.connections == 0 && now - m.reserved_at >= reservation_ttl_ms
                }
            })
            .map(|m| m.info.id)
            .collect();
        let mut fx = Vec::new();
        for id in expired {
            fx.extend(self.remove(id, LeaveReason::Disconnected));
        }
        self.hand_off_host(now, &mut fx);
        fx
    }

    /// Moves host controls away from a host who has been gone a while.
    fn hand_off_host(&mut self, now: i64, fx: &mut Effects) {
        let Some(host) = self.members.iter().find(|m| m.info.is_host) else {
            return;
        };
        let away = !host.info.connected && host.disconnected_at.is_some_and(|t| now - t >= HOST_HANDOFF_MS);
        if !away {
            return;
        }
        let from = host.info.id;
        let Some(to) = self.successor(Some(from)) else {
            return; // nobody else is here
        };
        // Remember the original holder only once, across several hand-offs.
        if self.handed_off_from.is_none() {
            self.handed_off_from = Some(from);
        }
        self.set_host(to, fx);
        let info = self.member(to).unwrap().info.clone();
        fx.push(Effect::Broadcast(ServerEvent::ParticipantUpdated { participant: info }));
    }

    /// Best next host: the room creator if connected, else whoever has been
    /// here longest and is connected, else anyone still announced.
    fn successor(&self, except: Option<Uuid>) -> Option<Uuid> {
        let eligible = || self.members.iter().filter(|m| m.announced && Some(m.info.id) != except);
        eligible()
            .find(|m| m.is_creator && m.info.connected)
            .or_else(|| eligible().filter(|m| m.info.connected).min_by_key(|m| m.info.joined_at))
            .or_else(|| eligible().find(|m| m.is_creator))
            .or_else(|| eligible().min_by_key(|m| m.info.joined_at))
            .map(|m| m.info.id)
    }

    /// Explicit leave: immediate removal.
    pub fn leave(&mut self, id: Uuid) -> Effects {
        self.remove(id, LeaveReason::Left)
    }

    fn remove(&mut self, id: Uuid, reason: LeaveReason) -> Effects {
        let Some(idx) = self.members.iter().position(|m| m.info.id == id) else {
            return Vec::new();
        };
        let member = self.members.remove(idx);
        self.skip_votes.retain(|v| *v != id);
        if self.handed_off_from == Some(id) {
            self.handed_off_from = None;
        }
        let mut fx = Vec::new();
        if member.announced {
            fx.push(Effect::Broadcast(ServerEvent::ParticipantLeft {
                participant_id: id,
                display_name: member.info.display_name,
                reason,
            }));
        }
        if member.info.is_host {
            self.elect_host(&mut fx);
        }
        fx
    }

    pub fn host_id(&self) -> Option<Uuid> {
        self.members.iter().find(|m| m.info.is_host).map(|m| m.info.id)
    }

    fn set_host(&mut self, id: Uuid, fx: &mut Effects) {
        if self.host_id() == Some(id) {
            return;
        }
        for m in self.members.iter_mut() {
            let should = m.info.id == id;
            if m.info.is_host != should {
                m.info.is_host = should;
                // The new host is announced by the caller (joined/updated event).
                if m.announced && !should {
                    fx.push(Effect::Broadcast(ServerEvent::ParticipantUpdated {
                        participant: m.info.clone(),
                    }));
                }
            }
        }
    }

    /// Picks a new host after the host left (see [`Self::successor`]).
    fn elect_host(&mut self, fx: &mut Effects) {
        if let Some(id) = self.successor(None) {
            self.set_host(id, fx);
            let info = self.member(id).unwrap().info.clone();
            fx.push(Effect::Broadcast(ServerEvent::ParticipantUpdated { participant: info }));
        }
    }

    /// Host removes someone. Their sessions are revoked by the hub.
    pub fn kick(&mut self, actor: Uuid, target: Uuid) -> Result<Effects, AppError> {
        if !self.require(actor)?.info.is_host {
            return Err(AppError::forbidden("Only the host can remove people"));
        }
        if actor == target {
            return Err(AppError::validation("You can't remove yourself; use Leave"));
        }
        if !self.is_member(target) {
            return Err(AppError::Conflict("That person already left".into()));
        }
        Ok(self.remove(target, LeaveReason::Kicked))
    }

    pub fn host_transfer(&mut self, actor: Uuid, to: Uuid) -> Result<Effects, AppError> {
        if !self.require(actor)?.info.is_host {
            return Err(AppError::forbidden("Only the host can hand over host controls"));
        }
        self.require(to)?;
        let mut fx = Vec::new();
        // A deliberate hand-over is final.
        self.handed_off_from = None;
        self.set_host(to, &mut fx);
        let info = self.member(to).unwrap().info.clone();
        fx.push(Effect::Broadcast(ServerEvent::ParticipantUpdated { participant: info }));
        Ok(fx)
    }

    // ---------------------------------------------------------------- settings

    pub fn update_settings(
        &mut self,
        actor: Uuid,
        name: Option<String>,
        control_mode: Option<ControlMode>,
        queue_mode: Option<QueueMode>,
        permanent: Option<bool>,
        // Some(Some(hash)) sets, Some(None) removes, None leaves it alone.
        password_hash: Option<Option<String>>,
    ) -> Result<Effects, AppError> {
        if !self.require(actor)?.info.is_host {
            return Err(AppError::forbidden("Only the host can change room settings"));
        }
        // Validate first so a rejected save changes nothing.
        let name = name.map(|n| validation::room_name(&n)).transpose()?;
        if let Some(name) = name {
            self.name = name;
        }
        if let Some(hash) = password_hash {
            self.password_hash = hash;
        }
        if let Some(mode) = control_mode {
            self.settings.control_mode = mode;
        }
        if let Some(mode) = queue_mode {
            self.settings.queue_mode = mode;
        }
        if let Some(keep) = permanent {
            self.settings.permanent = keep;
        }
        Ok(vec![Effect::Broadcast(ServerEvent::RoomSettingsChanged { room: self.info() })])
    }

    // ---------------------------------------------------------------- chat

    pub fn chat(&mut self, actor: Uuid, body: &str, nonce: Option<String>, reply_to: Option<Uuid>, now: i64) -> Result<Effects, AppError> {
        let name = self.require(actor)?.info.display_name.clone();
        let body = validation::chat_body(body, self.limits.chat_max_chars)?;
        // Replies to messages older than the kept history simply lose the quote.
        let reply_to = reply_to.and_then(|id| self.chat.iter().find(|m| m.id == id)).map(|m| ReplyRef {
            id: m.id,
            participant_id: m.participant_id,
            display_name: m.display_name.clone(),
            body: excerpt(&m.body),
        });
        let message = ChatMessage {
            id: Uuid::new_v4(),
            participant_id: actor,
            display_name: name,
            body,
            created_at: now,
            reply_to,
            nonce: nonce.filter(|n| n.len() <= crate::realtime::protocol::NONCE_MAX),
        };
        if self.chat.len() >= self.limits.chat_history {
            self.chat.pop_front();
        }
        let mut stored = message.clone();
        stored.nonce = None;
        self.chat.push_back(stored.clone());
        Ok(vec![
            Effect::Broadcast(ServerEvent::ChatMessageCreated { message }),
            Effect::PersistChat(stored),
        ])
    }

    // ---------------------------------------------------------------- reactions

    pub fn react(&self, actor: Uuid, emoji: &str) -> Result<Effects, AppError> {
        self.require(actor)?;
        if !REACTIONS.contains(&emoji) {
            return Err(AppError::validation("Unknown reaction"));
        }
        Ok(vec![Effect::Broadcast(ServerEvent::Reaction {
            participant_id: actor,
            emoji: emoji.to_string(),
        })])
    }

    // ---------------------------------------------------------------- skip votes

    fn current_skip_votes(&self) -> Vec<Uuid> {
        let current = self.playback.item.as_ref().map(|i| i.id);
        if current.is_some() && current == self.skip_item {
            self.skip_votes.clone()
        } else {
            Vec::new()
        }
    }

    /// Votes needed to skip: a majority of the people connected right now.
    pub fn skip_votes_needed(&self) -> usize {
        let connected = self.members.iter().filter(|m| m.announced && m.info.connected).count();
        connected / 2 + 1
    }

    pub fn skip_vote(&mut self, actor: Uuid, item_id: Uuid, vote: bool, now: i64) -> Result<Effects, AppError> {
        self.require(actor)?;
        let current = self.playback.item.as_ref().map(|i| i.id);
        if current != Some(item_id) || !matches!(self.playback.status, PlayStatus::Playing | PlayStatus::Paused) {
            return Ok(Vec::new()); // the video changed meanwhile
        }
        if self.skip_item != current {
            self.skip_item = current;
            self.skip_votes.clear();
        }
        let had = self.skip_votes.contains(&actor);
        if vote == had {
            return Ok(Vec::new());
        }
        if vote {
            self.skip_votes.push(actor);
        } else {
            self.skip_votes.retain(|v| *v != actor);
        }
        let connected: Vec<Uuid> = self
            .members
            .iter()
            .filter(|m| m.announced && m.info.connected)
            .map(|m| m.info.id)
            .collect();
        let counted = self.skip_votes.iter().filter(|v| connected.contains(v)).count();
        let skipped = vote && counted >= self.skip_votes_needed();
        let mut fx = vec![Effect::Broadcast(ServerEvent::SkipVotes {
            item_id,
            voters: self.skip_votes.clone(),
            skipped,
        })];
        if skipped {
            self.skip_votes.clear();
            self.skip_item = None;
            fx.extend(self.advance(None, now));
        }
        Ok(fx)
    }

    // ---------------------------------------------------------------- playback

    pub fn can_control(&self, actor: Uuid) -> bool {
        self.member(actor)
            .is_some_and(|m| m.info.is_host || self.settings.control_mode == ControlMode::Everyone)
    }

    fn require_control(&self, actor: Uuid) -> Result<(), AppError> {
        self.require(actor)?;
        if self.can_control(actor) {
            Ok(())
        } else {
            Err(AppError::forbidden(ONLY_HOST_PLAYBACK))
        }
    }

    fn playback_changed(&self, actor: Option<Uuid>, now: i64) -> Effect {
        Effect::Broadcast(ServerEvent::PlaybackChanged {
            playback: self.playback.clone(),
            actor_id: actor,
            server_time: now,
            nonce: None,
        })
    }

    fn queue_updated(&self, actor: Option<Uuid>) -> Effect {
        Effect::Broadcast(ServerEvent::QueueUpdated {
            queue: self.queue.items(),
            actor_id: actor,
            nonce: None,
        })
    }

    pub fn playback_control(&mut self, actor: Uuid, action: PlaybackAction, position: f64, now: i64) -> Result<Effects, AppError> {
        self.require_control(actor)?;
        if !position.is_finite() || position < 0.0 {
            return Err(AppError::validation("Invalid playback position"));
        }
        let changed = match action {
            PlaybackAction::Play => self.playback.play(position, now),
            PlaybackAction::Pause => self.playback.pause(position, now),
            PlaybackAction::Seek => self.playback.seek(position, now),
        };
        if !changed {
            return Err(AppError::Conflict("Nothing is playing yet. Add a video first".into()));
        }
        Ok(vec![self.playback_changed(Some(actor), now)])
    }

    /// Pauses where the video is and announces a start in [`COUNTDOWN_MS`].
    /// Returns the effects and the playback version the countdown belongs to.
    pub fn start_countdown(&mut self, actor: Uuid, now: i64) -> Result<(Effects, u64), AppError> {
        self.require_control(actor)?;
        if !matches!(self.playback.status, PlayStatus::Playing | PlayStatus::Paused) {
            return Err(AppError::Conflict("Nothing is playing yet. Add a video first".into()));
        }
        let position = self.playback.position_at(now);
        self.playback.pause(position, now);
        let version = self.playback.version;
        Ok((
            vec![
                self.playback_changed(Some(actor), now),
                Effect::Broadcast(ServerEvent::Countdown {
                    start_at: now + COUNTDOWN_MS,
                    version,
                    actor_id: actor,
                }),
            ],
            version,
        ))
    }

    /// The countdown ended: play, unless someone paused, seeked or changed
    /// the video meanwhile.
    pub fn finish_countdown(&mut self, version: u64, now: i64) -> Effects {
        if self.playback.version != version || self.playback.status != PlayStatus::Paused {
            return Vec::new();
        }
        let position = self.playback.position;
        self.playback.play(position, now);
        vec![self.playback_changed(None, now)]
    }

    /// Only people who may control playback report the duration: it bounds every
    /// position and decides when a viewer's "ended" report is believed.
    pub fn playback_duration(&mut self, actor: Uuid, item_id: Uuid, duration: f64) -> Result<Effects, AppError> {
        self.require(actor)?;
        if self.can_control(actor) && self.playback.item.as_ref().map(|i| i.id) == Some(item_id) {
            self.playback.set_duration(duration);
        }
        Ok(Vec::new())
    }

    /// A client's player reached the end. Controllers are trusted; others only
    /// when the server's own estimate agrees, so viewers cannot skip videos.
    pub fn playback_ended(&mut self, actor: Uuid, item_id: Uuid, now: i64) -> Result<Effects, AppError> {
        self.require(actor)?;
        let is_current = self.playback.item.as_ref().map(|i| i.id) == Some(item_id);
        if !is_current || self.playback.status == PlayStatus::Ended {
            return Ok(Vec::new()); // stale or duplicate report
        }
        if !self.can_control(actor) && !self.playback.near_end(now) {
            return Ok(Vec::new());
        }
        Ok(self.advance(Some(actor), now))
    }

    fn advance(&mut self, actor: Option<Uuid>, now: i64) -> Effects {
        match self.queue.pop_next() {
            Some(next) => {
                self.playback.load(next, true, now);
                vec![self.queue_updated(actor), self.playback_changed(actor, now)]
            }
            None if self.playback.has_item() => {
                self.playback.finish(now);
                vec![self.playback_changed(actor, now)]
            }
            None => Vec::new(),
        }
    }

    // ---------------------------------------------------------------- queue

    pub fn can_add_to_queue(&self, actor: Uuid) -> Result<(), AppError> {
        let m = self.require(actor)?;
        if self.settings.queue_mode == QueueMode::HostOnly && !m.info.is_host {
            return Err(AppError::forbidden("Only the host can add videos in this room"));
        }
        if self.queue.len() >= self.limits.max_queue {
            return Err(AppError::Conflict(format!("The queue is full ({} videos)", self.limits.max_queue)));
        }
        Ok(())
    }

    pub fn queue_add(&mut self, actor: Uuid, item: QueueItem, now: i64) -> Result<Effects, AppError> {
        self.can_add_to_queue(actor)?;
        Ok(self.enqueue(Some(actor), item, now))
    }

    /// Adds without participant permissions (used by the developer API, which
    /// authorizes by API key ownership). Starts playback when nothing is playing.
    pub fn enqueue(&mut self, actor: Option<Uuid>, item: QueueItem, now: i64) -> Effects {
        if matches!(self.playback.status, PlayStatus::Idle | PlayStatus::Ended) {
            self.playback.load(item, true, now);
            return vec![self.playback_changed(actor, now)];
        }
        if !self.queue.push(item, self.limits.max_queue) {
            return Vec::new();
        }
        vec![self.queue_updated(actor)]
    }

    /// Adds several videos at once (a playlist) with one queue update.
    /// Returns how many fit.
    pub fn queue_add_many(&mut self, actor: Uuid, items: Vec<QueueItem>, now: i64) -> Result<(Effects, usize), AppError> {
        self.can_add_to_queue(actor)?;
        let mut fx = Vec::new();
        let mut items = items.into_iter();
        let mut added = 0;
        if matches!(self.playback.status, PlayStatus::Idle | PlayStatus::Ended) {
            if let Some(first) = items.next() {
                self.playback.load(first, true, now);
                fx.push(self.playback_changed(Some(actor), now));
                added += 1;
            }
        }
        for item in items {
            if !self.queue.push(item, self.limits.max_queue) {
                break;
            }
            added += 1;
        }
        if added > fx.len() {
            fx.push(self.queue_updated(Some(actor)));
        }
        Ok((fx, added))
    }

    /// Replaces the current video immediately.
    pub fn play_now(&mut self, actor: Option<Uuid>, item: QueueItem, now: i64) -> Effects {
        self.playback.load(item, true, now);
        vec![self.playback_changed(actor, now)]
    }

    pub fn queue_remove(&mut self, actor: Uuid, item_id: Uuid) -> Result<Effects, AppError> {
        let m = self.require(actor)?;
        let item = self
            .queue
            .get(item_id)
            .ok_or_else(|| AppError::Conflict("That video is no longer in the queue".into()))?;
        let own = item.added_by == actor && self.settings.queue_mode == QueueMode::Everyone;
        if !(m.info.is_host || own) {
            return Err(AppError::forbidden("You can only remove videos you added"));
        }
        self.queue.remove(item_id);
        Ok(vec![self.queue_updated(Some(actor))])
    }

    pub fn queue_play(&mut self, actor: Uuid, item_id: Uuid, now: i64) -> Result<Effects, AppError> {
        self.require_control(actor)?;
        let item = self
            .queue
            .remove(item_id)
            .ok_or_else(|| AppError::Conflict("That video is no longer in the queue".into()))?;
        self.playback.load(item, true, now);
        Ok(vec![self.queue_updated(Some(actor)), self.playback_changed(Some(actor), now)])
    }

    pub fn queue_move(&mut self, actor: Uuid, item_id: Uuid, to_index: usize) -> Result<Effects, AppError> {
        self.require_control(actor)?;
        if !self.queue.move_to(item_id, to_index) {
            return Err(AppError::Conflict("That video is no longer in the queue".into()));
        }
        Ok(vec![self.queue_updated(Some(actor))])
    }

    pub fn queue_skip(&mut self, actor: Uuid, now: i64) -> Result<Effects, AppError> {
        self.require_control(actor)?;
        Ok(self.advance(Some(actor), now))
    }

    // ---------------------------------------------------------------- voice

    fn voice_count(&self) -> usize {
        self.members.iter().filter(|m| m.info.voice.is_some()).count()
    }

    fn voice_update_effect(&self, actor: Uuid) -> Effect {
        Effect::Broadcast(ServerEvent::ParticipantUpdated {
            participant: self.member(actor).unwrap().info.clone(),
        })
    }

    pub fn voice_join(&mut self, actor: Uuid, state: VoiceState) -> Result<Effects, AppError> {
        let in_call = self.require(actor)?.info.voice.is_some();
        if !in_call && self.voice_count() >= self.limits.max_voice as usize {
            return Err(AppError::Conflict(format!("The call is full ({} people)", self.limits.max_voice)));
        }
        self.member_mut(actor).unwrap().info.voice = Some(state);
        Ok(vec![self.voice_update_effect(actor)])
    }

    pub fn voice_update(&mut self, actor: Uuid, state: VoiceState) -> Result<Effects, AppError> {
        let m = self.require(actor)?;
        if m.info.voice.is_none() {
            return Err(AppError::Conflict("Join the call first".into()));
        }
        if m.info.voice == Some(state) {
            return Ok(Vec::new());
        }
        self.member_mut(actor).unwrap().info.voice = Some(state);
        Ok(vec![self.voice_update_effect(actor)])
    }

    pub fn voice_leave(&mut self, actor: Uuid) -> Result<Effects, AppError> {
        if self.require(actor)?.info.voice.is_none() {
            return Ok(Vec::new());
        }
        self.member_mut(actor).unwrap().info.voice = None;
        Ok(vec![self.voice_update_effect(actor)])
    }

    /// Relays an SDP description or ICE candidate to one peer. Both ends must be in the call.
    pub fn voice_signal(&self, actor: Uuid, to: Uuid, data: serde_json::Value) -> Result<Effects, AppError> {
        if actor == to {
            return Err(AppError::validation("Cannot signal yourself"));
        }
        if self.require(actor)?.info.voice.is_none() {
            return Err(AppError::Conflict("Join the call first".into()));
        }
        if self.member(to).and_then(|m| m.info.voice).is_none() {
            return Err(AppError::Conflict("That person is not in the call".into()));
        }
        let kind = data.get("kind").and_then(|k| k.as_str());
        if !matches!(kind, Some("description" | "candidate")) {
            return Err(AppError::validation("Unknown signal kind"));
        }
        if serde_json::to_vec(&data).map(|v| v.len()).unwrap_or(usize::MAX) > SIGNAL_MAX_BYTES {
            return Err(AppError::validation("Signal too large"));
        }
        Ok(vec![Effect::To(to, ServerEvent::VoiceSignal { from: actor, data })])
    }
}

fn excerpt(body: &str) -> String {
    let mut out: String = body.chars().take(REPLY_EXCERPT_CHARS).collect();
    if body.chars().count() > REPLY_EXCERPT_CHARS {
        out.push('…');
    }
    out
}
