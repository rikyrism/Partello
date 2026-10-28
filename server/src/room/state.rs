//! The in-memory state of one live room and every rule that changes it.
//!
//! This module does no I/O. Each operation validates permissions, mutates the
//! state, and returns a list of [`Effect`]s (who should receive which event,
//! what to persist). The realtime hub applies those effects. Keeping the rules
//! pure makes them easy to test and is the seam for moving room state to a
//! shared store later (see docs/ARCHITECTURE.md).

use std::collections::VecDeque;

use uuid::Uuid;

use super::{
    model::{ChatMessage, ControlMode, Participant, QueueMode, RoomInfo, RoomRecord, RoomSettings, VoiceState},
    playback::{PlayStatus, Playback},
    queue::{Queue, QueueItem},
};
use crate::{
    error::AppError,
    realtime::protocol::{IceServer, LeaveReason, LimitsView, PlaybackAction, SIGNAL_MAX_BYTES, ServerEvent},
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
        }
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
            },
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
            let info = member.info.clone();
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
        fx
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

    /// Picks a new host: the room creator if present, otherwise the longest-present member.
    fn elect_host(&mut self, fx: &mut Effects) {
        let candidate = self
            .members
            .iter()
            .filter(|m| m.announced)
            .find(|m| m.is_creator)
            .or_else(|| self.members.iter().find(|m| m.announced))
            .map(|m| m.info.id);
        if let Some(id) = candidate {
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
        Ok(vec![Effect::Broadcast(ServerEvent::RoomSettingsChanged { room: self.info() })])
    }

    // ---------------------------------------------------------------- chat

    pub fn chat(&mut self, actor: Uuid, body: &str, nonce: Option<String>, now: i64) -> Result<Effects, AppError> {
        let name = self.require(actor)?.info.display_name.clone();
        let body = validation::chat_body(body, self.limits.chat_max_chars)?;
        let message = ChatMessage {
            id: Uuid::new_v4(),
            participant_id: actor,
            display_name: name,
            body,
            created_at: now,
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
            return Err(AppError::Conflict(format!(
                "The call is full ({} people). Calls are peer-to-peer and limited for quality",
                self.limits.max_voice
            )));
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
