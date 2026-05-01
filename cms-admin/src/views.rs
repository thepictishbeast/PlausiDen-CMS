//! Maud templates. v0 is single-file, deliberate: every page reads
//! top-to-bottom, no template inheritance to chase. When the page
//! count grows, we extract a layout helper. Until then the
//! repetition is part of the readability story.

use chrono::NaiveDate;
use maud::{DOCTYPE, Markup, html};
use plausiden_cms_core::{
    AuditAction, AuditEvent, BlogPost, BlogStatus, CallToAction, Card, Page, PageLayout,
    PageStatus, Section,
};

/// Page chrome: head, top bar, footer.
fn shell(title: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " — PlausiDen-CMS" }
                style { (PreEscaped(BASE_CSS)) }
            }
            body {
                header {
                    nav {
                        a href="/sites" class="brand" { "PlausiDen-CMS" }
                        div class="nav-links" {
                            a href="/audit" class="nav-link" { "Audit log" }
                            form action="/logout" method="post" class="logout-form" {
                                button type="submit" class="link-button" { "Log out" }
                            }
                        }
                    }
                }
                main { (body) }
                footer {
                    p { "PlausiDen-CMS · content stored as files in git" }
                }
            }
        }
    }
}

use maud::PreEscaped;

/// All styles in one block — `<style>` is allowed in the admin UI
/// (its CSP is more permissive than the public-facing site's).
/// Kept short on purpose: any new style here is a doctrine
/// suggestion that should land in PlausiDen-Loom proper.
const BASE_CSS: &str = r"
:root { --ink: #0f172a; --ink-muted: #475569; --primary: hsl(220 90% 28%); --surface: #fff; --surface-muted: #f8fafc; --border: #e2e8f0; --danger: hsl(0 72% 51%); --success: hsl(160 84% 30%); }
* { box-sizing: border-box; }
body { font-family: system-ui, -apple-system, sans-serif; margin: 0; color: var(--ink); background: var(--surface-muted); line-height: 1.5; }
header { background: var(--surface); border-bottom: 1px solid var(--border); padding: 0.75rem 1.5rem; }
nav { max-width: 70rem; margin: 0 auto; display: flex; align-items: center; justify-content: space-between; }
.brand { font-weight: 700; color: var(--primary); text-decoration: none; font-size: 1.125rem; }
.logout-form { margin: 0; }
.link-button { background: none; border: none; color: var(--ink-muted); cursor: pointer; font: inherit; padding: 0; text-decoration: underline; }
main { max-width: 70rem; margin: 2rem auto; padding: 0 1.5rem; }
footer { max-width: 70rem; margin: 3rem auto 1.5rem; padding: 0 1.5rem; color: var(--ink-muted); font-size: 0.875rem; }
h1 { font-size: 1.875rem; margin: 0 0 1rem; }
h2 { font-size: 1.25rem; margin: 1.5rem 0 0.5rem; }
.card { background: var(--surface); border: 1px solid var(--border); border-radius: 0.5rem; padding: 1.5rem; margin-bottom: 1rem; }
table { width: 100%; border-collapse: collapse; }
th, td { text-align: left; padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border); }
th { font-weight: 600; color: var(--ink-muted); font-size: 0.875rem; text-transform: uppercase; letter-spacing: 0.025em; }
tr:hover td { background: var(--surface-muted); }
a { color: var(--primary); }
.btn { display: inline-block; padding: 0.5rem 1rem; border-radius: 0.375rem; background: var(--primary); color: white; text-decoration: none; border: none; cursor: pointer; font: inherit; }
.btn-secondary { background: var(--surface); color: var(--ink); border: 1px solid var(--border); }
.btn-danger { background: var(--danger); }
.btn-success { background: var(--success); }
.row-actions { display: flex; gap: 0.5rem; align-items: center; }
.row-actions form { margin: 0; }
form.stack { display: flex; flex-direction: column; gap: 0.75rem; max-width: 40rem; }
form.stack label { font-weight: 600; color: var(--ink); font-size: 0.875rem; display: block; margin-bottom: 0.25rem; }
form.stack input[type=text], form.stack input[type=password], form.stack input[type=date], form.stack textarea, form.stack select { width: 100%; padding: 0.5rem 0.75rem; border: 1px solid var(--border); border-radius: 0.375rem; font: inherit; background: var(--surface); }
form.stack textarea { font-family: ui-monospace, 'SF Mono', monospace; min-height: 18rem; }
.muted { color: var(--ink-muted); font-size: 0.875rem; }
.content-tabs { display: flex; gap: 0; border-bottom: 1px solid var(--border); margin-bottom: 1.5rem; }
.tab { padding: 0.75rem 1.25rem; text-decoration: none; color: var(--ink-muted); border-bottom: 2px solid transparent; }
.tab-active { color: var(--primary); border-bottom-color: var(--primary); font-weight: 600; }
.nav-links { display: flex; align-items: center; gap: 1rem; }
.nav-link { color: var(--ink-muted); text-decoration: none; font-size: 0.9375rem; }
.nav-link:hover { color: var(--primary); }
.audit-table { font-family: ui-monospace, 'SF Mono', monospace; font-size: 0.8125rem; }
.audit-table .ts { color: var(--ink-muted); white-space: nowrap; }
.audit-action-login { color: var(--success); }
.audit-action-login_failed { color: var(--danger); font-weight: 600; }
.audit-action-page_published, .audit-action-post_published { color: var(--success); font-weight: 600; }
.audit-action-section_deleted { color: var(--danger); }
.layout-default { color: var(--ink-muted); }
.layout-wide { color: hsl(40 90% 35%); font-weight: 600; }
.layout-landing { color: var(--primary); font-weight: 600; }
.section-card { background: var(--surface); border: 1px solid var(--border); border-radius: 0.5rem; padding: 1rem 1.25rem; margin-bottom: 0.75rem; display: grid; grid-template-columns: 1fr auto; gap: 0.75rem; align-items: start; }
.section-card .meta { display: flex; flex-direction: column; gap: 0.25rem; min-width: 0; }
.section-card .kind-badge { display: inline-block; padding: 0.125rem 0.5rem; background: var(--surface-muted); border: 1px solid var(--border); border-radius: 0.25rem; font-size: 0.75rem; font-weight: 600; color: var(--ink-muted); text-transform: uppercase; letter-spacing: 0.05em; align-self: flex-start; }
.section-card .preview { color: var(--ink); font-size: 0.9375rem; word-break: break-word; }
.section-card .preview-sub { color: var(--ink-muted); font-size: 0.8125rem; margin-top: 0.125rem; }
.section-card .actions { display: flex; gap: 0.25rem; align-items: center; }
.section-card .actions form { margin: 0; }
.section-card .actions button, .section-card .actions a { padding: 0.25rem 0.5rem; font-size: 0.875rem; }
.add-section { display: flex; gap: 0.5rem; flex-wrap: wrap; padding: 1rem; background: var(--surface-muted); border: 1px dashed var(--border); border-radius: 0.5rem; }
.add-section form { margin: 0; }
.add-section button { background: var(--surface); color: var(--ink); border: 1px solid var(--border); padding: 0.375rem 0.75rem; border-radius: 0.375rem; cursor: pointer; font: inherit; font-size: 0.875rem; }
.add-section button:hover { background: var(--primary); color: white; border-color: var(--primary); }
.card-slot { background: var(--surface-muted); border: 1px solid var(--border); border-radius: 0.375rem; padding: 0.75rem; margin-bottom: 0.5rem; }
.card-slot .stack { gap: 0.375rem; max-width: none; }
.status-draft { color: var(--ink-muted); font-weight: 600; }
.status-published { color: var(--success); font-weight: 600; }
.error { background: hsl(0 72% 95%); border: 1px solid var(--danger); color: var(--danger); padding: 0.75rem 1rem; border-radius: 0.375rem; margin-bottom: 1rem; }
.success-msg { background: hsl(160 84% 95%); border: 1px solid var(--success); color: var(--success); padding: 0.75rem 1rem; border-radius: 0.375rem; margin-bottom: 1rem; }
";

/// Login form.
#[must_use]
pub fn login_page(error: Option<&str>) -> Markup {
    let body = html! {
        h1 { "Sign in" }
        p class="muted" { "PlausiDen-CMS admin." }
        @if let Some(e) = error {
            div class="error" { (e) }
        }
        form action="/login" method="post" class="stack card" {
            label for="token" { "Admin token" }
            input type="password" id="token" name="token" required autofocus;
            button type="submit" class="btn" { "Sign in" }
        }
    };
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Sign in — PlausiDen-CMS" }
                style { (PreEscaped(BASE_CSS)) }
            }
            body {
                main { (body) }
            }
        }
    }
}

/// Site list.
#[must_use]
pub fn sites_page(sites: &[String], root: &str) -> Markup {
    let body = html! {
        h1 { "Sites" }
        p class="muted" { "Content tree: " code { (root) } }
        @if sites.is_empty() {
            div class="card" {
                p { "No sites yet. Create one by adding a directory under "
                    code { (root) } "." }
            }
        } @else {
            div class="card" {
                table {
                    thead { tr { th { "Site" } th { } } }
                    tbody {
                        @for s in sites {
                            tr {
                                td { (s) }
                                td {
                                    a href={ "/sites/" (s) } { "Manage →" }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    shell("Sites", body)
}

/// Blog post list for a site.
#[must_use]
pub fn posts_page(site: &str, posts: &[BlogPost], flash: Option<&str>) -> Markup {
    let body = html! {
        nav class="muted" {
            a href="/sites" { "Sites" } " / " (site)
        }
        div class="content-tabs" {
            a href={ "/sites/" (site) } class="tab tab-active" { "Blog posts" }
            a href={ "/sites/" (site) "/pages" } class="tab" { "Pages" }
        }
        h1 { (site) " — Blog posts" }
        @if let Some(f) = flash {
            div class="success-msg" { (f) }
        }
        div class="card" {
            div style="margin-bottom: 1rem;" {
                a href={ "/sites/" (site) "/blog/new" } class="btn" { "+ New post" }
            }
            @if posts.is_empty() {
                p class="muted" { "No posts yet. Click " strong { "+ New post" } " to write one." }
            } @else {
                table {
                    thead {
                        tr {
                            th { "Title" }
                            th { "Slug" }
                            th { "Date" }
                            th { "Status" }
                            th { }
                        }
                    }
                    tbody {
                        @for p in posts {
                            tr {
                                td { (p.front.title) }
                                td { code { (p.front.slug) } }
                                td { (p.front.date) }
                                td {
                                    @match p.front.status {
                                        BlogStatus::Draft => span class="status-draft" { "Draft" },
                                        BlogStatus::Published => span class="status-published" { "Published" },
                                    }
                                }
                                td class="row-actions" {
                                    a href={ "/sites/" (site) "/blog/" (p.front.slug) "/edit" } { "Edit" }
                                    @if p.front.status == BlogStatus::Draft {
                                        form action={ "/sites/" (site) "/blog/" (p.front.slug) "/publish" } method="post" {
                                            button type="submit" class="btn btn-success" style="font-size: 0.875rem; padding: 0.25rem 0.625rem;" { "Publish" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    shell(&format!("{site} blog"), body)
}

/// Pages list for a site.
#[must_use]
pub fn pages_page(site: &str, pages: &[Page], flash: Option<&str>) -> Markup {
    let body = html! {
        nav class="muted" {
            a href="/sites" { "Sites" } " / " (site)
        }
        div class="content-tabs" {
            a href={ "/sites/" (site) } class="tab" { "Blog posts" }
            a href={ "/sites/" (site) "/pages" } class="tab tab-active" { "Pages" }
        }
        h1 { (site) " — Pages" }
        @if let Some(f) = flash {
            div class="success-msg" { (f) }
        }
        div class="card" {
            div style="margin-bottom: 1rem;" {
                a href={ "/sites/" (site) "/pages/new" } class="btn" { "+ New page" }
            }
            @if pages.is_empty() {
                p class="muted" { "No pages yet. Pages are typed-section composed (Hero, Prose, Cards, CtaBand) and live at " code { "content/" (site) "/pages/" } "." }
            } @else {
                table {
                    thead {
                        tr {
                            th { "Title" }
                            th { "Slug" }
                            th { "Layout" }
                            th { "Nav" }
                            th { "Updated" }
                            th { "Status" }
                            th { }
                        }
                    }
                    tbody {
                        @for p in pages {
                            tr {
                                td { (p.front.title) }
                                td { code { (p.front.slug) } }
                                td {
                                    @match p.front.layout {
                                        PageLayout::Default => span class="layout-default" { "default" },
                                        PageLayout::Wide => span class="layout-wide" { "wide" },
                                        PageLayout::Landing => span class="layout-landing" { "landing" },
                                    }
                                }
                                td {
                                    @match p.front.nav_order {
                                        Some(n) => (n.to_string()),
                                        None => span class="muted" { "—" },
                                    }
                                }
                                td { (p.front.updated_at) }
                                td {
                                    @match p.front.status {
                                        PageStatus::Draft => span class="status-draft" { "Draft" },
                                        PageStatus::Published => span class="status-published" { "Published" },
                                    }
                                }
                                td class="row-actions" {
                                    a href={ "/sites/" (site) "/pages/" (p.front.slug) "/edit" } { "Edit" }
                                    @if p.front.status == PageStatus::Draft {
                                        form action={ "/sites/" (site) "/pages/" (p.front.slug) "/publish" } method="post" {
                                            button type="submit" class="btn btn-success" style="font-size: 0.875rem; padding: 0.25rem 0.625rem;" { "Publish" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    shell(&format!("{site} pages"), body)
}

/// New-page form — frontmatter only. Sections are added once the
/// page exists, via the section list on the edit view.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn new_page_form_view(
    site: &str,
    title: &str,
    slug: &str,
    summary: &str,
    status: PageStatus,
    layout: PageLayout,
    updated_at: NaiveDate,
    nav_order: Option<u32>,
    error: Option<&str>,
) -> Markup {
    let body = html! {
        nav class="muted" {
            a href="/sites" { "Sites" } " / "
            a href={ "/sites/" (site) } { (site) } " / "
            a href={ "/sites/" (site) "/pages" } { "Pages" } " / "
            "New page"
        }
        h1 { "New page" }
        p class="muted" { "Save the page first; you'll add sections on the next screen." }
        @if let Some(e) = error {
            div class="error" { (e) }
        }
        form action={ "/sites/" (site) "/pages/new" } method="post" class="stack card" {
            (frontmatter_inputs(true, title, slug, summary, status, layout, updated_at, nav_order))
            div class="row-actions" {
                button type="submit" class="btn" { "Create draft" }
                a href={ "/sites/" (site) "/pages" } class="btn btn-secondary" { "Cancel" }
            }
        }
    };
    shell("New page", body)
}

/// Edit-page view — frontmatter form + section list with per-section
/// edit/move/delete + add-section variant picker. This is the main
/// graphical editor for content authors.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn edit_page_view(
    site: &str,
    slug: &str,
    title: &str,
    summary: &str,
    status: PageStatus,
    layout: PageLayout,
    updated_at: NaiveDate,
    nav_order: Option<u32>,
    sections: &[Section],
    error: Option<&str>,
    flash: Option<&str>,
) -> Markup {
    let body = html! {
        nav class="muted" {
            a href="/sites" { "Sites" } " / "
            a href={ "/sites/" (site) } { (site) } " / "
            a href={ "/sites/" (site) "/pages" } { "Pages" } " / "
            "Edit"
        }
        h1 { "Edit: " (title) }
        @if let Some(f) = flash { div class="success-msg" { (f) } }
        @if let Some(e) = error { div class="error" { (e) } }

        h2 { "Frontmatter" }
        form action={ "/sites/" (site) "/pages/" (slug) "/edit" } method="post" class="stack card" {
            (frontmatter_inputs(false, title, slug, summary, status, layout, updated_at, nav_order))
            div class="row-actions" {
                button type="submit" class="btn" { "Save frontmatter" }
            }
        }

        h2 { "Sections" }
        @if sections.is_empty() {
            div class="card" {
                p class="muted" { "No sections yet. Add one below." }
            }
        } @else {
            @for (idx, section) in sections.iter().enumerate() {
                (section_card(site, slug, idx, sections.len(), section))
            }
        }

        div class="add-section" style="margin-top: 1rem;" {
            span class="muted" style="margin-right: 0.5rem; align-self: center;" { "Add section:" }
            form action={ "/sites/" (site) "/pages/" (slug) "/sections/new/hero" } method="get" {
                button type="submit" { "+ Hero" }
            }
            form action={ "/sites/" (site) "/pages/" (slug) "/sections/new/prose" } method="get" {
                button type="submit" { "+ Prose" }
            }
            form action={ "/sites/" (site) "/pages/" (slug) "/sections/new/cards" } method="get" {
                button type="submit" { "+ Cards" }
            }
            form action={ "/sites/" (site) "/pages/" (slug) "/sections/new/ctaband" } method="get" {
                button type="submit" { "+ CTA Band" }
            }
        }
    };
    shell(&format!("Edit {title}"), body)
}

/// Render one section as a card with preview + action buttons.
fn section_card(site: &str, slug: &str, idx: usize, total: usize, section: &Section) -> Markup {
    let (kind, preview, sub) = match section {
        Section::Hero { headline, subhead, .. } => ("Hero", headline.clone(), subhead.clone()),
        Section::Prose { markdown } => {
            let first_line = markdown.lines().next().unwrap_or("").to_string();
            let snippet = if first_line.len() > 80 {
                format!("{}…", &first_line[..80])
            } else {
                first_line
            };
            ("Prose", snippet, format!("{} chars", markdown.len()))
        }
        Section::Cards { heading, items } => (
            "Cards",
            heading.clone().unwrap_or_default(),
            format!("{} card{}", items.len(), if items.len() == 1 { "" } else { "s" }),
        ),
        Section::CtaBand { headline, cta } => {
            ("CTA Band", headline.clone(), format!("→ {}", cta.label))
        }
    };
    html! {
        div class="section-card" {
            div class="meta" {
                span class="kind-badge" { (kind) }
                div class="preview" { (preview) }
                @if !sub.is_empty() {
                    div class="preview-sub" { (sub) }
                }
            }
            div class="actions" {
                a href={ "/sites/" (site) "/pages/" (slug) "/sections/" (idx) "/edit" } class="btn btn-secondary" { "Edit" }
                @if idx > 0 {
                    form action={ "/sites/" (site) "/pages/" (slug) "/sections/" (idx) "/up" } method="post" {
                        button type="submit" class="btn btn-secondary" title="Move up" { "↑" }
                    }
                }
                @if idx + 1 < total {
                    form action={ "/sites/" (site) "/pages/" (slug) "/sections/" (idx) "/down" } method="post" {
                        button type="submit" class="btn btn-secondary" title="Move down" { "↓" }
                    }
                }
                form action={ "/sites/" (site) "/pages/" (slug) "/sections/" (idx) "/delete" } method="post" onsubmit="return confirm('Delete this section?');" {
                    button type="submit" class="btn btn-danger" title="Delete" { "✕" }
                }
            }
        }
    }
}

fn frontmatter_inputs(
    is_new: bool,
    title: &str,
    slug: &str,
    summary: &str,
    status: PageStatus,
    layout: PageLayout,
    updated_at: NaiveDate,
    nav_order: Option<u32>,
) -> Markup {
    html! {
        div {
            label for="title" { "Title" }
            input type="text" id="title" name="title" value=(title) required;
        }
        div {
            label for="slug" { "Slug" }
            input type="text" id="slug" name="slug" value=(slug) required readonly[!is_new];
            p class="muted" { "Lowercase ASCII + dashes. Cannot change after creation." }
        }
        div {
            label for="summary" { "Summary" }
            input type="text" id="summary" name="summary" value=(summary) maxlength="200" required;
            p class="muted" { "Used as " code { "<meta name=\"description\">" } ". ≤200 chars." }
        }
        div {
            label for="layout" { "Layout" }
            select id="layout" name="layout" {
                option value="default" selected[layout == PageLayout::Default] { "Default — standard column" }
                option value="wide"    selected[layout == PageLayout::Wide]    { "Wide — hero breaks out" }
                option value="landing" selected[layout == PageLayout::Landing] { "Landing — full-bleed" }
            }
        }
        div {
            label for="nav_order" { "Nav order (optional)" }
            input type="number" id="nav_order" name="nav_order" value=[nav_order.map(|n| n.to_string())] min="0" max="9999";
            p class="muted" { "Lower numbers come first in the main nav. Leave blank to omit." }
        }
        div {
            label for="updated_at" { "Last updated" }
            input type="date" id="updated_at" name="updated_at" value=(updated_at.format("%Y-%m-%d").to_string()) required;
        }
        div {
            label for="status" { "Status" }
            select id="status" name="status" {
                option value="draft" selected[status == PageStatus::Draft] { "Draft" }
                option value="published" selected[status == PageStatus::Published] { "Published" }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Per-section edit forms — one per Section variant
// ---------------------------------------------------------------------------

/// Hero section form — eyebrow / headline / subhead / optional CTA.
#[must_use]
pub fn hero_form(
    site: &str,
    slug: &str,
    section_idx: Option<usize>,
    eyebrow: &str,
    headline: &str,
    subhead: &str,
    cta_label: &str,
    cta_href: &str,
    error: Option<&str>,
) -> Markup {
    let action = section_action(site, slug, "hero", section_idx);
    let title = if section_idx.is_some() { "Edit Hero section" } else { "New Hero section" };
    let body = html! {
        (section_breadcrumb(site, slug, title))
        @if let Some(e) = error { div class="error" { (e) } }
        form action=(action) method="post" class="stack card" {
            div {
                label for="eyebrow" { "Eyebrow (small text above headline)" }
                input type="text" id="eyebrow" name="eyebrow" value=(eyebrow);
                p class="muted" { "Optional." }
            }
            div {
                label for="headline" { "Headline" }
                input type="text" id="headline" name="headline" value=(headline) required;
            }
            div {
                label for="subhead" { "Subhead" }
                input type="text" id="subhead" name="subhead" value=(subhead) required;
            }
            div {
                label for="cta_label" { "CTA label (optional)" }
                input type="text" id="cta_label" name="cta_label" value=(cta_label);
            }
            div {
                label for="cta_href" { "CTA link (required if label is set)" }
                input type="text" id="cta_href" name="cta_href" value=(cta_href);
                p class="muted" { "Internal path like " code { "/contact" } " or absolute URL." }
            }
            div class="row-actions" {
                button type="submit" class="btn" { "Save section" }
                a href={ "/sites/" (site) "/pages/" (slug) "/edit" } class="btn btn-secondary" { "Cancel" }
            }
        }
    };
    shell(title, body)
}

/// Prose section form — markdown textarea.
#[must_use]
pub fn prose_form(
    site: &str,
    slug: &str,
    section_idx: Option<usize>,
    markdown: &str,
    error: Option<&str>,
) -> Markup {
    let action = section_action(site, slug, "prose", section_idx);
    let title = if section_idx.is_some() { "Edit Prose section" } else { "New Prose section" };
    let body = html! {
        (section_breadcrumb(site, slug, title))
        @if let Some(e) = error { div class="error" { (e) } }
        form action=(action) method="post" class="stack card" {
            div {
                label for="markdown" { "Markdown" }
                textarea id="markdown" name="markdown" required { (markdown) }
                p class="muted" { "Standard markdown. Bold, italic, links, lists, headers all supported." }
            }
            div class="row-actions" {
                button type="submit" class="btn" { "Save section" }
                a href={ "/sites/" (site) "/pages/" (slug) "/edit" } class="btn btn-secondary" { "Cancel" }
            }
        }
    };
    shell(title, body)
}

/// Cards section form — heading + 6 fixed card slots (empties dropped).
#[must_use]
pub fn cards_form(
    site: &str,
    slug: &str,
    section_idx: Option<usize>,
    heading: &str,
    cards: &[Card],
    error: Option<&str>,
) -> Markup {
    let action = section_action(site, slug, "cards", section_idx);
    let title = if section_idx.is_some() { "Edit Cards section" } else { "New Cards section" };
    let body = html! {
        (section_breadcrumb(site, slug, title))
        @if let Some(e) = error { div class="error" { (e) } }
        form action=(action) method="post" class="stack card" {
            div {
                label for="heading" { "Section heading (optional)" }
                input type="text" id="heading" name="heading" value=(heading);
            }
            p class="muted" { "Up to 6 cards. Leave a slot blank to omit it. Cards are shown in the order entered." }
            @for i in 0..6 {
                @let card = cards.get(i);
                div class="card-slot" {
                    strong { "Card " (i + 1) }
                    div class="stack" {
                        div {
                            label for=(format!("card_{i}_heading")) { "Heading" }
                            input type="text" id=(format!("card_{i}_heading")) name=(format!("card_{i}_heading")) value=[card.map(|c| &c.heading)];
                        }
                        div {
                            label for=(format!("card_{i}_body")) { "Body" }
                            input type="text" id=(format!("card_{i}_body")) name=(format!("card_{i}_body")) value=[card.map(|c| &c.body)];
                        }
                        div {
                            label for=(format!("card_{i}_cta_label")) { "CTA label (optional)" }
                            input type="text" id=(format!("card_{i}_cta_label")) name=(format!("card_{i}_cta_label")) value=[card.and_then(|c| c.cta.as_ref()).map(|c| &c.label)];
                        }
                        div {
                            label for=(format!("card_{i}_cta_href")) { "CTA link" }
                            input type="text" id=(format!("card_{i}_cta_href")) name=(format!("card_{i}_cta_href")) value=[card.and_then(|c| c.cta.as_ref()).map(|c| &c.href)];
                        }
                    }
                }
            }
            div class="row-actions" {
                button type="submit" class="btn" { "Save section" }
                a href={ "/sites/" (site) "/pages/" (slug) "/edit" } class="btn btn-secondary" { "Cancel" }
            }
        }
    };
    shell(title, body)
}

/// CTA-Band section form — headline + label + href.
#[must_use]
pub fn ctaband_form(
    site: &str,
    slug: &str,
    section_idx: Option<usize>,
    headline: &str,
    cta_label: &str,
    cta_href: &str,
    error: Option<&str>,
) -> Markup {
    let action = section_action(site, slug, "ctaband", section_idx);
    let title = if section_idx.is_some() { "Edit CTA Band section" } else { "New CTA Band section" };
    let body = html! {
        (section_breadcrumb(site, slug, title))
        @if let Some(e) = error { div class="error" { (e) } }
        form action=(action) method="post" class="stack card" {
            div {
                label for="headline" { "Headline" }
                input type="text" id="headline" name="headline" value=(headline) required;
            }
            div {
                label for="cta_label" { "Button label" }
                input type="text" id="cta_label" name="cta_label" value=(cta_label) required;
            }
            div {
                label for="cta_href" { "Button link" }
                input type="text" id="cta_href" name="cta_href" value=(cta_href) required;
            }
            div class="row-actions" {
                button type="submit" class="btn" { "Save section" }
                a href={ "/sites/" (site) "/pages/" (slug) "/edit" } class="btn btn-secondary" { "Cancel" }
            }
        }
    };
    shell(title, body)
}

fn section_action(site: &str, slug: &str, kind: &str, idx: Option<usize>) -> String {
    match idx {
        Some(i) => format!("/sites/{site}/pages/{slug}/sections/{i}/edit"),
        None => format!("/sites/{site}/pages/{slug}/sections/new/{kind}"),
    }
}

fn section_breadcrumb(site: &str, slug: &str, title: &str) -> Markup {
    html! {
        nav class="muted" {
            a href="/sites" { "Sites" } " / "
            a href={ "/sites/" (site) } { (site) } " / "
            a href={ "/sites/" (site) "/pages" } { "Pages" } " / "
            a href={ "/sites/" (site) "/pages/" (slug) "/edit" } { (slug) } " / "
            (title)
        }
        h1 { (title) }
    }
}

/// Audit log view — most-recent-first listing of admin actions.
#[must_use]
pub fn audit_page(events: &[AuditEvent], log_path: &str) -> Markup {
    let body = html! {
        h1 { "Audit log" }
        p class="muted" {
            "Append-only log at " code { (log_path) } ". Every admin action lands a JSON line; "
            "the log answers " strong { "who changed what when" } ", never " strong { "what the change looked like" } " (that's git's job)."
        }
        @if events.is_empty() {
            div class="card" {
                p class="muted" { "No audit events yet." }
            }
        } @else {
            div class="card" {
                table class="audit-table" {
                    thead {
                        tr {
                            th { "Timestamp (UTC)" }
                            th { "Site" }
                            th { "Actor" }
                            th { "Action" }
                            th { "Detail" }
                        }
                    }
                    tbody {
                        @for ev in events.iter().rev() {
                            tr {
                                td class="ts" { (ev.ts.format("%Y-%m-%d %H:%M:%S").to_string()) }
                                td { (ev.site) }
                                td { (ev.actor) }
                                td { (audit_action_label(&ev.action)) }
                                td { (audit_action_detail(&ev.action)) }
                            }
                        }
                    }
                }
            }
        }
    };
    shell("Audit log", body)
}

fn audit_action_label(action: &AuditAction) -> Markup {
    let (label, css_kind) = match action {
        AuditAction::Login => ("login", "login"),
        AuditAction::LoginFailed => ("login (failed)", "login_failed"),
        AuditAction::Logout => ("logout", "logout"),
        AuditAction::PostCreated { .. } => ("post.created", "post_created"),
        AuditAction::PostUpdated { .. } => ("post.updated", "post_updated"),
        AuditAction::PostPublished { .. } => ("post.published", "post_published"),
        AuditAction::PageCreated { .. } => ("page.created", "page_created"),
        AuditAction::PageFrontmatterUpdated { .. } => ("page.frontmatter", "page_frontmatter"),
        AuditAction::PagePublished { .. } => ("page.published", "page_published"),
        AuditAction::SectionAdded { .. } => ("section.added", "section_added"),
        AuditAction::SectionUpdated { .. } => ("section.updated", "section_updated"),
        AuditAction::SectionMoved { .. } => ("section.moved", "section_moved"),
        AuditAction::SectionDeleted { .. } => ("section.deleted", "section_deleted"),
    };
    let cls = format!("audit-action-{css_kind}");
    html! {
        span class=(cls) { (label) }
    }
}

fn audit_action_detail(action: &AuditAction) -> Markup {
    html! {
        @match action {
            AuditAction::Login | AuditAction::LoginFailed | AuditAction::Logout => "",
            AuditAction::PostCreated { slug } |
            AuditAction::PostUpdated { slug } |
            AuditAction::PostPublished { slug } |
            AuditAction::PageCreated { slug } |
            AuditAction::PageFrontmatterUpdated { slug } |
            AuditAction::PagePublished { slug } => {
                code { (slug) }
            }
            AuditAction::SectionAdded { slug, kind } => {
                code { (slug) } " " (kind)
            }
            AuditAction::SectionUpdated { slug, idx } |
            AuditAction::SectionDeleted { slug, idx } => {
                code { (slug) } " §" (idx)
            }
            AuditAction::SectionMoved { slug, from, to } => {
                code { (slug) } " §" (from) " → §" (to)
            }
        }
    }
}

/// Legacy entry point retained for the old TOML-textarea-based
/// callers. Now redirects to the new structured edit view.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn page_form(
    site: &str,
    is_new: bool,
    title: &str,
    slug: &str,
    summary: &str,
    status: PageStatus,
    layout: PageLayout,
    updated_at: NaiveDate,
    nav_order: Option<u32>,
    _sections_toml: &str,
    error: Option<&str>,
) -> Markup {
    if is_new {
        new_page_form_view(site, title, slug, summary, status, layout, updated_at, nav_order, error)
    } else {
        // For existing pages, redirect-by-render to the structured editor.
        // The handler that calls page_form for an existing page should
        // really call edit_page_view directly with the loaded sections.
        edit_page_view(site, slug, title, summary, status, layout, updated_at, nav_order, &[], error, None)
    }
}

/// Edit / create form for a blog post.
#[must_use]
pub fn post_form(
    site: &str,
    is_new: bool,
    title: &str,
    slug: &str,
    date: NaiveDate,
    summary: &str,
    author: &str,
    status: BlogStatus,
    body_md: &str,
    error: Option<&str>,
) -> Markup {
    let action = if is_new {
        format!("/sites/{site}/blog/new")
    } else {
        format!("/sites/{site}/blog/{slug}/edit")
    };
    let heading = if is_new { "New post" } else { "Edit post" };
    let body = html! {
        nav class="muted" {
            a href="/sites" { "Sites" } " / "
            a href={ "/sites/" (site) } { (site) } " / "
            (heading)
        }
        h1 { (heading) }
        @if let Some(e) = error {
            div class="error" { (e) }
        }
        form action=(action) method="post" class="stack card" {
            div {
                label for="title" { "Title" }
                input type="text" id="title" name="title" value=(title) required;
            }
            div {
                label for="slug" { "Slug" }
                input type="text" id="slug" name="slug" value=(slug) required readonly[!is_new];
                p class="muted" { "URL slug — lowercase ASCII, dashes only. Cannot change after creation." }
            }
            div {
                label for="date" { "Publication date" }
                input type="date" id="date" name="date" value=(date.format("%Y-%m-%d").to_string()) required;
            }
            div {
                label for="summary" { "Summary" }
                input type="text" id="summary" name="summary" value=(summary) maxlength="200" required;
                p class="muted" { "One sentence. Used as the meta description and blog-index card subtitle. ≤200 chars." }
            }
            div {
                label for="author" { "Author" }
                input type="text" id="author" name="author" value=(author) required;
            }
            div {
                label for="status" { "Status" }
                select id="status" name="status" {
                    option value="draft" selected[status == BlogStatus::Draft] { "Draft" }
                    option value="published" selected[status == BlogStatus::Published] { "Published" }
                }
            }
            div {
                label for="body" { "Body (markdown)" }
                textarea id="body" name="body" required { (body_md) }
            }
            div class="row-actions" {
                button type="submit" class="btn" { "Save" }
                a href={ "/sites/" (site) } class="btn btn-secondary" { "Cancel" }
            }
        }
    };
    shell(heading, body)
}
