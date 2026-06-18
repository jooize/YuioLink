//! YuioLink core — platform-agnostic logic shared by the server, the CLI, and
//! the (future) macOS app.
//!
//! Everything in this crate is free of I/O and framework dependencies so it can
//! be linked directly into the Rust binaries and exposed over a C ABI by
//! `yuiolink-core-ffi` for Swift (the same code-sharing pattern as YuioPaste).

pub mod content;
pub mod crypto;
pub mod link;

pub use content::ContentType;
pub use crypto::{CryptoError, LinkKey, open, seal, seal_str};
pub use link::{
    DEFAULT_ALLOWED_SCHEMES, DEFAULT_NAME_LEN, UriError, generate_name, validate_redirect,
};
