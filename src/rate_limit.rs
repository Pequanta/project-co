//! Small, in-process fixed-window rate limiter for inbound webhook traffic.
//! It is independent of Telegram and keyed by the TCP peer IP.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct Window {
    started_at: Instant,
    requests: u32,
}

#[derive(Debug)]
pub struct WebhookRateLimiter {
    limit: u32,
    windows: Mutex<HashMap<IpAddr, Window>>,
}

impl WebhookRateLimiter {
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            windows: Mutex::new(HashMap::new()),
        }
    }

    /// Returns whether this request is allowed. Idle entries are pruned during
    /// normal use to keep memory bounded for a public webhook endpoint.
    pub fn check(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut windows = self.windows.lock().expect("rate limiter lock poisoned");
        windows
            .retain(|_, window| now.duration_since(window.started_at) < Duration::from_secs(120));
        let window = windows.entry(ip).or_insert(Window {
            started_at: now,
            requests: 0,
        });
        if now.duration_since(window.started_at) >= Duration::from_secs(60) {
            window.started_at = now;
            window.requests = 0;
        }
        if window.requests >= self.limit {
            return false;
        }
        window.requests += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::WebhookRateLimiter;

    #[test]
    fn limits_each_ip_independently() {
        let limiter = WebhookRateLimiter::new(2);
        let first = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let second = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        assert!(limiter.check(first));
        assert!(limiter.check(first));
        assert!(!limiter.check(first));
        assert!(limiter.check(second));
    }
}
