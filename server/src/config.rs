//! Runtime configuration, sourced from the environment with sane defaults.

use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    /// Address to bind, e.g. `127.0.0.1:8080`.
    pub bind: String,
    /// Public base URL used to build returned links; always ends with `/`.
    pub base_url: String,
    /// SQLite database file path.
    pub db_path: String,
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

        Self {
            bind,
            base_url,
            db_path,
        }
    }
}
