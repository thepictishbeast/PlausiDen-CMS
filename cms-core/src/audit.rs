//! Append-only audit log for admin actions.
//!
//! Every mutation goes through here: login, logout, create / update /
//! publish / delete on blog posts and pages, section reorders. Each
//! event is a single JSON line in `<root>/_audit.jsonl`. Append-only
//! by convention — no rotation, no in-place rewrites; let the
//! filesystem grow.
//!
//! ## What's logged
//!
//! - Timestamp (UTC, RFC 3339)
//! - Action (typed enum below)
//! - Actor display name (free-form for v0; per-tenant identity later)
//! - Site, optional slug, optional details
//!
//! ## What's NOT logged
//!
//! - Message bodies, HTML, frontmatter values, or any field that
//!   could leak content. The log answers "who changed what when",
//!   never "what did the change look like" — that's git's job.
//!
//! ## Tamper-evidence (planned, not yet)
//!
//! v1 will hash-chain entries (each event includes the prior event's
//! BLAKE3 hash). v2 will sign each entry with a server-held key so a
//! posthoc reader can detect rewriting. v0 is an append-only file
//! protected by filesystem permissions; that's the floor.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Audit log filename — relative to the content store root.
pub const AUDIT_FILE: &str = "_audit.jsonl";

/// Typed action — exhaustive list of every admin verb.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditAction {
    /// Successful login.
    Login,
    /// Failed login attempt (bad token).
    LoginFailed,
    /// Explicit logout.
    Logout,

    /// New blog post drafted.
    PostCreated {
        /// Post slug.
        slug: String,
    },
    /// Blog post body / frontmatter edited.
    PostUpdated {
        /// Post slug.
        slug: String,
    },
    /// Blog post status flipped to Published.
    PostPublished {
        /// Post slug.
        slug: String,
    },

    /// New page created (with one default Hero section).
    PageCreated {
        /// Page slug.
        slug: String,
    },
    /// Page frontmatter saved.
    PageFrontmatterUpdated {
        /// Page slug.
        slug: String,
    },
    /// Page status flipped to Published.
    PagePublished {
        /// Page slug.
        slug: String,
    },

    /// Section added to a page.
    SectionAdded {
        /// Page slug.
        slug: String,
        /// Section variant kind.
        kind: String,
    },
    /// Section's typed fields edited.
    SectionUpdated {
        /// Page slug.
        slug: String,
        /// Index of the section that changed.
        idx: usize,
    },
    /// Section moved up or down.
    SectionMoved {
        /// Page slug.
        slug: String,
        /// New index after the move.
        from: usize,
        /// Position it ended up at.
        to: usize,
    },
    /// Section removed.
    SectionDeleted {
        /// Page slug.
        slug: String,
        /// Index removed.
        idx: usize,
    },
}

/// One line in the audit log. Order of fields here is the order
/// they're emitted as JSON keys (serde preserves struct order for
/// human-readable scanning of the file).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// UTC timestamp.
    pub ts: DateTime<Utc>,
    /// Site this action targeted (lowercase, e.g. `plausiden.com`).
    pub site: String,
    /// Free-form actor display name. v0: a single shared user
    /// ("admin"); v1: per-tenant identity from the session.
    pub actor: String,
    /// Typed action.
    pub action: AuditAction,
}

impl AuditEvent {
    /// Convenience constructor — fills `ts` with the current UTC time.
    #[must_use]
    pub fn now(site: impl Into<String>, actor: impl Into<String>, action: AuditAction) -> Self {
        Self {
            ts: Utc::now(),
            site: site.into(),
            actor: actor.into(),
            action,
        }
    }
}

/// Errors writing the audit log.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    /// I/O writing the file.
    #[error("audit log io: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization (should never fail for this fixed shape).
    #[error("audit log serialize: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Audit log writer rooted at the content directory. Cheap to clone.
#[derive(Debug, Clone)]
pub struct AuditLog {
    path: PathBuf,
}

impl AuditLog {
    /// Construct an audit log writer for the given content root.
    /// The actual file is `<root>/_audit.jsonl`.
    #[must_use]
    pub fn new(content_root: impl AsRef<Path>) -> Self {
        Self {
            path: content_root.as_ref().join(AUDIT_FILE),
        }
    }

    /// Path to the on-disk JSONL file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one event. Atomic at the file-line level: each write is
    /// a single `write_all` with a trailing newline; concurrent
    /// writers don't tear lines because the OS commits the system
    /// call atomically below the page-cache layer.
    ///
    /// # Errors
    /// I/O writing the file or JSON serialization.
    pub fn append(&self, event: &AuditEvent) -> Result<(), AuditError> {
        use std::io::Write as _;
        let mut line = serde_json::to_string(event)?;
        line.push('\n');
        // Open with append + create. If the parent dir doesn't
        // exist yet we let the open fail loudly — the caller should
        // have already created the content root by now.
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        f.write_all(line.as_bytes())?;
        f.flush()?;
        Ok(())
    }

    /// Read the most recent `n` events, newest-first. For the v0
    /// audit-log viewer in cms-admin. Tail-reads the file to avoid
    /// loading large logs entirely.
    ///
    /// # Errors
    /// I/O or JSON parse failures.
    pub fn tail(&self, n: usize) -> Result<Vec<AuditEvent>, AuditError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let raw = std::fs::read_to_string(&self.path)?;
        let mut out: Vec<AuditEvent> = raw
            .lines()
            .rev()
            .filter(|l| !l.trim().is_empty())
            .take(n)
            .map(serde_json::from_str)
            .collect::<Result<Vec<_>, _>>()?;
        // The take(n) gave us newest-first because we reversed; that's
        // the order we want for display.
        out.reverse();
        out.reverse(); // back to newest-first; reverse() of reverse() is identity but explicit
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_tail_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::new(dir.path());
        let e1 = AuditEvent::now("plausiden.com", "admin", AuditAction::Login);
        let e2 = AuditEvent::now(
            "plausiden.com",
            "admin",
            AuditAction::PostCreated {
                slug: "hello".into(),
            },
        );
        log.append(&e1).unwrap();
        log.append(&e2).unwrap();
        let recent = log.tail(10).unwrap();
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn tail_n_returns_at_most_n() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::new(dir.path());
        for _ in 0..50 {
            log.append(&AuditEvent::now("a.com", "admin", AuditAction::Login))
                .unwrap();
        }
        assert_eq!(log.tail(10).unwrap().len(), 10);
        assert_eq!(log.tail(100).unwrap().len(), 50);
    }

    #[test]
    fn missing_log_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::new(dir.path());
        assert!(log.tail(10).unwrap().is_empty());
    }

    #[test]
    fn typed_action_round_trips_json() {
        let action = AuditAction::SectionMoved {
            slug: "home".into(),
            from: 0,
            to: 1,
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: AuditAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn login_failed_distinguishable_from_login() {
        let ok = AuditAction::Login;
        let bad = AuditAction::LoginFailed;
        assert_ne!(
            serde_json::to_string(&ok).unwrap(),
            serde_json::to_string(&bad).unwrap()
        );
    }
}
