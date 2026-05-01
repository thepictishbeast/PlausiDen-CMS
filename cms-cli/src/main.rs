//! `cms` — command-line for PlausiDen-CMS.
//!
//! Drop-in CLI for site init / page CRUD / export. Designed so an
//! editor can drive the store from a terminal without spinning up
//! the admin web surface — and so a CI / cron job can scrub or
//! export a site without a graphical session.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use cms_core::{FsStorage, Page, PageStatus, Site, Storage, page::ThemeChoice};

#[derive(Parser)]
#[command(name = "cms", about = "PlausiDen-CMS command-line", version)]
struct Cli {
    /// Storage root. Defaults to `./cms-store/`.
    #[arg(long, global = true, default_value = "cms-store")]
    root: PathBuf,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Initialise a new site directory.
    InitSite {
        slug: String,
        #[arg(long)]
        display_name: Option<String>,
        /// `loom_light` (default), `loom_dark`, or `loom_custom`.
        #[arg(long, default_value = "loom_light")]
        theme: String,
    },
    /// List every site in the store.
    ListSites,
    /// List every page in a site.
    ListPages { site: String },
    /// Create a fresh draft page.
    NewPage {
        site: String,
        slug: String,
        title: String,
    },
    /// Print one page as TOML.
    ShowPage { site: String, page: String },
    /// Mark a page as published.
    PublishPage { site: String, page: String },
    /// Export every site file as a sorted (path, sha256) listing —
    /// the manifest a deterministic-tar builder consumes.
    ExportManifest { site: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cms: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let storage = FsStorage::open(&cli.root)?;
    match cli.command {
        Cmd::InitSite {
            slug,
            display_name,
            theme,
        } => {
            let theme = parse_theme(&theme)?;
            let site = Site {
                slug: slug.clone(),
                display_name: display_name.unwrap_or_else(|| slug.clone()),
                theme,
            };
            storage.write_site(&site)?;
            println!("cms: site {slug:?} initialised");
        }
        Cmd::ListSites => {
            let sites = storage.list_sites()?;
            for s in sites {
                println!("{:<32} {:?}", s.slug, s.theme);
            }
        }
        Cmd::ListPages { site } => {
            let pages = storage.list_pages(&site)?;
            for p in pages {
                println!(
                    "{:<32} [{}] {}",
                    p.slug,
                    status_label(p.status),
                    p.title,
                );
            }
        }
        Cmd::NewPage { site, slug, title } => {
            let p = Page::draft(slug, title);
            storage.write_page(&site, &p)?;
            println!("cms: page {:?} created in {:?}", p.slug, site);
        }
        Cmd::ShowPage { site, page } => {
            let p = storage.read_page(&site, &page)?;
            println!("{}", toml::to_string_pretty(&p)?);
        }
        Cmd::PublishPage { site, page } => {
            let mut p = storage.read_page(&site, &page)?;
            p.status = PageStatus::Published;
            p.updated_at = chrono::Utc::now();
            storage.write_page(&site, &p)?;
            println!("cms: page {:?} published in {:?}", page, site);
        }
        Cmd::ExportManifest { site } => {
            let dump = cms_core::storage::export_site(&storage, &site)?;
            for (path, bytes) in dump {
                let digest = sha256_hex(&bytes);
                println!("{digest}  {path}");
            }
        }
    }
    Ok(())
}

fn parse_theme(s: &str) -> Result<ThemeChoice> {
    match s {
        "loom_light" => Ok(ThemeChoice::LoomLight),
        "loom_dark" => Ok(ThemeChoice::LoomDark),
        "loom_custom" => Ok(ThemeChoice::LoomCustom),
        other => bail!("unknown theme {other:?}; expected loom_light | loom_dark | loom_custom"),
    }
}

fn status_label(s: PageStatus) -> &'static str {
    match s {
        PageStatus::Draft => "draft",
        PageStatus::Reviewed => "reviewed",
        PageStatus::Published => "PUB",
        PageStatus::Archived => "archived",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in digest.iter() {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
