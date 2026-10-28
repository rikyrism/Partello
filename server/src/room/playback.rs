//! Authoritative playback state of a room.
//!
//! The server never runs a clock per room. It stores the position at the moment
//! of the last change (`updated_at`) and whether time is advancing; anyone can
//! compute the current position from that. Clients do the same arithmetic with
//! their estimated server clock offset.

use serde::Serialize;

use super::queue::QueueItem;

/// Longest position we accept when the duration is still unknown (24h).
const MAX_POSITION: f64 = 86_400.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayStatus {
    /// Nothing loaded.
    Idle,
    Playing,
    Paused,
    /// The current item finished and the queue was empty.
    Ended,
}

#[derive(Debug, Clone, Serialize)]
pub struct Playback {
    pub item: Option<QueueItem>,
    pub status: PlayStatus,
    /// Seconds into the video at `updated_at`.
    pub position: f64,
    /// Server wall clock in Unix milliseconds.
    pub updated_at: i64,
    /// Bumped on every change so clients can drop stale updates.
    pub version: u64,
    /// Reported by the first client whose player loaded the video.
    pub duration: Option<f64>,
}

impl Playback {
    pub fn new(now: i64) -> Self {
        Self {
            item: None,
            status: PlayStatus::Idle,
            position: 0.0,
            updated_at: now,
            version: 0,
            duration: None,
        }
    }

    pub fn position_at(&self, now: i64) -> f64 {
        let pos = match self.status {
            PlayStatus::Playing => self.position + (now - self.updated_at).max(0) as f64 / 1000.0,
            _ => self.position,
        };
        self.clamp(pos)
    }

    fn clamp(&self, pos: f64) -> f64 {
        let max = self.duration.unwrap_or(MAX_POSITION);
        if pos.is_finite() { pos.clamp(0.0, max) } else { 0.0 }
    }

    fn bump(&mut self, now: i64) {
        self.updated_at = now;
        self.version += 1;
    }

    pub fn has_item(&self) -> bool {
        self.item.is_some() && self.status != PlayStatus::Idle
    }

    pub fn play(&mut self, position: f64, now: i64) -> bool {
        if !self.has_item() {
            return false;
        }
        self.position = self.clamp(position);
        self.status = PlayStatus::Playing;
        self.bump(now);
        true
    }

    pub fn pause(&mut self, position: f64, now: i64) -> bool {
        if !self.has_item() {
            return false;
        }
        self.position = self.clamp(position);
        self.status = PlayStatus::Paused;
        self.bump(now);
        true
    }

    /// Seeking keeps the play/pause status; seeking an ended video pauses it there.
    pub fn seek(&mut self, position: f64, now: i64) -> bool {
        if !self.has_item() {
            return false;
        }
        self.position = self.clamp(position);
        if self.status == PlayStatus::Ended {
            self.status = PlayStatus::Paused;
        }
        self.bump(now);
        true
    }

    pub fn load(&mut self, item: QueueItem, autoplay: bool, now: i64) {
        self.position = item.start.map(f64::from).unwrap_or(0.0);
        self.item = Some(item);
        self.duration = None;
        self.status = if autoplay { PlayStatus::Playing } else { PlayStatus::Paused };
        self.bump(now);
    }

    pub fn finish(&mut self, now: i64) {
        self.position = self.duration.unwrap_or(self.position_at(now));
        self.status = PlayStatus::Ended;
        self.bump(now);
    }

    pub fn set_duration(&mut self, duration: f64) -> bool {
        if self.duration.is_some() || !duration.is_finite() || duration <= 0.0 || duration > 7.0 * MAX_POSITION {
            return false;
        }
        self.duration = Some(duration);
        true
    }

    /// True when the server-side estimate says the video has (nearly) reached its end.
    pub fn near_end(&self, now: i64) -> bool {
        match self.duration {
            Some(d) => self.status == PlayStatus::Playing && self.position_at(now) >= d - 5.0,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn item() -> QueueItem {
        QueueItem {
            id: Uuid::new_v4(),
            video_id: "dQw4w9WgXcQ".into(),
            title: "t".into(),
            author: None,
            thumbnail_url: String::new(),
            start: None,
            added_by: Uuid::nil(),
            added_by_name: "a".into(),
        }
    }

    #[test]
    fn commands_without_item_are_rejected() {
        let mut p = Playback::new(0);
        assert!(!p.play(0.0, 1));
        assert!(!p.pause(0.0, 1));
        assert!(!p.seek(10.0, 1));
        assert_eq!(p.version, 0);
    }

    #[test]
    fn position_advances_only_while_playing() {
        let mut p = Playback::new(0);
        p.load(item(), true, 1_000);
        assert_eq!(p.status, PlayStatus::Playing);
        assert_eq!(p.position_at(11_000), 10.0);
        p.pause(10.0, 11_000);
        assert_eq!(p.position_at(60_000), 10.0);
        p.play(10.0, 60_000);
        assert_eq!(p.position_at(62_500), 12.5);
    }

    #[test]
    fn seek_keeps_status_and_clamps() {
        let mut p = Playback::new(0);
        p.load(item(), false, 0);
        p.seek(42.0, 5);
        assert_eq!(p.status, PlayStatus::Paused);
        assert_eq!(p.position, 42.0);
        p.seek(-3.0, 6);
        assert_eq!(p.position, 0.0);
        p.seek(f64::NAN, 7);
        assert_eq!(p.position, 0.0);
        p.set_duration(100.0);
        p.seek(500.0, 8);
        assert_eq!(p.position, 100.0);
    }

    #[test]
    fn versions_increase_monotonically() {
        let mut p = Playback::new(0);
        p.load(item(), true, 0);
        let v1 = p.version;
        p.pause(1.0, 1);
        p.play(1.0, 2);
        p.seek(5.0, 3);
        assert_eq!(p.version, v1 + 3);
    }

    #[test]
    fn duration_is_reported_once_and_sane() {
        let mut p = Playback::new(0);
        assert!(!p.set_duration(-1.0));
        assert!(!p.set_duration(f64::INFINITY));
        assert!(p.set_duration(212.0));
        assert!(!p.set_duration(300.0), "first report wins");
        p.load(item(), true, 0);
        assert_eq!(p.duration, None, "loading resets duration");
    }

    #[test]
    fn near_end_uses_duration() {
        let mut p = Playback::new(0);
        p.load(item(), true, 0);
        assert!(!p.near_end(1_000_000));
        p.set_duration(60.0);
        assert!(!p.near_end(30_000));
        assert!(p.near_end(56_000));
        assert!(p.position_at(90_000) <= 60.0);
    }
}
