//! Runtime configuration, sourced from the environment with sane defaults.

use std::env;

/// Smallest accepted link lifetime (1 minute).
pub const MIN_TTL_SECS: i64 = 60;
/// Lifetime used when a request omits one (1 day).
pub const DEFAULT_TTL_SECS: i64 = 24 * 60 * 60;
/// Default ceiling on link lifetime (7 days); overridable via the environment.
pub const DEFAULT_MAX_TTL_SECS: i64 = 7 * 24 * 60 * 60;
/// Default interval between reaper sweeps (seconds).
pub const DEFAULT_REAP_SECS: u64 = 60;

#[derive(Clone, Debug)]
pub struct Config {
    /// Address to bind, e.g. `127.0.0.1:8080`.
    pub bind: String,
    /// Public base URL used to build returned links; always ends with `/`.
    pub base_url: String,
    /// SQLite database file path.
    pub db_path: String,
    /// Maximum link lifetime in seconds (requests above this are rejected).
    pub max_ttl_secs: i64,
    /// How often the reaper deletes expired rows, in seconds.
    pub reap_interval_secs: u64,
    /// Whether this server offers client-side encryption. Off by default so the
    /// public yuio.link instance need not be trusted; an operator opts in with
    /// `YUIOLINK_ENCRYPTION=1` (and their frontend can point at any backend).
    pub encryption_enabled: bool,
    /// API base URL the page's JS targets, exposed as a `<meta>` tag. Empty means
    /// same-origin; set `YUIOLINK_API_BASE` to point a hosted frontend at another
    /// backend (which may be the one that has encryption enabled).
    pub api_base: String,
}

impl Config {
    pub fn from_env() -> Self {
        let bind = env::var("YUIOLINK_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());

        let mut base_url =
            env::var("YUIOLINK_BASE_URL").unwrap_or_else(|_| format!("http://{bind}/"));
        if !base_url.ends_with('/') {
            base_url.push('/');
        }

        let db_path = env::var("YUIOLINK_DB").unwrap_or_else(|_| "yuiolink.db".to_string());

        let max_ttl_secs = env::var("YUIOLINK_MAX_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&v| v >= MIN_TTL_SECS)
            .unwrap_or(DEFAULT_MAX_TTL_SECS);

        let reap_interval_secs = env::var("YUIOLINK_REAP_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&v| v > 0)
            .unwrap_or(DEFAULT_REAP_SECS);

        let encryption_enabled = env::var("YUIOLINK_ENCRYPTION")
            .ok()
            .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes"));

        let api_base = env::var("YUIOLINK_API_BASE")
            .map(|v| v.trim_end_matches('/').to_string())
            .unwrap_or_default();

        Self {
            bind,
            base_url,
            db_path,
            max_ttl_secs,
            reap_interval_secs,
            encryption_enabled,
            api_base,
        }
    }
}
