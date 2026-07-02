//! Create-path rate limiting: a small in-memory token bucket per client IP.
//!
//! Scope is deliberately narrow (see docs/NAMESPACES.md): only *creation* is
//! limited, because that is what protects the short public tiers from squatting.
//! Resolution is never limited or slowed -- latency is not throughput, and
//! volumetric abuse is the upstream CDN's job. Over the limit means a fast 429,
//! never a delay.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Creates allowed instantly from a fresh client (burst).
const BURST: f64 = 10.0;
/// Sustained refill: one create every 6 seconds (~600/hour).
const REFILL_PER_SEC: f64 = 1.0 / 6.0;
/// Prune fully-refilled buckets once the table grows past this many clients,
/// bounding memory without a background task.
const PRUNE_AT: usize = 10_000;

struct Bucket {
    tokens: f64,
    last: Instant,
}

/// A token bucket per key (client IP). One instance lives in `AppState`.
#[derive(Default)]
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spend one token for `key`. `false` means the caller must answer 429.
    pub fn allow(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut map = self.buckets.lock().unwrap_or_else(|p| p.into_inner());
        if map.len() >= PRUNE_AT {
            map.retain(|_, b| now.duration_since(b.last).as_secs_f64() * REFILL_PER_SEC < BURST);
        }
        let b = map.entry(key.to_string()).or_insert(Bucket {
            tokens: BURST,
            last: now,
        });
        let refill = now.duration_since(b.last).as_secs_f64() * REFILL_PER_SEC;
        b.tokens = (b.tokens + refill).min(BURST);
        b.last = now;
        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_then_deny_and_keys_are_independent() {
        let rl = RateLimiter::new();
        for i in 0..BURST as usize {
            assert!(rl.allow("a"), "create {i} within burst must pass");
        }
        assert!(!rl.allow("a"), "over burst must be denied");
        // Another client is unaffected.
        assert!(rl.allow("b"));
    }
}
