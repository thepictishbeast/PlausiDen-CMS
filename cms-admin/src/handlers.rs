//! HTTP handlers — bind URL routes to template + storage calls.

use axum::Form;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{Local, NaiveDate};
use maud::Markup;
use plausiden_cms_core::{
    AuditAction, AuditEvent, BlogPost, BlogPostFrontmatter, BlogStatus, CallToAction, Card, Page,
    PageFrontmatter, PageLayout, PageStatus, Section, Site,
};
use serde::Deserialize;

use crate::auth::{
    AuthSession, clear_cookie, extract_token, generate_session_token, require_auth_or_401,
    set_cookie,
};
use crate::state::{AppState, Session};
use crate::views;

/// Health check — bypasses auth so a load balancer can probe it.
pub async fn healthz() -> &'static str {
    "ok"
}

/// Audit log viewer — auth-gated, tails the last 200 events.
pub async fn audit_view(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
) -> Response {
    let events = state.audit.tail(200).unwrap_or_default();
    views::audit_page(&events, &state.audit.path().display().to_string()).into_response()
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
        let _ = state
            .audit
            .append(&AuditEvent::now("(no-site)", "(unauthenticated)", AuditAction::LoginFailed));
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
    let _ = state.audit.append(&AuditEvent::now("(all)", "admin", AuditAction::Login));
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
    let _ = state.audit.append(&AuditEvent::now("(all)", "admin", AuditAction::Logout));
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
    let _ = state.audit.append(&AuditEvent::now(
        &site_name,
        "admin",
        AuditAction::PostCreated {
            slug: form.slug.clone(),
        },
    ));
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
    let _ = state.audit.append(&AuditEvent::now(
        &site_name,
        "admin",
        AuditAction::PostUpdated { slug: slug.clone() },
    ));
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
    let _ = state.audit.append(&AuditEvent::now(
        &site_name,
        "admin",
        AuditAction::PostPublished { slug: slug.clone() },
    ));
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
    views::new_page_form_view(
        &site_name,
        "",
        "",
        "",
        PageStatus::Draft,
        PageLayout::Default,
        Local::now().date_naive(),
        None,
        None,
    )
}

#[derive(Debug, Deserialize)]
pub struct NewPageForm {
    pub title: String,
    pub slug: String,
    pub summary: String,
    pub status: String,
    pub layout: String,
    pub updated_at: String,
    pub nav_order: Option<String>,
}

/// New-page form posts here — frontmatter only. Created with one
/// default Hero section so the editor lands on a non-empty page
/// when redirected to the edit view.
pub async fn create_page(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path(site_name): Path<String>,
    Form(form): Form<NewPageForm>,
) -> Response {
    let site = Site(site_name.clone());
    let path = state.store.page_path(&site, &form.slug);
    if path.exists() {
        return new_page_form_with_error(&site_name, &form, "A page with that slug already exists.")
            .into_response();
    }
    let updated_at = match parse_date(&form.updated_at) {
        Ok(d) => d,
        Err(e) => return new_page_form_with_error(&site_name, &form, e).into_response(),
    };
    let nav_order = match parse_nav_order(form.nav_order.as_deref()) {
        Ok(n) => n,
        Err(e) => return new_page_form_with_error(&site_name, &form, &e).into_response(),
    };
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
        sections: vec![Section::Hero {
            eyebrow: None,
            headline: form.title.clone(),
            subhead: form.summary.clone(),
            cta: None,
        }],
    };
    if let Err(e) = page.validate(&path) {
        return new_page_form_with_error(&site_name, &form, &e.to_string()).into_response();
    }
    if let Err(e) = page.write(&path) {
        return new_page_form_with_error(&site_name, &form, &format!("Disk write failed: {e}"))
            .into_response();
    }
    let _ = state.audit.append(&AuditEvent::now(
        &site_name,
        "admin",
        AuditAction::PageCreated {
            slug: form.slug.clone(),
        },
    ));
    Redirect::to(&format!("/sites/{site_name}/pages/{}/edit", form.slug)).into_response()
}

pub async fn edit_page_form(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path((site_name, slug)): Path<(String, String)>,
) -> Response {
    let site = Site(site_name.clone());
    match state.store.get_page(&site, &slug) {
        Ok(Some(p)) => views::edit_page_view(
            &site_name,
            &p.front.slug,
            &p.front.title,
            &p.front.summary,
            p.front.status,
            p.front.layout,
            p.front.updated_at,
            p.front.nav_order,
            &p.sections,
            None,
            None,
        )
        .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such page").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("load: {e}")).into_response(),
    }
}

/// Frontmatter-only update. Sections are managed via the per-section
/// endpoints below.
pub async fn update_page(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path((site_name, slug)): Path<(String, String)>,
    Form(form): Form<NewPageForm>,
) -> Response {
    if form.slug != slug {
        return (StatusCode::BAD_REQUEST, "slug cannot be changed via edit").into_response();
    }
    let site = Site(site_name.clone());
    let path = state.store.page_path(&site, &slug);
    let mut page = match Page::load_from_file(&path) {
        Ok(p) => p,
        Err(e) => return (StatusCode::NOT_FOUND, format!("load: {e}")).into_response(),
    };
    let updated_at = match parse_date(&form.updated_at) {
        Ok(d) => d,
        Err(e) => return reload_edit_with_error(&state, &site_name, &slug, e).await,
    };
    let nav_order = match parse_nav_order(form.nav_order.as_deref()) {
        Ok(n) => n,
        Err(e) => return reload_edit_with_error(&state, &site_name, &slug, &e).await,
    };
    page.front.title = form.title;
    page.front.summary = form.summary;
    page.front.status = parse_page_status(&form.status);
    page.front.layout = parse_layout(&form.layout);
    page.front.updated_at = updated_at;
    page.front.nav_order = nav_order;
    if let Err(e) = page.validate(&path) {
        return reload_edit_with_error(&state, &site_name, &slug, &e.to_string()).await;
    }
    if let Err(e) = page.write(&path) {
        return reload_edit_with_error(&state, &site_name, &slug, &format!("write: {e}")).await;
    }
    let _ = state.audit.append(&AuditEvent::now(
        &site_name,
        "admin",
        AuditAction::PageFrontmatterUpdated { slug: slug.clone() },
    ));
    Redirect::to(&format!("/sites/{site_name}/pages/{slug}/edit")).into_response()
}

// ---------------------------------------------------------------------------
// Per-section endpoints
// ---------------------------------------------------------------------------

pub async fn new_section_form(
    AuthSession(_): AuthSession,
    Path((site_name, slug, kind)): Path<(String, String, String)>,
) -> Response {
    match kind.as_str() {
        "hero" => views::hero_form(&site_name, &slug, None, "", "", "", "", "", None).into_response(),
        "prose" => views::prose_form(&site_name, &slug, None, "", None).into_response(),
        "cards" => views::cards_form(&site_name, &slug, None, "", &[], None).into_response(),
        "ctaband" => {
            views::ctaband_form(&site_name, &slug, None, "", "", "", None).into_response()
        }
        _ => (StatusCode::BAD_REQUEST, "unknown section kind").into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct HeroForm {
    pub eyebrow: String,
    pub headline: String,
    pub subhead: String,
    pub cta_label: String,
    pub cta_href: String,
}

#[derive(Debug, Deserialize)]
pub struct ProseForm {
    pub markdown: String,
}

#[derive(Debug, Deserialize)]
pub struct CardsForm {
    pub heading: String,
    pub card_0_heading: String,
    pub card_0_body: String,
    pub card_0_cta_label: String,
    pub card_0_cta_href: String,
    pub card_1_heading: String,
    pub card_1_body: String,
    pub card_1_cta_label: String,
    pub card_1_cta_href: String,
    pub card_2_heading: String,
    pub card_2_body: String,
    pub card_2_cta_label: String,
    pub card_2_cta_href: String,
    pub card_3_heading: String,
    pub card_3_body: String,
    pub card_3_cta_label: String,
    pub card_3_cta_href: String,
    pub card_4_heading: String,
    pub card_4_body: String,
    pub card_4_cta_label: String,
    pub card_4_cta_href: String,
    pub card_5_heading: String,
    pub card_5_body: String,
    pub card_5_cta_label: String,
    pub card_5_cta_href: String,
}

#[derive(Debug, Deserialize)]
pub struct CtaBandForm {
    pub headline: String,
    pub cta_label: String,
    pub cta_href: String,
}

pub async fn create_section_hero(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path((site_name, slug, kind)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    save_section_kind(&state, &site_name, &slug, &kind, None, &body, &headers).await
}

pub async fn edit_section_form(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path((site_name, slug, idx)): Path<(String, String, usize)>,
) -> Response {
    let site = Site(site_name.clone());
    let page = match state.store.get_page(&site, &slug) {
        Ok(Some(p)) => p,
        _ => return (StatusCode::NOT_FOUND, "no such page").into_response(),
    };
    let Some(section) = page.sections.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such section").into_response();
    };
    match section {
        Section::Hero { eyebrow, headline, subhead, cta } => {
            let (lbl, href) = cta
                .as_ref()
                .map_or((String::new(), String::new()), |c| (c.label.clone(), c.href.clone()));
            views::hero_form(
                &site_name,
                &slug,
                Some(idx),
                eyebrow.as_deref().unwrap_or(""),
                headline,
                subhead,
                &lbl,
                &href,
                None,
            )
            .into_response()
        }
        Section::Prose { markdown } => {
            views::prose_form(&site_name, &slug, Some(idx), markdown, None).into_response()
        }
        Section::Cards { heading, items } => views::cards_form(
            &site_name,
            &slug,
            Some(idx),
            heading.as_deref().unwrap_or(""),
            items,
            None,
        )
        .into_response(),
        Section::CtaBand { headline, cta } => views::ctaband_form(
            &site_name,
            &slug,
            Some(idx),
            headline,
            &cta.label,
            &cta.href,
            None,
        )
        .into_response(),
    }
}

pub async fn update_section(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path((site_name, slug, idx)): Path<(String, String, usize)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let site = Site(site_name.clone());
    let page = match state.store.get_page(&site, &slug) {
        Ok(Some(p)) => p,
        _ => return (StatusCode::NOT_FOUND, "no such page").into_response(),
    };
    let Some(existing) = page.sections.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such section").into_response();
    };
    let kind = match existing {
        Section::Hero { .. } => "hero",
        Section::Prose { .. } => "prose",
        Section::Cards { .. } => "cards",
        Section::CtaBand { .. } => "ctaband",
    };
    save_section_kind(&state, &site_name, &slug, kind, Some(idx), &body, &headers).await
}

pub async fn move_section_up(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path((site_name, slug, idx)): Path<(String, String, usize)>,
) -> Response {
    swap_sections(&state, &site_name, &slug, idx, idx.checked_sub(1)).await
}

pub async fn move_section_down(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path((site_name, slug, idx)): Path<(String, String, usize)>,
) -> Response {
    swap_sections(&state, &site_name, &slug, idx, Some(idx + 1)).await
}

pub async fn delete_section(
    AuthSession(_): AuthSession,
    State(state): State<AppState>,
    Path((site_name, slug, idx)): Path<(String, String, usize)>,
) -> Response {
    let site = Site(site_name.clone());
    let path = state.store.page_path(&site, &slug);
    let mut page = match Page::load_from_file(&path) {
        Ok(p) => p,
        Err(e) => return (StatusCode::NOT_FOUND, format!("load: {e}")).into_response(),
    };
    if idx >= page.sections.len() {
        return (StatusCode::BAD_REQUEST, "section index out of range").into_response();
    }
    page.sections.remove(idx);
    if let Err(e) = page.validate(&path) {
        return (StatusCode::BAD_REQUEST, format!("can't leave page in invalid state: {e}"))
            .into_response();
    }
    if let Err(e) = page.write(&path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("write: {e}")).into_response();
    }
    let _ = state.audit.append(&AuditEvent::now(
        &site_name,
        "admin",
        AuditAction::SectionDeleted {
            slug: slug.clone(),
            idx,
        },
    ));
    Redirect::to(&format!("/sites/{site_name}/pages/{slug}/edit")).into_response()
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
    let _ = state.audit.append(&AuditEvent::now(
        &site_name,
        "admin",
        AuditAction::PagePublished { slug: slug.clone() },
    ));
    Redirect::to(&format!("/sites/{site_name}/pages")).into_response()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn enumerate_sites(root: &std::path::Path) -> std::io::Result<Vec<String>> {
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

fn parse_nav_order(s: Option<&str>) -> Result<Option<u32>, String> {
    s.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u32>().map_err(|_| "Invalid nav order (0-9999).".to_string()))
        .transpose()
}

fn new_page_form_with_error(site: &str, form: &NewPageForm, error: &str) -> Markup {
    let updated_at = parse_date(&form.updated_at).unwrap_or_else(|_| Local::now().date_naive());
    let nav_order = parse_nav_order(form.nav_order.as_deref()).unwrap_or(None);
    views::new_page_form_view(
        site,
        &form.title,
        &form.slug,
        &form.summary,
        parse_page_status(&form.status),
        parse_layout(&form.layout),
        updated_at,
        nav_order,
        Some(error),
    )
}

async fn reload_edit_with_error(state: &AppState, site: &str, slug: &str, error: &str) -> Response {
    let site_obj = Site(site.to_string());
    match state.store.get_page(&site_obj, slug) {
        Ok(Some(p)) => views::edit_page_view(
            site,
            &p.front.slug,
            &p.front.title,
            &p.front.summary,
            p.front.status,
            p.front.layout,
            p.front.updated_at,
            p.front.nav_order,
            &p.sections,
            Some(error),
            None,
        )
        .into_response(),
        _ => (StatusCode::NOT_FOUND, "no such page").into_response(),
    }
}

/// Save (create or update) a section of the given kind. The form
/// body is the raw urlencoded bytes; we deserialize per-variant.
async fn save_section_kind(
    state: &AppState,
    site_name: &str,
    slug: &str,
    kind: &str,
    idx: Option<usize>,
    body: &[u8],
    _headers: &HeaderMap,
) -> Response {
    let site = Site(site_name.to_string());
    let path = state.store.page_path(&site, slug);
    let mut page = match Page::load_from_file(&path) {
        Ok(p) => p,
        Err(e) => return (StatusCode::NOT_FOUND, format!("load: {e}")).into_response(),
    };
    let new_section = match build_section(kind, body) {
        Ok(s) => s,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    let action = match idx {
        Some(i) if i < page.sections.len() => {
            page.sections[i] = new_section;
            AuditAction::SectionUpdated {
                slug: slug.to_string(),
                idx: i,
            }
        }
        Some(_) => return (StatusCode::NOT_FOUND, "section index out of range").into_response(),
        None => {
            page.sections.push(new_section);
            AuditAction::SectionAdded {
                slug: slug.to_string(),
                kind: kind.to_string(),
            }
        }
    };
    if let Err(e) = page.validate(&path) {
        return (StatusCode::BAD_REQUEST, format!("validation: {e}")).into_response();
    }
    if let Err(e) = page.write(&path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("write: {e}")).into_response();
    }
    let _ = state.audit.append(&AuditEvent::now(site_name, "admin", action));
    Redirect::to(&format!("/sites/{site_name}/pages/{slug}/edit")).into_response()
}

fn build_section(kind: &str, body: &[u8]) -> Result<Section, String> {
    match kind {
        "hero" => {
            let f: HeroForm =
                serde_urlencoded::from_bytes(body).map_err(|e| format!("hero form: {e}"))?;
            let cta = if f.cta_label.trim().is_empty() {
                None
            } else {
                Some(CallToAction {
                    label: f.cta_label,
                    href: f.cta_href,
                })
            };
            Ok(Section::Hero {
                eyebrow: option_from_string(f.eyebrow),
                headline: f.headline,
                subhead: f.subhead,
                cta,
            })
        }
        "prose" => {
            let f: ProseForm =
                serde_urlencoded::from_bytes(body).map_err(|e| format!("prose form: {e}"))?;
            Ok(Section::Prose { markdown: f.markdown })
        }
        "cards" => {
            let f: CardsForm =
                serde_urlencoded::from_bytes(body).map_err(|e| format!("cards form: {e}"))?;
            let raw = [
                (f.card_0_heading, f.card_0_body, f.card_0_cta_label, f.card_0_cta_href),
                (f.card_1_heading, f.card_1_body, f.card_1_cta_label, f.card_1_cta_href),
                (f.card_2_heading, f.card_2_body, f.card_2_cta_label, f.card_2_cta_href),
                (f.card_3_heading, f.card_3_body, f.card_3_cta_label, f.card_3_cta_href),
                (f.card_4_heading, f.card_4_body, f.card_4_cta_label, f.card_4_cta_href),
                (f.card_5_heading, f.card_5_body, f.card_5_cta_label, f.card_5_cta_href),
            ];
            let items: Vec<Card> = raw
                .into_iter()
                .filter(|(h, b, _, _)| !h.trim().is_empty() && !b.trim().is_empty())
                .map(|(h, b, cl, ch)| {
                    let cta = if cl.trim().is_empty() {
                        None
                    } else {
                        Some(CallToAction { label: cl, href: ch })
                    };
                    Card {
                        heading: h,
                        body: b,
                        cta,
                    }
                })
                .collect();
            if items.is_empty() {
                return Err("Cards section needs at least one filled card slot.".into());
            }
            Ok(Section::Cards {
                heading: option_from_string(f.heading),
                items,
            })
        }
        "ctaband" => {
            let f: CtaBandForm =
                serde_urlencoded::from_bytes(body).map_err(|e| format!("ctaband form: {e}"))?;
            Ok(Section::CtaBand {
                headline: f.headline,
                cta: CallToAction {
                    label: f.cta_label,
                    href: f.cta_href,
                },
            })
        }
        _ => Err(format!("unknown kind: {kind}")),
    }
}

fn option_from_string(s: String) -> Option<String> {
    if s.trim().is_empty() { None } else { Some(s) }
}

async fn swap_sections(
    state: &AppState,
    site_name: &str,
    slug: &str,
    idx: usize,
    other: Option<usize>,
) -> Response {
    let Some(other_idx) = other else {
        return (StatusCode::BAD_REQUEST, "out of range").into_response();
    };
    let site = Site(site_name.to_string());
    let path = state.store.page_path(&site, slug);
    let mut page = match Page::load_from_file(&path) {
        Ok(p) => p,
        Err(e) => return (StatusCode::NOT_FOUND, format!("load: {e}")).into_response(),
    };
    if idx >= page.sections.len() || other_idx >= page.sections.len() {
        return (StatusCode::BAD_REQUEST, "out of range").into_response();
    }
    page.sections.swap(idx, other_idx);
    if let Err(e) = page.write(&path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("write: {e}")).into_response();
    }
    let _ = state.audit.append(&AuditEvent::now(
        site_name,
        "admin",
        AuditAction::SectionMoved {
            slug: slug.to_string(),
            from: idx,
            to: other_idx,
        },
    ));
    Redirect::to(&format!("/sites/{site_name}/pages/{slug}/edit")).into_response()
}
