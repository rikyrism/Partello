//! YouTube search through the official YouTube Data API v3.
//!
//! Runs only on the server so the API key never reaches browsers. Only
//! embeddable videos are requested (`videoEmbeddable=true`), so every result
//! can actually play in a Partello room. A search costs 100 quota units
//! (default quota: 10,000/day), so results are cached and callers are rate
//! limited per participant.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::{error::AppError, rate_limit::TokenBucket, youtube};

const CACHE_TTL: Duration = Duration::from_secs(10 * 60);
const CACHE_CAP: usize = 500;
const MAX_RESULTS: &str = "12";
pub const QUERY_MAX_CHARS: usize = 100;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchResult {
    pub video_id: String,
    pub title: String,
    pub channel: String,
    pub thumbnail_url: String,
    /// Seconds; `None` for live streams or when YouTube omits it.
    pub duration: Option<u32>,
    pub is_live: bool,
}

pub struct SearchClient {
    http: reqwest::Client,
    api_key: Option<String>,
    cache: Mutex<HashMap<String, (Instant, Vec<SearchResult>)>>,
    /// Server-wide budget of uncached searches per day, so one busy room
    /// cannot spend the whole YouTube quota for everyone.
    budget: Mutex<TokenBucket>,
}

impl SearchClient {
    pub fn new(api_key: Option<String>, daily_budget: u32) -> Self {
        let per_sec = f64::from(daily_budget.max(1)) / 86_400.0;
        Self {
            budget: Mutex::new(TokenBucket::new(daily_budget.max(1), per_sec)),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("reqwest client"),
            api_key,
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn enabled(&self) -> bool {
        self.api_key.is_some()
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>, AppError> {
        let Some(key) = &self.api_key else {
            return Err(AppError::Unavailable(
                "Search isn't set up on this server. Paste a YouTube link instead.".into(),
            ));
        };
        let query = normalize_query(query)?;
        if let Some((at, hit)) = self.cache.lock().unwrap().get(&query) {
            if at.elapsed() < CACHE_TTL {
                return Ok(hit.clone());
            }
        }

        if !self.budget.lock().unwrap().try_take() {
            return Err(AppError::Unavailable(
                "Search has reached today's YouTube limit. Paste a link instead.".into(),
            ));
        }
        let search_url = Url::parse_with_params(
            "https://www.googleapis.com/youtube/v3/search",
            [
                ("part", "snippet"),
                ("type", "video"),
                ("videoEmbeddable", "true"),
                ("safeSearch", "moderate"),
                ("maxResults", MAX_RESULTS),
                ("q", query.as_str()),
                ("key", key.as_str()),
            ],
        )
        .expect("static url");
        let body = self.get(search_url).await?;
        let mut results = parse_search(&body)?;

        // Durations need a second, cheap call (1 unit) for all ids at once.
        if !results.is_empty() {
            let ids = results.iter().map(|r| r.video_id.as_str()).collect::<Vec<_>>().join(",");
            let videos_url = Url::parse_with_params(
                "https://www.googleapis.com/youtube/v3/videos",
                [("part", "contentDetails"), ("id", ids.as_str()), ("key", key.as_str())],
            )
            .expect("static url");
            match self.get(videos_url).await.and_then(|b| parse_durations(&b)) {
                Ok(durations) => {
                    for r in &mut results {
                        r.duration = durations.get(&r.video_id).copied().flatten();
                    }
                }
                Err(err) => tracing::debug!(error = %err, "duration lookup failed"),
            }
        }

        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_CAP {
            cache.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
            if cache.len() >= CACHE_CAP {
                cache.clear();
            }
        }
        cache.insert(query, (Instant::now(), results.clone()));
        Ok(results)
    }

    async fn get(&self, url: Url) -> Result<String, AppError> {
        let resp = self.http.get(url).send().await.map_err(|err| {
            // `without_url`: the request URL carries the API key.
            tracing::warn!(error = %err.without_url(), "youtube search request failed");
            AppError::Unavailable("YouTube search didn't answer. Try again, or paste a link.".into())
        })?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if status.is_success() {
            return Ok(body);
        }
        // Never log the request URL: it contains the API key.
        let reason = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v["error"]["errors"][0]["reason"].as_str().map(String::from))
            .unwrap_or_default();
        tracing::warn!(%status, %reason, "youtube search error");
        Err(AppError::Unavailable(if reason.contains("quota") {
            "Search has reached today's YouTube limit. Paste a link instead.".into()
        } else {
            "YouTube search is unavailable right now. Paste a link instead.".into()
        }))
    }
}

/// Trims and collapses whitespace; lowercased so cache keys are shared.
pub fn normalize_query(q: &str) -> Result<String, AppError> {
    let q = q.split_whitespace().collect::<Vec<_>>().join(" ");
    let len = q.chars().count();
    if len < 2 {
        return Err(AppError::validation("Type at least 2 characters to search"));
    }
    if len > QUERY_MAX_CHARS || q.chars().any(char::is_control) {
        return Err(AppError::validation("That search is too long"));
    }
    Ok(q.to_lowercase())
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    items: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    id: SearchId,
    snippet: Snippet,
}

#[derive(Deserialize)]
struct SearchId {
    #[serde(rename = "videoId")]
    video_id: Option<String>,
}

#[derive(Deserialize)]
struct Snippet {
    title: String,
    #[serde(rename = "channelTitle", default)]
    channel_title: String,
    #[serde(rename = "liveBroadcastContent", default)]
    live: String,
}

fn parse_search(body: &str) -> Result<Vec<SearchResult>, AppError> {
    let parsed: SearchResponse =
        serde_json::from_str(body).map_err(|_| AppError::Unavailable("YouTube search returned something unexpected".into()))?;
    Ok(parsed
        .items
        .into_iter()
        .filter_map(|item| {
            // Live streams have no fixed timeline to sync, so they are not offered.
            if item.snippet.live == "live" || item.snippet.live == "upcoming" {
                return None;
            }
            let id = item.id.video_id.filter(|id| youtube::is_valid_video_id(id))?;
            Some(SearchResult {
                // Thumbnails are built from the id so only YouTube's CDN is ever referenced.
                thumbnail_url: format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"),
                title: decode_entities(&item.snippet.title),
                channel: decode_entities(&item.snippet.channel_title),
                is_live: item.snippet.live == "live",
                duration: None,
                video_id: id,
            })
        })
        .collect())
}

#[derive(Deserialize)]
struct VideosResponse {
    #[serde(default)]
    items: Vec<VideoItem>,
}

#[derive(Deserialize)]
struct VideoItem {
    id: String,
    #[serde(rename = "contentDetails")]
    content_details: Option<ContentDetails>,
}

#[derive(Deserialize)]
struct ContentDetails {
    duration: Option<String>,
}

fn parse_durations(body: &str) -> Result<HashMap<String, Option<u32>>, AppError> {
    let parsed: VideosResponse = serde_json::from_str(body).map_err(|_| AppError::Unavailable("bad videos response".into()))?;
    Ok(parsed
        .items
        .into_iter()
        .map(|v| {
            (
                v.id,
                v.content_details.and_then(|c| c.duration).and_then(|d| parse_iso_duration(&d)),
            )
        })
        .collect())
}

/// `PT1H2M3S` → 3723. Live streams report `P0D` → `None`.
pub fn parse_iso_duration(s: &str) -> Option<u32> {
    let rest = s.strip_prefix('P')?;
    let (days, time) = match rest.split_once('T') {
        Some((d, t)) => (d, t),
        None => (rest, ""),
    };
    let mut total: u32 = 0;
    let mut num: u32 = 0;
    for c in days.chars() {
        match c {
            '0'..='9' => num = num.checked_mul(10)?.checked_add(c.to_digit(10)?)?,
            'D' => {
                total = total.checked_add(num.checked_mul(86_400)?)?;
                num = 0;
            }
            _ => return None,
        }
    }
    for c in time.chars() {
        match c {
            '0'..='9' => num = num.checked_mul(10)?.checked_add(c.to_digit(10)?)?,
            'H' | 'M' | 'S' => {
                let mult = match c {
                    'H' => 3600,
                    'M' => 60,
                    _ => 1,
                };
                total = total.checked_add(num.checked_mul(mult)?)?;
                num = 0;
            }
            _ => return None,
        }
    }
    (total > 0).then_some(total)
}

/// The Data API HTML-escapes snippet text.
fn decode_entities(s: &str) -> String {
    s.replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_and_skips_non_videos() {
        let body = r#"{"items":[
            {"id":{"kind":"youtube#video","videoId":"aqz-KE-bpKQ"},"snippet":{"title":"Big Buck Bunny &amp; friends","channelTitle":"Blender","liveBroadcastContent":"none"}},
            {"id":{"kind":"youtube#channel","channelId":"UC1"},"snippet":{"title":"A channel","channelTitle":"x"}},
            {"id":{"videoId":"bad id!"},"snippet":{"title":"x","channelTitle":"x"}},
            {"id":{"videoId":"dQw4w9WgXcQ"},"snippet":{"title":"Live &#39;now&#39;","channelTitle":"Y","liveBroadcastContent":"live"}}
        ]}"#;
        let r = parse_search(body).unwrap();
        assert_eq!(r.len(), 1, "channels, bad ids and live streams are dropped");
        assert_eq!(r[0].title, "Big Buck Bunny & friends");
        assert_eq!(r[0].thumbnail_url, "https://i.ytimg.com/vi/aqz-KE-bpKQ/mqdefault.jpg");
        assert!(parse_search("not json").is_err());
    }

    #[test]
    fn parses_iso_durations() {
        assert_eq!(parse_iso_duration("PT4M13S"), Some(253));
        assert_eq!(parse_iso_duration("PT1H2M3S"), Some(3723));
        assert_eq!(parse_iso_duration("P1DT1S"), Some(86_401));
        assert_eq!(parse_iso_duration("P0D"), None);
        assert_eq!(parse_iso_duration("garbage"), None);
        let d = parse_durations(r#"{"items":[{"id":"aqz-KE-bpKQ","contentDetails":{"duration":"PT10M34S"}}]}"#).unwrap();
        assert_eq!(d["aqz-KE-bpKQ"], Some(634));
    }

    #[test]
    fn normalizes_queries() {
        assert_eq!(normalize_query("  Big   Buck BUNNY ").unwrap(), "big buck bunny");
        assert!(normalize_query("a").is_err());
        assert!(normalize_query(&"x".repeat(101)).is_err());
    }

    #[tokio::test]
    async fn disabled_without_key() {
        let c = SearchClient::new(None, 90);
        assert!(!c.enabled());
        assert!(matches!(c.search("cats").await, Err(AppError::Unavailable(_))));
    }
}
