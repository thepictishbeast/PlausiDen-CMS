# PlausiDen-CMS — vision document

> "If PlausiDen-CMS was already built and did everything we
> wanted, what would this doc say?"

This is that doc. **[shipped]** works today. **[in-flight]** is
mid-build. **[queued]** has a task ID. **[concept]** has been
implied or requested and a developer should design it.

---

## 1. What PlausiDen-CMS IS

**Generic multi-site content substrate.** One CMS binary, N
independent sites — each with its own typed content tree, its
own admin-side auth, its own audit log, its own per-site
encryption keys, all backed by plain TOML on disk so no site
is a data hostage.

Operationally: two Rust crates today.

| Crate | Role |
|---|---|
| `cms-core` | Typed `Page` model, filesystem storage adapter, signed audit log, ed25519 / chacha20poly1305 crypto primitives |
| `cms-cli`  | Command-line surface: list sites, list pages, edit, audit-tail, export |

Sister tools (each in its own repo):

- **PlausiDen-Loom** — the rendering layer. Takes typed CMS pages
  → emits HTML. Same `loom_cms_render::page_shell` Mom-class
  Loom sites already use, so a CMS-served page inherits dual-
  theme + a11y for free.
- **PlausiDen-Forge** — the build pipeline. Audits the rendered
  output before publish.
- **PlausiDen-Crawler** — runtime audit. Drives any CMS-served
  site through a journey to verify it still works post-deploy.
- **PlausiDen-Canon** — design system (layouts + components the
  CMS does NOT ship; sites pull from Canon).

PlausiDen-CMS is **not**:

- A page-builder (sites pull layout from Canon / typed Loom
  primitives; CMS owns content, not chrome)
- A visitor-tracking surface (zero cookies on the public read
  path, no analytics, no fingerprinting)
- An auth provider for site visitors (only admin auth)
- A data hostage (everything exports as TOML + media tarball)
- A SaaS-only product (single-VPS install + multi-node cluster,
  both first-class)

PlausiDen-CMS's contract: feed it a per-site `content/` tree and
admin credentials, get back a typed-content API + an admin portal
+ a signed audit log, with zero state on the public read path.

## The meta-mission: making AI-built UI reliable

Every PlausiDen tool — Loom, CMS, Forge, Crawler, Annotator —
exists for one common reason: **AI agents building GUI / frontend /
UX work need a reliability substrate that humans don't.** PlausiDen-CMS
contributes the multi-tenant editing + audit substrate so an agent
running on behalf of N tenants can never accidentally cross
boundaries (a per-tenant capability scope makes leakage
mechanically impossible) AND so every action is forensically
attributable (signed audit log).

For an agent operating across many client sites at scale:

- **Typed `Page` model with `deny_unknown_fields`** means schema
  drift fails closed, never silently — agent edits that produce
  invalid pages refuse to save.
- **Per-tenant key isolation** means a stolen agent credential
  for tenant A reveals nothing about tenant B.
- **Signed append-only audit log** means a human can replay
  every agent action across any time window — agent regressions
  are diagnosable AFTER the fact.
- **Zero-state public read path** means an agent operating on
  behalf of a client never accidentally tracks the client's
  visitors — no consent UI, no GDPR risk, by construction.

Sibling tools close the loop: Loom renders the typed `Page`,
Forge audits the build before publish, Crawler verifies the
runtime, Annotator captures human review for agent-replay.

## 2. The supersociety stack PlausiDen-CMS uses

- **Memory-safe core** — Rust everywhere (no `unsafe_code`,
  `unwrap`/`expect` lint-deny in lib code).
- **Type-safe content** — the `Page` model is a serde-typed Rust
  struct with `deny_unknown_fields`; TOML parse fails closed.
- **Append-only audit log** — every edit recorded with
  ed25519 signature from the editor's hardware key. Forensically
  attributable. Tamper-evident via hash chain.
- **Hardware-key admin auth** — WebAuthn / passkey from day one;
  password-with-TOTP is the floor, never the design centre.
  `chacha20poly1305` for any at-rest secret material.
- **Zero-state public read path** — no cookies, no JS state, no
  fingerprinting, no third-party requests. CMS-served pages are
  static HTML + tokens.css.
- **At-rest encryption for drafts** — `chacha20poly1305` per-site
  key (sealed by editor's hardware key); published content can
  be public-plaintext by per-site policy toggle.
- **Reproducible exports** — every site's content exports as a
  tar of TOML + media; no proprietary format to migrate out of.
- **Local-first by default** — single VPS, no SaaS, no cloud
  object storage required. Cloud storage is opt-in for scale.
- **Post-quantum-forward primitives** — ML-KEM / ML-DSA via
  `sacredvote-crypto` once that library lands; today ed25519 is
  the floor.
- **Signed publish bundles** — every publish event produces a
  bundle hash recorded in the audit log AND signed by the
  publisher's key. Reverting is a new signed event, not a
  history rewrite.

## 3. Personas

### 3.1 Mom — non-technical client (the gold standard)

Mom runs a bakery, a knitting club, AND a family newsletter.
PlausiDen-CMS lets her run all three from one editor with three
isolated content trees.

What Mom does today:

1. `cms init mybakery` (queued — currently bootstrap-stage) —
   creates `~/cms/mybakery/{content/,media/,audit.log}`.
2. (queued) Opens the admin portal in her browser, logs in with
   her YubiKey (WebAuthn), edits a page via Loom-style typed
   forms.
3. (queued) Hits Publish. CMS hands the typed `Page` to Loom for
   rendering, then to Forge for audit, then writes the rendered
   bundle to her chosen publish target.
4. (queued) Her bakery, knitting-club, and family-newsletter
   sites all live in separate `cms/<site>/` trees with separate
   per-site keys. Compromising one tells the attacker nothing
   about the others.

What Mom never has to think about:

- Cookies, GDPR banners, analytics consent — no tracking exists
  to consent to.
- Backups — every edit is recorded in a signed append-only log;
  exporting the whole site is a single tar command.
- "Did the publish actually work?" — the audit log shows every
  publish event with its bundle hash and the auditor's signature.

What Mom can ALSO do (once queued capabilities ship):

- **Voice-to-CMS dictation** — dictate a paragraph; on-device
  Whisper transcribes; auto-creates a typed Page section.
- **One-button time-travel** — "show me what this page looked
  like a week ago" walks the audit log, signed-content checkpoints
  back.
- **Schedule a publish** — "publish on Tuesday at 9am" — the
  signed-content envelope is dated, a tiny systemd timer
  executes.
- **Newsletter from her CMS pages** — when she publishes a new
  blog post, subscribers get a digest, encrypted-list at rest,
  no Mailchimp dependency.

### 3.2 The technical client — wants control

A small civic / political / community site (think SacredVote.org,
plausiden.com, a local mutual-aid project).

What they get today:

- **Per-site theming** via Loom tokens — colour, spacing, fonts.
- **Plain TOML content tree** — version-controllable in their
  own Git if they want; CMS doesn't lock them into a database.
- **Append-only signed audit log** — every change is forensically
  attributable. Critical for civic-trust use cases.
- **Per-site encryption keys** — drafts and unpublished material
  are encrypted at rest with a key only the site's editors can
  unwrap.

What they get next:

- **Editor permissions** — admin / editor / contributor / viewer
  via capability tokens. Different sites can have different role
  sets [queued].
- **Workflow** — draft → review → schedule → publish. A
  contributor writes, an editor reviews, a site-admin publishes
  [queued].
- **Content branching** — fork a page set, edit in isolation,
  merge back. Git-like for non-developers, served by the audit
  log [queued].
- **WebMentions / IndieWeb support** — when someone else's site
  links to a CMS-served page, the CMS captures the backlink
  signed, surfaces it in the editor [concept].
- **C2PA content provenance** — every image carries a Content
  Authenticity manifest tying it to the editor's key [concept].

### 3.3 The developer — contributor or forker

What they get today:

- **`cms-core` is pure types + storage adapter + crypto** — no
  HTTP server, no UI, easy to embed in any binary.
- **`cms-cli` is a thin wrapper** that reads a config and operates
  the storage adapter — straightforward to fork.
- **Plain TOML wire format** — no proprietary serialization,
  trivial to write tests against.
- **AGPL-3.0-or-later licence** — modifications must stay open.

What developers want next:

- **Pluggable storage adapter** — filesystem today, SQLite
  next, then PostgreSQL for cluster mode, then optional S3-style
  for media [staged].
- **Pluggable rendering adapter** — Loom is the default; an
  alternate Maud / Askama / Tera renderer should be a one-trait
  swap [concept].
- **Migration tools from WordPress / Squarespace / Wix / Webflow**
  — bulk import that lands as TOML + media [concept].
- **Embeddable widget layer** — small JS-free widgets (signup
  form, donation link, comment box) that any external site can
  embed via a per-site rate-limited proxy [concept].
- **`cms-server`** crate — Axum-based admin portal + content API
  + WebAuthn integration. Currently sketched in README, not built
  [queued].
- **`cms-replay`** crate — replay any audit-log range to
  reconstruct content state at any past timestamp; useful for
  forensics + bug repro [concept].

### 3.4 Claude Code (and other autonomous agents)

What an agent gets today:

- **Stable typed-content API** — every page is a serde-typed
  TOML file. Read, mutate, write — the schema makes drift
  impossible.
- **Append-only audit log** — every agent action is logged with
  the agent's signing key, surfacing in the editor's audit view.
- **Content tree is filesystem** — no DB connection, no pool
  management, just paths.

What agents want next:

- **MCP server** exposing CMS capabilities (list sites, list
  pages, read page, create page, mutate page, publish page,
  audit-tail) as discoverable tools [concept].
- **JSON-RPC API** for orchestrators that don't speak Cargo
  [concept].
- **Per-tenant capability tokens** so an orchestrator can spawn
  one Claude per tenant, each scoped to its own site only
  [concept — depends on Loom T46-style sandboxing].
- **Cost / time budgets** per agent session, surfaced in the
  audit log with the agent's own ed25519 signature [concept].
- **Annotator integration** — an agent can request a human
  review of a draft via the Annotator browser overlay; the
  reviewer's annotations land in the audit log signed by the
  reviewer's key [concept].

## 4. Capability map

### 4.1 Content storage

| Capability | Status |
|---|---|
| Typed `Page` model with serde / `deny_unknown_fields` | shipped |
| Filesystem storage adapter (TOML on disk) | shipped |
| Per-site directory layout (`<root>/<site>/{content/,media/,audit.log}`) | shipped (bootstrap) |
| Signed audit log (ed25519, append-only, hash-chained) | shipped (bootstrap) |
| Reproducible export (tar of TOML + media) | shipped (bootstrap) |
| At-rest encryption for drafts (chacha20poly1305 per-site key) | queued |
| SQLite storage adapter (single-binary deployment) | queued |
| PostgreSQL storage adapter (multi-node cluster mode) | concept |
| S3-compatible media adapter (optional, opt-in) | concept |
| Content branching (Git-like) | concept |
| Time-travel debug via audit-log replay | concept |
| Pluggable storage adapter trait | concept |

### 4.2 Admin UI + auth

| Capability | Status |
|---|---|
| `cms-cli` command-line surface (list/edit/audit-tail/export) | shipped (bootstrap) |
| `cms-server` Axum-based admin portal | queued |
| WebAuthn / passkey admin auth | queued |
| Password-with-TOTP fallback | queued |
| Per-editor capability tokens (admin/editor/contributor/viewer) | queued |
| Workflow: draft → review → schedule → publish | queued |
| Real-time collab via CRDTs (multi-author editing) | concept |
| Editor permissions per site, per page, per section | concept |
| Hardware-key recovery via Shamir secret-sharing | concept |
| End-to-end encrypted editor-to-editor comments | concept |
| Tor-friendly admin login (onion service) | concept |

### 4.3 Rendering integration

| Capability | Status |
|---|---|
| Hand off `Page` to Loom's `loom_cms_render::page_shell` | queued (depends on Loom T70b — already shipped) |
| WCAG 2.1 AA / dual theme inherited from Loom | queued (free once integration lands) |
| Pluggable rendering adapter (alternative renderers) | concept |
| Per-locale content with translation status tracking | concept |
| Per-site Loom theme tokens (palette / spacing / fonts override) | queued |
| Generated email-template renderer using same CmsSection types | concept |
| PDF export of any page | concept |
| Print-stylesheet generation | concept |
| Responsive `<picture>` rendering with WebP/AVIF fallback | concept |

### 4.4 Publish + deploy

| Capability | Status |
|---|---|
| Publish event recorded in audit log with bundle hash | shipped (bootstrap) |
| Hand off rendered bundle to Forge for audit | queued |
| Hand off post-deploy verification to Crawler | queued |
| Time-locked publish (publish at future timestamp) | concept |
| Multi-region propagation | concept |
| Hetzner / Cloudflare R2 / IPFS / Tor publish targets | concept |
| C2PA content provenance signed at publish time | concept |
| Sigstore-style transparency log of every publish event | concept |
| ActivityPub / Fediverse cross-publish | concept |
| WebMention inbound capture at publish time | concept |

### 4.5 Forms + interactivity (without breaking zero-state)

| Capability | Status |
|---|---|
| Per-site form submission inbox (encrypted at rest) | concept |
| Drag-and-drop form-builder generating typed CmsSection + backend stub | concept |
| Newsletter signup with double opt-in, encrypted subscriber list | concept |
| Donation / payment links (no third-party trackers) | concept |
| Live chat with E2E encryption (and Tor option) | concept |
| Comment moderation queue with Tor-friendly comment posting | concept |
| Federated comment identity (signed identities, no email needed) | concept |
| Webhook outbound on publish | concept |
| Embeddable JS-free widgets via per-site rate-limited proxy | concept |

### 4.6 Privacy + opsec

| Capability | Status |
|---|---|
| Zero cookies on public read path | shipped (by design) |
| No analytics, no tracking, no fingerprinting | shipped (by design) |
| Append-only signed audit log | shipped |
| At-rest encryption for drafts | queued |
| Hardware-key auth | queued |
| Editor identity-binding via key (no email PII required) | queued |
| Tor onion-service admin login | concept |
| Tor onion-service publish target | concept |
| Reproducible exports | shipped |
| Bot moderation via federation tools (Mastodon-style) | concept |
| Memory-safe deserialization (every parser fuzz-targeted) | shipped (proptest in cms-core) |

### 4.7 Scale + reliability

| Capability | Status |
|---|---|
| Single-VPS install | shipped (bootstrap) |
| Multi-node cluster mode | concept |
| PostgreSQL adapter for clustering | concept |
| Read-replica federation | concept |
| Backup-to-IPFS / decentralized storage | concept |
| Self-healing audit-log replication | concept |
| Per-site rate-limit policies | concept |
| Per-site quota policies | concept |

### 4.8 Documentation

| Capability | Status |
|---|---|
| README with status + design anchors | shipped |
| `docs/CMS_VISION.md` (this doc) | shipped (T72) |
| Per-command `--help` with full doctrine | partial |
| Storage-adapter contributor guide | concept |
| Architecture decision records (ADRs) | concept |

## 5. Architecture (when fully built)

```
┌──────────────────── PlausiDen-CMS ────────────────────┐
│                                                        │
│  ┌──────────────┐    ┌──────────────────────────┐    │
│  │  cms-cli     │───▶│  cms-core                │    │
│  │  list/edit/  │    │  ┌────────────────────┐  │    │
│  │  audit/      │    │  │ Page (typed)       │  │    │
│  │  export      │    │  │ Storage adapter    │  │    │
│  └──────────────┘    │  │ Audit log (ed25519) │  │    │
│                      │  │ Crypto (chacha20)  │  │    │
│  ┌──────────────┐    │  └────────────────────┘  │    │
│  │  cms-server  │───▶│                          │    │
│  │  admin portal│    │                          │    │
│  │  WebAuthn    │    │                          │    │
│  │  per-site    │    │                          │    │
│  │  workflow    │    │                          │    │
│  └──────────────┘    └──────────────────────────┘    │
└────────────────────────────────────────────────────────┘
       │                          │                │
       ▼                          ▼                ▼
   storage                   sister repos       publish targets
   ┌──────────┐         ┌──────────────────┐    ┌─────────────┐
   │ ~/cms/   │         │ PlausiDen-Loom   │    │ static host │
   │  bakery/ │◀────────│ (renders Page)   │    │ Hetzner     │
   │   ├ con  │         ├──────────────────┤    │ R2 / IPFS   │
   │   ├ med  │         │ PlausiDen-Forge  │    │ Tor onion   │
   │   └ aud  │         │ (audits build)   │    │ ActivityPub │
   │  knit/   │         ├──────────────────┤    └─────────────┘
   │  family/ │         │ PlausiDen-Crawler│
   └──────────┘         │ (verifies live)  │
                        └──────────────────┘
```

Per-site isolation:

```
┌────── tenant A ──────┐  ┌────── tenant B ──────┐
│  cms/A/content/      │  │  cms/B/content/      │
│  cms/A/media/        │  │  cms/B/media/        │
│  cms/A/audit.log     │  │  cms/B/audit.log     │
│  cms/A/keys/         │  │  cms/B/keys/         │
│   (per-site Ed25519, │  │   (per-site Ed25519, │
│    chacha20 KDF root)│  │    chacha20 KDF root)│
└──────────────────────┘  └──────────────────────┘
            │                        │
            └──────────┬─────────────┘
                       ▼
            ┌──────────────────────┐
            │  cms-server binary   │
            │  (one process, N     │
            │   tenants, isolated  │
            │   per-tenant state + │
            │   per-tenant keys)   │
            └──────────────────────┘
```

## 6. Roadmap from now to "done"

### Sprint 1 — close the bootstrap → MVP gap

- Per-site SQLite storage adapter (alongside filesystem)
- `cms-server` Axum-based admin portal (skeleton)
- WebAuthn / passkey admin auth (RustCrypto webauthn-rs)
- Wire up Loom integration: `Page` → `loom_cms_render::page_shell`
  → published HTML
- Wire up Forge integration: rendered bundle → forge-cli audit
  before publish
- At-rest encryption for drafts (chacha20poly1305 + per-site
  key sealed by editor's hardware key)

### Sprint 2 — workflow + collab

- Editor capability tokens (admin / editor / contributor / viewer)
- Workflow: draft → review → schedule → publish
- Time-locked publish (signed envelope dated for future)
- Webhook outbound on publish
- Bulk import (WordPress / Squarespace / Wix → TOML)
- Bulk export (TOML + media tar)
- `cms-replay` crate for audit-log time-travel

### Sprint 3 — privacy-maximal publish targets

- Tor onion-service publish (CMS publishes to a `.onion` mirror)
- IPFS / Hypercore decentralized publish target
- ActivityPub / Fediverse cross-publish (every published page is
  a federated post)
- WebMention inbound capture at publish time
- C2PA content provenance signed at publish time
- Sigstore-style transparency log of every publish event

### Sprint 4 — interactivity without breaking zero-state

- Per-site form-submission inbox (encrypted at rest)
- Drag-and-drop form-builder → typed CmsSection + backend stub
- Newsletter signup (double opt-in, encrypted subscriber list)
- Per-site donation / payment links (no third-party trackers)
- Live chat (E2E encrypted, Tor option)
- Comment moderation queue with federated comment identity

### Sprint 5+ — the supersociety horizon

**For Mom (non-technical client):**
- Voice-to-CMS dictation (on-device Whisper)
- Local AI-assisted content suggestions (no cloud LLM)
- One-button "make it match my brand" — palette + tone derived
  from a single brand colour
- Self-healing layout (overflow → "fit to mobile" suggestion)
- Time-travel content viewer ("show me last Tuesday's home page")
- Per-page A/B test with statistical-significance reporting

**For the technical client:**
- CRDT-backed multi-author editing
- Loom-as-a-PWA that works offline
- End-to-end encrypted editor-to-editor comments
- Custom typed CmsSection variants declared in TOML/Rust, no
  fork required
- Component state-matrix renderer for design review

**For the developer:**
- Type-state phase pipeline shared with Forge
- TLA+ specification of edit + publish state machine
- Mutation-testing CI gate
- Differential renderer (two backends + diff)
- Reproducible-build attestation in transparency log

**For Claude Code (and other autonomous agents):**
- Per-tenant Claude SSH bridge (sandboxed, capability-scoped)
- Annotator integration (replay flagged sessions)
- MCP server exposing every CMS capability
- Stable JSON-RPC API
- Cost / time budgets per agent session in audit log

**Cross-cutting supersociety capabilities:**
- Hardware-attested deploys (TPM-backed signing)
- Post-quantum signature variant (ML-DSA alongside Ed25519,
  dual-signed for forward-secrecy)
- Tor / I2P / Hypercore decentralized everything
- Memory-safe deserialization throughout (every parser
  fuzz-targeted, every public surface property-tested)
- Compile-time CSP derivation
- Compile-time at-rest-encryption derivation (every
  unencrypted-draft path is a compile error)

## 7. Future shape — three years out

PlausiDen-CMS becomes the substrate behind hundreds of small
community / civic / family / single-creator sites. Each tenant
runs on a single VPS (or for the technical-client tier, a
multi-node cluster). The admin portal is an offline-first PWA
that syncs back when the editor reconnects. WebAuthn is
universal; passwords have aged out. Every content state in every
tenant is forensically attributable, signed by the editor whose
hardware key authored it. Every publish event is recorded in a
transparency log a third party can replay.

The CMS doesn't do layout (Loom owns that), build orchestration
(Forge owns that), or runtime verification (Crawler owns that).
What it owns: storage, audit, auth, workflow, multi-tenancy.
Sharper boundaries between sister repos make each one easier to
audit, harder to compromise, and trivially replaceable if a
better design emerges.

Mom runs three sites from one CMS install on a $5/month VPS. The
Sacred.Vote-class technical client runs hundreds of regional
sites from a small cluster. Both get the same supersociety
guarantees because PlausiDen-CMS does not have a Pro tier — the
defaults ARE the maximum.

## 8. Acceptance criteria for "done"

PlausiDen-CMS is **done** when:

1. Mom can spin up a new site in <60 seconds with one command
   and never need to touch a YAML file or read a tutorial.
2. Every edit is signed by the editor's hardware key and recorded
   in a tamper-evident audit log a third party can verify.
3. A site can be exported as a tar and re-imported cleanly into
   any CMS install on any machine — zero proprietary
   serialization.
4. The public read path serves zero cookies, zero JS state, zero
   third-party requests.
5. A per-tenant compromise (stolen key, account takeover) tells
   the attacker nothing about other tenants — per-site key
   isolation is provable.
6. WebAuthn / hardware-key admin auth is the default and only
   tier; password-with-TOTP exists as a fallback only.
7. The audit log can be replayed to reconstruct content state at
   any past timestamp.
8. Every publish event lands in a transparency log a third party
   can audit.
9. A developer can fork the repo, swap the storage adapter for
   PostgreSQL or SQLite or S3, and ship without touching any
   other layer.
10. The threat model from `~/.claude/CLAUDE.md` (state-actor
    adversary, full breach, unlimited time) holds against the
    deployed system.

The verdict is always **STILL BROKEN** — shipping is risk
acceptance, not a declaration of correctness. The loop resumes
on the next commit.
