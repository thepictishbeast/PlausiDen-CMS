//! `pdcms` — PlausiDen-CMS command-line.
//!
//! The CLI is the primary write surface today (the admin web UI is a
//! follow-up crate). Every change goes through here, which means
//! every change is a file edit + git commit — auditable by default.
//!
//! Subcommands:
//!
//!   pdcms list       — list blog posts for a site
//!   pdcms new        — create a new blog post (draft)
//!   pdcms validate   — typecheck every post in the content tree
//!   pdcms publish    — flip a draft to published
//!
//! Anything more invasive (delete, move, schema migration) lives in
//! a separate `pdcms admin` subcommand we'll add when needed; the
//! happy-path commands above stay simple.

#![doc(html_no_source)]

use anyhow::{Context, Result, bail};
use chrono::Local;
use clap::{Parser, Subcommand};
use plausiden_cms_core::{
    BlogPost, BlogPostFrontmatter, BlogStatus, Site, Store,
};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "pdcms")]
#[command(version)]
#[command(about = "PlausiDen-CMS content commands", long_about = None)]
struct Cli {
    /// Path to the content tree root (defaults to `./content`).
    #[arg(long, default_value = "content", global = true)]
    root: PathBuf,
    /// Site name (defaults to `plausiden.com`).
    #[arg(long, default_value = "plausiden.com", global = true)]
    site: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// List all blog posts for the site.
    List {
        /// Show only published posts.
        #[arg(long)]
        published: bool,
    },
    /// Create a new draft blog post.
    New {
        /// Post title (used to derive slug + populate frontmatter).
        title: String,
        /// Author name; defaults to "PlausiDen".
        #[arg(long, default_value = "PlausiDen")]
        author: String,
        /// Optional explicit slug; defaults to slugified title.
        #[arg(long)]
        slug: Option<String>,
        /// Optional summary; placeholder if omitted (you'll edit it).
        #[arg(long)]
        summary: Option<String>,
    },
    /// Walk the content tree and report any validation failures.
    Validate,
    /// Flip a draft post to published.
    Publish {
        /// Slug of the post to publish.
        slug: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::new(&cli.root);
    let site = Site(cli.site.clone());

    match cli.cmd {
        Cmd::List { published } => cmd_list(&store, &site, published),
        Cmd::New {
            title,
            author,
            slug,
            summary,
        } => cmd_new(&store, &site, &title, &author, slug.as_deref(), summary.as_deref()),
        Cmd::Validate => cmd_validate(&store, &site),
        Cmd::Publish { slug } => cmd_publish(&store, &site, &slug),
    }
}

fn cmd_list(store: &Store, site: &Site, published_only: bool) -> Result<()> {
    let posts = if published_only {
        store
            .list_published(site)
            .with_context(|| format!("loading published posts for {}", site.0))?
    } else {
        store
            .list_blog_posts(site)
            .with_context(|| format!("loading posts for {}", site.0))?
    };
    if posts.is_empty() {
        eprintln!("no posts under {}", store.root().display());
        return Ok(());
    }
    println!("{:<30} {:<10} {:<12} {}", "slug", "status", "date", "title");
    for p in &posts {
        let status = match p.front.status {
            BlogStatus::Draft => "draft",
            BlogStatus::Published => "published",
        };
        println!(
            "{:<30} {:<10} {:<12} {}",
            truncate(&p.front.slug, 30),
            status,
            p.front.date,
            truncate(&p.front.title, 60),
        );
    }
    Ok(())
}

fn cmd_new(
    store: &Store,
    site: &Site,
    title: &str,
    author: &str,
    slug_override: Option<&str>,
    summary_override: Option<&str>,
) -> Result<()> {
    let slug = slug_override
        .map_or_else(|| slug::slugify(title), str::to_string);
    let path = store.blog_path(site, &slug);
    if path.exists() {
        bail!("post already exists at {}", path.display());
    }
    let post = BlogPost {
        front: BlogPostFrontmatter {
            title: title.to_string(),
            slug: slug.clone(),
            date: Local::now().date_naive(),
            summary: summary_override
                .unwrap_or("(replace this summary before publishing)")
                .to_string(),
            author: author.to_string(),
            status: BlogStatus::Draft,
        },
        body: format!("# {title}\n\nDraft body — replace before publishing.\n"),
    };
    post.validate(&path)?;
    post.write(&path)?;
    println!("created {} (status=draft)", path.display());
    Ok(())
}

fn cmd_validate(store: &Store, site: &Site) -> Result<()> {
    let posts = store.list_blog_posts(site)?;
    println!("{} posts checked, all valid", posts.len());
    Ok(())
}

fn cmd_publish(store: &Store, site: &Site, slug: &str) -> Result<()> {
    let path = store.blog_path(site, slug);
    let mut post = BlogPost::load_from_file(&path)
        .with_context(|| format!("loading {}", path.display()))?;
    if post.front.status == BlogStatus::Published {
        println!("{} already published", slug);
        return Ok(());
    }
    post.front.status = BlogStatus::Published;
    post.write(&path)?;
    println!("published {}", path.display());
    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
