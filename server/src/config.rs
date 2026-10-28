//! Runtime configuration, read once from environment variables at startup.
//!
//! Every value has a safe local-development default except `DATABASE_URL`.
//! Secrets (TURN shared secret) are never logged; see [`Config::redacted_summary`].

use std::{env, net::SocketAddr, path::PathBuf, time::Duration};

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: SocketAddr,
    /// `None` runs the server with the in-memory store (tests, quick demos).
    pub database_url: Option<String>,
    /// Origins allowed for CORS and WebSocket `Origin` checks.
    pub allowed_origins: Vec<String>,
    /// Public base URL used to build invitation links in API responses.
    pub public_base_url: String,
    /// Directory with the built frontend; served with SPA fallback when set.
    pub static_dir: Option<PathBuf>,
    /// Trust `X-Forwarded-For` for client IPs (only behind a trusted proxy).
    pub trust_proxy: bool,
    /// With `trust_proxy`, read the client IP from this header instead of
    /// `X-Forwarded-For` (e.g. `CF-Connecting-IP` behind Cloudflare).
    pub client_ip_header: Option<String>,
    pub log_json: bool,

    pub limits: Limits,
    pub ice: IceConfig,
    pub youtube_oembed: bool,
    /// YouTube Data API v3 key for in-room search. Search is hidden when unset.
    pub youtube_api_key: Option<String>,
    /// Uncached searches allowed per day across the whole server (100 quota units each).
    pub youtube_search_daily_budget: u32,
    /// LiveKit SFU for calls; `None` keeps the peer-to-peer mesh.
    pub livekit: Option<LiveKitConfig>,
    /// Web Push key (base64url P-256 scalar). Generated and kept in the
    /// database when unset.
    pub vapid_private_key: Option<String>,
    /// Contact push services may use (`mailto:` or `https:` URL).
    pub vapid_subject: String,
}

#[derive(Clone, Debug)]
pub struct LiveKitConfig {
    /// Address browsers connect to (`wss://…`).
    pub url: String,
    /// Address this server calls for admin requests (defaults to `url` over https).
    pub api_url: String,
    pub api_key: String,
    pub api_secret: String,
}

#[derive(Clone, Debug)]
pub struct Limits {
    pub max_participants: u32,
    pub max_voice_participants: u32,
    pub max_queue_len: usize,
    pub chat_history_len: usize,
    pub chat_max_chars: usize,
    /// How long a participant may be disconnected before being removed.
    pub reconnect_grace: Duration,
    /// How long an empty room stays in memory before being unloaded.
    pub idle_room_ttl: Duration,
    /// Rooms (and their chat) untouched for this long are deleted from the database.
    pub room_retention: Duration,
    pub session_ttl: Duration,
}

#[derive(Clone, Debug)]
pub struct IceConfig {
    pub stun_urls: Vec<String>,
    pub turn_urls: Vec<String>,
    /// coturn `static-auth-secret`; used to mint short-lived credentials.
    pub turn_shared_secret: Option<String>,
    pub turn_credential_ttl: Duration,
    /// Fixed TURN credentials (e.g. a free Metered.ca account). Visible to
    /// everyone in a room, so prefer short-lived ones when available.
    pub turn_username: Option<String>,
    pub turn_credential: Option<String>,
    /// Cloudflare Realtime TURN: credentials are generated on demand.
    pub cloudflare_turn: Option<CloudflareTurn>,
}

#[derive(Clone, Debug)]
pub struct CloudflareTurn {
    pub key_id: String,
    pub api_token: String,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_participants: 16,
            max_voice_participants: 8,
            max_queue_len: 100,
            chat_history_len: 100,
            chat_max_chars: 500,
            // Phones drop sockets when switching apps or networks; keep seats long enough.
            reconnect_grace: Duration::from_secs(45),
            idle_room_ttl: Duration::from_secs(10 * 60),
            room_retention: Duration::from_secs(30 * 24 * 3600),
            session_ttl: Duration::from_secs(24 * 3600),
        }
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let port: u16 = parse_or("PORT", 8080)?;
        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
        let bind_addr: SocketAddr = format!("{host}:{port}").parse()?;

        // Browsers send Origin as scheme://host[:port] in lowercase with no
        // trailing slash; normalise the configured values the same way.
        let allowed_origins: Vec<String> = list("ALLOWED_ORIGINS")
            .unwrap_or_else(|| vec!["http://localhost:5173".into(), "http://localhost:8080".into()])
            .into_iter()
            .map(|o| o.trim_end_matches('/').to_ascii_lowercase())
            .collect();
        let public_base_url = env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| allowed_origins.first().cloned().unwrap_or_default());

        let defaults = Limits::default();
        let limits = Limits {
            max_participants: parse_or("ROOM_MAX_PARTICIPANTS", defaults.max_participants)?,
            max_voice_participants: parse_or("ROOM_MAX_VOICE", defaults.max_voice_participants)?,
            max_queue_len: parse_or("ROOM_MAX_QUEUE", defaults.max_queue_len)?,
            chat_history_len: defaults.chat_history_len,
            chat_max_chars: defaults.chat_max_chars,
            reconnect_grace: Duration::from_secs(parse_or("RECONNECT_GRACE_SECS", 45)?),
            idle_room_ttl: Duration::from_secs(parse_or("IDLE_ROOM_TTL_SECS", 600)?),
            room_retention: Duration::from_secs(parse_or::<u64>("ROOM_RETENTION_DAYS", 30)? * 24 * 3600),
            session_ttl: defaults.session_ttl,
        };

        let ice = IceConfig {
            stun_urls: list("STUN_URLS").unwrap_or_else(|| vec!["stun:stun.l.google.com:19302".into()]),
            turn_urls: list("TURN_URLS").unwrap_or_default(),
            turn_shared_secret: env::var("TURN_SHARED_SECRET").ok().filter(|s| !s.is_empty()),
            turn_credential_ttl: Duration::from_secs(parse_or("TURN_CREDENTIAL_TTL_SECS", 6 * 3600)?),
            turn_username: non_empty("TURN_USERNAME"),
            turn_credential: non_empty("TURN_CREDENTIAL"),
            cloudflare_turn: match (non_empty("CLOUDFLARE_TURN_KEY_ID"), non_empty("CLOUDFLARE_TURN_API_TOKEN")) {
                (Some(key_id), Some(api_token)) => Some(CloudflareTurn { key_id, api_token }),
                _ => None,
            },
        };

        let livekit = match (
            non_empty("LIVEKIT_URL"),
            non_empty("LIVEKIT_API_KEY"),
            non_empty("LIVEKIT_API_SECRET"),
        ) {
            (Some(url), Some(api_key), Some(api_secret)) => {
                let url = url.trim_end_matches('/').to_string();
                let api_url =
                    non_empty("LIVEKIT_API_URL").unwrap_or_else(|| url.replacen("wss://", "https://", 1).replacen("ws://", "http://", 1));
                Some(LiveKitConfig {
                    url,
                    api_url: api_url.trim_end_matches('/').to_string(),
                    api_key,
                    api_secret,
                })
            }
            _ => None,
        };
        // An SFU carries many more people than a peer-to-peer mesh.
        let voice_default = if livekit.is_some() {
            limits.max_participants
        } else {
            defaults.max_voice_participants
        };
        let limits = Limits {
            max_voice_participants: parse_or("ROOM_MAX_VOICE", voice_default)?,
            ..limits
        };

        Ok(Self {
            bind_addr,
            database_url: env::var("DATABASE_URL").ok().filter(|s| !s.is_empty()),
            allowed_origins,
            public_base_url: public_base_url.trim_end_matches('/').to_string(),
            static_dir: env::var("STATIC_DIR").ok().filter(|s| !s.is_empty()).map(PathBuf::from),
            trust_proxy: parse_or("TRUST_PROXY", false)?,
            client_ip_header: env::var("CLIENT_IP_HEADER").ok().filter(|s| !s.is_empty()),
            log_json: env::var("LOG_FORMAT").map(|v| v == "json").unwrap_or(false),
            limits,
            ice,
            youtube_oembed: parse_or("YOUTUBE_OEMBED", true)?,
            youtube_api_key: env::var("YOUTUBE_API_KEY").ok().filter(|s| !s.is_empty()),
            youtube_search_daily_budget: parse_or("YOUTUBE_SEARCH_DAILY_BUDGET", 90)?,
            livekit,
            vapid_private_key: non_empty("VAPID_PRIVATE_KEY"),
            vapid_subject: non_empty("VAPID_SUBJECT").unwrap_or_else(|| default_vapid_subject(&public_base_url)),
        })
    }

    /// Config suitable for tests: in-memory store, no outbound HTTP, short timers.
    pub fn for_tests() -> Self {
        Self {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            database_url: None,
            allowed_origins: vec!["http://localhost:5173".into()],
            public_base_url: "http://localhost:5173".into(),
            static_dir: None,
            trust_proxy: false,
            client_ip_header: None,
            log_json: false,
            limits: Limits {
                reconnect_grace: Duration::from_millis(300),
                ..Limits::default()
            },
            ice: IceConfig {
                stun_urls: vec!["stun:stun.example.org:3478".into()],
                turn_urls: vec![],
                turn_shared_secret: None,
                turn_credential_ttl: Duration::from_secs(3600),
                turn_username: None,
                turn_credential: None,
                cloudflare_turn: None,
            },
            youtube_oembed: false,
            youtube_api_key: None,
            youtube_search_daily_budget: 90,
            livekit: None,
            vapid_private_key: None,
            vapid_subject: "mailto:push@example.org".into(),
        }
    }

    /// One-line description for startup logs. Never includes secrets.
    pub fn redacted_summary(&self) -> String {
        format!(
            "bind={} store={} origins={:?} static_dir={:?} turn={} oembed={} youtube_search={} calls={}",
            self.bind_addr,
            if self.database_url.is_some() { "postgres" } else { "memory" },
            self.allowed_origins,
            self.static_dir,
            self.ice.mode(),
            self.youtube_oembed,
            if self.youtube_api_key.is_some() { "enabled" } else { "disabled" },
            if self.livekit.is_some() { "sfu" } else { "mesh" },
        )
    }
}

impl IceConfig {
    /// Which TURN source is active, for logs (never the secrets themselves).
    pub fn mode(&self) -> &'static str {
        if self.cloudflare_turn.is_some() {
            "cloudflare"
        } else if !self.turn_urls.is_empty() && self.turn_shared_secret.is_some() {
            "coturn"
        } else if !self.turn_urls.is_empty() && self.turn_username.is_some() && self.turn_credential.is_some() {
            "static"
        } else {
            "none"
        }
    }
}

/// Push services want a way to reach the operator; the public site works.
fn default_vapid_subject(public_base_url: &str) -> String {
    if public_base_url.starts_with("https://") {
        public_base_url.trim_end_matches('/').to_string()
    } else {
        "mailto:push@example.org".into()
    }
}

fn non_empty(key: &str) -> Option<String> {
    env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn list(key: &str) -> Option<Vec<String>> {
    env::var(key)
        .ok()
        .map(|v| v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect())
}

fn parse_or<T: std::str::FromStr>(key: &str, default: T) -> anyhow::Result<T>
where
    T::Err: std::fmt::Display,
{
    match env::var(key) {
        Ok(v) if !v.trim().is_empty() => v.trim().parse().map_err(|e| anyhow::anyhow!("invalid value for {key}: {e}")),
        _ => Ok(default),
    }
}
