//! Pairing: a single-use 6-digit code shown in the tray, exchanged for a token.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use axum::extract::State;
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use super::{ApiError, ApiJson, AppState};
use crate::config::PairedDevice;

/// Wrong codes allowed before `/pair` locks.
pub const MAX_FAILED_ATTEMPTS: u32 = 5;
/// How long `/pair` refuses everything after too many wrong codes.
pub const LOCKOUT: Duration = Duration::from_secs(60);
const MAX_DEVICE_NAME_CHARS: usize = 64;
const TOKEN_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PairRequest {
    pub code: String,
    /// Optional (contract C-004): missing, `null`, not a string or empty all become
    /// "Unknown device". It is never an error.
    #[serde(default, deserialize_with = "lenient_device_name")]
    pub device_name: String,
}

/// Any JSON value is accepted: a string is kept, everything else (null, a number, ...) is empty.
fn lenient_device_name<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::String(name) => name,
        _ => String::new(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PairResponse {
    pub token: String,
}

/// 32 random bytes from the OS CSPRNG, base64url without padding.
pub fn generate_token() -> Result<String> {
    let mut bytes = [0u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(|e| anyhow!("OS random source failed: {e}"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// Uniform 6-digit code (leading zeros kept) from the OS CSPRNG.
pub fn generate_code() -> Result<String> {
    // Largest multiple of 1_000_000 that fits in a u32; rejecting above it avoids modulo bias.
    const LIMIT: u32 = 4_294_000_000;
    loop {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).map_err(|e| anyhow!("OS random source failed: {e}"))?;
        let n = u32::from_le_bytes(bytes);
        if n < LIMIT {
            return Ok(format!("{:06}", n % 1_000_000));
        }
    }
}

/// The current code plus the brute-force lockout.
#[derive(Debug)]
pub struct PairingState {
    code: String,
    failed: u32,
    locked_until: Option<Instant>,
}

impl PairingState {
    pub fn new(code: String) -> Self {
        Self {
            code,
            failed: 0,
            locked_until: None,
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn is_locked(&self, now: Instant) -> bool {
        self.locked_until.is_some_and(|until| now < until)
    }

    /// Checks a candidate code. While locked it is always `false`, even for the right code.
    /// The 5th wrong code starts the lock, so the 6th attempt is already refused.
    pub fn attempt(&mut self, candidate: &str, now: Instant) -> bool {
        if self.is_locked(now) {
            return false;
        }
        if self.locked_until.take().is_some() {
            // The lock has expired: start counting again.
            self.failed = 0;
        }
        let ok: bool = candidate.as_bytes().ct_eq(self.code.as_bytes()).into();
        if !ok {
            self.failed += 1;
            if self.failed >= MAX_FAILED_ATTEMPTS {
                self.locked_until = Some(now + LOCKOUT);
            }
        }
        ok
    }

    /// Replaces the code after a successful pairing and clears the failure count.
    pub fn rotate(&mut self, new_code: String) {
        self.code = new_code;
        self.failed = 0;
        self.locked_until = None;
    }
}

fn clean_device_name(raw: &str) -> String {
    let name: String = raw.trim().chars().take(MAX_DEVICE_NAME_CHARS).collect();
    if name.is_empty() {
        "Unknown device".to_owned()
    } else {
        name
    }
}

/// `POST /pair`
pub async fn pair(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<PairRequest>,
) -> Result<Json<PairResponse>, ApiError> {
    let mut pairing = state.pairing();
    if !pairing.attempt(&request.code, Instant::now()) {
        tracing::warn!("pairing attempt rejected");
        return Err(ApiError::bad_pairing_code());
    }

    let internal = |e: anyhow::Error| {
        tracing::error!("pairing failed: {e:#}");
        ApiError::internal()
    };
    let token = generate_token().map_err(internal)?;
    let next_code = generate_code().map_err(internal)?;
    let paired_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let name = clean_device_name(&request.device_name);
    state
        .config
        .add_device(PairedDevice {
            name: name.clone(),
            token: token.clone(),
            paired_at,
        })
        .map_err(internal)?;
    pairing.rotate(next_code);

    tracing::info!(device = %name, "device paired");
    Ok(Json(PairResponse { token }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_43_url_safe_chars() {
        let token = generate_token().unwrap();
        assert_eq!(token.len(), 43, "32 bytes in base64url without padding");
        assert!(token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(token, generate_token().unwrap());
    }

    #[test]
    fn code_is_six_digits() {
        for _ in 0..200 {
            let code = generate_code().unwrap();
            assert_eq!(code.len(), 6);
            assert!(code.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn locks_after_five_wrong_codes_then_unlocks() {
        let t0 = Instant::now();
        let mut state = PairingState::new("123456".into());
        for _ in 0..MAX_FAILED_ATTEMPTS {
            assert!(!state.attempt("000000", t0));
        }
        assert!(state.is_locked(t0));
        assert!(
            !state.attempt("123456", t0),
            "right code is refused while locked"
        );
        assert!(state.is_locked(t0 + LOCKOUT - Duration::from_secs(1)));

        let later = t0 + LOCKOUT + Duration::from_secs(1);
        assert!(!state.is_locked(later));
        assert!(
            state.attempt("123456", later),
            "works again after the lock expires"
        );
    }

    #[test]
    fn counter_restarts_after_lock_expires() {
        let t0 = Instant::now();
        let mut state = PairingState::new("123456".into());
        for _ in 0..MAX_FAILED_ATTEMPTS {
            state.attempt("000000", t0);
        }
        let later = t0 + LOCKOUT + Duration::from_secs(1);
        assert!(!state.attempt("000000", later));
        assert!(
            !state.is_locked(later),
            "one wrong code after expiry must not re-lock"
        );
    }

    #[test]
    fn rotate_replaces_code_and_clears_failures() {
        let t0 = Instant::now();
        let mut state = PairingState::new("123456".into());
        state.attempt("000000", t0);
        state.rotate("654321".into());
        assert!(!state.attempt("123456", t0));
        assert!(state.attempt("654321", t0));
    }

    #[test]
    fn device_names_are_cleaned() {
        assert_eq!(clean_device_name("  Pixel 8 "), "Pixel 8");
        assert_eq!(clean_device_name("   "), "Unknown device");
        assert_eq!(clean_device_name(&"x".repeat(200)).chars().count(), 64);
    }

    #[test]
    fn the_device_name_in_a_request_is_optional_and_lenient() {
        let name = |json: &str| {
            serde_json::from_str::<PairRequest>(json)
                .unwrap()
                .device_name
        };
        assert_eq!(name(r#"{"code":"1","device_name":"Pixel 8"}"#), "Pixel 8");
        assert_eq!(name(r#"{"code":"1"}"#), "");
        assert_eq!(name(r#"{"code":"1","device_name":null}"#), "");
        assert_eq!(name(r#"{"code":"1","device_name":7}"#), "");
        // The code is still required and must be a string.
        assert!(serde_json::from_str::<PairRequest>(r#"{"device_name":"x"}"#).is_err());
        assert!(serde_json::from_str::<PairRequest>(r#"{"code":1}"#).is_err());
    }
}
