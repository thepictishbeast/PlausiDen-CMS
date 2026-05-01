//! `pdcms-admin` — PlausiDen-CMS admin web UI.
//!
//! Graphical edit surface for clients. A client logs in, sees the
//! sites they own, and edits content (today: blog posts). Every
//! save calls the same `plausiden-cms-core` API the CLI uses, so
//! the on-disk file format is identical across surfaces — meaning
//! a CLI edit and a web edit are indistinguishable post-hoc.
//!
//! ## v0 auth
//!
//! Single shared password from `PLAUSIDEN_CMS_ADMIN_TOKEN`. Sessions
//! are an in-memory HashMap of opaque random tokens; the cookie is
//! `HttpOnly`, `SameSite=Lax`, `Secure` when the server runs behind
//! TLS. WebAuthn / per-client auth lands in a follow-up; this is
//! the smallest thing that protects the admin surface from drive-by
//! traffic without inventing crypto.
//!
//! ## What's not here
//!
//! - Layout / page templates — that lives in the *site* binaries
//!   (plausiden-site etc). The CMS owns content, not rendering.
//! - Per-tenant theming — that's PlausiDen-Loom's domain (planned
//!   per-tenant token overrides crate).
//! - WYSIWYG markdown — markdown is the source of truth; live
//!   preview is nice-to-have, comes later.
//!
//! ## Run
//!
//! ```bash
//! PLAUSIDEN_CMS_ADMIN_TOKEN=somelongstring \
//!   pdcms-admin --root /var/lib/plausiden-cms/content --bind 127.0.0.1:8090
//! ```

#![doc(html_no_source)]
#![allow(clippy::doc_markdown)]

mod auth;
mod handlers;
mod state;
mod views;

use anyhow::Context;
use axum::Router;
use axum::routing::{get, post};
use std::path::PathBuf;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_target(false)
        .compact()
        .init();

    let args: Vec<String> = std::env::args().collect();
    let root = parse_arg(&args, "--root")
        .unwrap_or_else(|| PathBuf::from("./content"));
    let bind = parse_arg(&args, "--bind")
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "127.0.0.1:8090".to_string());

    let token = std::env::var("PLAUSIDEN_CMS_ADMIN_TOKEN")
        .context("PLAUSIDEN_CMS_ADMIN_TOKEN env var must be set; admin UI refuses to boot without it")?;
    if token.len() < 24 {
        anyhow::bail!("PLAUSIDEN_CMS_ADMIN_TOKEN must be ≥24 chars (current length: {})", token.len());
    }

    let state = state::AppState::new(root.clone(), token);

    let app = Router::new()
        .route("/", get(handlers::root))
        .route("/login", get(handlers::login_form).post(handlers::login_submit))
        .route("/logout", post(handlers::logout))
        .route("/sites", get(handlers::list_sites))
        .route("/sites/{site}", get(handlers::list_posts))
        .route("/sites/{site}/blog/new", get(handlers::new_form).post(handlers::create_post))
        .route("/sites/{site}/blog/{slug}/edit", get(handlers::edit_form).post(handlers::update_post))
        .route("/sites/{site}/blog/{slug}/publish", post(handlers::publish_post))
        .route("/sites/{site}/pages", get(handlers::list_pages))
        .route("/sites/{site}/pages/new", get(handlers::new_page_form).post(handlers::create_page))
        .route("/sites/{site}/pages/{slug}/edit", get(handlers::edit_page_form).post(handlers::update_page))
        .route("/sites/{site}/pages/{slug}/publish", post(handlers::publish_page))
        .route(
            "/sites/{site}/pages/{slug}/sections/new/{kind}",
            get(handlers::new_section_form).post(handlers::create_section_hero),
        )
        .route(
            "/sites/{site}/pages/{slug}/sections/{idx}/edit",
            get(handlers::edit_section_form).post(handlers::update_section),
        )
        .route(
            "/sites/{site}/pages/{slug}/sections/{idx}/up",
            post(handlers::move_section_up),
        )
        .route(
            "/sites/{site}/pages/{slug}/sections/{idx}/down",
            post(handlers::move_section_down),
        )
        .route(
            "/sites/{site}/pages/{slug}/sections/{idx}/delete",
            post(handlers::delete_section),
        )
        .route("/audit", get(handlers::audit_view))
        .route("/healthz", get(handlers::healthz))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("bind {bind}"))?;
    tracing::info!(?root, %bind, "pdcms-admin listening");
    axum::serve(listener, app).await?;
    Ok(())
}

fn parse_arg(args: &[String], flag: &str) -> Option<PathBuf> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
}
