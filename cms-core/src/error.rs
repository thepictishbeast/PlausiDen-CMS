//! Error type for `cms-core`.

use thiserror::Error;

/// Result alias used throughout the crate.
pub type CmsResult<T> = std::result::Result<T, CmsError>;

/// Closed enum of error kinds. Any new variant requires a
/// reviewer's sign-off — error proliferation is the strongest
/// signal that an abstraction is missing.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CmsError {
    /// Underlying I/O failed (read / write / mkdir).
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// Page TOML round-trip failed (parse or serialize).
    #[error("toml: {0}")]
    Toml(String),

    /// Page ID lookup failed.
    #[error("page not found: {0}")]
    PageNotFound(String),

    /// Site lookup failed (no such site directory).
    #[error("site not found: {0}")]
    SiteNotFound(String),

    /// Audit-log signature verification failed.
    #[error("audit log signature invalid at entry {entry}")]
    AuditLogTampered { entry: u64 },

    /// Hash chain broken — entry's `prev_hash` doesn't match the
    /// tail.
    #[error("audit log hash chain broken at entry {entry}")]
    AuditLogChainBroken { entry: u64 },

    /// Encryption / decryption of a draft failed.
    #[error("draft cipher error")]
    DraftCipher,

    /// Editor identity attestation missing — caller didn't
    /// authenticate before invoking a write API.
    #[error("editor identity missing")]
    EditorIdentityMissing,

    /// Validation failed — page shape is well-formed but a
    /// constraint (slug uniqueness, required field, etc.) was
    /// violated.
    #[error("validation: {0}")]
    Validation(String),
}

impl From<toml::de::Error> for CmsError {
    fn from(value: toml::de::Error) -> Self {
        Self::Toml(value.to_string())
    }
}

impl From<toml::ser::Error> for CmsError {
    fn from(value: toml::ser::Error) -> Self {
        Self::Toml(value.to_string())
    }
}
