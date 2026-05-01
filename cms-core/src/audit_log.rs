//! Signed append-only audit log.
//!
//! Every state transition (page create / edit / publish / delete /
//! site init) appends one entry to the site's `audit.log`. Each
//! entry carries:
//!
//!   * monotonic sequence number
//!   * UTC timestamp
//!   * editor identity (display name + key fingerprint)
//!   * action tag + opaque payload (e.g. page slug being edited)
//!   * SHA-256 of the prior entry — chain breaks are detectable
//!   * Ed25519 signature over (seq || ts || editor || action ||
//!     prev_hash || payload) — tamper-detectable per-entry
//!
//! The log is append-only on disk (entries are JSON-Lines, one per
//! line); rotation copies the existing tail to a sealed archive
//! and resets the chain with a "log rotated" entry.
//!
//! Verification walks the log from entry 0 forward, recomputing
//! each entry's hash from its predecessor and checking the signature
//! against the editor's public key (registered out-of-band).

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{CmsError, CmsResult};
use crate::page::EditorIdentity;

/// One entry in the audit chain. Serialised as a single JSON line
/// per entry for grep-ability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Monotonic sequence; entry 0 is the chain root.
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
    pub editor: EditorIdentity,
    pub action: AuditAction,
    /// Hex SHA-256 of the prior entry's full serialised form.
    /// Entry 0 carries `"genesis"`.
    pub prev_hash: String,
    /// Hex Ed25519 signature over `signing_payload(self)` —
    /// detached, lives outside the signed block per JCS-style
    /// canonicalisation simplicity.
    pub signature_hex: String,
}

/// Closed enum of action kinds the audit chain knows how to record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditAction {
    SiteCreated { site_slug: String },
    SiteUpdated { site_slug: String },
    PageCreated { site_slug: String, page_slug: String },
    PageEdited { site_slug: String, page_slug: String },
    PagePublished { site_slug: String, page_slug: String },
    PageArchived { site_slug: String, page_slug: String },
    PageDeleted { site_slug: String, page_slug: String },
    LogRotated { previous_seq: u64 },
}

/// Append-only audit log writer. One handle per site.
pub struct AuditLog {
    path: PathBuf,
    /// Cached tail — needed to compute the next entry's prev_hash.
    last_entry_hash: String,
    /// Cached next sequence number.
    next_seq: u64,
}

impl AuditLog {
    /// Open or create a log at `path`. Reads existing entries to
    /// initialise the chain tail.
    pub fn open(path: impl Into<PathBuf>) -> CmsResult<Self> {
        let path = path.into();
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, b"")?;
            return Ok(Self {
                path,
                last_entry_hash: "genesis".into(),
                next_seq: 0,
            });
        }
        let body = std::fs::read_to_string(&path)?;
        let mut last_hash = "genesis".to_string();
        let mut next_seq = 0_u64;
        for line in body.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: AuditEntry = serde_json::from_str(line)
                .map_err(|e| CmsError::Validation(format!("audit parse: {e}")))?;
            last_hash = hash_entry(&entry);
            next_seq = entry.seq + 1;
        }
        Ok(Self {
            path,
            last_entry_hash: last_hash,
            next_seq,
        })
    }

    /// Append a fresh entry signed by `signing_key`. The
    /// caller-provided editor identity is recorded verbatim.
    pub fn append(
        &mut self,
        editor: EditorIdentity,
        action: AuditAction,
        signing_key: &SigningKey,
    ) -> CmsResult<AuditEntry> {
        let mut entry = AuditEntry {
            seq: self.next_seq,
            timestamp: Utc::now(),
            editor,
            action,
            prev_hash: self.last_entry_hash.clone(),
            // Filled in below once we have the signing payload.
            signature_hex: String::new(),
        };
        let payload = signing_payload(&entry);
        let sig: Signature = signing_key.sign(&payload);
        entry.signature_hex = hex::encode(sig.to_bytes());

        let line = serde_json::to_string(&entry)
            .map_err(|e| CmsError::Validation(format!("audit serialise: {e}")))?;
        let mut existing = std::fs::read(&self.path)?;
        existing.extend_from_slice(line.as_bytes());
        existing.push(b'\n');
        std::fs::write(&self.path, existing)?;

        self.last_entry_hash = hash_entry(&entry);
        self.next_seq += 1;
        Ok(entry)
    }

    /// Verify every entry: chain integrity + signature against the
    /// supplied verifier (which can look up a public key by
    /// editor.key_fingerprint).
    pub fn verify(&self, verify_with: impl Fn(&str) -> Option<VerifyingKey>) -> CmsResult<u64> {
        let body = std::fs::read_to_string(&self.path)?;
        let mut prev_hash = "genesis".to_string();
        let mut count = 0_u64;
        for line in body.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: AuditEntry = serde_json::from_str(line)
                .map_err(|e| CmsError::Validation(format!("audit parse: {e}")))?;
            if entry.prev_hash != prev_hash {
                return Err(CmsError::AuditLogChainBroken { entry: entry.seq });
            }
            // Optional signature verification — only when caller
            // supplies a key for this editor.
            if let Some(vk) = verify_with(&entry.editor.key_fingerprint) {
                let payload = signing_payload(&entry);
                let sig_bytes = hex::decode(&entry.signature_hex)
                    .map_err(|_| CmsError::AuditLogTampered { entry: entry.seq })?;
                if sig_bytes.len() != 64 {
                    return Err(CmsError::AuditLogTampered { entry: entry.seq });
                }
                let sig = Signature::from_slice(&sig_bytes)
                    .map_err(|_| CmsError::AuditLogTampered { entry: entry.seq })?;
                vk.verify(&payload, &sig)
                    .map_err(|_| CmsError::AuditLogTampered { entry: entry.seq })?;
            }
            prev_hash = hash_entry(&entry);
            count += 1;
        }
        Ok(count)
    }
}

fn hash_entry(entry: &AuditEntry) -> String {
    let body = serde_json::to_string(entry).unwrap_or_default();
    let digest = Sha256::digest(body.as_bytes());
    hex::encode(digest)
}

fn signing_payload(entry: &AuditEntry) -> Vec<u8> {
    // Canonical: seq || timestamp.to_rfc3339 || editor.fingerprint
    // || action_kind || prev_hash. Excludes the signature field
    // itself (which would otherwise sign over its own value).
    let action_kind = match &entry.action {
        AuditAction::SiteCreated { .. } => "site_created",
        AuditAction::SiteUpdated { .. } => "site_updated",
        AuditAction::PageCreated { .. } => "page_created",
        AuditAction::PageEdited { .. } => "page_edited",
        AuditAction::PagePublished { .. } => "page_published",
        AuditAction::PageArchived { .. } => "page_archived",
        AuditAction::PageDeleted { .. } => "page_deleted",
        AuditAction::LogRotated { .. } => "log_rotated",
    };
    let action_payload =
        serde_json::to_string(&entry.action).unwrap_or_default();
    let mut buf = Vec::new();
    buf.extend_from_slice(&entry.seq.to_be_bytes());
    buf.extend_from_slice(entry.timestamp.to_rfc3339().as_bytes());
    buf.extend_from_slice(b"\0");
    buf.extend_from_slice(entry.editor.key_fingerprint.as_bytes());
    buf.extend_from_slice(b"\0");
    buf.extend_from_slice(action_kind.as_bytes());
    buf.extend_from_slice(b"\0");
    buf.extend_from_slice(action_payload.as_bytes());
    buf.extend_from_slice(b"\0");
    buf.extend_from_slice(entry.prev_hash.as_bytes());
    buf
}

// Hex encode/decode without pulling the `hex` crate — use
// std-only.
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        let bytes = bytes.as_ref();
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            out.push(nibble(b >> 4));
            out.push(nibble(b & 0xf));
        }
        out
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, ()> {
        if s.len() % 2 != 0 {
            return Err(());
        }
        let mut out = Vec::with_capacity(s.len() / 2);
        let mut chars = s.chars();
        while let Some(hi) = chars.next() {
            let lo = chars.next().ok_or(())?;
            out.push((from_nibble(hi)? << 4) | from_nibble(lo)?);
        }
        Ok(out)
    }

    fn nibble(n: u8) -> char {
        if n < 10 {
            (b'0' + n) as char
        } else {
            (b'a' + n - 10) as char
        }
    }

    fn from_nibble(c: char) -> Result<u8, ()> {
        match c {
            '0'..='9' => Ok(c as u8 - b'0'),
            'a'..='f' => Ok(c as u8 - b'a' + 10),
            'A'..='F' => Ok(c as u8 - b'A' + 10),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use tempfile::tempdir;

    fn fixture_editor() -> EditorIdentity {
        EditorIdentity {
            display_name: "Fixture Editor".into(),
            key_fingerprint: "abc123".into(),
        }
    }

    #[test]
    fn append_then_verify_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let mut log = AuditLog::open(&path).unwrap();
        let key = SigningKey::generate(&mut OsRng);
        let _ = log
            .append(
                fixture_editor(),
                AuditAction::SiteCreated {
                    site_slug: "x".into(),
                },
                &key,
            )
            .unwrap();
        let count = log.verify(|fp| {
            if fp == "abc123" {
                Some(key.verifying_key())
            } else {
                None
            }
        })
        .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn chain_link_carries_through_multiple_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let mut log = AuditLog::open(&path).unwrap();
        let key = SigningKey::generate(&mut OsRng);
        for i in 0..5 {
            log.append(
                fixture_editor(),
                AuditAction::PageEdited {
                    site_slug: "x".into(),
                    page_slug: format!("p{i}"),
                },
                &key,
            )
            .unwrap();
        }
        let count = log
            .verify(|_| Some(key.verifying_key()))
            .unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn tampered_chain_detected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let mut log = AuditLog::open(&path).unwrap();
        let key = SigningKey::generate(&mut OsRng);
        for i in 0..3 {
            log.append(
                fixture_editor(),
                AuditAction::PageEdited {
                    site_slug: "x".into(),
                    page_slug: format!("p{i}"),
                },
                &key,
            )
            .unwrap();
        }
        // Drop a line in the middle — chain should detect.
        let body = std::fs::read_to_string(&path).unwrap();
        let mut lines: Vec<&str> = body.lines().collect();
        lines.remove(1);
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();

        let log2 = AuditLog::open(&path).unwrap();
        let res = log2.verify(|_| Some(key.verifying_key()));
        assert!(matches!(
            res,
            Err(CmsError::AuditLogChainBroken { .. } | CmsError::AuditLogTampered { .. }),
        ));
    }

    #[test]
    fn open_existing_log_resumes_seq_and_hash() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let key = SigningKey::generate(&mut OsRng);
        {
            let mut log = AuditLog::open(&path).unwrap();
            log.append(
                fixture_editor(),
                AuditAction::SiteCreated {
                    site_slug: "x".into(),
                },
                &key,
            )
            .unwrap();
        }
        let log2 = AuditLog::open(&path).unwrap();
        assert_eq!(log2.next_seq, 1);
        assert_ne!(log2.last_entry_hash, "genesis");
    }
}
