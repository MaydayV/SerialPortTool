//! Local loopback bearer-token protection for the embedded MCP endpoint.
//!
//! This is intentionally a deployment guard for a local pairing flow, not MCP
//! OAuth Authorization. The token is generated in memory, never logged, and is
//! only exposed through an explicit server handle method for the future UI.

use axum::http::HeaderMap;
use getrandom::fill;
use std::sync::{Arc, RwLock};

const TOKEN_BYTES: usize = 32;

#[derive(Clone)]
pub struct LocalBearerAuth {
    token: Arc<RwLock<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthFailure {
    Missing,
    Invalid,
    Malformed,
}

impl LocalBearerAuth {
    pub fn new() -> Result<Self, getrandom::Error> {
        Ok(Self {
            token: Arc::new(RwLock::new(generate_token()?)),
        })
    }

    /// Returns the token only for an explicit pairing/export action.
    pub fn token(&self) -> String {
        self.token
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Rotates the local pairing token. The old token is immediately revoked.
    pub fn reset(&self) -> Result<String, getrandom::Error> {
        let next = generate_token()?;
        *self
            .token
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = next.clone();
        Ok(next)
    }

    pub fn authorize(&self, headers: &HeaderMap) -> Result<(), AuthFailure> {
        let Some(value) = headers.get(axum::http::header::AUTHORIZATION) else {
            return Err(AuthFailure::Missing);
        };
        let value = value.to_str().map_err(|_| AuthFailure::Malformed)?;
        let mut parts = value.split_ascii_whitespace();
        let scheme = parts.next();
        let token = parts.next();
        if scheme != Some("Bearer") || token.is_none() || parts.next().is_some() {
            return Err(AuthFailure::Malformed);
        }
        if token == Some(self.token().as_str()) {
            Ok(())
        } else {
            Err(AuthFailure::Invalid)
        }
    }
}

fn generate_token() -> Result<String, getrandom::Error> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    fill(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_and_reset_revokes_the_previous_value() {
        let auth = LocalBearerAuth::new().unwrap();
        let first = auth.token();
        let second = auth.reset().unwrap();
        assert_ne!(first, second);
        assert_eq!(first.len(), TOKEN_BYTES * 2);
        assert_eq!(second.len(), TOKEN_BYTES * 2);
    }
}
