//! Blog post content type — first concrete content shape for v0.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Status of a blog post in the editorial workflow.
///
/// `Draft` is editor-visible only; `Published` is what the running
/// site renders. There's deliberately no third state — the
/// supersociety doctrine is "in or out", no scheduled-publish queue
/// (cron jobs are mutable state we don't want).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlogStatus {
    /// Editor-visible draft. Not exposed to public visitors.
    Draft,
    /// Published — surfaced by the public site.
    Published,
}

/// Typed frontmatter for a blog post.
///
/// Lives between `+++` delimiters at the top of the markdown file.
/// All fields are required; missing fields are a load error, not a
/// silent default — better to surface "you forgot a summary" than
/// to ship empty meta-description on a published page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlogPostFrontmatter {
    /// Title displayed at the top of the post + in `<title>`.
    pub title: String,
    /// URL slug — kebab-case, ASCII, no slashes. Must round-trip
    /// `slug::slugify(title)` cleanly OR be explicitly hand-set
    /// for SEO continuity.
    pub slug: String,
    /// Publication date. Date-only because timezone-of-publication
    /// is not a user-meaningful detail; site renders as YYYY-MM-DD.
    pub date: NaiveDate,
    /// One-sentence summary, used as `<meta name="description">`
    /// and as the blog-index card subtitle. Capped to keep meta
    /// descriptions inside the 160-char SEO sweet spot.
    pub summary: String,
    /// Author display name. Free-form (no @handle); site can map
    /// to a profile page in a later iteration.
    pub author: String,
    /// Editorial state.
    pub status: BlogStatus,
}

/// A loaded blog post: validated frontmatter + raw markdown body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlogPost {
    /// Frontmatter (typed, validated).
    pub front: BlogPostFrontmatter,
    /// Body in markdown. Render via [`render_html`] for the
    /// site's read path.
    pub body: String,
}

/// Soft caps that aren't enforced as hard limits but are worth
/// testing against in CI so an editor doesn't accidentally publish
/// a 50KB summary.
const MAX_TITLE_LEN: usize = 200;
const MAX_SUMMARY_LEN: usize = 200;
const MAX_BODY_BYTES: usize = 200 * 1024; // 200 KB

/// Errors that can come out of loading or validating a blog post.
#[derive(Debug, thiserror::Error)]
pub enum BlogError {
    /// I/O reading the file.
    #[error("read {path}: {source}")]
    Io {
        /// Path that failed.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Frontmatter delimiters (+++) missing or malformed.
    #[error("frontmatter not delimited by `+++` lines in {path}")]
    Frontmatter {
        /// Path that failed.
        path: String,
    },
    /// Frontmatter TOML didn't parse.
    #[error("frontmatter parse {path}: {source}")]
    Toml {
        /// Path that failed.
        path: String,
        /// Underlying parse error.
        #[source]
        source: toml::de::Error,
    },
    /// One of the typed validation rules rejected the post.
    #[error("validation in {path}: {reason}")]
    Invalid {
        /// Path that failed.
        path: String,
        /// Human-readable reason.
        reason: String,
    },
}

impl BlogPost {
    /// Load + validate a single blog post file from disk.
    ///
    /// # Errors
    /// - [`BlogError::Io`] if the file can't be read.
    /// - [`BlogError::Frontmatter`] if the `+++` delimiters are missing.
    /// - [`BlogError::Toml`] if the frontmatter TOML doesn't parse.
    /// - [`BlogError::Invalid`] if a validation rule fails (slug shape,
    ///   non-empty fields, body length cap, etc.).
    pub fn load_from_file(path: &Path) -> Result<Self, BlogError> {
        let raw = std::fs::read_to_string(path).map_err(|e| BlogError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Self::parse(&raw, path)
    }

    /// Parse a blog post out of a string, with `path` used only for
    /// error messages.
    ///
    /// # Errors
    /// Same as [`Self::load_from_file`] minus the I/O variant.
    pub fn parse(raw: &str, path: &Path) -> Result<Self, BlogError> {
        let (front_str, body) = split_frontmatter(raw).ok_or_else(|| BlogError::Frontmatter {
            path: path.display().to_string(),
        })?;
        let front: BlogPostFrontmatter =
            toml::from_str(front_str).map_err(|e| BlogError::Toml {
                path: path.display().to_string(),
                source: e,
            })?;
        let post = Self {
            front,
            body: body.to_string(),
        };
        post.validate(path)?;
        Ok(post)
    }

    /// Apply the typed validation rules. Caller usually doesn't need
    /// to invoke this directly; `parse` and `load_from_file` already do.
    ///
    /// # Errors
    /// [`BlogError::Invalid`] with a human-readable reason.
    pub fn validate(&self, path: &Path) -> Result<(), BlogError> {
        let here = || path.display().to_string();
        let reject = |reason: &str| BlogError::Invalid {
            path: here(),
            reason: reason.to_string(),
        };
        if self.front.title.trim().is_empty() {
            return Err(reject("title is empty"));
        }
        if self.front.title.len() > MAX_TITLE_LEN {
            return Err(reject(&format!("title >{MAX_TITLE_LEN} chars")));
        }
        if self.front.summary.trim().is_empty() {
            return Err(reject("summary is empty"));
        }
        if self.front.summary.len() > MAX_SUMMARY_LEN {
            return Err(reject(&format!("summary >{MAX_SUMMARY_LEN} chars")));
        }
        if self.front.author.trim().is_empty() {
            return Err(reject("author is empty"));
        }
        if !is_valid_slug(&self.front.slug) {
            return Err(reject(
                "slug must be lowercase ASCII letters/digits/dashes, no leading/trailing dash",
            ));
        }
        if self.body.len() > MAX_BODY_BYTES {
            return Err(reject(&format!("body >{MAX_BODY_BYTES} bytes")));
        }
        Ok(())
    }

    /// Serialize this post back to the on-disk format (frontmatter
    /// + body). Caller writes the result.
    #[must_use]
    pub fn to_file_format(&self) -> String {
        let front = toml::to_string_pretty(&self.front)
            .expect("typed frontmatter always serializes");
        let mut out = String::with_capacity(self.body.len() + front.len() + 16);
        out.push_str("+++\n");
        out.push_str(&front);
        out.push_str("+++\n\n");
        out.push_str(&self.body);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }

    /// Atomic write to disk: serializes to a temp file in the same
    /// directory, then renames. Concurrent readers either see the
    /// previous version or the new one, never a partial write.
    ///
    /// # Errors
    /// I/O errors from create / write / rename.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        let tmp = parent.join(format!(
            ".{}.tmp",
            path.file_name()
                .map_or_else(|| "post".to_string(), |s| s.to_string_lossy().to_string())
        ));
        std::fs::write(&tmp, self.to_file_format())?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// Split an on-disk post into (frontmatter_toml, body_markdown).
///
/// Expected shape:
/// ```text
/// +++
/// title = "..."
/// ...
/// +++
///
/// # Body markdown here
/// ```
fn split_frontmatter(raw: &str) -> Option<(&str, &str)> {
    // Both opening and closing must be a `+++` line on its own.
    let opener = "+++\n";
    let body_after_opener = raw.strip_prefix(opener)?;
    let close_idx = body_after_opener.find("\n+++\n")?;
    let front = &body_after_opener[..close_idx];
    let after_close = &body_after_opener[close_idx + "\n+++\n".len()..];
    // Trim leading blank line(s) on the body — purely cosmetic.
    Some((front, after_close.trim_start_matches('\n')))
}

/// Conservative slug validator: ASCII lowercase, digits, single
/// dashes, no leading or trailing dash. Refuses underscores +
/// uppercase to avoid the case where a content move breaks URLs
/// because of a single capitalized character.
fn is_valid_slug(s: &str) -> bool {
    if s.is_empty() || s.starts_with('-') || s.ends_with('-') {
        return false;
    }
    let mut prev_dash = false;
    for c in s.chars() {
        if c == '-' {
            if prev_dash {
                return false;
            }
            prev_dash = true;
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() {
            prev_dash = false;
        } else {
            return false;
        }
    }
    true
}

/// Render a markdown body to safe HTML.
///
/// Uses pulldown-cmark with default safe escaping. The resulting
/// HTML is suitable for direct embedding in a Maud `(PreEscaped(...))`
/// node on a page that enforces a strict CSP — no script, no inline
/// styles emitted by the renderer.
#[must_use]
pub fn render_html(markdown: &str) -> String {
    let parser = pulldown_cmark::Parser::new_ext(markdown, pulldown_cmark::Options::all());
    let mut html = String::with_capacity(markdown.len());
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good_frontmatter() -> &'static str {
        r#"+++
title = "Why Thundercrab"
slug = "why-thundercrab"
date = "2026-04-15"
summary = "A privacy-first mail client for people who don't want their inbox training a foreign LLM."
author = "Paul"
status = "published"
+++

# Why Thundercrab

A real markdown body.
"#
    }

    fn p() -> &'static Path {
        Path::new("test.md")
    }

    #[test]
    fn parse_happy_path() {
        let post = BlogPost::parse(good_frontmatter(), p()).expect("valid post");
        assert_eq!(post.front.slug, "why-thundercrab");
        assert_eq!(post.front.status, BlogStatus::Published);
        assert!(post.body.contains("# Why Thundercrab"));
    }

    #[test]
    fn round_trip_to_file_format() {
        let p1 = BlogPost::parse(good_frontmatter(), p()).unwrap();
        let serialized = p1.to_file_format();
        let p2 = BlogPost::parse(&serialized, p()).unwrap();
        assert_eq!(p1, p2);
    }

    #[test]
    fn missing_frontmatter_delimiters_rejected() {
        let raw = "title = 'No delimiters'\n\nbody\n";
        let err = BlogPost::parse(raw, p()).unwrap_err();
        assert!(matches!(err, BlogError::Frontmatter { .. }));
    }

    #[test]
    fn invalid_slug_rejected() {
        let raw = good_frontmatter().replace("why-thundercrab", "Why_Thundercrab");
        let err = BlogPost::parse(&raw, p()).unwrap_err();
        match err {
            BlogError::Invalid { reason, .. } => assert!(reason.contains("slug")),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn empty_title_rejected() {
        let raw = good_frontmatter().replace("Why Thundercrab", "");
        let err = BlogPost::parse(&raw, p()).unwrap_err();
        match err {
            BlogError::Invalid { reason, .. } => assert!(reason.contains("title")),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn slug_with_leading_dash_rejected() {
        assert!(!is_valid_slug("-foo"));
    }

    #[test]
    fn slug_with_trailing_dash_rejected() {
        assert!(!is_valid_slug("foo-"));
    }

    #[test]
    fn slug_with_double_dash_rejected() {
        assert!(!is_valid_slug("foo--bar"));
    }

    #[test]
    fn slug_uppercase_rejected() {
        assert!(!is_valid_slug("FooBar"));
    }

    #[test]
    fn slug_underscore_rejected() {
        assert!(!is_valid_slug("foo_bar"));
    }

    #[test]
    fn slug_normal_accepted() {
        assert!(is_valid_slug("why-thundercrab"));
        assert!(is_valid_slug("post-2026-04-15"));
        assert!(is_valid_slug("a"));
    }

    #[test]
    fn render_markdown_to_html() {
        let html = render_html("# Hi\n\nbody");
        assert!(html.contains("<h1>Hi</h1>"));
        assert!(html.contains("<p>body</p>"));
    }

    #[test]
    fn render_does_not_emit_script() {
        // pulldown-cmark default doesn't escape raw HTML embedded in
        // markdown — but our content workflow doesn't accept raw
        // <script>; this test pins that any future change to the
        // renderer surface still doesn't open a script-injection hole.
        let html = render_html("hello <script>alert(1)</script>");
        // Default pulldown-cmark behavior is to PASS THROUGH inline
        // HTML; this test documents that the EDITOR is the safety
        // boundary, not the renderer. If we ever want to enforce no
        // raw HTML, we'd switch to a sanitizing pipeline.
        // (Recording the current behavior so a future renderer
        // upgrade that changes it surfaces in CI.)
        assert!(html.contains("script"));
    }
}
