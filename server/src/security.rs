//! Random identifiers, secret hashing, and ephemeral TURN credentials.

use base64::{Engine, engine::general_purpose::STANDARD, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

/// Alphabet for room slugs: no look-alike characters (0/O, 1/l/I).
const SLUG_ALPHABET: &[u8] = b"23456789abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ";
pub const SLUG_LEN: usize = 12;

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).expect("OS random source unavailable");
    buf
}

/// A 12-character room slug (~70 bits of entropy), unguessable but short enough to share.
pub fn new_room_slug() -> String {
    let n = SLUG_ALPHABET.len();
    // Rejection sampling keeps the distribution uniform.
    let limit = 256 - (256 % n);
    let mut out = String::with_capacity(SLUG_LEN);
    while out.len() < SLUG_LEN {
        for b in random_bytes::<32>() {
            if (b as usize) < limit {
                out.push(SLUG_ALPHABET[b as usize % n] as char);
                if out.len() == SLUG_LEN {
                    break;
                }
            }
        }
    }
    out
}

pub fn is_valid_slug(slug: &str) -> bool {
    slug.len() == SLUG_LEN && slug.bytes().all(|b| SLUG_ALPHABET.contains(&b))
}

/// A 256-bit bearer secret, URL-safe, with a readable prefix (`pts_`, `phk_`, `ptk_`).
pub fn new_secret(prefix: &str) -> String {
    format!("{prefix}{}", URL_SAFE_NO_PAD.encode(random_bytes::<32>()))
}

/// Secrets are stored and compared only as SHA-256 digests.
pub fn hash_secret(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}

/// Constant-time equality for digests.
pub fn digest_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Credentials for coturn's REST API (`use-auth-secret`): the username carries
/// the expiry, the password is `base64(HMAC-SHA1(secret, username))`. The shared
/// secret itself never leaves the server.
pub fn turn_credentials(secret: &str, user_tag: &str, expires_unix: i64) -> (String, String) {
    let username = format!("{expires_unix}:{user_tag}");
    let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(username.as_bytes());
    let credential = STANDARD.encode(mac.finalize().into_bytes());
    (username, credential)
}

/// Argon2id with a random salt. CPU-heavy: call from `spawn_blocking`.
pub fn hash_password(password: &str) -> anyhow::Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("password hashing failed: {e}"))
}

/// Constant-time check against a stored PHC string. CPU-heavy: call from `spawn_blocking`.
pub fn verify_password(password: &str, phc: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_valid_and_distinct() {
        let a = new_room_slug();
        let b = new_room_slug();
        assert!(is_valid_slug(&a), "{a}");
        assert_ne!(a, b);
        assert!(!is_valid_slug("short"));
        assert!(!is_valid_slug("0OlI0OlI0OlI"));
        assert!(!is_valid_slug("../../etc/pa"));
    }

    #[test]
    fn secrets_hash_and_compare() {
        let s = new_secret("pts_");
        assert!(s.starts_with("pts_"));
        assert!(s.len() > 40);
        assert!(digest_eq(&hash_secret(&s), &hash_secret(&s)));
        assert!(!digest_eq(&hash_secret(&s), &hash_secret("other")));
    }

    #[test]
    fn passwords_hash_and_verify() {
        let h = hash_password("0420").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(!h.contains("0420"));
        assert!(verify_password("0420", &h));
        assert!(!verify_password("0421", &h));
        assert!(!verify_password("0420", "not a hash"));
    }

    #[test]
    fn turn_credentials_match_coturn_formula() {
        // Reference value computed with: printf '1700000000:alice' | openssl dgst -sha1 -hmac secret -binary | base64
        let (user, cred) = turn_credentials("secret", "alice", 1_700_000_000);
        assert_eq!(user, "1700000000:alice");
        assert_eq!(cred, "d8soP47RbdIKLDUOpnJPVQyq5Ts=");
    }
}
