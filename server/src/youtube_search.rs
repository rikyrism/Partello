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

/// One playable video of a playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistVideo {
    pub video_id: String,
    pub title: String,
    pub channel: String,
}

impl SearchClient {
    /// Up to `max` (≤ 50) embeddable, public, non-live videos of a playlist,
    /// in playlist order. Costs 2 quota units, so it is not budgeted like search.
    pub async fn playlist(&self, list_id: &str, max: usize) -> Result<Vec<PlaylistVideo>, AppError> {
        let Some(key) = &self.api_key else {
            return Err(AppError::Unavailable(
                "Playlist import isn't set up on this server. Add the videos one by one.".into(),
            ));
        };
        let items_url = Url::parse_with_params(
            "https://www.googleapis.com/youtube/v3/playlistItems",
            [
                ("part", "contentDetails"),
                ("maxResults", "50"),
                ("playlistId", list_id),
                ("key", key.as_str()),
            ],
        )
        .expect("static url");
        let body = self.get_playlist(items_url).await?;
        let ids = parse_playlist_ids(&body)?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let videos_url = Url::parse_with_params(
            "https://www.googleapis.com/youtube/v3/videos",
            [("part", "snippet,status"), ("id", ids.join(",").as_str()), ("key", key.as_str())],
        )
        .expect("static url");
        let body = self.get(videos_url).await?;
        let mut playable = parse_playable(&body)?;
        // `videos.list` does not keep the request order.
        let mut out = Vec::new();
        for id in &ids {
            if let Some(pos) = playable.iter().position(|v| &v.video_id == id) {
                out.push(playable.swap_remove(pos));
            }
            if out.len() >= max {
                break;
            }
        }
        Ok(out)
    }

    /// Like `get`, but a missing playlist is the user's mistake, not an outage.
    async fn get_playlist(&self, url: Url) -> Result<String, AppError> {
        let resp = self.http.get(url).send().await.map_err(|err| {
            tracing::warn!(error = %err.without_url(), "youtube playlist request failed");
            AppError::Unavailable("YouTube didn't answer. Try again in a moment.".into())
        })?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        match status.as_u16() {
            200 => Ok(body),
            403 | 404 => Err(AppError::InvalidVideo("That playlist is private or doesn't exist".into())),
            _ => {
                tracing::warn!(%status, "youtube playlist error");
                Err(AppError::Unavailable("YouTube didn't answer. Try again in a moment.".into()))
            }
        }
    }
}

#[derive(Deserialize)]
struct PlaylistItemsResponse {
    #[serde(default)]
    items: Vec<PlaylistItem>,
}

#[derive(Deserialize)]
struct PlaylistItem {
    #[serde(rename = "contentDetails")]
    content_details: PlaylistItemDetails,
}

#[derive(Deserialize)]
struct PlaylistItemDetails {
    #[serde(rename = "videoId")]
    video_id: String,
}

fn parse_playlist_ids(body: &str) -> Result<Vec<String>, AppError> {
    let parsed: PlaylistItemsResponse =
        serde_json::from_str(body).map_err(|_| AppError::Unavailable("YouTube returned something unexpected".into()))?;
    let mut ids: Vec<String> = Vec::new();
    for item in parsed.items {
        let id = item.content_details.video_id;
        if youtube::is_valid_video_id(&id) && !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

#[derive(Deserialize)]
struct StatusVideos {
    #[serde(default)]
    items: Vec<StatusVideo>,
}

#[derive(Deserialize)]
struct StatusVideo {
    id: String,
    snippet: Snippet,
    status: VideoStatus,
}

#[derive(Deserialize)]
struct VideoStatus {
    #[serde(default)]
    embeddable: bool,
    #[serde(rename = "privacyStatus", default)]
    privacy: String,
}

fn parse_playable(body: &str) -> Result<Vec<PlaylistVideo>, AppError> {
    let parsed: StatusVideos =
        serde_json::from_str(body).map_err(|_| AppError::Unavailable("YouTube returned something unexpected".into()))?;
    Ok(parsed
        .items
        .into_iter()
        .filter(|v| v.status.embeddable && v.status.privacy != "private")
        .filter(|v| v.snippet.live != "live" && v.snippet.live != "upcoming")
        .filter(|v| youtube::is_valid_video_id(&v.id))
        .map(|v| PlaylistVideo {
            title: decode_entities(&v.snippet.title),
            channel: decode_entities(&v.snippet.channel_title),
            video_id: v.id,
        })
        .collect())
}

/// One video of YouTube's "most popular" chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartVideo {
    pub id: String,
    pub title: String,
    pub channel: String,
    /// Largest thumbnail YouTube has for it; always on i.ytimg.com.
    pub thumb: String,
    pub duration: Option<u32>,
}

/// Shorter than this is a Short: not much to watch together.
const CHART_MIN_SECONDS: u32 = 60;
/// A title with this many hashtags is a clip made for the feed, not a video.
const CHART_MAX_HASHTAGS: usize = 2;

/// Shorts and feed clips (up to 3 minutes, so length alone misses them).
fn looks_like_a_short(title: &str) -> bool {
    let lower = title.to_lowercase();
    lower.contains("#short") || title.split_whitespace().filter(|w| w.starts_with('#') && w.len() > 1).count() > CHART_MAX_HASHTAGS
}

impl SearchClient {
    /// Up to `max` videos of YouTube's most-popular chart for `region`,
    /// optionally one video category. Only videos that can play in a room
    /// (embeddable, public, not live, not age-restricted) are kept.
    /// Costs 1 quota unit.
    pub async fn chart(&self, region: &str, category: Option<&str>, max: usize) -> Result<Vec<ChartVideo>, AppError> {
        let Some(key) = &self.api_key else {
            return Err(AppError::Unavailable("YouTube isn't set up on this server".into()));
        };
        let mut params = vec![
            ("part", "snippet,contentDetails,status"),
            ("chart", "mostPopular"),
            ("regionCode", region),
            ("maxResults", "50"),
            ("key", key.as_str()),
        ];
        if let Some(c) = category {
            params.push(("videoCategoryId", c));
        }
        let url = Url::parse_with_params("https://www.googleapis.com/youtube/v3/videos", params).expect("static url");
        let body = self.get(url).await?;
        let mut videos = parse_chart(&body)?;
        videos.truncate(max);
        Ok(videos)
    }
}

#[derive(Deserialize)]
struct ChartResponse {
    #[serde(default)]
    items: Vec<ChartItem>,
}

#[derive(Deserialize)]
struct ChartItem {
    id: String,
    snippet: ChartSnippet,
    #[serde(rename = "contentDetails")]
    content_details: Option<ChartDetails>,
    status: VideoStatus,
}

#[derive(Deserialize)]
struct ChartSnippet {
    title: String,
    #[serde(rename = "channelTitle", default)]
    channel_title: String,
    #[serde(rename = "liveBroadcastContent", default)]
    live: String,
    #[serde(default)]
    thumbnails: HashMap<String, Thumbnail>,
}

#[derive(Deserialize)]
struct Thumbnail {
    url: String,
}

#[derive(Deserialize)]
struct ChartDetails {
    duration: Option<String>,
    #[serde(rename = "contentRating", default)]
    content_rating: HashMap<String, serde_json::Value>,
}

fn parse_chart(body: &str) -> Result<Vec<ChartVideo>, AppError> {
    let parsed: ChartResponse =
        serde_json::from_str(body).map_err(|_| AppError::Unavailable("YouTube returned something unexpected".into()))?;
    Ok(parsed
        .items
        .into_iter()
        .filter(|v| v.status.embeddable && v.status.privacy == "public")
        .filter(|v| v.snippet.live != "live" && v.snippet.live != "upcoming")
        .filter(|v| youtube::is_valid_video_id(&v.id))
        .filter(|v| !looks_like_a_short(&decode_entities(&v.snippet.title)))
        .filter_map(|v| {
            let details = v.content_details?;
            if details.content_rating.get("ytRating").and_then(|r| r.as_str()) == Some("ytAgeRestricted") {
                return None;
            }
            let duration = details.duration.as_deref().and_then(parse_iso_duration);
            if duration.is_some_and(|d| d < CHART_MIN_SECONDS) {
                return None;
            }
            // Only YouTube's own image host, whatever the response says.
            let thumb = ["maxres", "standard", "high"]
                .iter()
                .filter_map(|k| v.snippet.thumbnails.get(*k))
                .map(|t| t.url.clone())
                .find(|u| u.starts_with("https://i.ytimg.com/"))
                .unwrap_or_else(|| format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", v.id));
            Some(ChartVideo {
                title: decode_entities(&v.snippet.title),
                channel: decode_entities(&v.snippet.channel_title),
                thumb,
                duration,
                id: v.id,
            })
        })
        .collect())
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
    fn chart_keeps_only_playable_videos() {
        let item = |id: &str, extra: &str| {
            format!(
                r#"{{"id":"{id}","snippet":{{"title":"T &amp; U","channelTitle":"C","liveBroadcastContent":"none","thumbnails":{{"high":{{"url":"https://i.ytimg.com/vi/{id}/hqdefault.jpg"}},"maxres":{{"url":"https://i.ytimg.com/vi/{id}/maxresdefault.jpg"}}}}}},"contentDetails":{{"duration":"PT3M"{extra}}},"status":{{"embeddable":true,"privacyStatus":"public"}}}}"#
            )
        };
        let body = format!(
            r#"{{"items":[{},{},{},{}]}}"#,
            item("aqz-KE-bpKQ", ""),
            item("dQw4w9WgXcQ", r#","contentRating":{"ytRating":"ytAgeRestricted"}"#),
            item("bad id!", ""),
            item("XdPt8QWTypI", "").replace("PT3M", "PT45S"),
        );
        let v = parse_chart(&body).unwrap();
        assert_eq!(v.len(), 1, "age-restricted, bad ids and Shorts are dropped");
        assert_eq!(v[0].title, "T & U");
        assert_eq!(v[0].thumb, "https://i.ytimg.com/vi/aqz-KE-bpKQ/maxresdefault.jpg");
        assert_eq!(v[0].duration, Some(180));

        let short = item("aqz-KE-bpKQ", "").replace("T &amp; U", "lol #standup #comedy #funny");
        assert!(
            parse_chart(&format!(r#"{{"items":[{short}]}}"#)).unwrap().is_empty(),
            "hashtag clips are dropped"
        );
        assert!(looks_like_a_short("Wait for it #Shorts"));
        assert!(!looks_like_a_short("Song (Official Video) #1 #music"));

        let not_embeddable = item("aqz-KE-bpKQ", "").replace(r#""embeddable":true"#, r#""embeddable":false"#);
        assert!(parse_chart(&format!(r#"{{"items":[{not_embeddable}]}}"#)).unwrap().is_empty());
        let foreign_thumb =
            item("aqz-KE-bpKQ", "").replace("https://i.ytimg.com/vi/aqz-KE-bpKQ/maxresdefault.jpg", "https://evil.example/x.jpg");
        let v = parse_chart(&format!(r#"{{"items":[{foreign_thumb}]}}"#)).unwrap();
        assert_eq!(
            v[0].thumb, "https://i.ytimg.com/vi/aqz-KE-bpKQ/hqdefault.jpg",
            "falls back past foreign hosts"
        );
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

    #[test]
    fn parses_playlists_in_order_and_drops_unplayable() {
        let ids = parse_playlist_ids(
            r#"{"items":[{"contentDetails":{"videoId":"aqz-KE-bpKQ"}},{"contentDetails":{"videoId":"dQw4w9WgXcQ"}},{"contentDetails":{"videoId":"aqz-KE-bpKQ"}},{"contentDetails":{"videoId":"bad"}}]}"#,
        )
        .unwrap();
        assert_eq!(ids, ["aqz-KE-bpKQ", "dQw4w9WgXcQ"], "duplicates and bad ids are dropped");
        let playable = parse_playable(
            r#"{"items":[
                {"id":"dQw4w9WgXcQ","snippet":{"title":"B &amp; C","channelTitle":"Ch"},"status":{"embeddable":true,"privacyStatus":"public"}},
                {"id":"aqz-KE-bpKQ","snippet":{"title":"No embed","channelTitle":"Ch"},"status":{"embeddable":false,"privacyStatus":"public"}},
                {"id":"jNQXAC9IVRw","snippet":{"title":"Live","channelTitle":"Ch","liveBroadcastContent":"live"},"status":{"embeddable":true,"privacyStatus":"public"}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(playable.len(), 1);
        assert_eq!(playable[0].title, "B & C");
    }

    #[tokio::test]
    async fn playlist_disabled_without_key() {
        let c = SearchClient::new(None, 90);
        assert!(matches!(c.playlist("PLabc", 50).await, Err(AppError::Unavailable(_))));
    }

    #[tokio::test]
    async fn disabled_without_key() {
        let c = SearchClient::new(None, 90);
        assert!(!c.enabled());
        assert!(matches!(c.search("cats").await, Err(AppError::Unavailable(_))));
    }
}
