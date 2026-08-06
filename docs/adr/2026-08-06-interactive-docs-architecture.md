# ADR: Interactive docs run leased studio instances

- **Status:** Accepted
- **Date:** 2026-08-06
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The docs section renders inside the running app on purpose — the
site-chrome ADR (`2026-08-02-site-chrome-in-app-sections.md`) accepted
booting the studio runtime for every docs visitor "deliberately: live
docs … need it," and listed interactive docs as a follow-up initiative.
This ADR is that initiative's architecture, shipped with the first live
article ("What's a shader?"): docs pages that run a real in-browser sim
and embed live, interactive Studio components against it — knobs, lamp
views, and the real GLSL editor with auto-apply.

Planning: `~/.photomancer/planning/lp2025/2026-08-05-2019-interactive-docs-engine/`
(vision D1–D8, G1 rulings R1–R6 in notes.md).

## Decision

**Docs sims are leased, real `StudioController` instances.** A docs
page's declared sims each boot a `DocsSimHost`
(`lpa-studio-core/src/app/docs_host/`): a second controller + actor
around one browser-worker sim. Embeds read the host's real
`UiStudioView` and dispatch real `UiAction`s — the components are the
same ones Studio renders, on the same dispatch path. There is no
docs-specific view mirror (rejected: a lesser parallel controller
chasing parity with the real one forever).

**Docs sims are not roster sessions.** They live in their own
controller's pool — never device cards, never holders of the user's
editor lens. The roster's "sim capacity stays 1" ADR
(`2026-08-03-studio-runs-n-device-sessions.md`) governs the user's pool
and is untouched. Teardown is explicit and one-enqueue
(`DocsSimHost::shutdown()` → StopSimulator then actor Shutdown; nothing
in the chain has a `Drop` that terminates the Worker), wired to page
unmount; the actor is spawned detached so it completes teardown after
the page dies.

**Deploys never touch the library.** `ProjectOp::OpenDocsExample`
pushes a compiled-in example via `deploy_project_files` into a
`docs-…` storage dir — no catalog transaction, no OPFS seeding, nothing
planted in the user's gallery (the `OpenExample` path would seed it).
Re-dispatch is the docs Reset: root-scope panel clear, then a pristine
re-deploy.

**Secondary actors do not drain the log sink.**
`StudioActorOptions { drain_logs: false }` — the global `log::` sink's
per-thread queue has one drainer (the main actor); a second draining
actor silently steals its records.

**Articles are markdown with one extension.** A fenced block whose info
string is `embed <name> key=value…` resolves against a closed registry
(`app/docs/embeds/`). Chat markdown has no resolver prop at all —
untrusted model output can never summon live components. `build.rs`
scans registered articles and generates checks: unknown embed names,
undeclared `sim=` refs, dead `#/docs` links, and dead heading anchors
fail validation, and per-page `docs_links` constants make help-link
targets compile-checked (`HelpLink` + the "?" plantings).

## Consequences

- A docs article is a sandbox: readers drag real knobs and edit real
  GLSL (real auto-apply, real compile errors, keep-last-good frames)
  with a Reset that restores the pristine example. One Worker per
  declared sim, terminated on navigation (verified 1:1 spawn:terminate).
- Writing an interactive article = markdown + a `PAGES` entry with sim
  declarations; a typo anywhere in the wiring is a failing build, not a
  broken page.
- Every docs visitor's runtime cost grows only with declared sims on
  the page they are reading; view pulls pause while the document is
  hidden.
- The "?" flywheel is compile-safe: a help link to a page or anchor
  that stops existing stops the build.

## Alternatives Considered

- **PreviewHost view-mirror** (a purpose-built read-only projection
  over preview slots): ~1/20th the initial work but a second, lesser
  controller whose parity with the real one rots; rejected in the
  vision session (D1) and validated by round 2, where the page's beats
  needed the full dispatch path (panel writes, asset edit ops).
- **Two sims for two shapes** (round 1's shape): replaced by one
  example project carrying both fixtures (`examples/plasma-duo`) after
  the G1 review — one worker, knobs coherently drive everything, and it
  teaches the real multi-fixture model.
- **Read-only code figure as the ending**: replaced by the editable
  real editor (G1 R3) — "lets them play with it" is the product thesis;
  `CodeFigure` remains for annotated listings.

## Follow-ups

- The docs "book": full scope of comprehensive docs is its own
  initiative (vision follow-up); the embed vocabulary deliberately
  stays small — richness comes from examples, not new directives.
- Voice: the style guide's register is provisional until the canonical
  copy is hand-written (`docs/user-guide/STYLE.md` notes this).
- Per-embed IntersectionObserver visibility pause (document-level today).
- A "grid of N shapes" showcase example (G1 round 2 idea).
