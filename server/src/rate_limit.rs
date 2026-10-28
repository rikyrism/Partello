//! Token-bucket rate limiting.
//!
//! [`TokenBucket`] is used per WebSocket connection (no locking, owned by the
//! connection task). [`KeyedLimiter`] guards HTTP endpoints per client IP or
//! API key. Both are in-memory; with several instances each enforces its own
//! budget (documented in docs/ARCHITECTURE.md).

use std::{
    collections::HashMap,
    hash::Hash,
    net::{IpAddr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::http::HeaderMap;

#[derive(Debug, Clone)]
pub struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_per_sec: f64,
    pub(crate) last: Instant,
}

impl TokenBucket {
    /// `capacity` requests in a burst, refilled at `per_sec` per second.
    pub fn new(capacity: u32, per_sec: f64) -> Self {
        Self {
            capacity: capacity as f64,
            tokens: capacity as f64,
            refill_per_sec: per_sec,
            last: Instant::now(),
        }
    }

    pub fn try_take(&mut self) -> bool {
        self.try_take_at(Instant::now())
    }

    pub fn try_take_at(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn is_full_at(&self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.tokens + elapsed * self.refill_per_sec >= self.capacity
    }
}

pub struct KeyedLimiter<K> {
    buckets: Mutex<HashMap<K, TokenBucket>>,
    capacity: u32,
    per_sec: f64,
}

impl<K: Eq + Hash + Clone> KeyedLimiter<K> {
    pub fn new(capacity: u32, per: Duration) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            capacity,
            per_sec: capacity as f64 / per.as_secs_f64(),
        }
    }

    pub fn check(&self, key: &K) -> bool {
        let mut buckets = self.buckets.lock().unwrap();
        // Keep memory bounded under many distinct keys: drop idle buckets, then
        // the least recently used ones. Never reset everything at once, which
        // would hand a flooder fresh budgets for the keys that matter.
        if buckets.len() > 10_000 {
            let now = Instant::now();
            buckets.retain(|_, b| !b.is_full_at(now));
            if buckets.len() > 10_000 {
                let mut by_age: Vec<(Instant, K)> = buckets.iter().map(|(k, b)| (b.last, k.clone())).collect();
                by_age.sort_by_key(|(at, _)| *at);
                let excess = buckets.len() / 5;
                for (_, k) in by_age.into_iter().take(excess) {
                    buckets.remove(&k);
                }
            }
        }
        buckets
            .entry(key.clone())
            .or_insert_with(|| TokenBucket::new(self.capacity, self.per_sec))
            .try_take()
    }
}

/// The client address used for rate limits and bans: the socket peer, or,
/// behind one trusted reverse proxy, the right-most `X-Forwarded-For` entry
/// (the one that proxy appended; entries to its left come from the client),
/// or a dedicated header such as `CF-Connecting-IP`. IPv6 addresses are
/// reduced to their /64, since one subscriber usually holds a whole /64.
pub fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>, trust_proxy: bool) -> IpAddr {
    client_ip_with(headers, peer, trust_proxy, None)
}

pub fn client_ip_with(headers: &HeaderMap, peer: Option<SocketAddr>, trust_proxy: bool, header: Option<&str>) -> IpAddr {
    let forwarded: Option<IpAddr> = if trust_proxy {
        match header {
            Some(name) => headers.get(name).and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse().ok()),
            None => headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.rsplit(',').next())
                .and_then(|v| v.trim().parse().ok()),
        }
    } else {
        None
    };
    let ip = forwarded.unwrap_or_else(|| peer.map(|p| p.ip()).unwrap_or(IpAddr::from([127, 0, 0, 1])));
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                let s = v6.segments();
                IpAddr::V6(std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
            }
        },
        v4 => v4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_allows_burst_then_refills() {
        let start = Instant::now();
        let mut b = TokenBucket::new(3, 1.0);
        assert!(b.try_take_at(start));
        assert!(b.try_take_at(start));
        assert!(b.try_take_at(start));
        assert!(!b.try_take_at(start));
        assert!(b.try_take_at(start + Duration::from_millis(1_100)));
        assert!(!b.try_take_at(start + Duration::from_millis(1_200)));
    }

    #[test]
    fn keyed_limiter_isolates_keys() {
        let l = KeyedLimiter::new(2, Duration::from_secs(60));
        assert!(l.check(&"a"));
        assert!(l.check(&"a"));
        assert!(!l.check(&"a"));
        assert!(l.check(&"b"));
    }

    #[test]
    fn forwarded_for_only_when_trusted() {
        let mut h = HeaderMap::new();
        // "1.1.1.1" was sent by the client; the proxy appended the real address.
        h.insert("x-forwarded-for", "1.1.1.1, 203.0.113.7".parse().unwrap());
        let peer: SocketAddr = "10.0.0.2:5000".parse().unwrap();
        assert_eq!(client_ip(&h, Some(peer), true).to_string(), "203.0.113.7");
        assert_eq!(client_ip(&h, Some(peer), false).to_string(), "10.0.0.2");
    }

    #[test]
    fn dedicated_header_and_ipv6_grouping() {
        let mut h = HeaderMap::new();
        h.insert("cf-connecting-ip", "198.51.100.9".parse().unwrap());
        let peer: SocketAddr = "10.0.0.2:5000".parse().unwrap();
        assert_eq!(
            client_ip_with(&h, Some(peer), true, Some("cf-connecting-ip")).to_string(),
            "198.51.100.9"
        );
        let a: SocketAddr = "[2001:db8:1:2:aaaa::1]:1".parse().unwrap();
        let b: SocketAddr = "[2001:db8:1:2:bbbb::9]:1".parse().unwrap();
        let none = HeaderMap::new();
        assert_eq!(client_ip(&none, Some(a), false), client_ip(&none, Some(b), false));
        assert_eq!(client_ip(&none, Some(a), false).to_string(), "2001:db8:1:2::");
    }

    #[test]
    fn keyed_limiter_evicts_oldest_not_everything() {
        let l = KeyedLimiter::new(1, Duration::from_secs(3600));
        for k in 0..=10_001u32 {
            l.check(&k);
        }
        assert!(!l.check(&10_001u32), "recent keys keep their spent budget");
    }
}
