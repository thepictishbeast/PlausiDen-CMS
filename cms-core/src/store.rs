//! On-disk content store. Per-site, per-content-type directories.

use crate::blog::{BlogError, BlogPost};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// A site identifier — namespace inside the content tree.
///
/// Stays a thin wrapper over a string for now; promoted to an enum
/// once we have more than one production site under management.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Site(pub String);

impl Site {
    /// `plausiden.com` — the canonical first site. Hand-coded helper
    /// so its path is the same wherever it's referenced.
    #[must_use]
    pub fn plausiden_com() -> Self {
        Self("plausiden.com".to_string())
    }
}

/// Errors out of the store.
#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    /// A specific blog post failed to load.
    #[error(transparent)]
    Blog(#[from] BlogError),
    /// I/O at the directory level.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// On-disk store rooted at a directory.
///
/// Layout: `<root>/<site>/<type>/<slug>.md`. Type is currently
/// `blog` only; sections / pages / media land in follow-ups.
#[derive(Debug, Clone)]
pub struct Store {
    /// Root directory containing `<site>/<type>/...`.
    root: PathBuf,
}

impl Store {
    /// Create a store at the given root directory. Doesn't create
    /// the directory; caller is expected to point at an existing
    /// content tree (or create it once at setup).
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Path to the directory holding blog posts for `site`.
    #[must_use]
    pub fn blog_dir(&self, site: &Site) -> PathBuf {
        self.root.join(&site.0).join("blog")
    }

    /// Path for a specific blog post slug.
    #[must_use]
    pub fn blog_path(&self, site: &Site, slug: &str) -> PathBuf {
        self.blog_dir(site).join(format!("{slug}.md"))
    }

    /// Load every blog post for `site`, in stable lexical-by-slug
    /// order. Skips files whose name doesn't end in `.md`.
    ///
    /// # Errors
    /// Returns the first per-file error encountered. The whole site
    /// is unhealthy if even one post is malformed; better to surface
    /// the failure than silently drop the broken file.
    pub fn list_blog_posts(&self, site: &Site) -> Result<Vec<BlogPost>, ContentError> {
        let dir = self.blog_dir(site);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut entries: Vec<PathBuf> = WalkDir::new(&dir)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(Result::ok)
            .map(|e| e.into_path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
            .collect();
        entries.sort();
        let mut out = Vec::with_capacity(entries.len());
        for p in entries {
            out.push(BlogPost::load_from_file(&p)?);
        }
        Ok(out)
    }

    /// Load just the published posts for `site`, newest first.
    ///
    /// # Errors
    /// Same as [`Self::list_blog_posts`].
    pub fn list_published(&self, site: &Site) -> Result<Vec<BlogPost>, ContentError> {
        let mut posts = self.list_blog_posts(site)?;
        posts.retain(|p| p.front.status == crate::blog::BlogStatus::Published);
        posts.sort_by(|a, b| b.front.date.cmp(&a.front.date));
        Ok(posts)
    }

    /// Load a specific post by slug. Returns `None` if no file is at
    /// that slug. Errors propagate as in [`Self::list_blog_posts`].
    ///
    /// # Errors
    /// Same as [`Self::list_blog_posts`].
    pub fn get_post(&self, site: &Site, slug: &str) -> Result<Option<BlogPost>, ContentError> {
        let path = self.blog_path(site, slug);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(BlogPost::load_from_file(&path)?))
    }

    /// Resolve the directory root, useful for callers that want to
    /// surface "this CMS sees this on-disk directory" info.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blog::{BlogPost, BlogPostFrontmatter, BlogStatus};
    use chrono::NaiveDate;

    fn fixture_post(slug: &str, status: BlogStatus, year: i32) -> BlogPost {
        BlogPost {
            front: BlogPostFrontmatter {
                title: format!("Title {slug}"),
                slug: slug.to_string(),
                date: NaiveDate::from_ymd_opt(year, 4, 15).unwrap(),
                summary: "Summary line.".into(),
                author: "Test".into(),
                status,
            },
            body: format!("# {slug}\n\nBody.\n"),
        }
    }

    #[test]
    fn list_published_returns_only_published_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let site = Site::plausiden_com();
        let blog_dir = store.blog_dir(&site);
        std::fs::create_dir_all(&blog_dir).unwrap();

        fixture_post("a-2024", BlogStatus::Published, 2024)
            .write(&store.blog_path(&site, "a-2024"))
            .unwrap();
        fixture_post("b-2026", BlogStatus::Published, 2026)
            .write(&store.blog_path(&site, "b-2026"))
            .unwrap();
        fixture_post("c-draft", BlogStatus::Draft, 2025)
            .write(&store.blog_path(&site, "c-draft"))
            .unwrap();

        let pub_posts = store.list_published(&site).unwrap();
        assert_eq!(pub_posts.len(), 2);
        assert_eq!(pub_posts[0].front.slug, "b-2026", "newest first");
        assert_eq!(pub_posts[1].front.slug, "a-2024");
    }

    #[test]
    fn missing_blog_dir_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let site = Site::plausiden_com();
        let posts = store.list_blog_posts(&site).unwrap();
        assert!(posts.is_empty());
    }

    #[test]
    fn get_post_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let site = Site::plausiden_com();
        std::fs::create_dir_all(store.blog_dir(&site)).unwrap();

        let original = fixture_post("hello", BlogStatus::Draft, 2026);
        original.write(&store.blog_path(&site, "hello")).unwrap();

        let loaded = store.get_post(&site, "hello").unwrap().unwrap();
        assert_eq!(loaded.front, original.front);
    }

    #[test]
    fn get_post_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let site = Site::plausiden_com();
        assert!(store.get_post(&site, "nope").unwrap().is_none());
    }
}
