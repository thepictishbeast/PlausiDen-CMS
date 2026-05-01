+++
title = "PlausiDen-CMS, v0"
slug = "cms-launch"
date = "2026-05-01"
summary = "A typed, git-backed CMS for our marketing sites — no SaaS, no runtime DB, no surprise mutations."
author = "PlausiDen"
status = "draft"
+++

# PlausiDen-CMS, v0

This is the first post written through the new CMS. It's a markdown
file in git, with TOML frontmatter at the top describing its title,
slug, date, summary, author, and editorial status. The site loads it
the same way it loads source code: at build time, with the schema
checked at compile time.

## Why we built it

We had marketing copy spread across two-plus sites with no shared
write path. Editing a homepage tagline meant a code change; nothing
auditable below the level of "what commit changed this string."

CMS is the substrate that fixes this:

- One content tree per site, rooted at `content/<site>/`.
- Schemas live in `cms-core` — strongly typed Rust structs.
- Writes go through the `pdcms` CLI (today) or the admin web UI
  (when it ships). Either way, the result is a file edit and a
  commit, never a mutation no one can audit.
- Reads happen at build time. The site binary doesn't talk to a
  database to render a page; it embeds the validated content.

## What "v0" means

This release ships a single content type, `BlogPost`. Pages,
sections, blocks, and media land as separate schemas in follow-ups.
Same shape, same workflow.
