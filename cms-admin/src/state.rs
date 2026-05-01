//! Shared application state — content store + session map + admin token.

use plausiden_cms_core::Store;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Shared state cloned per request.
#[derive(Clone)]
pub struct AppState {
    /// On-disk content store (per-site directories).
    pub store: Store,
    /// Admin password (read once at startup; constant-time compared
    /// per login attempt). Never logged.
    pub admin_token: Arc<String>,
    /// Active sessions: opaque random token → minimal metadata.
    /// Wrapped in a `Mutex` because writes are rare (one per
    /// login/logout) and the contention cost is irrelevant.
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
    /// Root content directory; surfaced for "you're looking at
    /// this disk path" UI.
    pub root: PathBuf,
}

impl AppState {
    /// Construct fresh app state with no active sessions.
    #[must_use]
    pub fn new(root: PathBuf, admin_token: String) -> Self {
        Self {
            store: Store::new(root.clone()),
            admin_token: Arc::new(admin_token),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            root,
        }
    }

    /// Constant-time check of `provided` against the admin token.
    #[must_use]
    pub fn verify_token(&self, provided: &str) -> bool {
        ct_eq(provided.as_bytes(), self.admin_token.as_bytes())
    }
}

/// Minimal session metadata. Single-user-per-server today; expanded
/// to per-client identity once the auth surface is genuine.
#[derive(Debug, Clone)]
pub struct Session {
    /// Display name for the logged-in admin. Free-form for v0.
    pub display_name: String,
}

/// Constant-time bytewise equality. Avoids a timing oracle on the
/// admin token. Returns false unconditionally on length mismatch
/// (revealing length is acceptable since the token's length isn't
/// secret-dependent).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ct_eq_same_returns_true() {
        assert!(ct_eq(b"hello", b"hello"));
    }

    #[test]
    fn ct_eq_different_returns_false() {
        assert!(!ct_eq(b"hello", b"world"));
    }

    #[test]
    fn ct_eq_length_mismatch_false() {
        assert!(!ct_eq(b"hello", b"hello!"));
    }
}
