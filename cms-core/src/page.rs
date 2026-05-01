//! Typed page schema. Pages are the unit of editing in the CMS.
//!
//! Every page belongs to exactly one [`Site`]. A page is composed
//! of an ordered list of [`Section`]s, each section is an ordered
//! list of [`Block`]s, each block has a closed [`BlockKind`] tag
//! and a typed payload. The closed-enum shape keeps ad-hoc HTML
//! out of the content store: a future renderer change applies to
//! every site, no per-site escape hatch.
//!
//! All structures are TOML-friendly so the wire format on disk is
//! human-editable + human-reviewable.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identifier for a page within its site. Generated on
/// creation; the slug can change without breaking inbound links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageId(pub Uuid);

impl PageId {
    /// Fresh random ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PageId {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-site context. The CMS supports N sites in one store; each
/// site has its own pages, media, theme, audit log, and key
/// material.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    /// Short slug used as the on-disk directory name.
    pub slug: String,
    /// Display name shown to admins.
    pub display_name: String,
    /// Default loom theme variant the published pages render with.
    /// Closed enum so a typo doesn't ship to a public site.
    pub theme: ThemeChoice,
}

/// Theme variant a site renders with. Closed enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    /// Loom defaults — light theme.
    LoomLight,
    /// Loom defaults — dark theme.
    LoomDark,
    /// Custom theme overlay applied on top of loom — per-site
    /// brand colours.
    LoomCustom,
}

/// Cryptographically attested editor identity. This crate accepts
/// the identity as opaque and trusts the consuming admin binary to
/// have verified the WebAuthn / hardware-key challenge before
/// invoking any write API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorIdentity {
    /// Editor's display name (recorded in audit log).
    pub display_name: String,
    /// Public-key fingerprint (hex-encoded SHA-256 of the editor's
    /// Ed25519 public key) — used in the audit log to attribute
    /// the entry without storing the raw key inline.
    pub key_fingerprint: String,
}

/// Page lifecycle status. Closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageStatus {
    /// Currently being edited; never served to public.
    Draft,
    /// Reviewed but not yet live; visible only on the preview path.
    Reviewed,
    /// Live; served on the public read path.
    Published,
    /// Soft-deleted; retained in the audit log + restorable.
    Archived,
}

/// One page.
///
/// On disk this serialises to `<site>/pages/<slug>.toml`. The
/// renderer reads the page, walks sections + blocks, and emits
/// HTML by mapping each `BlockKind` to a Loom component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub id: PageId,
    /// URL-safe path component. Must match
    /// `^[a-z0-9][a-z0-9-]{0,40}$` per [`Self::validate_slug`].
    pub slug: String,
    /// Page title (used as `<title>` and as the sitemap entry).
    pub title: String,
    /// SEO description (used as `<meta name="description">`).
    pub description: Option<String>,
    pub status: PageStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Ordered sections.
    pub sections: Vec<Section>,
    /// Free-form metadata (future-proof escape hatch — but every
    /// load-bearing field should graduate into a typed slot).
    #[serde(default)]
    pub meta: BTreeMap<String, String>,
}

impl Page {
    /// Construct a fresh empty draft.
    #[must_use]
    pub fn draft(slug: impl Into<String>, title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: PageId::new(),
            slug: slug.into(),
            title: title.into(),
            description: None,
            status: PageStatus::Draft,
            created_at: now,
            updated_at: now,
            sections: Vec::new(),
            meta: BTreeMap::new(),
        }
    }

    /// Validate slug shape. Slugs become URL path components, so
    /// keep them tight — lowercase + dashes only.
    pub fn validate_slug(slug: &str) -> Result<(), String> {
        if slug.is_empty() || slug.len() > 41 {
            return Err(format!("slug length {} not in 1..=41", slug.len()));
        }
        let first = slug
            .chars()
            .next()
            .ok_or_else(|| "empty slug".to_string())?;
        if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
            return Err(format!("slug must start with [a-z0-9], got {first:?}"));
        }
        for c in slug.chars() {
            if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
                return Err(format!("slug contains invalid char {c:?}"));
            }
        }
        if slug.ends_with('-') {
            return Err("slug must not end with '-'".into());
        }
        if slug.contains("--") {
            return Err("slug must not contain consecutive dashes".into());
        }
        Ok(())
    }
}

/// One section of a page. Sections render as `<section>` bands;
/// every section composes through a typed [`section_theme`] so a
/// page can't accidentally land an off-system colour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    /// Optional anchor id (`#about`).
    pub anchor: Option<String>,
    /// Visual theme for this band — closed enum mirroring
    /// `loom_components::SectionTheme`.
    pub theme: SectionTheme,
    /// Ordered blocks.
    pub blocks: Vec<Block>,
}

/// Mirror of `loom_components::SectionTheme`. Closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionTheme {
    Light,
    Muted,
    Dark,
    Tinted,
}

/// One typed block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub kind: BlockKind,
    /// Field payload. Keys + value types are determined by
    /// [`BlockKind`] — the `validate` pass enforces shape.
    pub fields: BTreeMap<String, FieldValue>,
}

/// Closed enum of block kinds the renderer knows how to render.
/// Adding a kind is a doctrine review — don't widen this without
/// a corresponding loom-components primitive landing first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    /// Single-paragraph hero (`Hero` primitive).
    Hero,
    /// Heading + body prose.
    HeadingBody,
    /// Card grid (3 cards or so).
    CardGrid,
    /// Pull-quote.
    PullQuote,
    /// Linked CTA panel (button on tinted band).
    Cta,
    /// Markdown body for long-form posts.
    Markdown,
    /// Image with caption.
    Image,
    /// Embedded video (no autoplay, no third-party tracking).
    Video,
}

/// Typed value of a single field in a block. Closed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldValue {
    Text(String),
    Number(i64),
    Bool(bool),
    Url(String),
    List(Vec<FieldValue>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_valid_shapes_pass() {
        for s in ["home", "about-us", "post-2026-04-30", "x", "9-lives"] {
            assert!(Page::validate_slug(s).is_ok(), "should accept {s:?}");
        }
    }

    #[test]
    fn slug_invalid_shapes_fail() {
        for bad in ["", "Home", "with space", "trailing-", "-leading", "café"] {
            assert!(
                Page::validate_slug(bad).is_err(),
                "should reject {bad:?}",
            );
        }
    }

    #[test]
    fn page_round_trips_through_toml() {
        let p = Page::draft("hello", "Hello, world.");
        let s = toml::to_string_pretty(&p).expect("serialize");
        let r: Page = toml::from_str(&s).expect("deserialize");
        assert_eq!(p.id, r.id);
        assert_eq!(p.slug, r.slug);
        assert_eq!(p.title, r.title);
        assert!(matches!(r.status, PageStatus::Draft));
    }

    #[test]
    fn page_with_sections_round_trips() {
        let mut p = Page::draft("with-sections", "With sections");
        let mut hero_fields = BTreeMap::new();
        hero_fields.insert("eyebrow".into(), FieldValue::Text("Eyebrow".into()));
        hero_fields.insert("headline".into(), FieldValue::Text("Headline".into()));
        p.sections.push(Section {
            anchor: None,
            theme: SectionTheme::Muted,
            blocks: vec![Block {
                kind: BlockKind::Hero,
                fields: hero_fields,
            }],
        });
        let s = toml::to_string_pretty(&p).unwrap();
        let r: Page = toml::from_str(&s).unwrap();
        assert_eq!(r.sections.len(), 1);
        assert_eq!(r.sections[0].blocks.len(), 1);
        assert!(matches!(r.sections[0].blocks[0].kind, BlockKind::Hero));
    }

    #[test]
    fn page_id_unique_per_call() {
        let a = PageId::new();
        let b = PageId::new();
        assert_ne!(a, b);
    }
}
