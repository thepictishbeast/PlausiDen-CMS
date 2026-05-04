//! `cms-core` — typed content substrate for PlausiDen-namespace
//! marketing sites.
//!
//! Three layers:
//!   1. Typed page schema (this crate, [`page`]).
//!   2. Storage adapter trait + filesystem implementation
//!      ([`storage`]).
//!   3. Signed append-only audit log of every edit ([`audit_log`]).
//!
//! The CMS is local-first by design: every site's content is a
//! directory of TOML files plus a media subdirectory. The
//! filesystem layout is the wire format — admin actions never need
//! a network round-trip, exports are just `tar`s of the site dir.
//!
//! ## Supersociety posture
//!
//! Per the repo charter, every layer is non-optional for the admin
//! surface that ships against this crate:
//!
//! * **Encrypted at rest.** Drafts + unpublished material encrypted
//!   with a per-site key (ChaCha20-Poly1305). Published content is
//!   plaintext by policy toggle so it can be served by a static
//!   binary.
//! * **Signed audit log.** Every state transition (create, edit,
//!   publish, delete) appends one entry signed by the editor's
//!   Ed25519 key. The tail of the chain hashes the previous entry
//!   so tampering is detectable.
//! * **WebAuthn admin.** Out of scope for this crate — the API
//!   here accepts an opaque [`page::EditorIdentity`] and delegates
//!   challenge/response to the consuming admin binary. The
//!   identity must be cryptographically attested before reaching
//!   the API surface.
//! * **Reproducible exports.** [`storage::export_site`] dumps the site
//!   to a deterministic tar of TOML + media, no clock-leaking
//!   timestamps in the archive header.

#![doc(html_no_source)]

pub mod audit_log;
pub mod error;
pub mod page;
pub mod storage;

pub use error::{CmsError, CmsResult};
pub use page::{
    Block, BlockKind, EditorIdentity, FieldValue, Page, PageId, PageStatus, Section, Site,
};
pub use storage::{FsStorage, Storage};
