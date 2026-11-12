# Studio routing: routes are history entries; the shell is route-framed, actor-filled

- Status: accepted
- Date: 2026-07-08

## Context

The Studio web shell had no router. Three URL mechanisms coexisted:
query params (`?project=`, `?connect=`) written with `replaceState`
(so back/forward did nothing), the story book's private hash listener
(`#/stories/…`), and the story-capture harness's query params. Reloading
an open project flashed the gallery, because the shell rendered whatever
the studio actor's view said and at boot that is home — the URL's
intent was known synchronously but nothing used it to pick the first
frame. Upcoming work leans on real URLs: device/provision deep links
(roadmap M5) and shareable example links (M6 — the growth loop).

## Decision

One owned route model at the web edge (`lpa-studio-web/src/router.rs`);
the sans-IO core stays route-free.

- **Route table, hash-based** (static hosts serve the app unmodified):
  `#/` home, `#/project/<prj_uid>` an open project, `#/stories[/<id>]`
  the story book. Unknown hashes read as home; a legacy `?project=`
  boot URL maps to its route once and is stripped.
- **Routes are history entries.** Opening a project from the gallery
  pushes state; back returns to the gallery (a *full* return —
  disconnect — because the gallery only renders when the link is
  idle); forward reopens the project.
- **The shell is route-framed, actor-filled.** The route picks the
  frame (gallery / project opening frame / story book); the actor's
  emitted view fills it. A project route whose project the view hasn't
  reached yet renders a project-shaped opening frame — the gallery
  never flashes on reload.
- **Reconciliation, not coupling.** View → URL: an open project
  navigates (push from home) or syncs (replace, for boot/forward
  resolution); losing the project replaces to home only after an open
  actually started this session (the boot-time home flash must not
  erase the route that requested the startup reopen). URL → actor:
  `popstate`/`hashchange` (which programmatic History-API writes never
  fire) dispatch disconnect/open. This asymmetry is what prevents echo
  loops without flags.
- **Hand-rolled over `dioxus-router`**: the router crate wants to own
  the component tree via route-driven rendering, but this shell is a
  projection of actor state — the route↔actor reconciliation is the
  actual problem and would exist either way. Three routes do not
  justify the machinery; revisit if the route space grows teeth.
- The story capture harness's `?story-png=1&story=…&viewport=…` params
  are **not routing** and are frozen as a seam with
  `scripts/studio-story-pngs.mjs`.

## Consequences

- Back/forward are meaningful for the first time; "back = leave the
  project" is the documented gesture.
- New routes (devices in M5, examples in M6) extend one enum and one
  reconciliation site; share links get real URLs for free.
- The story book still mounts only on fresh page loads (its early
  return precedes the app's hooks); live hash navigation into it
  triggers a full reload deliberately.
- `?connect=simulator`/`?connect=usb` are gone; the pre-gallery demo
  auto-open is superseded by project routes and the example card.
