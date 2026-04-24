<!-- repo-label: infrastructure -->
<!-- repo-class: content-management-substrate-for-marketing-sites -->
<!-- repo-consumes: PlausiDen-AVP-Doctrine, PlausiDen-Obs, PlausiDen-Canon (when UI surfaces ship) -->
<!-- repo-consumed-by: plausiden-site, SacredVote.org, and future PlausiDen-namespace brochure/marketing sites -->
<!-- repo-tier: tbd -->
<!-- repo-doctrine-version: n/a -->
<!-- repo-engine-version: 0.0.0-placeholder -->
<!-- repo-status: experimental -->
<!-- repo-avp-subject: yes -->
<!-- repo-harvest-candidates: no -->
<!-- repo-reference-impl-language: rust -->
<!-- repo-target-stack-scope: linux-x86_64 -->

# PlausiDen-CMS

> **Scaffold only.** Implementation deferred per [`PlausiDen-Meta/PRIORITY.md`](https://github.com/thepictishbeast/PlausiDen-Meta/blob/main/PRIORITY.md) trigger gating. This repo exists as a name reservation and design anchor — do not build against it.

Generic content management substrate for PlausiDen-namespace marketing / brochure sites. Intended to power **`plausiden-site`** (plausiden.com), **`SacredVote.org`** (marketing), and any future outward-facing marketing surface from a single codebase, with per-site theming and content stored independently.

## The need

Two-plus marketing sites in the PlausiDen ecosystem currently ship as static or React-rendered HTML with marketing copy baked into the code. Editing requires a code change, a commit, a deploy. Tolerable for one site; not tolerable as the count grows. Each site also ends up with its own ad-hoc form handler, its own image-upload path, its own "where do I put this PDF?" folder convention. Every net-new marketing site today pays this cost from scratch.

The design intent is **one CMS, N sites**, with the CMS owning:

- Structured content (pages, sections, blocks, typed fields) per site
- Admin portal for non-technical editors
- Content API consumed by the site binaries
- Media storage (self-hosted, no S3 required)
- Publish / draft / preview workflow
- Audit log of every edit (who, when, what, why)

And explicitly **not owning**:

- Layout, components, or UI primitives — those live in [`PlausiDen-Canon`](https://github.com/thepictishbeast/PlausiDen-Canon)
- Authentication for site visitors — sites remain cookie-free; CMS auth applies only to admin access
- Analytics, tracking, or visitor state — never

## Design anchors (not a spec; a direction)

### Supersociety posture

- **Zero-state for public visitors.** CMS-served pages remain cookie-free, no tracking, no session state on the read path.
- **Authenticated admin surface.** WebAuthn / hardware-key only — password-with-TOTP is the floor, never the design centre.
- **Audit-log everything admin does.** Append-only. Signed by the editor's key. Every edit is forensically attributable.
- **Local-first.** Runs on a single VPS; no external SaaS required. Cloud object storage is optional, not default.
- **Content at rest is encrypted.** Drafts and unpublished material encrypted with per-site keys; published content can be public-plaintext by policy toggle.
- **Reproducible exports.** Every site's content can be exported as a tar of TOML + media; the CMS is not a data hostage.

### Reference point: Sacred.Vote

The Sacred.Vote platform stack (and its marketing mirror SacredVote.org) is the architectural model we're generalizing from. Specifically:

- Rust backend (Axum + Tokio)
- WebAuthn for admin authentication
- Post-quantum-forward crypto primitives (ML-KEM / ML-DSA via [`sacredvote-crypto`](https://github.com/thepictishbeast/sacredvote-crypto))
- Zero-state public read path
- Hardware-key bound editor identity

PlausiDen-CMS should lift the patterns, not the consumer-specific code (per [`PlausiDen-Meta/SCOPE.md`](https://github.com/thepictishbeast/PlausiDen-Meta/blob/main/SCOPE.md) independence test).

### Stack direction (subject to a real trigger before implementation)

- **Axum + Maud** for both admin portal and content API (same pattern as `plausiden-site`)
- **SQLite** with WAL mode for content store (zero-deps, file-based, easy to back up, easy to reproduce)
- **WebAuthn** via `webauthn-rs` crate for admin login
- **age** encryption for at-rest drafts
- **S3-compatible** media storage as an *optional* adapter; filesystem is the default
- **PlausiDen-Canon** design tokens for the admin UI
- **PlausiDen-Obs** for logging and audit sinks
- **In-process TLS** via `rustls-acme` (same v2 direction as `plausiden-site`)

## Why this is a placeholder and not a project

Per [`PlausiDen-Meta/OPERATING_PRINCIPLES.md`](https://github.com/thepictishbeast/PlausiDen-Meta/blob/main/OPERATING_PRINCIPLES.md):

- **§1 Meta-infrastructure is net-negative until proven otherwise.** A CMS is meta-infrastructure. We need more than one concrete marketing site flinching at copy-edit friction before we build it.
- **§5 One consumer in production before generalization.** Exactly one marketing site (`plausiden-site`) is live today. SacredVote.org is a second candidate; a third hasn't emerged. Below three consumers, we write content directly in the site repos and accept the per-edit friction.
- **§6 Trigger-gated, not anticipated.** No trigger has fired for this repo. Its existence is design-anchor-only, noted so that if a trigger does fire we have the intent captured.

## Trigger for promotion

Promote this repo from `status: experimental` (scaffold) to `status: in-progress` when **any one** of:

1. **Three distinct marketing sites** in the PlausiDen namespace need structured content editing without re-deploys.
2. A **non-technical editor** (marketing, comms, legal) asks to change a site's copy and the friction of filing a code PR becomes the blocker.
3. A marketing site gains a surface that needs **versioned + scheduled publishing** (e.g. a press-release page with embargo dates).

Until then, do not open implementation issues against this repo.

## Layout (currently empty — will populate when triggered)

```
integrations/
  avp.toml          AVP tier targets (all "not_started")
  (future) canon.toml, obs.toml — when adopted in anger
harvest.toml        participates in the harvest convention (no candidates yet)
README.md           this file
LICENSE             MIT
```

Future layout (sketch — the shape implementation will aim at):

```
crates/
  plausiden-cms-server/   Axum server, admin + public API
  plausiden-cms-content/  content types + migrations + sqlite schema
  plausiden-cms-auth/     WebAuthn + audit log
  plausiden-cms-media/    filesystem + optional S3 adapter
adapters/
  rust-sdk/               typed content client for Rust site binaries
  typescript-sdk/         typed content client for any JS/TS frontend
admin/                    Maud templates for the admin portal
examples/
  plausiden-site/         how a consumer site pulls content at build or runtime
  sacredvote-org/         same, for the SacredVote.org case
```

## License

MIT. See [LICENSE](LICENSE).
