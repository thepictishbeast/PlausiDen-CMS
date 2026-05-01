//! `plausiden-cms-core` — typed content store for PlausiDen marketing sites.
//!
//! ## Design
//!
//! - Content lives as files in git (`content/<site>/<type>/<slug>.md`).
//! - Each file is markdown body with TOML frontmatter (between `+++`
//!   delimiters) holding typed metadata: title, slug, date, summary,
//!   author, status. The frontmatter is the source of truth for the
//!   content's shape; the markdown body is opaque copy.
//! - Loading a content file validates the frontmatter against a typed
//!   schema. Invalid files refuse to load — a site binary either gets
//!   a known-good document or a typed error, never partial data.
//! - Writes are PRs by default — every change is git-auditable. The
//!   admin UI (when it ships) commits via the same path the CLI uses.
//!
//! ## What ships in this crate today
//!
//! - The `BlogPost` content type — frontmatter schema + markdown body.
//! - `BlogPost::load_from_file` and `Store::list_blog_posts` reads.
//! - `BlogPost::write` — atomic write (tempfile + rename).
//! - Validation: status enum, slug character set, date parseability,
//!   non-empty title/summary, body byte cap.
//! - `render_html` — pulldown-cmark over the body, no JS, no inline
//!   styles (the published HTML still has to satisfy the strict CSP
//!   plausiden-site enforces).
//!
//! ## Not yet
//!
//! - Other content types (page, section, block, media) — added as
//!   sites need them. Schema landing requires a doctrine review.
//! - Admin web UI — separate crate, `plausiden-cms-admin`.
//! - Per-site theming — that's `PlausiDen-Canon`'s lane.
//! - Authentication / authz — admin-side concern, not core.
//! - Encryption at rest for drafts — planned per README.

#![doc(html_no_source)]

pub mod audit;
pub mod blog;
pub mod page;
pub mod store;

pub use audit::{AuditAction, AuditEvent, AuditLog};
pub use blog::{BlogPost, BlogPostFrontmatter, BlogStatus};
pub use page::{Card, CallToAction, Page, PageFrontmatter, PageLayout, PageStatus, Section};
pub use store::{ContentError, Site, Store};
