//! Shared "up next" list. The playing item lives in [`super::playback::Playback`].

use std::collections::VecDeque;

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QueueItem {
    pub id: Uuid,
    pub video_id: String,
    pub title: String,
    pub author: Option<String>,
    pub thumbnail_url: String,
    pub start: Option<u32>,
    pub added_by: Uuid,
    pub added_by_name: String,
}

#[derive(Debug, Default)]
pub struct Queue {
    items: VecDeque<QueueItem>,
}

impl Queue {
    pub fn items(&self) -> Vec<QueueItem> {
        self.items.iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, id: Uuid) -> Option<&QueueItem> {
        self.items.iter().find(|i| i.id == id)
    }

    /// Returns false when the queue is at capacity.
    pub fn push(&mut self, item: QueueItem, max_len: usize) -> bool {
        if self.items.len() >= max_len {
            return false;
        }
        self.items.push_back(item);
        true
    }

    pub fn remove(&mut self, id: Uuid) -> Option<QueueItem> {
        let idx = self.items.iter().position(|i| i.id == id)?;
        self.items.remove(idx)
    }

    pub fn pop_next(&mut self) -> Option<QueueItem> {
        self.items.pop_front()
    }

    /// Moves an item to `to_index` (clamped). Returns false when the id is unknown.
    pub fn move_to(&mut self, id: Uuid, to_index: usize) -> bool {
        let Some(item) = self.remove(id) else {
            return false;
        };
        let idx = to_index.min(self.items.len());
        self.items.insert(idx, item);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(n: u8) -> QueueItem {
        QueueItem {
            id: Uuid::from_bytes([n; 16]),
            video_id: format!("video{n:06}"),
            title: format!("Video {n}"),
            author: None,
            thumbnail_url: String::new(),
            start: None,
            added_by: Uuid::nil(),
            added_by_name: "a".into(),
        }
    }

    #[test]
    fn push_respects_capacity() {
        let mut q = Queue::default();
        assert!(q.push(item(1), 2));
        assert!(q.push(item(2), 2));
        assert!(!q.push(item(3), 2));
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn remove_pop_and_move() {
        let mut q = Queue::default();
        for n in 1..=4 {
            q.push(item(n), 10);
        }
        assert_eq!(q.remove(item(2).id).unwrap().title, "Video 2");
        assert!(q.remove(item(2).id).is_none());
        assert!(q.move_to(item(4).id, 0));
        assert_eq!(
            q.items().iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
            ["Video 4", "Video 1", "Video 3"]
        );
        assert!(q.move_to(item(4).id, 99), "index is clamped");
        assert_eq!(q.items().last().unwrap().title, "Video 4");
        assert_eq!(q.pop_next().unwrap().title, "Video 1");
        assert!(!q.move_to(Uuid::new_v4(), 0));
    }
}
