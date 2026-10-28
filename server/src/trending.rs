//! What is trending on YouTube today, for the landing page's "Pick a genre".
//!
//! Fetched from the official Data API (`chart=mostPopular`) once a day per
//! server at a fixed hour, never per visitor: about 8
//! quota units a day. The result is kept in `server_settings`, so a restart
//! serves the stored list and waits for the next scheduled hour instead of
//! asking YouTube again. A list that missed its hour (the server was down) is
//! refreshed right away.
//!
//! The default hour, 15:00 WIB, is just after the YouTube quota resets
//! (midnight Pacific: 14:00 WIB in US summer time, 15:00 WIB otherwise), so
//! the refresh still works on a day the quota ran out.

use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::{
    database::{Store, now_ms},
    youtube_search::{ChartVideo, SearchClient},
};

const SETTING_KEY: &str = "trending";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const HOUR_MS: i64 = 60 * 60 * 1000;
/// After a failed refresh: try again sooner, keep serving the old list.
const RETRY_MS: i64 = 60 * 60 * 1000;
const PER_GENRE: usize = 6;

/// Shown name (English; the web app translates it) and YouTube category id.
/// News is left out on purpose: it is not something to watch together.
pub const GENRES: &[(&str, Option<&str>)] = &[
    ("Trending", None),
    ("Music", Some("10")),
    ("Gaming", Some("20")),
    ("Film & Animation", Some("1")),
    ("Entertainment", Some("24")),
    ("Comedy", Some("23")),
    ("Sports", Some("17")),
    ("Science & Technology", Some("28")),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendingGenre {
    pub name: String,
    pub videos: Vec<ChartVideo>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Trending {
    pub region: String,
    /// Unix milliseconds; 0 when nothing was fetched yet.
    pub fetched_at: i64,
    pub genres: Vec<TrendingGenre>,
}

#[derive(Default)]
pub struct TrendingCache(RwLock<Arc<Trending>>);

impl TrendingCache {
    pub fn get(&self) -> Arc<Trending> {
        self.0.read().unwrap().clone()
    }

    fn set(&self, t: Trending) {
        *self.0.write().unwrap() = Arc::new(t);
    }
}

/// When the daily refresh happens: `hour` o'clock at UTC+`utc_offset_hours`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    pub hour: u8,
    pub utc_offset_hours: i8,
}

impl Schedule {
    /// The first scheduled moment after `fetched_at` (Unix ms). In the past
    /// when a refresh was missed, so it runs at once.
    pub fn next_after(self, fetched_at: i64) -> i64 {
        let offset = i64::from(self.utc_offset_hours) * HOUR_MS;
        let local = fetched_at + offset;
        let mut slot = local.div_euclid(DAY_MS) * DAY_MS + i64::from(self.hour) * HOUR_MS;
        if slot <= local {
            slot += DAY_MS;
        }
        slot - offset
    }
}

/// Loads the stored list, then refreshes it every day at the scheduled hour.
/// Does nothing without a YouTube API key.
pub fn spawn(store: Store, search: Arc<SearchClient>, cache: Arc<TrendingCache>, region: String, schedule: Schedule) {
    if !search.enabled() {
        return;
    }
    tokio::spawn(async move {
        match store.setting(SETTING_KEY).await {
            Ok(Some(raw)) => match serde_json::from_str::<Trending>(&raw) {
                Ok(t) if t.region == region => cache.set(t),
                Ok(_) => {}
                Err(err) => tracing::warn!(error = %err, "stored trending list unreadable"),
            },
            Ok(None) => {}
            Err(err) => tracing::warn!(error = %err, "could not load trending list"),
        }
        let mut next_at = schedule.next_after(cache.get().fetched_at);
        loop {
            let wait = (next_at - now_ms()).max(0);
            tokio::time::sleep(Duration::from_millis(wait as u64)).await;
            next_at = match refresh(&search, &region).await {
                Some(t) => {
                    if let Ok(raw) = serde_json::to_string(&t) {
                        if let Err(err) = store.put_setting(SETTING_KEY, &raw).await {
                            tracing::warn!(error = %err, "could not store trending list");
                        }
                    }
                    let at = t.fetched_at;
                    tracing::info!(genres = t.genres.len(), "trending list refreshed");
                    cache.set(t);
                    schedule.next_after(at)
                }
                None => now_ms() + RETRY_MS,
            };
        }
    });
}

/// One chart call per genre. A genre YouTube has no chart for is skipped;
/// `None` when nothing at all came back (keep the old list).
async fn refresh(search: &SearchClient, region: &str) -> Option<Trending> {
    let mut genres = Vec::new();
    let mut covers: Vec<String> = Vec::new();
    for (name, category) in GENRES {
        match search.chart(region, *category, PER_GENRE).await {
            Ok(mut videos) if !videos.is_empty() => {
                lead_with_unused(&mut videos, &covers);
                covers.push(videos[0].id.clone());
                genres.push(TrendingGenre {
                    name: (*name).to_string(),
                    videos,
                })
            }
            Ok(_) => {}
            Err(err) => tracing::debug!(genre = name, error = %err, "trending genre skipped"),
        }
    }
    (!genres.is_empty()).then(|| Trending {
        region: region.to_string(),
        fetched_at: now_ms(),
        genres,
    })
}

/// The first video is the genre card's picture: start with one no earlier
/// card uses, so the grid does not show the same picture twice.
fn lead_with_unused(videos: &mut [ChartVideo], covers: &[String]) {
    if let Some(i) = videos.iter().position(|v| !covers.contains(&v.id)) {
        videos[..=i].rotate_right(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(id: &str) -> ChartVideo {
        ChartVideo {
            id: id.into(),
            title: id.into(),
            channel: "C".into(),
            thumb: String::new(),
            duration: Some(120),
        }
    }

    #[test]
    fn refreshes_daily_at_the_scheduled_hour() {
        let wib7 = Schedule {
            hour: 7,
            utc_offset_hours: 7,
        };
        // 07:00 WIB is 00:00 UTC. 2026-10-28T15:00Z (22:00 WIB) → 2026-10-29T00:00Z.
        let evening = 1_793_199_600_000;
        assert_eq!(wib7.next_after(evening), 1_793_232_000_000);
        // Fetched exactly at 07:00 WIB: the next one is a day later, not now.
        assert_eq!(wib7.next_after(1_793_232_000_000), 1_793_232_000_000 + DAY_MS);
        // 06:59 WIB → a minute later.
        assert_eq!(wib7.next_after(1_793_232_000_000 - 60_000), 1_793_232_000_000);
        // Never fetched: due long ago, so it runs at once.
        assert!(wib7.next_after(0) < 1_000_000_000_000);
        // Negative offsets work too: 07:00 in New York (UTC−4) is 11:00 UTC.
        let ny7 = Schedule {
            hour: 7,
            utc_offset_hours: -4,
        };
        assert_eq!(ny7.next_after(evening), 1_793_199_600_000 - 4 * HOUR_MS + DAY_MS);
    }

    #[test]
    fn genre_cards_get_distinct_pictures() {
        let mut v = vec![video("a"), video("b"), video("c")];
        lead_with_unused(&mut v, &["a".into(), "b".into()]);
        assert_eq!(v.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["c", "a", "b"]);
        let mut all_used = vec![video("a")];
        lead_with_unused(&mut all_used, &["a".into()]);
        assert_eq!(all_used[0].id, "a", "kept when every video is already a cover");
    }
}
