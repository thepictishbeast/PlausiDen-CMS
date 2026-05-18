//! `cms-pre-publish-audit` — typed pre-publish audit for a
//! [`cms_core::Page`].
//!
//! The CMS admin surfaces these findings in the publish dialog so
//! the operator sees every quality + accessibility + SEO concern
//! BEFORE the page becomes [`cms_core::PageStatus::Published`].
//!
//! Per `feedback_use_forge_for_websites`: this audit is the
//! pre-publish counterpart to Forge's build-time gates. Where
//! Forge inspects the rendered HTML, this crate inspects the
//! typed [`cms_core::Page`] itself — so the operator gets
//! feedback while editing, not after a 30s render cycle.
//!
//! ### Scope
//!
//! Pure typed audit; no IO, no network, no LFI. The Critic seam
//! (LFI integration, per `feedback_lfi_as_core_llm_as_peripheral`)
//! plugs in via [`AuditExtensions::critics`] — but THIS crate
//! ships only the deterministic rule set. LFI critics land in
//! the LFI crate (out of scope for this instance per
//! `lfi-out-of-scope-for-this-instance`).
//!
//! ### Findings shipped
//!
//! Page-level:
//!   * `title.too-short`         strict  title < 4 chars
//!   * `title.too-long`           warn   title > 70 chars (SEO)
//!   * `slug.invalid`             strict slug fails validate_slug
//!   * `description.missing`      warn   no meta description
//!   * `description.too-short`    warn   description < 50 chars
//!   * `description.too-long`     warn   description > 160 chars
//!   * `sections.empty`           strict page has zero sections
//!   * `sections.empty-section`   strict any section has zero blocks
//!   * `block.video.no-third-party` info  per cms-core doctrine
//!                                         (`block_kind = Video`
//!                                         already disallows
//!                                         third-party hosts)
//!
//! Composition-level:
//!   * `composition.no-hero`      info   no Hero block at top
//!   * `composition.no-cta`       info   no CTA anywhere on page
//!   * `composition.headings`     warn   no HeadingBody at all
//!
//! ### Severity
//!
//! Mirrors Forge's gate semantics:
//!   * `Strict` blocks publish (operator sees red banner)
//!   * `Warn` surfaces but doesn't block (operator can override)
//!   * `Info` is advisory only (no banner; appears in expanded
//!     audit details)

#![deny(unsafe_code)]
#![deny(missing_docs)]

use cms_core::page::{BlockKind, Page};
use serde::{Deserialize, Serialize};

/// Severity bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// Strict: blocks publish.
    Strict,
    /// Warn: surfaces but operator can override.
    Warn,
    /// Info: advisory only.
    Info,
}

/// One audit finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AuditFinding {
    /// Severity bucket.
    pub severity: Severity,
    /// Machine-grepable kind id (e.g. `"title.too-short"`).
    pub kind: String,
    /// Human-readable explanation shown in the admin UI.
    pub detail: String,
}

/// Operator-tunable thresholds. Defaults match Google SEO + WCAG
/// guidelines; sites with different concerns override these
/// before invoking the audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AuditThresholds {
    /// Minimum acceptable title length.
    pub title_min: usize,
    /// Maximum recommended title length (Google SEO).
    pub title_max: usize,
    /// Minimum useful meta-description length.
    pub description_min: usize,
    /// Maximum recommended meta-description length (Google SEO).
    pub description_max: usize,
}

impl Default for AuditThresholds {
    fn default() -> Self {
        Self {
            title_min: 4,
            title_max: 70,
            description_min: 50,
            description_max: 160,
        }
    }
}

/// Operator-pluggable extension points. Default empty — built-in
/// audit alone runs.
///
/// Doesn't derive `Debug` or `Clone` — `dyn Critic` is not
/// Debug-bound (would force every critic impl to be Debug too).
#[derive(Default)]
pub struct AuditExtensions<'a> {
    /// Additional rules the operator wants applied. Each takes the
    /// `Page` + emits any findings. Used by sites with custom
    /// quality bars + by the LFI Critic seam (lfi-critic, out of
    /// scope for this instance).
    pub critics: Vec<&'a dyn Critic>,
}

/// A pluggable critic. Implementations are external (LFI Critic,
/// brand-voice critic, accessibility-deep critic, etc.).
pub trait Critic {
    /// Stable kebab-case identifier of the critic.
    fn id(&self) -> &'static str;
    /// Run against a page; emit zero or more findings.
    fn audit(&self, page: &Page) -> Vec<AuditFinding>;
}

/// Run the audit against `page` with default thresholds + no
/// extensions.
pub fn audit_page(page: &Page) -> Vec<AuditFinding> {
    audit_page_with(
        page,
        &AuditThresholds::default(),
        &AuditExtensions::default(),
    )
}

/// Run the audit with explicit thresholds + extensions.
pub fn audit_page_with(
    page: &Page,
    thresholds: &AuditThresholds,
    extensions: &AuditExtensions<'_>,
) -> Vec<AuditFinding> {
    let mut out = Vec::new();

    // ---- title ----
    let title_chars = page.title.chars().count();
    if title_chars < thresholds.title_min {
        out.push(AuditFinding {
            severity: Severity::Strict,
            kind: "title.too-short".into(),
            detail: format!(
                "title is {} chars (< {} minimum); pages without a real title can't be linked or searched",
                title_chars, thresholds.title_min
            ),
        });
    } else if title_chars > thresholds.title_max {
        out.push(AuditFinding {
            severity: Severity::Warn,
            kind: "title.too-long".into(),
            detail: format!(
                "title is {} chars (> {} recommended); will be truncated in search results",
                title_chars, thresholds.title_max
            ),
        });
    }

    // ---- slug ----
    if let Err(e) = Page::validate_slug(&page.slug) {
        out.push(AuditFinding {
            severity: Severity::Strict,
            kind: "slug.invalid".into(),
            detail: format!("slug {:?} invalid: {}", page.slug, e),
        });
    }

    // ---- description ----
    match &page.description {
        None => {
            out.push(AuditFinding {
                severity: Severity::Warn,
                kind: "description.missing".into(),
                detail: "no meta description; search engines + social cards will guess a fallback"
                    .into(),
            });
        }
        Some(d) => {
            let len = d.chars().count();
            if len < thresholds.description_min {
                out.push(AuditFinding {
                    severity: Severity::Warn,
                    kind: "description.too-short".into(),
                    detail: format!(
                        "description is {} chars (< {} useful minimum)",
                        len, thresholds.description_min
                    ),
                });
            } else if len > thresholds.description_max {
                out.push(AuditFinding {
                    severity: Severity::Warn,
                    kind: "description.too-long".into(),
                    detail: format!(
                        "description is {} chars (> {} recommended; will be truncated)",
                        len, thresholds.description_max
                    ),
                });
            }
        }
    }

    // ---- structure ----
    if page.sections.is_empty() {
        out.push(AuditFinding {
            severity: Severity::Strict,
            kind: "sections.empty".into(),
            detail: "page has zero sections; nothing will render".into(),
        });
    } else {
        for (i, section) in page.sections.iter().enumerate() {
            if section.blocks.is_empty() {
                out.push(AuditFinding {
                    severity: Severity::Strict,
                    kind: "sections.empty-section".into(),
                    detail: format!("section {i} has zero blocks; will render an empty band"),
                });
            }
        }
    }

    // ---- composition advisories ----
    let has_hero_at_top = page
        .sections
        .first()
        .map(|s| {
            s.blocks
                .first()
                .map(|b| b.kind == BlockKind::Hero)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if !page.sections.is_empty() && !has_hero_at_top {
        out.push(AuditFinding {
            severity: Severity::Info,
            kind: "composition.no-hero".into(),
            detail:
                "first section doesn't lead with a Hero block; readers land on body content first"
                    .into(),
        });
    }

    let has_cta = page
        .sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .any(|b| b.kind == BlockKind::Cta);
    if !has_cta && !page.sections.is_empty() {
        out.push(AuditFinding {
            severity: Severity::Info,
            kind: "composition.no-cta".into(),
            detail: "page has no Cta block; no clear next action for the reader".into(),
        });
    }

    let has_heading = page
        .sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .any(|b| b.kind == BlockKind::HeadingBody);
    if !has_heading && page.sections.len() > 1 {
        out.push(AuditFinding {
            severity: Severity::Warn,
            kind: "composition.headings".into(),
            detail:
                "multi-section page has no HeadingBody blocks; readers can't scan the structure"
                    .into(),
        });
    }

    // ---- pluggable critics ----
    for critic in &extensions.critics {
        out.extend(critic.audit(page));
    }

    out
}

/// True iff the audit has any [`Severity::Strict`] finding.
pub fn blocks_publish(findings: &[AuditFinding]) -> bool {
    findings.iter().any(|f| f.severity == Severity::Strict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cms_core::page::{Block, BlockKind, FieldValue, Page, Section, SectionTheme};
    use std::collections::BTreeMap;

    fn mk_page(slug: &str, title: &str, description: Option<&str>) -> Page {
        let mut p = Page::draft(slug, title);
        p.description = description.map(String::from);
        p
    }

    fn block(kind: BlockKind) -> Block {
        Block {
            kind,
            fields: BTreeMap::from([("text".into(), FieldValue::Text("x".into()))]),
        }
    }

    fn section(blocks: Vec<Block>) -> Section {
        Section {
            anchor: None,
            theme: SectionTheme::Light,
            blocks,
        }
    }

    #[test]
    fn empty_draft_emits_strict_findings() {
        let p = mk_page("home", "Hi", None);
        let f = audit_page(&p);
        // title too short + description missing + sections empty.
        assert!(f.iter().any(|x| x.kind == "title.too-short"));
        assert!(f.iter().any(|x| x.kind == "description.missing"));
        assert!(f.iter().any(|x| x.kind == "sections.empty"));
        assert!(blocks_publish(&f));
    }

    #[test]
    fn fully_filled_page_passes_strict_gate() {
        let mut p = mk_page(
            "about",
            "About PlausiDen",
            Some(
                "A description that's long enough to convey what the page is about — over 50 characters.",
            ),
        );
        p.sections = vec![
            section(vec![block(BlockKind::Hero)]),
            section(vec![block(BlockKind::HeadingBody)]),
            section(vec![block(BlockKind::Cta)]),
        ];
        let f = audit_page(&p);
        assert!(!blocks_publish(&f), "findings: {:?}", f);
    }

    #[test]
    fn long_title_warns() {
        let p = mk_page("home", &"X".repeat(80), Some(&"y".repeat(80)));
        let f = audit_page(&p);
        assert!(f.iter().any(|x| x.kind == "title.too-long"));
    }

    #[test]
    fn invalid_slug_is_strict() {
        let mut p = mk_page("home", "OK title", Some(&"y".repeat(80)));
        p.slug = "Bad Slug!".into();
        let f = audit_page(&p);
        assert!(f.iter().any(|x| x.kind == "slug.invalid"));
        assert!(blocks_publish(&f));
    }

    #[test]
    fn description_too_short_warns() {
        let p = mk_page("home", "OK", Some("short"));
        let f = audit_page(&p);
        assert!(f.iter().any(|x| x.kind == "description.too-short"));
    }

    #[test]
    fn description_too_long_warns() {
        let p = mk_page("home", "OK", Some(&"y".repeat(200)));
        let f = audit_page(&p);
        assert!(f.iter().any(|x| x.kind == "description.too-long"));
    }

    #[test]
    fn empty_section_is_strict() {
        let mut p = mk_page("home", "Title here", Some(&"y".repeat(80)));
        p.sections = vec![section(vec![])];
        let f = audit_page(&p);
        assert!(f.iter().any(|x| x.kind == "sections.empty-section"));
        assert!(blocks_publish(&f));
    }

    #[test]
    fn missing_hero_at_top_is_info_only() {
        let mut p = mk_page("home", "Title here", Some(&"y".repeat(80)));
        p.sections = vec![section(vec![block(BlockKind::HeadingBody)])];
        let f = audit_page(&p);
        let has_hero_info = f.iter().any(|x| x.kind == "composition.no-hero");
        assert!(has_hero_info);
        assert!(!blocks_publish(&f));
    }

    #[test]
    fn missing_cta_is_info_only() {
        let mut p = mk_page("home", "Title here", Some(&"y".repeat(80)));
        p.sections = vec![section(vec![block(BlockKind::HeadingBody)])];
        let f = audit_page(&p);
        assert!(f.iter().any(|x| x.kind == "composition.no-cta"));
    }

    #[test]
    fn pluggable_critic_runs() {
        struct AlwaysWarn;
        impl Critic for AlwaysWarn {
            fn id(&self) -> &'static str {
                "always-warn"
            }
            fn audit(&self, _: &Page) -> Vec<AuditFinding> {
                vec![AuditFinding {
                    severity: Severity::Warn,
                    kind: "critic.always-warn".into(),
                    detail: "demo".into(),
                }]
            }
        }
        let mut p = mk_page("home", "Title here", Some(&"y".repeat(80)));
        p.sections = vec![section(vec![block(BlockKind::Hero)])];
        let critic = AlwaysWarn;
        let extensions = AuditExtensions {
            critics: vec![&critic],
        };
        let f = audit_page_with(&p, &AuditThresholds::default(), &extensions);
        assert!(f.iter().any(|x| x.kind == "critic.always-warn"));
    }

    #[test]
    fn audit_finding_serde_round_trips() {
        let f = AuditFinding {
            severity: Severity::Strict,
            kind: "title.too-short".into(),
            detail: "x".into(),
        };
        let s = serde_json::to_string(&f).unwrap();
        let back: AuditFinding = serde_json::from_str(&s).unwrap();
        assert_eq!(f, back);
    }

    #[test]
    fn thresholds_serde_round_trips() {
        let t = AuditThresholds::default();
        let s = serde_json::to_string(&t).unwrap();
        let back: AuditThresholds = serde_json::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn blocks_publish_predicate_matches_severity() {
        assert!(!blocks_publish(&[]));
        assert!(blocks_publish(&[AuditFinding {
            severity: Severity::Strict,
            kind: "x".into(),
            detail: "x".into(),
        }]));
        assert!(!blocks_publish(&[AuditFinding {
            severity: Severity::Warn,
            kind: "x".into(),
            detail: "x".into(),
        }]));
    }
}
