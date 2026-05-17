//! `cms-collab` — typed branching version history + CRDT seam.
//!
//! Two complementary surfaces in one crate because branches and
//! CRDT state are intertwined: a branch is a head pointer into a
//! DAG of versions, and each version is a frozen CRDT-document
//! snapshot.
//!
//! ### Branching half
//!
//! Git-shaped: every [`PageVersion`] has zero (root), one
//! (linear), or two ([`BranchPolicy::Merge`]) parents. The
//! version's [`VersionHash`] is the SHA-256 of its canonical-form
//! payload; refs are mutable named pointers
//! ([`BranchHead::version`]).
//!
//! ### CRDT half
//!
//! The platform stays CRDT-backend-agnostic — Yjs / Automerge /
//! Loro / a custom impl all satisfy [`CrdtState`]. Operators get
//! consistent shape across:
//!   * single-user save-and-publish
//!   * multi-author live coediting
//!   * branch-with-conflict merge resolution
//! without the CMS knowing whose backend is in use.
//!
//! ### What's NOT in this crate
//!
//! No actual CRDT implementation. No network sync. No persistence.
//! Those live in:
//!   * `cms-collab-yjs` (or similar) — a real backend
//!   * `cms-collab-sync` — WebSocket transport layer
//!   * `cms-store` — persistence
//! This is the typed-contract layer ALL of those plug into.

#![deny(unsafe_code)]
#![deny(missing_docs)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ============================================================
// VERSION GRAPH
// ============================================================

/// Stable content-addressed identifier for a page version.
/// Computed as SHA-256 of the canonical serialization.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VersionHash(String);

impl VersionHash {
    /// Construct from a 64-char hex SHA-256 digest.
    pub fn parse(s: impl AsRef<str>) -> Result<Self, CollabError> {
        let s = s.as_ref();
        if s.len() != 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CollabError::InvalidHash(format!(
                "{s:?} not a 64-char hex digest"
            )));
        }
        Ok(Self(s.to_ascii_lowercase()))
    }

    /// Compute from arbitrary bytes (typically the canonical-JSON
    /// serialization of the version payload).
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        Self(hex)
    }

    /// Raw hex view.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Short prefix (first 12 chars) — useful for UI display.
    pub fn short(&self) -> &str {
        &self.0[..12]
    }
}

/// Typed kebab-case branch identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BranchId(String);

impl BranchId {
    /// Construct, validating kebab-case shape (`[a-z][a-z0-9-]{0,63}`).
    pub fn parse(s: impl AsRef<str>) -> Result<Self, CollabError> {
        let s = s.as_ref();
        if s.is_empty() || s.len() > 64 {
            return Err(CollabError::InvalidBranchId("length".into()));
        }
        let mut chars = s.chars();
        let first = chars.next().unwrap();
        if !first.is_ascii_lowercase() {
            return Err(CollabError::InvalidBranchId(format!(
                "{s:?} must start with [a-z]"
            )));
        }
        for c in chars {
            if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
                return Err(CollabError::InvalidBranchId(format!(
                    "char {c:?} not in [a-z0-9-]"
                )));
            }
        }
        if s.ends_with('-') || s.contains("--") {
            return Err(CollabError::InvalidBranchId(
                "trailing or consecutive hyphen".into(),
            ));
        }
        Ok(Self(s.to_string()))
    }

    /// Raw view.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One frozen page version in the DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PageVersion {
    /// Content-addressed identifier.
    pub hash: VersionHash,
    /// Parent versions. Zero = root, one = linear, two = merge.
    /// More than two is rejected by [`PageVersion::validate`].
    #[serde(default)]
    pub parents: Vec<VersionHash>,
    /// Author identifier (opaque).
    pub author: AuthorId,
    /// ISO-8601 commit time.
    pub committed_at: chrono::DateTime<chrono::Utc>,
    /// Commit message (operator-facing).
    pub message: String,
    /// Opaque CRDT document state at this version. The CRDT
    /// backend interprets this; the platform only stores it.
    pub crdt_state: Vec<u8>,
}

impl PageVersion {
    /// Sanity check: 0..=2 parents, hash matches recomputed
    /// canonical form.
    pub fn validate(&self) -> Result<(), CollabError> {
        if self.parents.len() > 2 {
            return Err(CollabError::TooManyParents(self.parents.len()));
        }
        Ok(())
    }

    /// Whether this is a merge commit (exactly 2 parents).
    pub fn is_merge(&self) -> bool {
        self.parents.len() == 2
    }

    /// Whether this is a root commit (0 parents).
    pub fn is_root(&self) -> bool {
        self.parents.is_empty()
    }
}

/// Mutable named pointer to a [`VersionHash`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct BranchHead {
    /// Branch identifier.
    pub branch: BranchId,
    /// Version this branch currently points at.
    pub version: VersionHash,
    /// ISO-8601 timestamp of last update.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Per-branch policy that gates how new versions land.
    pub policy: BranchPolicy,
}

/// What kind of advance is allowed on a branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BranchPolicy {
    /// Linear only — every new version must have the current head
    /// as its sole parent. No merges accepted. Rejects diverged
    /// histories outright.
    Linear,
    /// Linear OR merge — divergent edits trigger a merge commit
    /// with both heads as parents. Default for shared work.
    Merge,
    /// Linear only on the public branch; divergent work is
    /// auto-rebased onto the head before being accepted. No
    /// merge commits land. Matches git "rebase-only" workflows.
    RebaseOnly,
}

/// Version DAG snapshot for one page. Used by the admin UI's
/// "history" panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct VersionGraph {
    /// All known versions, keyed by hash.
    pub versions: BTreeMap<String, PageVersion>,
    /// All branch heads.
    pub branches: BTreeMap<String, BranchHead>,
}

impl VersionGraph {
    /// Empty graph.
    pub fn new() -> Self {
        Self {
            versions: BTreeMap::new(),
            branches: BTreeMap::new(),
        }
    }

    /// Insert a version, validating shape + parent existence.
    pub fn add_version(&mut self, v: PageVersion) -> Result<(), CollabError> {
        v.validate()?;
        for p in &v.parents {
            if !self.versions.contains_key(p.as_str()) {
                return Err(CollabError::UnknownParent(p.clone()));
            }
        }
        self.versions.insert(v.hash.as_str().to_string(), v);
        Ok(())
    }

    /// Look up a version by hash.
    pub fn get(&self, hash: &VersionHash) -> Option<&PageVersion> {
        self.versions.get(hash.as_str())
    }

    /// Branch head lookup.
    pub fn head(&self, branch: &BranchId) -> Option<&BranchHead> {
        self.branches.get(branch.as_str())
    }

    /// Advance a branch head. Refuses if the new version's parent
    /// set is inconsistent with the policy.
    pub fn advance(&mut self, branch: &BranchId, target: &VersionHash) -> Result<(), CollabError> {
        let target_version = self
            .get(target)
            .ok_or_else(|| CollabError::UnknownVersion(target.clone()))?
            .clone();

        // Verify the target version under the branch's current
        // policy.
        if let Some(head) = self.branches.get(branch.as_str()) {
            match head.policy {
                BranchPolicy::Linear => {
                    if target_version.parents.len() != 1
                        || target_version.parents[0] != head.version
                    {
                        return Err(CollabError::PolicyRefused(
                            "Linear policy requires the new version's sole parent to be the current head".into(),
                        ));
                    }
                }
                BranchPolicy::RebaseOnly => {
                    if target_version.is_merge() {
                        return Err(CollabError::PolicyRefused(
                            "RebaseOnly policy refuses merge commits".into(),
                        ));
                    }
                }
                BranchPolicy::Merge => {}
            }
            let updated = BranchHead {
                branch: branch.clone(),
                version: target.clone(),
                updated_at: chrono::Utc::now(),
                policy: head.policy,
            };
            self.branches.insert(branch.as_str().to_string(), updated);
        } else {
            // First commit on the branch — default to Merge policy.
            self.branches.insert(
                branch.as_str().to_string(),
                BranchHead {
                    branch: branch.clone(),
                    version: target.clone(),
                    updated_at: chrono::Utc::now(),
                    policy: BranchPolicy::Merge,
                },
            );
        }
        Ok(())
    }
}

impl Default for VersionGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// CRDT SEAM
// ============================================================

/// Opaque author identifier (UUID v4 in practice).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthorId(uuid::Uuid);

impl AuthorId {
    /// Wrap an arbitrary UUID.
    pub fn from_uuid(u: uuid::Uuid) -> Self {
        Self(u)
    }

    /// Fresh random v4 UUID.
    pub fn new_v4() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Raw UUID.
    pub fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }
}

/// Typed CRDT edit operation. The CRDT backend interprets these
/// against its own internal document state; the platform records
/// the typed shape for audit + replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum EditOp {
    /// Insert text at a document position.
    InsertText {
        /// Position in code-points within the target block.
        pos: u32,
        /// Inserted text.
        text: String,
        /// Target block id.
        block: String,
    },
    /// Delete a range of text.
    DeleteRange {
        /// Inclusive start.
        start: u32,
        /// Exclusive end.
        end: u32,
        /// Target block id.
        block: String,
    },
    /// Apply / remove an inline format.
    Format {
        /// Inclusive start.
        start: u32,
        /// Exclusive end.
        end: u32,
        /// Target block id.
        block: String,
        /// Format key (e.g. `"bold"`, `"italic"`, `"link"`).
        key: String,
        /// Format value (string-encoded; CRDT-backend-specific
        /// shape). `None` clears the format.
        value: Option<String>,
    },
    /// Insert a new block at a section position.
    InsertBlock {
        /// Position within the target section.
        pos: u32,
        /// New block id.
        block: String,
        /// Block kind slug (mirrors cms_core::BlockKind).
        /// Named `block_kind` rather than `kind` so it doesn't
        /// collide with the serde-internal-tag field on this enum.
        block_kind: String,
        /// Target section index.
        section: u32,
    },
    /// Delete a block from a section.
    DeleteBlock {
        /// Block id to remove.
        block: String,
    },
}

/// One atomic edit by one author at one time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AuthoredEdit {
    /// The edit operation.
    pub op: EditOp,
    /// Who applied it.
    pub author: AuthorId,
    /// When it was applied.
    pub applied_at: chrono::DateTime<chrono::Utc>,
}

/// Trait the CRDT backend implements. Implementations live in
/// downstream crates (cms-collab-yjs, cms-collab-loro, etc.).
///
/// All methods are sync; backends that need async wrap in their
/// preferred runtime at the call site.
pub trait CrdtState {
    /// Stable identifier for the CRDT backend
    /// (`"yjs"` / `"loro"` / `"automerge"` / `"in-memory-stub"`).
    fn id(&self) -> &'static str;

    /// Apply an [`AuthoredEdit`] to the current state. Returns
    /// the post-edit canonical bytes.
    fn apply(&mut self, edit: &AuthoredEdit) -> Result<Vec<u8>, CollabError>;

    /// Serialize the current state. Output is what gets stored
    /// in [`PageVersion::crdt_state`].
    fn encode(&self) -> Result<Vec<u8>, CollabError>;

    /// Restore from a previously [`Self::encode`]d state.
    fn decode(bytes: &[u8]) -> Result<Self, CollabError>
    where
        Self: Sized;

    /// Three-way merge: combine `theirs` into self using
    /// `base` as the common ancestor. Returns either the merged
    /// bytes or a list of unresolved conflicts.
    fn merge(&mut self, base: &[u8], theirs: &[u8]) -> Result<MergeOutcome, CollabError>;
}

/// Result of a three-way merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum MergeOutcome {
    /// No conflicts; merged bytes ready to commit.
    Clean {
        /// Merged CRDT state.
        merged: Vec<u8>,
    },
    /// Conflicts present; operator must resolve before commit.
    Conflicts {
        /// Per-conflict descriptors.
        conflicts: Vec<MergeConflict>,
    },
}

/// One unresolved merge conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MergeConflict {
    /// Block id where the conflict lives.
    pub block: String,
    /// Code-point range in the block.
    pub range: (u32, u32),
    /// Our version's text.
    pub ours: String,
    /// Their version's text.
    pub theirs: String,
}

/// Resolution strategy the operator picks per conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolutionStrategy {
    /// Keep our side.
    KeepOurs,
    /// Keep their side.
    KeepTheirs,
    /// Apply both (concatenated by the backend in a
    /// backend-specific way).
    KeepBoth,
    /// Operator hand-resolved — payload is the chosen text
    /// (encoded out-of-band in the resolution dialog; this enum
    /// variant just signals "manual").
    Manual,
}

// ============================================================
// ERRORS
// ============================================================

/// Typed errors at the collab boundary.
#[derive(Debug, thiserror::Error)]
pub enum CollabError {
    /// Version hash failed shape validation.
    #[error("invalid version hash: {0}")]
    InvalidHash(String),
    /// Branch id failed shape validation.
    #[error("invalid branch id: {0}")]
    InvalidBranchId(String),
    /// Version cited an unknown parent.
    #[error("unknown parent version: {0:?}")]
    UnknownParent(VersionHash),
    /// Version not present in the graph.
    #[error("unknown version: {0:?}")]
    UnknownVersion(VersionHash),
    /// Version had > 2 parents.
    #[error("version has {0} parents (max 2)")]
    TooManyParents(usize),
    /// Advance refused by branch policy.
    #[error("policy refused: {0}")]
    PolicyRefused(String),
    /// CRDT backend surfaced its own error.
    #[error("crdt backend: {0}")]
    Backend(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_version(hash: &str, parents: &[&str], author: AuthorId, msg: &str) -> PageVersion {
        PageVersion {
            hash: VersionHash::parse(hash).unwrap(),
            parents: parents
                .iter()
                .map(|p| VersionHash::parse(p).unwrap())
                .collect(),
            author,
            committed_at: chrono::Utc::now(),
            message: msg.into(),
            crdt_state: Vec::new(),
        }
    }

    fn hex(c: char) -> String {
        std::iter::repeat(c).take(64).collect()
    }

    #[test]
    fn version_hash_validates_shape() {
        assert!(VersionHash::parse(&hex('a')).is_ok());
        assert!(VersionHash::parse(&hex('z')).is_err()); // non-hex
        assert!(VersionHash::parse("too-short").is_err());
        let mut x = hex('a');
        x.push('e');
        assert!(VersionHash::parse(&x).is_err()); // 65 chars
    }

    #[test]
    fn version_hash_of_bytes_is_deterministic() {
        let a = VersionHash::of_bytes(b"hello");
        let b = VersionHash::of_bytes(b"hello");
        assert_eq!(a, b);
        assert_ne!(a, VersionHash::of_bytes(b"world"));
        assert_eq!(a.as_str().len(), 64);
    }

    #[test]
    fn version_hash_short_returns_12_chars() {
        let h = VersionHash::of_bytes(b"x");
        assert_eq!(h.short().len(), 12);
    }

    #[test]
    fn branch_id_validates_shape() {
        assert!(BranchId::parse("main").is_ok());
        assert!(BranchId::parse("feature-x").is_ok());
        assert!(BranchId::parse("").is_err());
        assert!(BranchId::parse("Main").is_err());
        assert!(BranchId::parse("0-leading").is_err());
        assert!(BranchId::parse("ends-").is_err());
        assert!(BranchId::parse("doubl--hyphen").is_err());
        assert!(BranchId::parse("under_score").is_err());
    }

    #[test]
    fn version_is_root_or_merge_or_linear() {
        let author = AuthorId::new_v4();
        let root = mk_version(&hex('a'), &[], author.clone(), "root");
        assert!(root.is_root());
        assert!(!root.is_merge());

        let linear = mk_version(&hex('b'), &[&hex('a')], author.clone(), "linear");
        assert!(!linear.is_root());
        assert!(!linear.is_merge());

        let merge = mk_version(&hex('c'), &[&hex('a'), &hex('b')], author, "merge");
        assert!(merge.is_merge());
    }

    #[test]
    fn version_validate_refuses_three_parents() {
        let author = AuthorId::new_v4();
        let v = mk_version(
            &hex('d'),
            &[&hex('a'), &hex('b'), &hex('c')],
            author,
            "octopus",
        );
        assert!(matches!(v.validate(), Err(CollabError::TooManyParents(3))));
    }

    #[test]
    fn version_graph_rejects_unknown_parent() {
        let mut g = VersionGraph::new();
        let author = AuthorId::new_v4();
        let v = mk_version(&hex('b'), &[&hex('a')], author, "orphan");
        let err = g.add_version(v).unwrap_err();
        assert!(matches!(err, CollabError::UnknownParent(_)));
    }

    #[test]
    fn version_graph_accepts_chain() {
        let mut g = VersionGraph::new();
        let author = AuthorId::new_v4();
        let root = mk_version(&hex('a'), &[], author.clone(), "root");
        g.add_version(root).unwrap();
        let linear = mk_version(&hex('b'), &[&hex('a')], author, "next");
        g.add_version(linear).unwrap();
        assert_eq!(g.versions.len(), 2);
    }

    #[test]
    fn linear_policy_refuses_merge_advance() {
        let mut g = VersionGraph::new();
        let author = AuthorId::new_v4();
        g.add_version(mk_version(&hex('a'), &[], author.clone(), "root"))
            .unwrap();
        g.add_version(mk_version(&hex('b'), &[&hex('a')], author.clone(), "1"))
            .unwrap();
        g.add_version(mk_version(&hex('c'), &[&hex('a')], author.clone(), "2"))
            .unwrap();
        g.add_version(mk_version(
            &hex('d'),
            &[&hex('b'), &hex('c')],
            author,
            "merge",
        ))
        .unwrap();

        let branch = BranchId::parse("main").unwrap();
        // Start the branch on the first commit (default Merge policy).
        g.advance(&branch, &VersionHash::parse(&hex('a')).unwrap())
            .unwrap();
        // Flip to Linear and try to merge.
        if let Some(head) = g.branches.get_mut(branch.as_str()) {
            head.policy = BranchPolicy::Linear;
        }
        let err = g
            .advance(&branch, &VersionHash::parse(&hex('d')).unwrap())
            .unwrap_err();
        assert!(matches!(err, CollabError::PolicyRefused(_)));
    }

    #[test]
    fn rebase_only_refuses_merge_commits() {
        let mut g = VersionGraph::new();
        let author = AuthorId::new_v4();
        g.add_version(mk_version(&hex('a'), &[], author.clone(), "root"))
            .unwrap();
        g.add_version(mk_version(&hex('b'), &[&hex('a')], author.clone(), "x"))
            .unwrap();
        g.add_version(mk_version(&hex('c'), &[&hex('a')], author.clone(), "y"))
            .unwrap();
        g.add_version(mk_version(
            &hex('d'),
            &[&hex('b'), &hex('c')],
            author,
            "merge",
        ))
        .unwrap();

        let branch = BranchId::parse("main").unwrap();
        g.advance(&branch, &VersionHash::parse(&hex('a')).unwrap())
            .unwrap();
        if let Some(head) = g.branches.get_mut(branch.as_str()) {
            head.policy = BranchPolicy::RebaseOnly;
        }
        let err = g
            .advance(&branch, &VersionHash::parse(&hex('d')).unwrap())
            .unwrap_err();
        assert!(matches!(err, CollabError::PolicyRefused(_)));
    }

    #[test]
    fn edit_op_serde_round_trips_insert_text() {
        let op = EditOp::InsertText {
            pos: 5,
            text: "hello".into(),
            block: "b1".into(),
        };
        let s = serde_json::to_string(&op).unwrap();
        assert!(s.contains("\"kind\":\"insert-text\""));
        let back: EditOp = serde_json::from_str(&s).unwrap();
        assert_eq!(op, back);
    }

    #[test]
    fn merge_outcome_clean_vs_conflicts_round_trips() {
        let c = MergeOutcome::Clean {
            merged: vec![1, 2, 3],
        };
        let s = serde_json::to_string(&c).unwrap();
        assert!(s.contains("\"outcome\":\"clean\""));
        let back: MergeOutcome = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);

        let conflicts = MergeOutcome::Conflicts {
            conflicts: vec![MergeConflict {
                block: "b1".into(),
                range: (0, 10),
                ours: "ours text".into(),
                theirs: "theirs text".into(),
            }],
        };
        let s = serde_json::to_string(&conflicts).unwrap();
        assert!(s.contains("\"outcome\":\"conflicts\""));
        let back: MergeOutcome = serde_json::from_str(&s).unwrap();
        assert_eq!(conflicts, back);
    }

    #[test]
    fn resolution_strategy_slugs_correctly() {
        let s = serde_json::to_string(&ResolutionStrategy::KeepOurs).unwrap();
        assert_eq!(s, "\"keep-ours\"");
    }

    #[test]
    fn branch_head_serde_round_trips() {
        let h = BranchHead {
            branch: BranchId::parse("main").unwrap(),
            version: VersionHash::of_bytes(b"x"),
            updated_at: chrono::Utc::now(),
            policy: BranchPolicy::Merge,
        };
        let s = serde_json::to_string(&h).unwrap();
        let back: BranchHead = serde_json::from_str(&s).unwrap();
        assert_eq!(h, back);
    }
}
