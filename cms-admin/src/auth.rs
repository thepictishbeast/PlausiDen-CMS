//! Cookie session middleware + helpers.
//!
//! Session cookies are opaque random tokens. The map of token →
//! [`crate::state::Session`] lives in `AppState`; an unauthenticated
//! request gets redirected to `/login` by [`require_auth`].
//!
//! Cookie attributes: `HttpOnly`, `SameSite=Lax`, `Path=/`, and
//! `Secure` when the server is reachable via HTTPS (set the
//! `PLAUSIDEN_CMS_COOKIE_SECURE=1` env var; default off so local
//! HTTP dev still works).

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use rand::RngCore;

use crate::state::{AppState, Session};

/// Name of the session cookie.
pub const COOKIE_NAME: &str = "pdcms_session";

/// Generate a 256-bit random session token, hex-encoded.
#[must_use]
pub fn generate_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Produce the `Set-Cookie` header value for a fresh session.
#[must_use]
pub fn set_cookie(token: &str) -> String {
    let secure = std::env::var("PLAUSIDEN_CMS_COOKIE_SECURE")
        .ok()
        .as_deref()
        == Some("1");
    let mut s = format!("{COOKIE_NAME}={token}; HttpOnly; SameSite=Lax; Path=/");
    if secure {
        s.push_str("; Secure");
    }
    s
}

/// Produce the cookie value that immediately expires the session.
#[must_use]
pub fn clear_cookie() -> String {
    format!("{COOKIE_NAME}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0")
}

/// Read the session token from the request's `Cookie` header.
#[must_use]
pub fn extract_token(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in cookie_header.split(';') {
        let pair = pair.trim();
        if let Some(rest) = pair.strip_prefix(&format!("{COOKIE_NAME}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

/// Look up the session for an authenticated request, or return
/// `None` if the request has no valid session cookie.
#[must_use]
pub fn current_session(headers: &HeaderMap, state: &AppState) -> Option<Session> {
    let token = extract_token(headers)?;
    let map = state.sessions.lock().ok()?;
    map.get(&token).cloned()
}

/// Extractor that returns the [`Session`] or short-circuits the
/// request with a 303 redirect to `/login`.
#[derive(Debug, Clone)]
pub struct AuthSession(pub Session);

impl<S> FromRequestParts<S> for AuthSession
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app: AppState = AppState::from_ref(state);
        match current_session(&parts.headers, &app) {
            Some(s) => Ok(Self(s)),
            None => Err(Redirect::to("/login").into_response()),
        }
    }
}

/// Convenience wrapper for handlers that need to *fail* (not
/// redirect) on missing auth — POST endpoints called from forms
/// where a redirect would lose the form payload.
#[must_use]
pub fn require_auth_or_401(headers: &HeaderMap, state: &AppState) -> Result<Session, StatusCode> {
    current_session(headers, state).ok_or(StatusCode::UNAUTHORIZED)
}
