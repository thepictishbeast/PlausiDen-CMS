//! HTTP handlers — bind URL routes to template + storage calls.

use axum::Form;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{Local, NaiveDate};
use maud::Markup;
use plausiden_cms_core::{
    BlogPost, BlogPostFrontmatter, BlogStatus, Page, PageFrontmatter, PageLayout, PageStatus,
    Section, Site,
};
use serde::Deserialize;
use std::path::Path as StdPath;

use crate::auth::{
    AuthSession, COOKIE_NAME, clear_cookie, extract_token, generate_session_token,
    require_auth_or_401, set_cookie,
};
use crate::state::{AppState, Session};
use crate::views;

/// Health check — bypasses auth so a load balancer can probe it.
pub async fn healthz() -> &'static str {
    "ok"
}

/// `/` → bounce based on auth state.
pub async fn root(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Response {
    if extract_token(&headers).is_some()
        && crate::auth::current_session(&headers, &state).is_some()
    {
        Redirect::to("/sites").into_response()
    } else {
        Redirect::to("/login").into_response()
    }
}

pub async fn login_form() -> Markup {
    views::login_page(None)
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub token: String,
}

pub async fn login_submit(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> Response {
    if !state.verify_token(&form.token) {
        tracing::warn!("admin login: bad token");
        return (StatusCode::UNAUTHORIZED, views::login_page(Some("Invalid token."))).into_response();
    }
    let token = generate_session_token();
    if let Ok(mut map) = state.sessions.lock() {
        map.insert(
            token.clone(),
            Session {
                display_name: "admin".into(),
            },
        );
    }
    let mut headers = HeaderMap::new();
    let cookie = set_cookie(&token);
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).expect("cookie value is ASCII"),
    );
    tracing::info!("admin login OK");
    (StatusCode::SEE_OTHER, headers, [(header::LOCATION, "/sites")]).into_response()
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Some(token) = extract_token(&headers) {
        if let Ok(mut map) = state.sessions.lock() {
            map.remove(&token);
        }
    }
    let mut h = HeaderMap::new();
    h.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&clear_cookie()).expect("cookie value is ASCII"),
    );
    (StatusCode::SEE_OTHER, h, [(header::LOCATION, "/login")]).into_response()
}

pub async fn list_sites(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
) -> Markup {
    let sites = enumerate_sites(state.root.as_path()).unwrap_or_default();
    views::sites_page(&sites, &state.root.display().to_string())
}

pub async fn list_posts(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path(site_name): Path<String>,
) -> Markup {
    let site = Site(site_name.clone());
    let posts = state.store.list_blog_posts(&site).unwrap_or_default();
    views::posts_page(&site_name, &posts, None)
}

pub async fn new_form(
    AuthSession(_): AuthSession,
    Path(site_name): Path<String>,
) -> Markup {
    views::post_form(
        &site_name,
        true,
        "",
        "",
        Local::now().date_naive(),
        "",
        "PlausiDen",
        BlogStatus::Draft,
        "",
        None,
    )
}

#[derive(Debug, Deserialize)]
pub struct PostForm {
    pub title: String,
    pub slug: String,
    pub date: String,
    pub summary: String,
    pub author: String,
    pub status: String,
    pub body: String,
}

pub async fn create_post(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path(site_name): Path<String>,
    Form(form): Form<PostForm>,
) -> Response {
    let site = Site(site_name.clone());
    let date = match parse_date(&form.date) {
        Ok(d) => d,
        Err(e) => return form_with_error(&site_name, true, &form, e).into_response(),
    };
    let status = parse_status(&form.status);
    let post = BlogPost {
        front: BlogPostFrontmatter {
            title: form.title.clone(),
            slug: form.slug.clone(),
            date,
            summary: form.summary.clone(),
            author: form.author.clone(),
            status,
        },
        body: form.body.clone(),
    };
    let path = state.store.blog_path(&site, &form.slug);
    if path.exists() {
        return form_with_error(&site_name, true, &form, "A post with that slug already exists.")
            .into_response();
    }
    if let Err(e) = post.validate(&path) {
        return form_with_error(&site_name, true, &form, &e.to_string()).into_response();
    }
    if let Err(e) = post.write(&path) {
        return form_with_error(&site_name, true, &form, &format!("Disk write failed: {e}"))
            .into_response();
    }
    Redirect::to(&format!("/sites/{site_name}")).into_response()
}

pub async fn edit_form(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path((site_name, slug)): Path<(String, String)>,
) -> Response {
    let site = Site(site_name.clone());
    match state.store.get_post(&site, &slug) {
        Ok(Some(p)) => views::post_form(
            &site_name,
            false,
            &p.front.title,
            &p.front.slug,
            p.front.date,
            &p.front.summary,
            &p.front.author,
            p.front.status,
            &p.body,
            None,
        )
        .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such post").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("load: {e}")).into_response(),
    }
}

pub async fn update_post(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path((site_name, slug)): Path<(String, String)>,
    Form(form): Form<PostForm>,
) -> Response {
    if form.slug != slug {
        return (StatusCode::BAD_REQUEST, "slug cannot be changed via edit").into_response();
    }
    let site = Site(site_name.clone());
    let date = match parse_date(&form.date) {
        Ok(d) => d,
        Err(e) => return form_with_error(&site_name, false, &form, e).into_response(),
    };
    let status = parse_status(&form.status);
    let post = BlogPost {
        front: BlogPostFrontmatter {
            title: form.title.clone(),
            slug: form.slug.clone(),
            date,
            summary: form.summary.clone(),
            author: form.author.clone(),
            status,
        },
        body: form.body.clone(),
    };
    let path = state.store.blog_path(&site, &slug);
    if let Err(e) = post.validate(&path) {
        return form_with_error(&site_name, false, &form, &e.to_string()).into_response();
    }
    if let Err(e) = post.write(&path) {
        return form_with_error(&site_name, false, &form, &format!("Disk write failed: {e}"))
            .into_response();
    }
    Redirect::to(&format!("/sites/{site_name}")).into_response()
}

pub async fn publish_post(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path((site_name, slug)): Path<(String, String)>,
) -> Response {
    if require_auth_or_401(&headers, &state).is_err() {
        return (StatusCode::UNAUTHORIZED, "not signed in").into_response();
    }
    let site = Site(site_name.clone());
    let path = state.store.blog_path(&site, &slug);
    let mut post = match BlogPost::load_from_file(&path) {
        Ok(p) => p,
        Err(e) => return (StatusCode::NOT_FOUND, format!("load: {e}")).into_response(),
    };
    post.front.status = BlogStatus::Published;
    if let Err(e) = post.write(&path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("write: {e}")).into_response();
    }
    Redirect::to(&format!("/sites/{site_name}")).into_response()
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

pub async fn list_pages(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path(site_name): Path<String>,
) -> Markup {
    let site = Site(site_name.clone());
    let pages = state.store.list_pages(&site).unwrap_or_default();
    views::pages_page(&site_name, &pages, None)
}

pub async fn new_page_form(
    AuthSession(_): AuthSession,
    Path(site_name): Path<String>,
) -> Markup {
    let starter = include_starter_sections();
    views::page_form(
        &site_name,
        true,
        "",
        "",
        "",
        PageStatus::Draft,
        PageLayout::Default,
        Local::now().date_naive(),
        None,
        &starter,
        None,
    )
}

#[derive(Debug, Deserialize)]
pub struct PageForm {
    pub title: String,
    pub slug: String,
    pub summary: String,
    pub status: String,
    pub layout: String,
    pub updated_at: String,
    pub nav_order: Option<String>,
    pub sections: String,
}

pub async fn create_page(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path(site_name): Path<String>,
    Form(form): Form<PageForm>,
) -> Response {
    let site = Site(site_name.clone());
    let path = state.store.page_path(&site, &form.slug);
    if path.exists() {
        return page_form_with_error(&site_name, true, &form, "A page with that slug already exists.")
            .into_response();
    }
    match build_page(&form) {
        Ok(page) => match page.write(&path) {
            Ok(()) => Redirect::to(&format!("/sites/{site_name}/pages")).into_response(),
            Err(e) => page_form_with_error(&site_name, true, &form, &format!("Disk write failed: {e}"))
                .into_response(),
        },
        Err(e) => page_form_with_error(&site_name, true, &form, &e).into_response(),
    }
}

pub async fn edit_page_form(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path((site_name, slug)): Path<(String, String)>,
) -> Response {
    let site = Site(site_name.clone());
    match state.store.get_page(&site, &slug) {
        Ok(Some(p)) => {
            let toml_sections = sections_to_toml(&p.sections);
            views::page_form(
                &site_name,
                false,
                &p.front.title,
                &p.front.slug,
                &p.front.summary,
                p.front.status,
                p.front.layout,
                p.front.updated_at,
                p.front.nav_order,
                &toml_sections,
                None,
            )
            .into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "no such page").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("load: {e}")).into_response(),
    }
}

pub async fn update_page(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path((site_name, slug)): Path<(String, String)>,
    Form(form): Form<PageForm>,
) -> Response {
    if form.slug != slug {
        return (StatusCode::BAD_REQUEST, "slug cannot be changed via edit").into_response();
    }
    let site = Site(site_name.clone());
    match build_page(&form) {
        Ok(page) => {
            let path = state.store.page_path(&site, &slug);
            match page.write(&path) {
                Ok(()) => Redirect::to(&format!("/sites/{site_name}/pages")).into_response(),
                Err(e) => page_form_with_error(&site_name, false, &form, &format!("Disk write failed: {e}"))
                    .into_response(),
            }
        }
        Err(e) => page_form_with_error(&site_name, false, &form, &e).into_response(),
    }
}

pub async fn publish_page(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path((site_name, slug)): Path<(String, String)>,
) -> Response {
    if require_auth_or_401(&headers, &state).is_err() {
        return (StatusCode::UNAUTHORIZED, "not signed in").into_response();
    }
    let site = Site(site_name.clone());
    let path = state.store.page_path(&site, &slug);
    let mut page = match Page::load_from_file(&path) {
        Ok(p) => p,
        Err(e) => return (StatusCode::NOT_FOUND, format!("load: {e}")).into_response(),
    };
    page.front.status = PageStatus::Published;
    page.front.updated_at = Local::now().date_naive();
    if let Err(e) = page.write(&path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("write: {e}")).into_response();
    }
    Redirect::to(&format!("/sites/{site_name}/pages")).into_response()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn enumerate_sites(root: &StdPath) -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                out.push(name.to_string());
            }
        }
    }
    out.sort();
    Ok(out)
}

fn parse_date(s: &str) -> Result<NaiveDate, &'static str> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| "Invalid date — expected YYYY-MM-DD.")
}

fn parse_status(s: &str) -> BlogStatus {
    match s {
        "published" => BlogStatus::Published,
        _ => BlogStatus::Draft,
    }
}

fn form_with_error(site: &str, is_new: bool, form: &PostForm, error: &str) -> Markup {
    let date = parse_date(&form.date).unwrap_or_else(|_| Local::now().date_naive());
    let status = parse_status(&form.status);
    views::post_form(
        site,
        is_new,
        &form.title,
        &form.slug,
        date,
        &form.summary,
        &form.author,
        status,
        &form.body,
        Some(error),
    )
}

fn parse_layout(s: &str) -> PageLayout {
    match s {
        "wide" => PageLayout::Wide,
        "landing" => PageLayout::Landing,
        _ => PageLayout::Default,
    }
}

fn parse_page_status(s: &str) -> PageStatus {
    if s == "published" {
        PageStatus::Published
    } else {
        PageStatus::Draft
    }
}

/// Build a typed [`Page`] from raw form fields. Returns a human-
/// readable error string suitable to surface in the form.
fn build_page(form: &PageForm) -> Result<Page, String> {
    let updated_at = parse_date(&form.updated_at)
        .map_err(|e| e.to_string())?;
    let nav_order = form
        .nav_order
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u32>().map_err(|_| "Invalid nav order — must be 0-9999.".to_string()))
        .transpose()?;

    // Parse sections. The TOML the editor types is a fragment that
    // we wrap with `sections =` to feed serde_toml. Validation on
    // the resulting Page catches per-section issues with typed
    // messages.
    let wrapped = format!("{}\n", form.sections.trim());
    #[derive(Deserialize)]
    struct SectionsWrapper {
        sections: Vec<Section>,
    }
    let parsed: SectionsWrapper = toml::from_str(&format!("sections = [\n{wrapped}\n]"))
        .or_else(|_| {
            // Allow the user to write `[[sections]]` blocks directly
            // without bracketing — simpler for hand-editing.
            toml::from_str(&wrapped)
        })
        .map_err(|e| format!("Sections TOML: {e}"))?;

    let page = Page {
        front: PageFrontmatter {
            title: form.title.clone(),
            slug: form.slug.clone(),
            summary: form.summary.clone(),
            status: parse_page_status(&form.status),
            layout: parse_layout(&form.layout),
            updated_at,
            nav_order,
        },
        sections: parsed.sections,
    };
    page.validate(StdPath::new(&format!("{}.toml", form.slug)))
        .map_err(|e| e.to_string())?;
    Ok(page)
}

/// Re-emit a `Vec<Section>` as the TOML fragment the editor sees.
/// We serialize a wrapper struct and strip the wrapping `[[sections]]`
/// table-array key so the editor's textarea contains only the
/// section blocks.
fn sections_to_toml(sections: &[Section]) -> String {
    #[derive(serde::Serialize)]
    struct Wrap<'a> {
        sections: &'a [Section],
    }
    let full = toml::to_string_pretty(&Wrap { sections }).unwrap_or_default();
    full
}

/// Default starter content for a new page — gives editors a
/// working template instead of an empty textarea.
fn include_starter_sections() -> String {
    r#"[[sections]]
kind = "hero"
headline = "New page"
subhead = "Replace this subhead with a one-sentence summary of what the page is for."

[[sections]]
kind = "prose"
markdown = "Replace this prose with the page body. Markdown is fine: **bold**, _italic_, [links](https://example.com)."
"#
    .to_string()
}

fn page_form_with_error(site: &str, is_new: bool, form: &PageForm, error: &str) -> Markup {
    let updated_at = parse_date(&form.updated_at).unwrap_or_else(|_| Local::now().date_naive());
    let nav_order = form
        .nav_order
        .as_deref()
        .and_then(|s| s.trim().parse::<u32>().ok());
    views::page_form(
        site,
        is_new,
        &form.title,
        &form.slug,
        &form.summary,
        parse_page_status(&form.status),
        parse_layout(&form.layout),
        updated_at,
        nav_order,
        &form.sections,
        Some(error),
    )
}
