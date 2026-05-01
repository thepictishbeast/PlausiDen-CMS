//! `Page` content type — typed-section composition.
//!
//! Where `BlogPost` is mostly prose-in-markdown, a `Page` is a
//! sequence of *typed* sections — Hero, Prose, Cards, CTA — each
//! with its own schema. The doctrine: a non-technical editor can't
//! invent novel HTML; they pick a Section variant from the
//! existing palette and fill its typed fields. New visual shapes
//! become new Section variants in a doctrine PR.
//!
//! ## Storage
//!
//! Pages live at `<root>/<site>/pages/<slug>.toml` (note: `.toml`,
//! not `.md` — pages are structured, not prose).
//!
//! Example:
//!
//! ```toml
//! [front]
//! title = "Privacy-first IT for the modern enterprise"
//! slug = "home"
//! summary = "..."
//! status = "published"
//! layout = "landing"
//! updated_at = "2026-05-01"
//!
//! [[sections]]
//! kind = "hero"
//! eyebrow = "Engineered for confidentiality"
//! headline = "We don't read your data."
//! subhead = "..."
//!
//! [[sections.cta]]
//! label = "Talk to us"
//! href = "/contact"
//!
//! [[sections]]
//! kind = "prose"
//! markdown = "..."
//! ```

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Editorial status — same shape as blog posts (intentional).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageStatus {
    /// Editor-visible only.
    Draft,
    /// Surfaced by the public site.
    Published,
}

/// Layout hint passed to the renderer.
///
/// `Default` is the standard centred-content layout. `Wide` is for
/// content with a hero band that breaks out of the column. `Landing`
/// is full-bleed with no max-width on the hero. The renderer (in
/// each site binary) picks a template based on this value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageLayout {
    /// Standard column.
    Default,
    /// Wide: hero breaks out, body still column.
    Wide,
    /// Full-bleed landing page.
    Landing,
}

/// Page-level metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageFrontmatter {
    /// Page title.
    pub title: String,
    /// URL slug (same rules as `BlogPost::slug`).
    pub slug: String,
    /// One-sentence summary for `<meta name="description">`.
    pub summary: String,
    /// Editorial status.
    pub status: PageStatus,
    /// Layout hint.
    #[serde(default = "default_layout")]
    pub layout: PageLayout,
    /// Last update date — informs the renderer's "last-modified"
    /// metadata. Pages don't have a "publication date" the way blog
    /// posts do, so we track update instead.
    pub updated_at: NaiveDate,
    /// Optional navigation order. `None` = not in the primary nav.
    /// Lower numbers come first.
    #[serde(default)]
    pub nav_order: Option<u32>,
}

const fn default_layout() -> PageLayout {
    PageLayout::Default
}

/// A call-to-action button. Constrained shape — label + href, no
/// color overrides, no inline styles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallToAction {
    /// Visible button text.
    pub label: String,
    /// Link target. Internal paths only validated by the renderer.
    pub href: String,
}

/// One card in a `Section::Cards` strip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    /// Card heading.
    pub heading: String,
    /// Card body text.
    pub body: String,
    /// Optional CTA.
    #[serde(default)]
    pub cta: Option<CallToAction>,
}

/// Typed section — every variant is enumerated, no escape hatch.
///
/// New visual shapes land as new variants here, not as raw markup.
/// The renderer in each site binary matches on the variant; an
/// unknown variant is a compile-time error, not a silent fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Section {
    /// Hero band: eyebrow + big headline + subhead + optional CTA.
    Hero {
        /// Small caption above the headline.
        #[serde(default)]
        eyebrow: Option<String>,
        /// Main headline.
        headline: String,
        /// Subhead one-liner.
        subhead: String,
        /// Optional CTA.
        #[serde(default)]
        cta: Option<CallToAction>,
    },
    /// Free-flowing markdown prose. The escape hatch — but the
    /// markdown is still rendered through pulldown-cmark, no raw
    /// HTML embedded inside is whitelisted at the public-site CSP.
    Prose {
        /// Markdown body.
        markdown: String,
    },
    /// A row of cards.
    Cards {
        /// Optional heading above the row.
        #[serde(default)]
        heading: Option<String>,
        /// The cards. ≤6 enforced at validate.
        items: Vec<Card>,
    },
    /// Standalone CTA band — single big call to action.
    CtaBand {
        /// Headline above the button.
        headline: String,
        /// Button.
        cta: CallToAction,
    },
}

/// A loaded page: validated frontmatter + typed sections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    /// Frontmatter.
    pub front: PageFrontmatter,
    /// Ordered sections.
    pub sections: Vec<Section>,
}

const MAX_TITLE_LEN: usize = 200;
const MAX_SUMMARY_LEN: usize = 200;
const MAX_SECTIONS: usize = 16;
const MAX_CARDS_PER_ROW: usize = 6;

/// Page-load errors.
#[derive(Debug, thiserror::Error)]
pub enum PageError {
    /// I/O reading the file.
    #[error("read {path}: {source}")]
    Io {
        /// Path that failed.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// TOML parse failure.
    #[error("parse {path}: {source}")]
    Toml {
        /// Path that failed.
        path: String,
        /// Underlying parse error.
        #[source]
        source: toml::de::Error,
    },
    /// Validation rule rejected the page.
    #[error("validation in {path}: {reason}")]
    Invalid {
        /// Path that failed.
        path: String,
        /// Human-readable reason.
        reason: String,
    },
}

impl Page {
    /// Load + validate a page from disk.
    ///
    /// # Errors
    /// I/O, parse, or validation failures.
    pub fn load_from_file(path: &Path) -> Result<Self, PageError> {
        let raw = std::fs::read_to_string(path).map_err(|e| PageError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Self::parse(&raw, path)
    }

    /// Parse a page from a string.
    ///
    /// # Errors
    /// Parse or validation failures.
    pub fn parse(raw: &str, path: &Path) -> Result<Self, PageError> {
        let page: Self = toml::from_str(raw).map_err(|e| PageError::Toml {
            path: path.display().to_string(),
            source: e,
        })?;
        page.validate(path)?;
        Ok(page)
    }

    /// Apply typed validation rules.
    ///
    /// # Errors
    /// [`PageError::Invalid`] with a reason.
    pub fn validate(&self, path: &Path) -> Result<(), PageError> {
        let here = || path.display().to_string();
        let reject = |reason: String| PageError::Invalid {
            path: here(),
            reason,
        };
        if self.front.title.trim().is_empty() {
            return Err(reject("title is empty".into()));
        }
        if self.front.title.len() > MAX_TITLE_LEN {
            return Err(reject(format!("title >{MAX_TITLE_LEN} chars")));
        }
        if self.front.summary.trim().is_empty() {
            return Err(reject("summary is empty".into()));
        }
        if self.front.summary.len() > MAX_SUMMARY_LEN {
            return Err(reject(format!("summary >{MAX_SUMMARY_LEN} chars")));
        }
        if !crate::blog::is_valid_slug(&self.front.slug) {
            return Err(reject("slug invalid (lowercase ASCII + dashes only)".into()));
        }
        if self.sections.is_empty() {
            return Err(reject("page has no sections".into()));
        }
        if self.sections.len() > MAX_SECTIONS {
            return Err(reject(format!("more than {MAX_SECTIONS} sections")));
        }
        for (i, section) in self.sections.iter().enumerate() {
            section.validate(i, &reject)?;
        }
        Ok(())
    }

    /// Serialize back to TOML for writing.
    #[must_use]
    pub fn to_file_format(&self) -> String {
        toml::to_string_pretty(self).expect("typed page always serializes")
    }

    /// Atomic write.
    ///
    /// # Errors
    /// I/O.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        let tmp = parent.join(format!(
            ".{}.tmp",
            path.file_name()
                .map_or_else(|| "page".to_string(), |s| s.to_string_lossy().to_string())
        ));
        std::fs::write(&tmp, self.to_file_format())?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

impl Section {
    fn validate(
        &self,
        idx: usize,
        reject: &impl Fn(String) -> PageError,
    ) -> Result<(), PageError> {
        match self {
            Self::Hero { headline, subhead, .. } => {
                if headline.trim().is_empty() {
                    return Err(reject(format!("section {idx} hero headline is empty")));
                }
                if subhead.trim().is_empty() {
                    return Err(reject(format!("section {idx} hero subhead is empty")));
                }
            }
            Self::Prose { markdown } => {
                if markdown.trim().is_empty() {
                    return Err(reject(format!("section {idx} prose markdown is empty")));
                }
            }
            Self::Cards { items, .. } => {
                if items.is_empty() {
                    return Err(reject(format!("section {idx} cards has no items")));
                }
                if items.len() > MAX_CARDS_PER_ROW {
                    return Err(reject(format!(
                        "section {idx} cards has >{MAX_CARDS_PER_ROW} items"
                    )));
                }
                for (j, c) in items.iter().enumerate() {
                    if c.heading.trim().is_empty() {
                        return Err(reject(format!(
                            "section {idx} card {j} heading is empty"
                        )));
                    }
                    if c.body.trim().is_empty() {
                        return Err(reject(format!("section {idx} card {j} body is empty")));
                    }
                }
            }
            Self::CtaBand { headline, cta } => {
                if headline.trim().is_empty() {
                    return Err(reject(format!("section {idx} ctaband headline is empty")));
                }
                if cta.label.trim().is_empty() || cta.href.trim().is_empty() {
                    return Err(reject(format!("section {idx} ctaband cta missing label/href")));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx() -> Page {
        Page {
            front: PageFrontmatter {
                title: "Home".into(),
                slug: "home".into(),
                summary: "A page summary line for SEO.".into(),
                status: PageStatus::Published,
                layout: PageLayout::Landing,
                updated_at: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
                nav_order: Some(1),
            },
            sections: vec![
                Section::Hero {
                    eyebrow: Some("Engineered for confidentiality".into()),
                    headline: "We don't read your data.".into(),
                    subhead: "Comprehensive IT for the modern enterprise.".into(),
                    cta: Some(CallToAction {
                        label: "Talk to us".into(),
                        href: "/contact".into(),
                    }),
                },
                Section::Prose {
                    markdown: "Some prose section here.".into(),
                },
            ],
        }
    }

    fn p() -> &'static Path {
        Path::new("test.toml")
    }

    #[test]
    fn happy_path_validates() {
        fx().validate(p()).unwrap();
    }

    #[test]
    fn round_trip_through_toml() {
        let original = fx();
        let s = original.to_file_format();
        let parsed = Page::parse(&s, p()).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn empty_title_rejected() {
        let mut page = fx();
        page.front.title.clear();
        assert!(page.validate(p()).is_err());
    }

    #[test]
    fn no_sections_rejected() {
        let mut page = fx();
        page.sections.clear();
        match page.validate(p()).unwrap_err() {
            PageError::Invalid { reason, .. } => assert!(reason.contains("no sections")),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn too_many_sections_rejected() {
        let mut page = fx();
        page.sections = (0..MAX_SECTIONS + 1)
            .map(|_| Section::Prose {
                markdown: "x".into(),
            })
            .collect();
        match page.validate(p()).unwrap_err() {
            PageError::Invalid { reason, .. } => assert!(reason.contains("sections")),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn empty_hero_headline_rejected() {
        let mut page = fx();
        page.sections[0] = Section::Hero {
            eyebrow: None,
            headline: String::new(),
            subhead: "fine".into(),
            cta: None,
        };
        assert!(page.validate(p()).is_err());
    }

    #[test]
    fn empty_prose_rejected() {
        let mut page = fx();
        page.sections.push(Section::Prose {
            markdown: "   ".into(),
        });
        assert!(page.validate(p()).is_err());
    }

    #[test]
    fn cards_too_many_items_rejected() {
        let mut page = fx();
        page.sections.push(Section::Cards {
            heading: None,
            items: (0..7)
                .map(|i| Card {
                    heading: format!("h{i}"),
                    body: format!("b{i}"),
                    cta: None,
                })
                .collect(),
        });
        assert!(page.validate(p()).is_err());
    }

    #[test]
    fn cards_empty_items_rejected() {
        let mut page = fx();
        page.sections.push(Section::Cards {
            heading: None,
            items: vec![],
        });
        assert!(page.validate(p()).is_err());
    }

    #[test]
    fn ctaband_missing_label_rejected() {
        let mut page = fx();
        page.sections.push(Section::CtaBand {
            headline: "Ready?".into(),
            cta: CallToAction {
                label: String::new(),
                href: "/x".into(),
            },
        });
        assert!(page.validate(p()).is_err());
    }

    #[test]
    fn invalid_slug_rejected() {
        let mut page = fx();
        page.front.slug = "Bad_Slug".into();
        assert!(page.validate(p()).is_err());
    }
}
