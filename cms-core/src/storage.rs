//! Storage adapter — read / write pages to the underlying store.
//!
//! Today only [`FsStorage`] (filesystem-backed) ships. The
//! [`Storage`] trait exists so a future replacement (sqlite,
//! object-store, etc.) drops in without a rewrite.
//!
//! On-disk layout:
//!
//! ```text
//! <root>/
//!   sites/
//!     plausiden-com/
//!       site.toml             ← Site metadata
//!       pages/
//!         home.toml
//!         about.toml
//!         …
//!       media/
//!         …                   ← image / pdf / video assets
//!       audit.log              ← signed append-only chain
//!     sacredvote-org/
//!       …
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::error::{CmsError, CmsResult};
use crate::page::{Page, Site};

/// Storage adapter trait. Every CMS read or write goes through one
/// of these.
pub trait Storage {
    /// List every site this store carries.
    fn list_sites(&self) -> CmsResult<Vec<Site>>;
    /// Read one site's metadata.
    fn read_site(&self, slug: &str) -> CmsResult<Site>;
    /// Persist a site's metadata. Creates the site directory if it
    /// doesn't yet exist.
    fn write_site(&self, site: &Site) -> CmsResult<()>;
    /// List every page in a site.
    fn list_pages(&self, site_slug: &str) -> CmsResult<Vec<Page>>;
    /// Read one page by slug.
    fn read_page(&self, site_slug: &str, page_slug: &str) -> CmsResult<Page>;
    /// Persist a page. Validates the slug + uniqueness before
    /// writing.
    fn write_page(&self, site_slug: &str, page: &Page) -> CmsResult<()>;
    /// Hard-delete a page. The audit log retains the prior shape.
    fn delete_page(&self, site_slug: &str, page_slug: &str) -> CmsResult<()>;
}

/// Filesystem-backed [`Storage`].
#[derive(Debug, Clone)]
pub struct FsStorage {
    root: PathBuf,
}

impl FsStorage {
    /// Open a store rooted at `root`. Creates the `sites/`
    /// subdirectory if it doesn't exist.
    pub fn open(root: impl Into<PathBuf>) -> CmsResult<Self> {
        let root = root.into();
        std::fs::create_dir_all(root.join("sites"))?;
        Ok(Self { root })
    }

    fn site_dir(&self, slug: &str) -> PathBuf {
        self.root.join("sites").join(slug)
    }

    fn pages_dir(&self, slug: &str) -> PathBuf {
        self.site_dir(slug).join("pages")
    }

    fn page_path(&self, site_slug: &str, page_slug: &str) -> PathBuf {
        self.pages_dir(site_slug).join(format!("{page_slug}.toml"))
    }

    fn site_meta_path(&self, slug: &str) -> PathBuf {
        self.site_dir(slug).join("site.toml")
    }
}

impl Storage for FsStorage {
    fn list_sites(&self) -> CmsResult<Vec<Site>> {
        let sites_root = self.root.join("sites");
        if !sites_root.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&sites_root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let slug = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| CmsError::Validation("non-utf8 site slug".into()))?;
                if let Ok(site) = self.read_site(&slug) {
                    out.push(site);
                }
            }
        }
        out.sort_by(|a, b| a.slug.cmp(&b.slug));
        Ok(out)
    }

    fn read_site(&self, slug: &str) -> CmsResult<Site> {
        let path = self.site_meta_path(slug);
        if !path.exists() {
            return Err(CmsError::SiteNotFound(slug.into()));
        }
        let body = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&body)?)
    }

    fn write_site(&self, site: &Site) -> CmsResult<()> {
        std::fs::create_dir_all(self.pages_dir(&site.slug))?;
        let body = toml::to_string_pretty(site)?;
        std::fs::write(self.site_meta_path(&site.slug), body)?;
        Ok(())
    }

    fn list_pages(&self, site_slug: &str) -> CmsResult<Vec<Page>> {
        let dir = self.pages_dir(site_slug);
        if !dir.exists() {
            return Err(CmsError::SiteNotFound(site_slug.into()));
        }
        let mut out: Vec<Page> = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().map(|e| e == "toml").unwrap_or(false) {
                let body = std::fs::read_to_string(&path)?;
                let p: Page = toml::from_str(&body)?;
                out.push(p);
            }
        }
        out.sort_by(|a, b| a.slug.cmp(&b.slug));
        Ok(out)
    }

    fn read_page(&self, site_slug: &str, page_slug: &str) -> CmsResult<Page> {
        let path = self.page_path(site_slug, page_slug);
        if !path.exists() {
            return Err(CmsError::PageNotFound(page_slug.into()));
        }
        let body = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&body)?)
    }

    fn write_page(&self, site_slug: &str, page: &Page) -> CmsResult<()> {
        Page::validate_slug(&page.slug).map_err(CmsError::Validation)?;
        // Slug uniqueness — check existing pages
        let dir = self.pages_dir(site_slug);
        std::fs::create_dir_all(&dir)?;
        if dir.join(format!("{}.toml", page.slug)).exists() {
            // Allow overwrite when the IDs match (edit), reject
            // when the slug exists under a different id (collision).
            if let Ok(existing) = self.read_page(site_slug, &page.slug) {
                if existing.id != page.id {
                    return Err(CmsError::Validation(format!(
                        "slug {:?} already used by a different page",
                        page.slug,
                    )));
                }
            }
        }
        let body = toml::to_string_pretty(page)?;
        std::fs::write(self.page_path(site_slug, &page.slug), body)?;
        Ok(())
    }

    fn delete_page(&self, site_slug: &str, page_slug: &str) -> CmsResult<()> {
        let path = self.page_path(site_slug, page_slug);
        if !path.exists() {
            return Err(CmsError::PageNotFound(page_slug.into()));
        }
        std::fs::remove_file(&path)?;
        Ok(())
    }
}

/// Reproducible export — dumps a site's directory tree as a sorted
/// list of `(relative_path, bytes)` tuples. Caller can pipe to
/// `tar` with a deterministic header for a byte-identical archive
/// across runs (no embedded timestamps).
pub fn export_site(storage: &FsStorage, site_slug: &str) -> CmsResult<Vec<(String, Vec<u8>)>> {
    let dir = storage.site_dir(site_slug);
    if !dir.exists() {
        return Err(CmsError::SiteNotFound(site_slug.into()));
    }
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for entry in walkdir::WalkDir::new(&dir).sort_by_file_name() {
        let entry = entry.map_err(|e| CmsError::Io(std::io::Error::other(e)))?;
        if entry.file_type().is_file() {
            let rel = entry
                .path()
                .strip_prefix(&dir)
                .map_err(|_| CmsError::Validation("path strip".into()))?
                .to_string_lossy()
                .into_owned();
            out.insert(rel, std::fs::read(entry.path())?);
        }
    }
    Ok(out.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::ThemeChoice;
    use tempfile::tempdir;

    fn fixture_site() -> Site {
        Site {
            slug: "fixture".into(),
            display_name: "Fixture".into(),
            theme: ThemeChoice::LoomLight,
        }
    }

    #[test]
    fn open_creates_sites_dir() {
        let dir = tempdir().unwrap();
        let _ = FsStorage::open(dir.path()).unwrap();
        assert!(dir.path().join("sites").exists());
    }

    #[test]
    fn write_then_read_site_roundtrips() {
        let dir = tempdir().unwrap();
        let s = FsStorage::open(dir.path()).unwrap();
        s.write_site(&fixture_site()).unwrap();
        let back = s.read_site("fixture").unwrap();
        assert_eq!(back.display_name, "Fixture");
        assert!(matches!(back.theme, ThemeChoice::LoomLight));
    }

    #[test]
    fn list_sites_returns_sorted() {
        let dir = tempdir().unwrap();
        let s = FsStorage::open(dir.path()).unwrap();
        let mut a = fixture_site();
        a.slug = "alpha".into();
        a.display_name = "A".into();
        let mut b = fixture_site();
        b.slug = "beta".into();
        b.display_name = "B".into();
        s.write_site(&a).unwrap();
        s.write_site(&b).unwrap();
        let listed = s.list_sites().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].slug, "alpha");
        assert_eq!(listed[1].slug, "beta");
    }

    #[test]
    fn write_then_read_page_roundtrips() {
        let dir = tempdir().unwrap();
        let s = FsStorage::open(dir.path()).unwrap();
        s.write_site(&fixture_site()).unwrap();
        let p = Page::draft("home", "Home");
        s.write_page("fixture", &p).unwrap();
        let back = s.read_page("fixture", "home").unwrap();
        assert_eq!(back.id, p.id);
        assert_eq!(back.title, "Home");
    }

    #[test]
    fn invalid_slug_rejected() {
        let dir = tempdir().unwrap();
        let s = FsStorage::open(dir.path()).unwrap();
        s.write_site(&fixture_site()).unwrap();
        let mut p = Page::draft("bad slug with spaces", "x");
        // Override slug after construction so we can hit validate
        p.slug = "Bad Slug".into();
        let err = s.write_page("fixture", &p).unwrap_err();
        assert!(matches!(err, CmsError::Validation(_)));
    }

    #[test]
    fn slug_collision_under_different_id_rejected() {
        let dir = tempdir().unwrap();
        let s = FsStorage::open(dir.path()).unwrap();
        s.write_site(&fixture_site()).unwrap();
        let p1 = Page::draft("home", "Home v1");
        s.write_page("fixture", &p1).unwrap();
        let p2 = Page::draft("home", "Home v2");
        let err = s.write_page("fixture", &p2).unwrap_err();
        assert!(matches!(err, CmsError::Validation(_)));
    }

    #[test]
    fn slug_collision_with_same_id_treated_as_edit() {
        let dir = tempdir().unwrap();
        let s = FsStorage::open(dir.path()).unwrap();
        s.write_site(&fixture_site()).unwrap();
        let mut p = Page::draft("home", "Home v1");
        s.write_page("fixture", &p).unwrap();
        p.title = "Home v2".into();
        s.write_page("fixture", &p).unwrap();
        let back = s.read_page("fixture", "home").unwrap();
        assert_eq!(back.title, "Home v2");
    }

    #[test]
    fn delete_page_removes_file() {
        let dir = tempdir().unwrap();
        let s = FsStorage::open(dir.path()).unwrap();
        s.write_site(&fixture_site()).unwrap();
        s.write_page("fixture", &Page::draft("home", "Home")).unwrap();
        s.delete_page("fixture", "home").unwrap();
        assert!(matches!(
            s.read_page("fixture", "home"),
            Err(CmsError::PageNotFound(_)),
        ));
    }

    #[test]
    fn export_site_dumps_sorted_tree() {
        let dir = tempdir().unwrap();
        let s = FsStorage::open(dir.path()).unwrap();
        s.write_site(&fixture_site()).unwrap();
        s.write_page("fixture", &Page::draft("alpha", "A")).unwrap();
        s.write_page("fixture", &Page::draft("beta", "B")).unwrap();
        let dump = export_site(&s, "fixture").unwrap();
        // Two pages + site.toml = 3 entries.
        assert_eq!(dump.len(), 3);
        // Entries are sorted by relative path.
        let paths: Vec<_> = dump.iter().map(|(p, _)| p.clone()).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted);
    }
}
