//! Web Push: "Riky started watching in Friday Movie Night" on the phones of
//! people who asked to be told, even when Partello is closed.
//!
//! Implemented directly on RustCrypto (no OpenSSL): payloads are encrypted
//! with the `aes128gcm` content coding of RFC 8188/8291, and requests are
//! signed with a VAPID key (RFC 8292). The VAPID key pair is generated once
//! and kept in the database, because every browser subscription is tied to it.
//!
//! The server only ever sends to the push services of the major browsers, so
//! a subscription can never be used to make it call an arbitrary address.

use std::time::Duration;

use aes_gcm::{Aes128Gcm, KeyInit, aead::Aead};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use p256::{
    PublicKey, SecretKey,
    ecdsa::{Signature, SigningKey, signature::Signer},
};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use url::Url;

use crate::{database::now_ms, error::AppError};

/// Push services of the browsers people actually use.
const PUSH_HOSTS: [&str; 6] = [
    "fcm.googleapis.com",
    "android.googleapis.com",
    "push.services.mozilla.com",
    "push.apple.com",
    "notify.windows.com",
    "push.api.chrome.google.com",
];

const RECORD_SIZE: u32 = 4096;
/// How long a push service keeps a notification for an offline device.
const TTL_SECS: u32 = 6 * 3600;

/// A browser's push subscription (the JSON from `PushSubscription.toJSON()`).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Subscription {
    pub endpoint: String,
    pub keys: SubscriptionKeys,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SubscriptionKeys {
    /// The browser's P-256 public key, base64url (65 bytes uncompressed).
    pub p256dh: String,
    /// 16-byte authentication secret, base64url.
    pub auth: String,
}

/// What the service worker shows.
#[derive(Debug, Clone, Serialize)]
pub struct Notice {
    pub title: String,
    pub body: String,
    /// Path to open on click, e.g. `/r/k7Qm2xPa9Rt4`.
    pub url: String,
    /// One notification per room: a newer one replaces the older.
    pub tag: String,
}

impl Subscription {
    /// Checks everything the server will later rely on, before storing it.
    pub fn validate(&self) -> Result<(), AppError> {
        let bad = || AppError::validation("That push subscription isn't valid");
        if self.endpoint.len() > 1024 {
            return Err(bad());
        }
        let url = Url::parse(&self.endpoint).map_err(|_| bad())?;
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        let known = PUSH_HOSTS.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")));
        if url.scheme() != "https" || !known || url.port().is_some_and(|p| p != 443) {
            return Err(AppError::validation("Notifications from this browser aren't supported"));
        }
        let key = URL_SAFE_NO_PAD.decode(self.keys.p256dh.trim_end_matches('=')).map_err(|_| bad())?;
        PublicKey::from_sec1_bytes(&key).map_err(|_| bad())?;
        let auth = URL_SAFE_NO_PAD.decode(self.keys.auth.trim_end_matches('=')).map_err(|_| bad())?;
        if auth.len() != 16 {
            return Err(bad());
        }
        Ok(())
    }
}

fn random<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).expect("OS random source unavailable");
    buf
}

fn random_secret() -> SecretKey {
    loop {
        // A random 32-byte string is a valid P-256 scalar except with
        // negligible probability; try again in that case.
        if let Ok(key) = SecretKey::from_slice(&random::<32>()) {
            return key;
        }
    }
}

/// The server's VAPID identity.
pub struct Vapid {
    signing: SigningKey,
    /// Uncompressed public key, base64url: what browsers subscribe with.
    pub public_key: String,
    subject: String,
}

impl Vapid {
    /// From a stored private key (base64url, 32 bytes), or a fresh one.
    pub fn from_private(private_b64: Option<&str>, subject: String) -> Result<(Self, String), AppError> {
        let secret = match private_b64 {
            Some(b64) => URL_SAFE_NO_PAD
                .decode(b64.trim())
                .ok()
                .and_then(|b| SecretKey::from_slice(&b).ok())
                .ok_or_else(|| AppError::Internal(anyhow::anyhow!("invalid VAPID private key")))?,
            None => random_secret(),
        };
        let private = URL_SAFE_NO_PAD.encode(secret.to_bytes());
        let public_key = URL_SAFE_NO_PAD.encode(secret.public_key().to_sec1_bytes());
        Ok((
            Self {
                signing: SigningKey::from(&secret),
                public_key,
                subject,
            },
            private,
        ))
    }

    /// `Authorization` header value for one push service.
    fn authorization(&self, endpoint: &Url) -> String {
        let audience = endpoint.origin().ascii_serialization();
        let header = URL_SAFE_NO_PAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = serde_json::json!({
            "aud": audience,
            "exp": now_ms() / 1000 + 12 * 3600,
            "sub": self.subject,
        });
        let input = format!("{header}.{}", URL_SAFE_NO_PAD.encode(claims.to_string()));
        let signature: Signature = self.signing.sign(input.as_bytes());
        let jwt = format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()));
        format!("vapid t={jwt}, k={}", self.public_key)
    }
}

/// Encrypts `plaintext` for one subscription (RFC 8291, single record).
pub fn encrypt(plaintext: &[u8], keys: &SubscriptionKeys) -> Result<Vec<u8>, AppError> {
    encrypt_with(plaintext, keys, &random_secret(), random::<16>())
}

fn encrypt_with(plaintext: &[u8], keys: &SubscriptionKeys, sender: &SecretKey, salt: [u8; 16]) -> Result<Vec<u8>, AppError> {
    let bad = || AppError::validation("That push subscription isn't valid");
    let ua_public_bytes = URL_SAFE_NO_PAD.decode(keys.p256dh.trim_end_matches('=')).map_err(|_| bad())?;
    let ua_public = PublicKey::from_sec1_bytes(&ua_public_bytes).map_err(|_| bad())?;
    let auth = URL_SAFE_NO_PAD.decode(keys.auth.trim_end_matches('=')).map_err(|_| bad())?;
    let as_public = sender.public_key().to_sec1_bytes();
    // One record holds the payload, the delimiter and the 16-byte tag.
    if plaintext.len() + 1 + 16 > RECORD_SIZE as usize - 86 {
        return Err(AppError::validation("Notification too long"));
    }

    let shared = p256::ecdh::diffie_hellman(sender.to_nonzero_scalar(), ua_public.as_affine());

    // IKM = HKDF(auth, ecdh, "WebPush: info\0" || ua_public || as_public, 32)
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(&ua_public_bytes);
    key_info.extend_from_slice(&as_public);
    let mut ikm = [0u8; 32];
    Hkdf::<Sha256>::new(Some(&auth), shared.raw_secret_bytes())
        .expand(&key_info, &mut ikm)
        .expect("valid length");

    let prk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut cek = [0u8; 16];
    let mut nonce = [0u8; 12];
    prk.expand(b"Content-Encoding: aes128gcm\0", &mut cek).expect("valid length");
    prk.expand(b"Content-Encoding: nonce\0", &mut nonce).expect("valid length");

    let mut record = plaintext.to_vec();
    record.push(0x02); // last (and only) record
    let cipher = Aes128Gcm::new_from_slice(&cek).expect("16-byte key");
    let ciphertext = cipher
        .encrypt(&aes_gcm::Nonce::from(nonce), record.as_slice())
        .map_err(|_| AppError::Internal(anyhow::anyhow!("push encryption failed")))?;

    let mut out = Vec::with_capacity(86 + ciphertext.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    out.push(as_public.len() as u8);
    out.extend_from_slice(&as_public);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// What happened to one delivery.
#[derive(Debug, PartialEq, Eq)]
pub enum Delivery {
    Sent,
    /// The subscription is gone (unsubscribed, expired): forget it.
    Gone,
    Failed,
}

pub struct Pusher {
    http: reqwest::Client,
    pub vapid: Vapid,
}

impl Pusher {
    pub fn new(vapid: Vapid) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()
                .expect("reqwest client"),
            vapid,
        }
    }

    pub async fn send(&self, sub: &Subscription, notice: &Notice) -> Delivery {
        let Ok(endpoint) = Url::parse(&sub.endpoint) else {
            return Delivery::Gone;
        };
        let payload = serde_json::to_vec(notice).expect("notice serializes");
        let body = match encrypt(&payload, &sub.keys) {
            Ok(b) => b,
            Err(_) => return Delivery::Gone,
        };
        let res = self
            .http
            .post(endpoint.clone())
            .header(reqwest::header::AUTHORIZATION, self.vapid.authorization(&endpoint))
            .header(reqwest::header::CONTENT_ENCODING, "aes128gcm")
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .header("TTL", TTL_SECS.to_string())
            .header("Urgency", "normal")
            .header("Topic", topic(&notice.tag))
            .body(body)
            .send()
            .await;
        match res {
            Ok(r) if r.status().is_success() => Delivery::Sent,
            Ok(r) if matches!(r.status().as_u16(), 404 | 410) => Delivery::Gone,
            Ok(r) => {
                tracing::warn!(status = %r.status(), host = endpoint.host_str().unwrap_or(""), "push rejected");
                Delivery::Failed
            }
            Err(err) => {
                // The endpoint URL is a capability: never log it.
                tracing::warn!(error = %err.without_url(), "push request failed");
                Delivery::Failed
            }
        }
    }
}

/// `Topic` lets the push service replace an undelivered notice for the same
/// room; it allows only 32 URL-safe characters.
fn topic(tag: &str) -> String {
    tag.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser() -> (SecretKey, SubscriptionKeys) {
        let ua = random_secret();
        let keys = SubscriptionKeys {
            p256dh: URL_SAFE_NO_PAD.encode(ua.public_key().to_sec1_bytes()),
            auth: URL_SAFE_NO_PAD.encode(random::<16>()),
        };
        (ua, keys)
    }

    /// The receiving side of RFC 8291, written from the spec, so the
    /// encryption is checked by an independent decryption.
    fn decrypt(body: &[u8], ua: &SecretKey, keys: &SubscriptionKeys) -> Vec<u8> {
        let salt = &body[..16];
        assert_eq!(u32::from_be_bytes(body[16..20].try_into().unwrap()), RECORD_SIZE);
        let idlen = body[20] as usize;
        let as_public_bytes = &body[21..21 + idlen];
        let ciphertext = &body[21 + idlen..];
        let as_public = PublicKey::from_sec1_bytes(as_public_bytes).unwrap();
        let shared = p256::ecdh::diffie_hellman(ua.to_nonzero_scalar(), as_public.as_affine());
        let auth = URL_SAFE_NO_PAD.decode(&keys.auth).unwrap();
        let mut info = b"WebPush: info\0".to_vec();
        info.extend_from_slice(&ua.public_key().to_sec1_bytes());
        info.extend_from_slice(as_public_bytes);
        let mut ikm = [0u8; 32];
        Hkdf::<Sha256>::new(Some(&auth), shared.raw_secret_bytes())
            .expand(&info, &mut ikm)
            .unwrap();
        let prk = Hkdf::<Sha256>::new(Some(salt), &ikm);
        let (mut cek, mut nonce) = ([0u8; 16], [0u8; 12]);
        prk.expand(b"Content-Encoding: aes128gcm\0", &mut cek).unwrap();
        prk.expand(b"Content-Encoding: nonce\0", &mut nonce).unwrap();
        let mut plain = Aes128Gcm::new_from_slice(&cek)
            .unwrap()
            .decrypt(&aes_gcm::Nonce::from(nonce), ciphertext)
            .unwrap();
        assert_eq!(plain.pop(), Some(0x02), "single final record");
        plain
    }

    #[test]
    fn payload_round_trips() {
        let (ua, keys) = browser();
        let body = encrypt(br#"{"title":"Riky started watching"}"#, &keys).unwrap();
        assert_eq!(body[20], 65, "sender key in the header");
        assert_eq!(decrypt(&body, &ua, &keys), br#"{"title":"Riky started watching"}"#);
    }

    /// Prints a vector for checking against another implementation:
    /// `cargo test --lib push::tests::vector -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn vector() {
        let (ua, keys) = browser();
        let body = encrypt(b"hello from partello", &keys).unwrap();
        println!(
            "VECTOR {}",
            serde_json::json!({
                "private": URL_SAFE_NO_PAD.encode(ua.to_bytes()),
                "public": keys.p256dh,
                "auth": keys.auth,
                "body": URL_SAFE_NO_PAD.encode(body),
            })
        );
    }

    #[test]
    fn each_message_uses_fresh_keys() {
        let (_, keys) = browser();
        let a = encrypt(b"x", &keys).unwrap();
        let b = encrypt(b"x", &keys).unwrap();
        assert_ne!(a[..16], b[..16], "salt");
        assert_ne!(a[21..86], b[21..86], "sender key");
    }

    #[test]
    fn subscriptions_are_limited_to_browser_push_services() {
        let (_, keys) = browser();
        let sub = |endpoint: &str| Subscription {
            endpoint: endpoint.into(),
            keys: keys.clone(),
        };
        assert!(sub("https://fcm.googleapis.com/fcm/send/abc").validate().is_ok());
        assert!(sub("https://updates.push.services.mozilla.com/wpush/v2/abc").validate().is_ok());
        assert!(sub("https://web.push.apple.com/QGx").validate().is_ok());
        assert!(sub("http://fcm.googleapis.com/fcm/send/abc").validate().is_err(), "https only");
        assert!(sub("https://evil.example/fcm.googleapis.com").validate().is_err());
        assert!(sub("https://fcm.googleapis.com.evil.example/x").validate().is_err());
        assert!(sub("https://127.0.0.1/x").validate().is_err());
        assert!(sub("https://fcm.googleapis.com:8443/x").validate().is_err());
        let mut bad = sub("https://fcm.googleapis.com/fcm/send/abc");
        bad.keys.auth = URL_SAFE_NO_PAD.encode([0u8; 8]);
        assert!(bad.validate().is_err());
        bad.keys = SubscriptionKeys {
            p256dh: "AAAA".into(),
            auth: keys.auth.clone(),
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn vapid_tokens_are_signed_for_the_push_service() {
        let (vapid, private) = Vapid::from_private(None, "mailto:ops@example.org".into()).unwrap();
        let (again, _) = Vapid::from_private(Some(&private), "mailto:ops@example.org".into()).unwrap();
        assert_eq!(vapid.public_key, again.public_key, "the stored key restores the same identity");

        let header = vapid.authorization(&Url::parse("https://fcm.googleapis.com/fcm/send/abc").unwrap());
        let (jwt, k) = header.strip_prefix("vapid t=").unwrap().split_once(", k=").unwrap();
        assert_eq!(k, vapid.public_key);
        let parts: Vec<&str> = jwt.split('.').collect();
        let claims: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");
        assert_eq!(claims["sub"], "mailto:ops@example.org");

        use p256::ecdsa::{VerifyingKey, signature::Verifier};
        let public = PublicKey::from_sec1_bytes(&URL_SAFE_NO_PAD.decode(k).unwrap()).unwrap();
        let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap();
        VerifyingKey::from(&public)
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .unwrap();
    }

    #[test]
    fn topics_are_url_safe_and_short() {
        assert_eq!(topic("room:k7Qm2xPa9Rt4"), "roomk7Qm2xPa9Rt4");
        assert!(topic(&"x".repeat(100)).len() <= 32);
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// Sends to a made-up FCM endpoint: a valid VAPID signature gets past
    /// authentication and is told the subscription doesn't exist.
    /// `cargo test --lib push::live -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn fcm_accepts_our_signature() {
        let (vapid, _) = Vapid::from_private(None, "mailto:push@example.org".into()).unwrap();
        let ua = random_secret();
        let sub = Subscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/not-a-real-token:APA91b".into(),
            keys: SubscriptionKeys {
                p256dh: URL_SAFE_NO_PAD.encode(ua.public_key().to_sec1_bytes()),
                auth: URL_SAFE_NO_PAD.encode(random::<16>()),
            },
        };
        let endpoint = Url::parse(&sub.endpoint).unwrap();
        let res = reqwest::Client::new()
            .post(endpoint.clone())
            .header(reqwest::header::AUTHORIZATION, vapid.authorization(&endpoint))
            .header(reqwest::header::CONTENT_ENCODING, "aes128gcm")
            .header("TTL", "60")
            .body(encrypt(b"{}", &sub.keys).unwrap())
            .send()
            .await
            .unwrap();
        let status = res.status();
        println!(
            "FCM {status} {}",
            res.text().await.unwrap_or_default().chars().take(300).collect::<String>()
        );
    }
}
