//! ICE servers handed to browsers for WebRTC calls.
//!
//! STUN alone connects most home networks. Phones on mobile data (CGNAT) and
//! strict office networks need a TURN relay, from one of three sources:
//!
//! * **coturn** with `use-auth-secret`: short-lived credentials minted here.
//! * **Cloudflare Realtime TURN**: short-lived credentials fetched from
//!   Cloudflare's API and cached; no server of your own needed.
//! * **Static credentials** (e.g. a free Metered.ca account).

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Deserialize;
use uuid::Uuid;

use crate::{
    config::{CloudflareTurn, IceConfig},
    database::now_ms,
    realtime::protocol::IceServer,
    security,
};

/// Lifetime requested for Cloudflare credentials; refreshed at half of it.
const CLOUDFLARE_TTL: Duration = Duration::from_secs(6 * 3600);

pub struct IceProvider {
    config: IceConfig,
    http: reqwest::Client,
    cloudflare: Mutex<Option<(Instant, Vec<IceServer>)>>,
}

impl IceProvider {
    pub fn new(config: IceConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(4))
                .build()
                .expect("reqwest client"),
            cloudflare: Mutex::new(None),
        }
    }

    pub async fn servers(&self, participant_id: Uuid) -> Vec<IceServer> {
        let ice = &self.config;
        let mut out = Vec::new();
        if !ice.stun_urls.is_empty() {
            out.push(IceServer {
                urls: ice.stun_urls.clone(),
                username: None,
                credential: None,
            });
        }
        if let Some(cf) = &ice.cloudflare_turn {
            out.extend(self.cloudflare(cf).await);
        } else if let (false, Some(secret)) = (ice.turn_urls.is_empty(), &ice.turn_shared_secret) {
            let expires = now_ms() / 1000 + ice.turn_credential_ttl.as_secs() as i64;
            let tag = participant_id.simple().to_string();
            let (username, credential) = security::turn_credentials(secret, &tag[..12], expires);
            out.push(IceServer {
                urls: ice.turn_urls.clone(),
                username: Some(username),
                credential: Some(credential),
            });
        } else if let (false, Some(username), Some(credential)) = (ice.turn_urls.is_empty(), &ice.turn_username, &ice.turn_credential) {
            out.push(IceServer {
                urls: ice.turn_urls.clone(),
                username: Some(username.clone()),
                credential: Some(credential.clone()),
            });
        }
        out
    }

    /// Cached Cloudflare credentials. On failure the call still works for
    /// everyone STUN can connect, so errors are logged, not returned.
    async fn cloudflare(&self, cf: &CloudflareTurn) -> Vec<IceServer> {
        if let Some((at, servers)) = &*self.cloudflare.lock().unwrap() {
            if at.elapsed() < CLOUDFLARE_TTL / 2 {
                return servers.clone();
            }
        }
        let url = format!(
            "https://rtc.live.cloudflare.com/v1/turn/keys/{}/credentials/generate-ice-servers",
            cf.key_id
        );
        let body = format!(r#"{{"ttl":{}}}"#, CLOUDFLARE_TTL.as_secs());
        let resp = self
            .http
            .post(url)
            .bearer_auth(&cf.api_token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await;
        let text = match resp {
            Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
            Ok(r) => {
                tracing::warn!(status = %r.status(), "cloudflare turn credentials refused");
                return Vec::new();
            }
            Err(err) => {
                tracing::warn!(error = %err.without_url(), "cloudflare turn request failed");
                return Vec::new();
            }
        };
        match parse_cloudflare(&text) {
            Some(servers) => {
                *self.cloudflare.lock().unwrap() = Some((Instant::now(), servers.clone()));
                servers
            }
            None => {
                tracing::warn!("cloudflare turn answered something unexpected");
                Vec::new()
            }
        }
    }
}

#[derive(Deserialize)]
struct CloudflareResponse {
    #[serde(rename = "iceServers")]
    ice_servers: Vec<CloudflareServer>,
}

#[derive(Deserialize)]
struct CloudflareServer {
    urls: Vec<String>,
    username: Option<String>,
    credential: Option<String>,
}

/// Keeps only the TURN entries (STUN is configured separately) and drops
/// port 53, which browsers often cannot reach and which slows ICE down.
fn parse_cloudflare(body: &str) -> Option<Vec<IceServer>> {
    let parsed: CloudflareResponse = serde_json::from_str(body).ok()?;
    let servers: Vec<IceServer> = parsed
        .ice_servers
        .into_iter()
        .filter(|s| s.username.is_some() && s.credential.is_some())
        .map(|s| IceServer {
            urls: s.urls.into_iter().filter(|u| !u.contains(":53?") && !u.ends_with(":53")).collect(),
            username: s.username,
            credential: s.credential,
        })
        .filter(|s| !s.urls.is_empty())
        .collect();
    (!servers.is_empty()).then_some(servers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> IceConfig {
        IceConfig {
            stun_urls: vec!["stun:stun.example.org:3478".into()],
            turn_urls: vec![],
            turn_shared_secret: None,
            turn_credential_ttl: Duration::from_secs(3600),
            turn_username: None,
            turn_credential: None,
            cloudflare_turn: None,
        }
    }

    #[test]
    fn parses_cloudflare_servers() {
        let body = r#"{"iceServers":[
            {"urls":["stun:stun.cloudflare.com:3478"]},
            {"urls":["turn:turn.cloudflare.com:3478?transport=udp","turn:turn.cloudflare.com:53?transport=udp","turns:turn.cloudflare.com:443?transport=tcp"],
             "username":"u","credential":"c"}
        ]}"#;
        let servers = parse_cloudflare(body).unwrap();
        assert_eq!(servers.len(), 1, "STUN entries are dropped");
        assert_eq!(servers[0].urls.len(), 2, "port 53 is dropped");
        assert_eq!(servers[0].username.as_deref(), Some("u"));
        assert!(parse_cloudflare("nope").is_none());
        assert!(parse_cloudflare(r#"{"iceServers":[]}"#).is_none());
    }

    #[tokio::test]
    async fn static_credentials_are_passed_through() {
        let p = IceProvider::new(IceConfig {
            turn_urls: vec!["turn:a.relay.metered.ca:80".into()],
            turn_username: Some("user".into()),
            turn_credential: Some("pass".into()),
            ..config()
        });
        let s = p.servers(Uuid::new_v4()).await;
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].credential.as_deref(), Some("pass"));
    }

    #[tokio::test]
    async fn coturn_secret_wins_over_static() {
        let p = IceProvider::new(IceConfig {
            turn_urls: vec!["turn:turn.example.org:3478".into()],
            turn_shared_secret: Some("secret".into()),
            turn_username: Some("user".into()),
            turn_credential: Some("pass".into()),
            ..config()
        });
        let s = p.servers(Uuid::new_v4()).await;
        assert_ne!(s[1].credential.as_deref(), Some("pass"));
        assert!(s[1].username.as_deref().unwrap().contains(':'));
    }
}
