//! YouTube URL parsing and metadata lookup.
//!
//! Metadata comes from YouTube's public oEmbed endpoint, which needs no API key
//! and doubles as an embeddability check: videos whose owners disabled embedding
//! answer 401, private or removed videos answer 404. Video bytes are never
//! fetched or proxied; playback happens in the official IFrame player.

use std::{collections::HashMap, sync::Mutex, time::Duration};

use serde::Deserialize;
use url::Url;

use crate::error::AppError;

const MAX_URL_LEN: usize = 2048;
const CACHE_CAP: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedVideo {
    pub video_id: String,
    /// Start offset from `t=` / `start=`, in seconds.
    pub start: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct VideoMeta {
    pub title: String,
    pub author: Option<String>,
    pub thumbnail_url: String,
}

pub fn is_valid_video_id(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Accepts the common YouTube URL shapes and returns the 11-character video id.
pub fn parse_video_url(input: &str) -> Result<ParsedVideo, AppError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(AppError::InvalidVideo("Paste a YouTube link".into()));
    }
    if input.len() > MAX_URL_LEN {
        return Err(AppError::InvalidVideo("That link is too long".into()));
    }
    let with_scheme = if input.starts_with("http://") || input.starts_with("https://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };
    let url = Url::parse(&with_scheme).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid());
    }
    let host = url.host_str().ok_or_else(invalid)?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let host = host.strip_prefix("m.").unwrap_or(host);

    let query = |key: &str| url.query_pairs().find(|(k, _)| k == key).map(|(_, v)| v.into_owned());
    let segments = url
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect::<Vec<_>>())
        .unwrap_or_default();

    let id = match host {
        "youtu.be" => segments.first().map(|s| s.to_string()),
        "youtube.com" | "music.youtube.com" | "youtube-nocookie.com" => match segments.as_slice() {
            ["watch"] => query("v"),
            ["embed" | "shorts" | "live" | "v", id, ..] => Some(id.to_string()),
            _ => None,
        },
        _ => return Err(AppError::InvalidVideo("Only YouTube links are supported for now".into())),
    };

    let video_id = id.filter(|id| is_valid_video_id(id)).ok_or_else(invalid)?;
    let start = query("t").or_else(|| query("start")).and_then(|t| parse_timestamp(&t));
    Ok(ParsedVideo { video_id, start })
}

fn invalid() -> AppError {
    AppError::InvalidVideo("That doesn't look like a YouTube video link".into())
}

/// Parses `90`, `90s`, `1m30s`, `1h2m3s`.
fn parse_timestamp(t: &str) -> Option<u32> {
    if let Ok(secs) = t.parse::<u32>() {
        return Some(secs);
    }
    let mut total: u32 = 0;
    let mut num: u32 = 0;
    let mut seen = false;
    for c in t.chars() {
        match c {
            '0'..='9' => {
                num = num.checked_mul(10)?.checked_add(c.to_digit(10)?)?;
                seen = true;
            }
            'h' | 'm' | 's' if seen => {
                let mult = match c {
                    'h' => 3600,
                    'm' => 60,
                    _ => 1,
                };
                total = total.checked_add(num.checked_mul(mult)?)?;
                num = 0;
                seen = false;
            }
            _ => return None,
        }
    }
    if seen { None } else { Some(total) }
}

pub fn thumbnail_for(video_id: &str) -> String {
    format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg")
}

#[derive(Deserialize)]
struct OEmbed {
    title: String,
    author_name: Option<String>,
    thumbnail_url: Option<String>,
}

pub struct YoutubeClient {
    http: Option<reqwest::Client>,
    cache: Mutex<HashMap<String, VideoMeta>>,
}

impl YoutubeClient {
    pub fn new(enabled: bool) -> Self {
        let http = enabled.then(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(4))
                .user_agent("Partello/0.1 (+watch party)")
                .build()
                .expect("reqwest client")
        });
        Self {
            http,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Looks up title/thumbnail. Errors only when YouTube positively says the
    /// video cannot be embedded or does not exist; network trouble falls back
    /// to a generic title so a flaky upstream never blocks the room.
    pub async fn metadata(&self, video_id: &str) -> Result<VideoMeta, AppError> {
        if let Some(hit) = self.cache.lock().unwrap().get(video_id).cloned() {
            return Ok(hit);
        }
        let fallback = VideoMeta {
            title: "YouTube video".into(),
            author: None,
            thumbnail_url: thumbnail_for(video_id),
        };
        let Some(http) = &self.http else {
            return Ok(fallback);
        };

        let watch = format!("https://www.youtube.com/watch?v={video_id}");
        let endpoint = Url::parse_with_params("https://www.youtube.com/oembed", [("url", watch.as_str()), ("format", "json")])
            .expect("static oembed url");
        let resp = http.get(endpoint).send().await;
        let meta = match resp {
            Ok(r) if r.status().is_success() => match r.text().await.ok().and_then(|t| serde_json::from_str::<OEmbed>(&t).ok()) {
                Some(o) => VideoMeta {
                    title: truncate(&o.title, 200),
                    author: o.author_name.map(|a| truncate(&a, 100)),
                    // Only trust thumbnails hosted on YouTube's image CDN.
                    thumbnail_url: o
                        .thumbnail_url
                        .filter(|u| u.starts_with("https://i.ytimg.com/"))
                        .unwrap_or_else(|| thumbnail_for(video_id)),
                },
                None => fallback,
            },
            Ok(r) if r.status() == reqwest::StatusCode::UNAUTHORIZED || r.status() == reqwest::StatusCode::FORBIDDEN => {
                return Err(AppError::InvalidVideo(
                    "The owner of this video doesn't allow playing it on other sites".into(),
                ));
            }
            Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND || r.status() == reqwest::StatusCode::BAD_REQUEST => {
                return Err(AppError::InvalidVideo("This video is private, removed, or doesn't exist".into()));
            }
            Ok(r) => {
                tracing::warn!(status = %r.status(), "oembed unexpected status");
                fallback
            }
            Err(err) => {
                tracing::warn!(error = %err.without_url(), "oembed request failed");
                return Ok(fallback);
            }
        };

        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_CAP {
            cache.clear();
        }
        cache.insert(video_id.to_string(), meta.clone());
        Ok(meta)
    }
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_shapes() {
        let id = "dQw4w9WgXcQ";
        for url in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?v=dQw4w9WgXcQ&list=PL123",
            "http://m.youtube.com/watch?v=dQw4w9WgXcQ",
            "youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://www.youtube.com/embed/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
            "https://www.youtube.com/live/dQw4w9WgXcQ?feature=share",
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ",
        ] {
            assert_eq!(parse_video_url(url).unwrap().video_id, id, "{url}");
        }
    }

    #[test]
    fn parses_start_time() {
        assert_eq!(parse_video_url("https://youtu.be/dQw4w9WgXcQ?t=90").unwrap().start, Some(90));
        assert_eq!(parse_video_url("https://youtu.be/dQw4w9WgXcQ?t=1m30s").unwrap().start, Some(90));
        assert_eq!(
            parse_video_url("https://youtube.com/watch?v=dQw4w9WgXcQ&t=1h0m5s").unwrap().start,
            Some(3605)
        );
        assert_eq!(parse_video_url("https://youtu.be/dQw4w9WgXcQ?t=abc").unwrap().start, None);
    }

    #[test]
    fn rejects_bad_input() {
        for url in [
            "",
            "not a url at all",
            "https://vimeo.com/123456",
            "https://youtube.com.evil.example/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/watch?v=short",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ<script>",
            "javascript:alert(1)",
            "https://www.youtube.com/channel/UC123",
            "ftp://youtube.com/watch?v=dQw4w9WgXcQ",
        ] {
            assert!(parse_video_url(url).is_err(), "{url} should be rejected");
        }
        assert!(parse_video_url(&format!("https://youtu.be/{}", "a".repeat(3000))).is_err());
    }
}
