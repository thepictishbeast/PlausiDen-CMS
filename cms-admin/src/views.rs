//! Maud templates. v0 is single-file, deliberate: every page reads
//! top-to-bottom, no template inheritance to chase. When the page
//! count grows, we extract a layout helper. Until then the
//! repetition is part of the readability story.

use chrono::NaiveDate;
use maud::{DOCTYPE, Markup, html};
use plausiden_cms_core::{BlogPost, BlogStatus};

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
                        form action="/logout" method="post" class="logout-form" {
                            button type="submit" class="link-button" { "Log out" }
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
