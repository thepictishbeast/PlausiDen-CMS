//! `cms-admin-auth` — typed WebAuthn admin authentication +
//! cookie-session + per-tenant SQLite paths.
//!
//! Per `PLATFORM_ROADMAP.md` §5 + the
//! `plausiden_design_premium` framing: the CMS admin login does
//! NOT use passwords. Operators register a passkey
//! (FIDO2 / WebAuthn) on first sign-in and authenticate via
//! platform-attested public-key challenge thereafter. No
//! password to phish, no SMS code to SIM-swap, no recovery email
//! to reset.
//!
//! ### What this crate ships
//!
//! Typed surface only — the **server** runtime (axum / actix /
//! warp / whatever the admin host uses) implements the
//! [`AdminAuthBackend`] trait. This crate is the cross-server
//! contract: same types whether the admin host is built as a
//! per-tenant subprocess (FullyIsolated tenancy tier per
//! `tenancy-core`) or a shared server with row-scoped data
//! (DataIsolated tier).
//!
//!   * [`PasskeyCredentialId`] — typed credential id newtype
//!   * [`PasskeyRecord`]      — stored credential metadata
//!   * [`AdminSession`]       — cookie-session payload
//!   * [`SessionCookie`]      — typed cookie attributes
//!   * [`TenantSqlitePath`]   — typed per-tenant SQLite location
//!   * [`AdminAuthBackend`]   — trait the server impl satisfies
//!
//! Per `tenancy-core::TenantBoundary`: every type in this crate
//! that touches tenant-owned state carries a TenantId or a
//! TenantSqlitePath constructed from one — there's no path to
//! authenticate across tenants by accident.

#![deny(unsafe_code)]
#![deny(missing_docs)]

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Typed WebAuthn credential identifier. Opaque byte string from
/// the authenticator + relying party assertion; never to be
/// confused with the user's display name or email.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PasskeyCredentialId(String);

impl PasskeyCredentialId {
    /// Construct from a base64url-encoded credential id. Validates
    /// shape (base64url alphabet, 1..=512 chars).
    pub fn parse(s: impl AsRef<str>) -> Result<Self, AuthError> {
        let s = s.as_ref();
        if s.is_empty() || s.len() > 512 {
            return Err(AuthError::InvalidCredentialId("length out of range".into()));
        }
        for c in s.chars() {
            if !(c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Err(AuthError::InvalidCredentialId(format!(
                    "char {c:?} not in base64url alphabet"
                )));
            }
        }
        Ok(Self(s.to_string()))
    }

    /// Raw view.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stored metadata for one registered passkey. The actual public
/// key + signature counter live in the backend's storage — this
/// type is the metadata operators see in the admin UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PasskeyRecord {
    /// Credential ID.
    pub credential_id: PasskeyCredentialId,
    /// Tenant the credential belongs to.
    pub tenant_id: String,
    /// Operator-supplied display name (e.g. `"MacBook Touch ID"`).
    pub label: String,
    /// Signature counter — incremented per authentication.
    /// Backend MUST reject assertions whose counter is ≤ stored.
    pub signature_counter: u32,
    /// Authenticator AAGUID hex string if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aaguid: Option<String>,
    /// ISO-8601 timestamp of registration.
    pub registered_at: chrono::DateTime<chrono::Utc>,
    /// ISO-8601 timestamp of last successful use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Cookie-session payload — what gets serialized into the encrypted
/// session cookie. Never includes the credential public key or
/// signature counter (those live in the backend).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AdminSession {
    /// Random opaque session id (UUID v4).
    pub session_id: uuid::Uuid,
    /// Tenant the session is scoped to.
    pub tenant_id: String,
    /// Credential id the session was minted with.
    pub credential_id: PasskeyCredentialId,
    /// ISO-8601 timestamp the session was minted.
    pub minted_at: chrono::DateTime<chrono::Utc>,
    /// ISO-8601 timestamp after which the session is invalid.
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl AdminSession {
    /// True iff `now` is at or past `expires_at`.
    pub fn is_expired(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        now >= self.expires_at
    }
}

/// Typed Set-Cookie attributes for the session cookie. Backend
/// emits these on the response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SessionCookie {
    /// Cookie name (default `"plausiden_admin"`).
    pub name: String,
    /// Encrypted + signed cookie value.
    pub value: String,
    /// Cookie path scope. Default `"/admin"`.
    pub path: String,
    /// Cookie domain. None == host-only cookie.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Whether to set the Secure flag (HTTPS-only).
    pub secure: bool,
    /// Whether to set the HttpOnly flag (no JS access).
    pub http_only: bool,
    /// SameSite policy.
    pub same_site: SameSite,
    /// Max-Age in seconds.
    pub max_age_secs: u32,
}

impl SessionCookie {
    /// Build a SessionCookie with the platform's safe defaults:
    /// HttpOnly, Secure, SameSite=Strict, path=/admin, 12h Max-Age.
    pub fn safe_default(value: impl Into<String>) -> Self {
        Self {
            name: "plausiden_admin".into(),
            value: value.into(),
            path: "/admin".into(),
            domain: None,
            secure: true,
            http_only: true,
            same_site: SameSite::Strict,
            max_age_secs: 12 * 60 * 60,
        }
    }

    /// Emit the cookie as a Set-Cookie header value.
    pub fn to_set_cookie_header(&self) -> String {
        let mut s = format!(
            "{}={}; Path={}; Max-Age={}",
            self.name, self.value, self.path, self.max_age_secs
        );
        if let Some(domain) = &self.domain {
            s.push_str(&format!("; Domain={domain}"));
        }
        if self.secure {
            s.push_str("; Secure");
        }
        if self.http_only {
            s.push_str("; HttpOnly");
        }
        s.push_str(&format!("; SameSite={}", self.same_site.token()));
        s
    }
}

/// SameSite cookie policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SameSite {
    /// SameSite=Strict — never sent on cross-site requests.
    Strict,
    /// SameSite=Lax — sent on top-level navigations only.
    Lax,
    /// SameSite=None — sent everywhere. Requires Secure.
    None,
}

impl SameSite {
    /// Header token form (`"Strict"`, `"Lax"`, `"None"`).
    pub fn token(&self) -> &'static str {
        match self {
            Self::Strict => "Strict",
            Self::Lax => "Lax",
            Self::None => "None",
        }
    }
}

/// Typed per-tenant SQLite path.
///
/// Constructed only via [`Self::for_tenant`] — guarantees the
/// resulting path lives under the platform-configured tenants
/// root, with the file named `{tenant-id}.db`. No way to point
/// at another tenant's file by accident.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TenantSqlitePath(PathBuf);

impl TenantSqlitePath {
    /// Build the per-tenant path under `root` for `tenant_id`.
    /// Validates that `tenant_id` is kebab-case (lowercase
    /// alphanumeric + hyphens, 1..=64 chars) so it can't be a
    /// path-traversal payload.
    pub fn for_tenant(root: &Path, tenant_id: &str) -> Result<Self, AuthError> {
        if tenant_id.is_empty() || tenant_id.len() > 64 {
            return Err(AuthError::InvalidTenantId("length".into()));
        }
        for c in tenant_id.chars() {
            if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
                return Err(AuthError::InvalidTenantId(format!(
                    "char {c:?} not in [a-z0-9-]"
                )));
            }
        }
        if tenant_id.starts_with('-') || tenant_id.ends_with('-') || tenant_id.contains("--") {
            return Err(AuthError::InvalidTenantId(
                "leading/trailing/consecutive hyphen".into(),
            ));
        }
        Ok(Self(root.join(format!("{tenant_id}.db"))))
    }

    /// Raw filesystem path.
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// Trait the server admin host implements.
///
/// Async-runtime-agnostic; the server crate wraps each method in
/// its preferred runtime + transport. The platform CLI plus
/// integration tests use a NullBackend implementation for
/// hermetic flow validation.
pub trait AdminAuthBackend {
    /// Backend identifier (`"sqlite"`, `"null"`, etc.).
    fn id(&self) -> &'static str;

    /// Begin a passkey registration ceremony for a new credential.
    /// Returns the challenge bytes the browser feeds to
    /// `navigator.credentials.create()`.
    fn begin_registration(
        &mut self,
        tenant_id: &str,
        label: &str,
    ) -> Result<RegistrationChallenge, AuthError>;

    /// Finish the registration: verify attestation, store the
    /// resulting PasskeyRecord.
    fn finish_registration(
        &mut self,
        ceremony_id: uuid::Uuid,
        client_data_json: &str,
        attestation_object_b64u: &str,
    ) -> Result<PasskeyRecord, AuthError>;

    /// Begin a sign-in ceremony. Returns the challenge bytes for
    /// `navigator.credentials.get()`.
    fn begin_authentication(
        &mut self,
        tenant_id: &str,
    ) -> Result<AuthenticationChallenge, AuthError>;

    /// Finish the sign-in. Verifies assertion + counter, mints a
    /// session.
    fn finish_authentication(
        &mut self,
        ceremony_id: uuid::Uuid,
        credential_id: &PasskeyCredentialId,
        client_data_json: &str,
        authenticator_data_b64u: &str,
        signature_b64u: &str,
    ) -> Result<AdminSession, AuthError>;

    /// Validate a session ID (from a cookie). Returns the session
    /// when fresh + not revoked, else error.
    fn validate_session(&self, session_id: uuid::Uuid) -> Result<AdminSession, AuthError>;

    /// Revoke a session (sign out).
    fn revoke_session(&mut self, session_id: uuid::Uuid) -> Result<(), AuthError>;

    // ─── Credential management ───────────────────────────────────────
    //
    // Three CRUD methods over registered passkeys. Without these, a
    // tenant operator can only ever ADD credentials — never see what's
    // enrolled, never remove a lost device, never relabel an old key.
    // All three are tenant-scoped and MUST reject cross-tenant access
    // — backend impls do this by including the tenant_id in every
    // storage-layer WHERE clause.
    //
    // Default `Err(AuthError::NotImplemented)` impls let the existing
    // null / sqlite backends opt in incrementally without breaking
    // compile. Real backends override.

    /// List all PasskeyRecord rows for one tenant. Order is backend
    /// choice (typically `registered_at ASC` for stable UI). Returns
    /// an empty vec when the tenant has zero credentials enrolled.
    fn list_passkeys(&self, _tenant_id: &str) -> Result<Vec<PasskeyRecord>, AuthError> {
        Err(AuthError::NotImplemented("list_passkeys"))
    }

    /// Rename one passkey's operator-visible label. Cryptographic
    /// state (credential id, signature counter, attestation chain)
    /// is untouched — this is a pure display-label update. Backend
    /// MUST verify (tenant_id, credential_id) belongs to this tenant
    /// before applying the update.
    ///
    /// `new_label`: 1..=64 chars; backend SHOULD strip control /
    /// zero-width / RTL-override characters before storing.
    fn rename_passkey(
        &mut self,
        _tenant_id: &str,
        _credential_id: &PasskeyCredentialId,
        _new_label: &str,
    ) -> Result<PasskeyRecord, AuthError> {
        Err(AuthError::NotImplemented("rename_passkey"))
    }

    /// Permanently delete one passkey. Subsequent
    /// `finish_authentication` attempts with that credential_id MUST
    /// fail with `AuthError::Verification("unknown credential")`.
    /// Backend SHOULD also revoke any active sessions issued from
    /// this credential (or document a contrary policy).
    fn delete_passkey(
        &mut self,
        _tenant_id: &str,
        _credential_id: &PasskeyCredentialId,
    ) -> Result<(), AuthError> {
        Err(AuthError::NotImplemented("delete_passkey"))
    }
}

/// Challenge issued at the start of a registration ceremony.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RegistrationChallenge {
    /// Opaque ceremony id the client returns at finish time.
    pub ceremony_id: uuid::Uuid,
    /// Base64url-encoded challenge bytes.
    pub challenge_b64u: String,
    /// Relying-party id (effective domain).
    pub rp_id: String,
    /// User-handle base64url (server-chosen opaque blob; NOT the
    /// email or display name).
    pub user_handle_b64u: String,
    /// Display name to show in the platform authenticator UI.
    pub user_display_name: String,
}

/// Challenge issued at the start of an authentication ceremony.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AuthenticationChallenge {
    /// Opaque ceremony id.
    pub ceremony_id: uuid::Uuid,
    /// Base64url-encoded challenge bytes.
    pub challenge_b64u: String,
    /// Relying-party id.
    pub rp_id: String,
    /// Allowed credential IDs (the tenant's registered passkeys).
    pub allow_credentials: Vec<PasskeyCredentialId>,
}

/// Typed errors at the auth boundary.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Credential id failed shape validation.
    #[error("invalid credential id: {0}")]
    InvalidCredentialId(String),
    /// Tenant id failed shape validation.
    #[error("invalid tenant id: {0}")]
    InvalidTenantId(String),
    /// Session was expired or revoked.
    #[error("session expired or revoked: {0}")]
    SessionInvalid(String),
    /// Signature counter went backwards — possible cloned authenticator.
    #[error("signature counter regressed for {0}")]
    CounterRegression(String),
    /// Attestation or assertion failed cryptographic verification.
    #[error("attestation/assertion verification failed: {0}")]
    Verification(String),
    /// Underlying backend storage error (the impl wraps its own
    /// error type in this).
    #[error("storage: {0}")]
    Storage(String),
    /// Backend does not implement this operation yet. Surfaces the
    /// trait method name so the caller can decide whether to fall
    /// back or surface a friendly "feature unavailable" message.
    #[error("operation not implemented in this backend: {0}")]
    NotImplemented(&'static str),
    /// Label fails validation (length, control chars, etc.). Used by
    /// `rename_passkey` impls so callers get a typed reason rather
    /// than a generic Storage error.
    #[error("invalid label: {0}")]
    InvalidLabel(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn credential_id_validates_base64url_shape() {
        assert!(PasskeyCredentialId::parse("abcDEF123_-").is_ok());
        assert!(PasskeyCredentialId::parse("").is_err());
        assert!(PasskeyCredentialId::parse("has space").is_err());
        assert!(PasskeyCredentialId::parse("has.dot").is_err());
        assert!(PasskeyCredentialId::parse(&"a".repeat(513)).is_err());
    }

    #[test]
    fn session_is_expired_at_or_after_expires_at() {
        let minted = chrono::DateTime::parse_from_rfc3339("2026-05-17T22:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let expires = chrono::DateTime::parse_from_rfc3339("2026-05-17T23:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let s = AdminSession {
            session_id: uuid::Uuid::new_v4(),
            tenant_id: "acme".into(),
            credential_id: PasskeyCredentialId::parse("abc").unwrap(),
            minted_at: minted,
            expires_at: expires,
        };
        let before = chrono::DateTime::parse_from_rfc3339("2026-05-17T22:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let at = expires;
        let after = chrono::DateTime::parse_from_rfc3339("2026-05-17T23:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert!(!s.is_expired(before));
        assert!(s.is_expired(at));
        assert!(s.is_expired(after));
    }

    #[test]
    fn safe_default_cookie_is_locked_down() {
        let c = SessionCookie::safe_default("payload");
        assert_eq!(c.name, "plausiden_admin");
        assert_eq!(c.path, "/admin");
        assert!(c.secure);
        assert!(c.http_only);
        assert_eq!(c.same_site, SameSite::Strict);
        assert_eq!(c.max_age_secs, 12 * 60 * 60);
    }

    #[test]
    fn to_set_cookie_header_emits_expected_form() {
        let c = SessionCookie::safe_default("xyz");
        let h = c.to_set_cookie_header();
        assert!(h.starts_with("plausiden_admin=xyz; Path=/admin"));
        assert!(h.contains("; Max-Age=43200"));
        assert!(h.contains("; Secure"));
        assert!(h.contains("; HttpOnly"));
        assert!(h.contains("; SameSite=Strict"));
    }

    #[test]
    fn tenant_path_refuses_traversal_and_bad_id() {
        let root = PathBuf::from("/var/lib/cms/tenants");
        assert!(TenantSqlitePath::for_tenant(&root, "acme").is_ok());
        assert!(TenantSqlitePath::for_tenant(&root, "acme-corp").is_ok());
        assert!(TenantSqlitePath::for_tenant(&root, "").is_err());
        assert!(TenantSqlitePath::for_tenant(&root, "Acme").is_err());
        assert!(TenantSqlitePath::for_tenant(&root, "acme_corp").is_err());
        assert!(TenantSqlitePath::for_tenant(&root, "../etc/passwd").is_err());
        assert!(TenantSqlitePath::for_tenant(&root, "acme/sub").is_err());
        assert!(TenantSqlitePath::for_tenant(&root, "acme.db").is_err());
        assert!(TenantSqlitePath::for_tenant(&root, "-acme").is_err());
        assert!(TenantSqlitePath::for_tenant(&root, "acme-").is_err());
        assert!(TenantSqlitePath::for_tenant(&root, "acme--corp").is_err());
    }

    #[test]
    fn tenant_path_files_under_root_with_expected_name() {
        let root = PathBuf::from("/var/lib/cms/tenants");
        let p = TenantSqlitePath::for_tenant(&root, "acme").unwrap();
        assert_eq!(p.as_path(), PathBuf::from("/var/lib/cms/tenants/acme.db"));
    }

    #[test]
    fn passkey_record_serde_round_trips() {
        let r = PasskeyRecord {
            credential_id: PasskeyCredentialId::parse("abcDEF").unwrap(),
            tenant_id: "acme".into(),
            label: "MacBook Touch ID".into(),
            signature_counter: 42,
            aaguid: Some("ee882879721c491e8dd6b3df6c45b1a3".into()),
            registered_at: chrono::Utc::now(),
            last_used_at: None,
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: PasskeyRecord = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn passkey_record_rejects_unknown_field() {
        let bad = r#"{"credential-id":"x","tenant-id":"t","label":"l","signature-counter":0,"registered-at":"2026-05-17T22:00:00Z","ahem":1}"#;
        let r: Result<PasskeyRecord, _> = serde_json::from_str(bad);
        assert!(r.is_err());
    }

    #[test]
    fn same_site_tokens_match_spec() {
        assert_eq!(SameSite::Strict.token(), "Strict");
        assert_eq!(SameSite::Lax.token(), "Lax");
        assert_eq!(SameSite::None.token(), "None");
    }

    // ─── Credential CRUD trait defaults (task #62 follow-on) ─────────
    //
    // The newly-added `list_passkeys` / `rename_passkey` /
    // `delete_passkey` methods ship with `Err(NotImplemented)`
    // defaults so existing backends (null, sqlite stub) compile
    // unchanged. Real backends override these. The tests below pin
    // the default behaviour so a future refactor can't accidentally
    // turn a NotImplemented into a silent success.

    /// Minimal AdminAuthBackend impl that satisfies only the
    /// pre-existing required methods. Used to prove the new CRUD
    /// methods fall through to their `Err(NotImplemented)` defaults
    /// without forcing every backend to implement them on day one.
    struct StubBackend;

    impl AdminAuthBackend for StubBackend {
        fn id(&self) -> &'static str {
            "stub"
        }
        fn begin_registration(
            &mut self,
            _tenant_id: &str,
            _label: &str,
        ) -> Result<RegistrationChallenge, AuthError> {
            Err(AuthError::Storage("stub".into()))
        }
        fn finish_registration(
            &mut self,
            _ceremony_id: uuid::Uuid,
            _client_data_json: &str,
            _attestation_object_b64u: &str,
        ) -> Result<PasskeyRecord, AuthError> {
            Err(AuthError::Storage("stub".into()))
        }
        fn begin_authentication(
            &mut self,
            _tenant_id: &str,
        ) -> Result<AuthenticationChallenge, AuthError> {
            Err(AuthError::Storage("stub".into()))
        }
        fn finish_authentication(
            &mut self,
            _ceremony_id: uuid::Uuid,
            _credential_id: &PasskeyCredentialId,
            _client_data_json: &str,
            _authenticator_data_b64u: &str,
            _signature_b64u: &str,
        ) -> Result<AdminSession, AuthError> {
            Err(AuthError::Storage("stub".into()))
        }
        fn validate_session(&self, _session_id: uuid::Uuid) -> Result<AdminSession, AuthError> {
            Err(AuthError::Storage("stub".into()))
        }
        fn revoke_session(&mut self, _session_id: uuid::Uuid) -> Result<(), AuthError> {
            Err(AuthError::Storage("stub".into()))
        }
    }

    #[test]
    fn list_passkeys_default_returns_not_implemented() {
        let b = StubBackend;
        let r = b.list_passkeys("acme");
        match r {
            Err(AuthError::NotImplemented(m)) => assert_eq!(m, "list_passkeys"),
            other => panic!("expected NotImplemented, got {other:?}"),
        }
    }

    #[test]
    fn rename_passkey_default_returns_not_implemented() {
        let mut b = StubBackend;
        let cid = PasskeyCredentialId::parse("abc").unwrap();
        let r = b.rename_passkey("acme", &cid, "Office YubiKey");
        match r {
            Err(AuthError::NotImplemented(m)) => assert_eq!(m, "rename_passkey"),
            other => panic!("expected NotImplemented, got {other:?}"),
        }
    }

    #[test]
    fn delete_passkey_default_returns_not_implemented() {
        let mut b = StubBackend;
        let cid = PasskeyCredentialId::parse("abc").unwrap();
        let r = b.delete_passkey("acme", &cid);
        match r {
            Err(AuthError::NotImplemented(m)) => assert_eq!(m, "delete_passkey"),
            other => panic!("expected NotImplemented, got {other:?}"),
        }
    }

    #[test]
    fn auth_error_invalid_label_displays_reason() {
        let e = AuthError::InvalidLabel("empty after sanitization".into());
        assert!(e.to_string().contains("invalid label"));
        assert!(e.to_string().contains("empty after sanitization"));
    }

    #[test]
    fn auth_error_not_implemented_displays_method_name() {
        let e = AuthError::NotImplemented("rename_passkey");
        assert!(e.to_string().contains("rename_passkey"));
    }
}
