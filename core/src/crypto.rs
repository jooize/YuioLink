//! Client-side authenticated encryption for YuioLink.
//!
//! The sealed format is shared verbatim across the browser (WebCrypto) and any
//! native client (CryptoKit) so a link encrypted on one platform opens on
//! another:
//!
//! - Cipher: **AES-256-GCM** — 256-bit key, 96-bit random nonce, 128-bit tag.
//! - Sealed string: `yl1.<b64url(nonce)>.<b64url(ciphertext||tag)>`
//!   (base64url, no padding; the `aes-gcm` crate appends the tag to the
//!   ciphertext, matching WebCrypto's `AES-GCM` output).
//! - Key transport: the 32-byte key travels in the URL **fragment** as
//!   `b64url(key)` and is therefore NEVER sent to the server.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use rand::RngCore;
use rand::rngs::OsRng;
use zeroize::Zeroize;

const VERSION_TAG: &str = "yl1";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("invalid key length: expected {KEY_LEN} bytes, got {0}")]
    KeyLength(usize),
    #[error("malformed sealed payload")]
    MalformedPayload,
    #[error("unsupported cipher version: {0}")]
    UnsupportedVersion(String),
    #[error("base64 decode failed")]
    Base64,
    #[error("decryption failed (wrong key or corrupted data)")]
    Decrypt,
}

/// A 256-bit symmetric key, zeroized on drop.
pub struct LinkKey([u8; KEY_LEN]);

impl LinkKey {
    /// Generate a fresh key from the operating-system CSPRNG.
    pub fn generate() -> Self {
        let mut bytes = [0u8; KEY_LEN];
        OsRng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != KEY_LEN {
            return Err(CryptoError::KeyLength(bytes.len()));
        }
        let mut arr = [0u8; KEY_LEN];
        arr.copy_from_slice(bytes);
        Ok(Self(arr))
    }

    /// Encode the key for the URL fragment (base64url, no padding).
    pub fn to_fragment(&self) -> String {
        B64.encode(self.0)
    }

    /// Decode a key from a URL fragment (a leading `#` is tolerated).
    pub fn from_fragment(s: &str) -> Result<Self, CryptoError> {
        let bytes = B64
            .decode(s.trim_start_matches('#'))
            .map_err(|_| CryptoError::Base64)?;
        Self::from_slice(&bytes)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl Drop for LinkKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Encrypt `plaintext` under `key`, returning the sealed string.
pub fn seal(key: &LinkKey, plaintext: &[u8]) -> Result<String, CryptoError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.0));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| CryptoError::Decrypt)?;
    Ok(format!(
        "{}.{}.{}",
        VERSION_TAG,
        B64.encode(nonce_bytes),
        B64.encode(ciphertext)
    ))
}

/// Encrypt a UTF-8 string under `key`.
pub fn seal_str(key: &LinkKey, plaintext: &str) -> Result<String, CryptoError> {
    seal(key, plaintext.as_bytes())
}

/// Open a sealed string under `key`, returning the plaintext bytes.
pub fn open(key: &LinkKey, sealed: &str) -> Result<Vec<u8>, CryptoError> {
    let mut parts = sealed.splitn(3, '.');
    let version = parts.next().ok_or(CryptoError::MalformedPayload)?;
    if version != VERSION_TAG {
        return Err(CryptoError::UnsupportedVersion(version.to_string()));
    }
    let nonce_b64 = parts.next().ok_or(CryptoError::MalformedPayload)?;
    let ct_b64 = parts.next().ok_or(CryptoError::MalformedPayload)?;

    let nonce_bytes = B64.decode(nonce_b64).map_err(|_| CryptoError::Base64)?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err(CryptoError::MalformedPayload);
    }
    let ciphertext = B64.decode(ct_b64).map_err(|_| CryptoError::Base64)?;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.0));
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| CryptoError::Decrypt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let key = LinkKey::generate();
        let sealed = seal_str(&key, "https://example.com/secret").unwrap();
        assert!(sealed.starts_with("yl1."));
        assert_eq!(sealed.split('.').count(), 3);
        let opened = open(&key, &sealed).unwrap();
        assert_eq!(opened, b"https://example.com/secret");
    }

    #[test]
    fn wrong_key_fails() {
        let key = LinkKey::generate();
        let other = LinkKey::generate();
        let sealed = seal_str(&key, "secret").unwrap();
        assert!(matches!(open(&other, &sealed), Err(CryptoError::Decrypt)));
    }

    #[test]
    fn fragment_round_trip() {
        let key = LinkKey::generate();
        let frag = key.to_fragment();
        let key2 = LinkKey::from_fragment(&frag).unwrap();
        assert_eq!(key.as_bytes(), key2.as_bytes());
        // A leading '#' (as it appears in window.location.hash) is tolerated.
        let key3 = LinkKey::from_fragment(&format!("#{frag}")).unwrap();
        assert_eq!(key.as_bytes(), key3.as_bytes());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = LinkKey::generate();
        let sealed = seal_str(&key, "secret").unwrap();
        let mut chars: Vec<char> = sealed.chars().collect();
        let last = chars.last_mut().unwrap();
        *last = if *last == 'A' { 'B' } else { 'A' };
        let tampered: String = chars.into_iter().collect();
        assert!(open(&key, &tampered).is_err());
    }

    #[test]
    fn rejects_unknown_version() {
        let key = LinkKey::generate();
        let sealed = seal_str(&key, "x").unwrap();
        let bumped = sealed.replacen("yl1.", "yl9.", 1);
        assert!(matches!(
            open(&key, &bumped),
            Err(CryptoError::UnsupportedVersion(_))
        ));
    }
}
