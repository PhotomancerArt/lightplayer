# ADR: One site chrome; boards and docs are in-app sections

- **Status:** Accepted
- **Date:** 2026-08-02
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The studio's top bar was a tiny wordmark-as-home-link plus two icon chips;
the boards catalog and the (new) docs pages were standalone surfaces with no
shared navigation and no way back. A UX spike
(`spikes/top-bar/index.html`, PR #269) compared bar structures and settled
the direction at a reviewed visual gate.

The product question underneath: are boards and docs satellite pages, or
sections of one app? The boards catalog is expected to *do* things (start
device setup from "I have one of these"), and docs are headed toward live
embedded nodes as the teaching strategy — both point at in-app.

## Decision

- **One shared top bar** (`lpa-studio-web::app::layout::SiteChrome`) renders
  on the studio app, the boards catalog, and the docs section, with
  **Studio / Boards / Docs** nav tabs. Boards and docs are in-app sections,
  not satellite pages.
- **The brand lockup is inert.** The logo (interim `LogoMark`, a WS2812
  pixel mark — placeholder until the commissioned logo lands) is reserved
  for a future landing/marketing page. Home is the Studio tab.
- **The authoring tools stay chromeless.** The mapping editor and board
  editor are tools, not sections; they are reachable from the chrome's
  overflow menu.
- **The chrome lives in `lpa-studio-web` only.** `lpa-boards` stays
  platform-blind and chrome-free; the web shell composes the chrome above
  `BoardsCatalogPage` in its render body.
- **Sections render inside the running app — nothing unloads.** Boards and
  docs are NOT early returns. `App()` always runs its hooks (actor, runtime
  pool, route listener) and its render body switches on the route between
  the studio shell, the catalog, and the docs section. Moving between
  sections is a re-render: sims keep running, device sessions stay
  attached, and docs articles can host live, running examples — the
  foundation the interactive-docs initiative needs.
- **One shell owns the chrome.** `web_app` renders the single `main`
  container, the chrome, and the local-store banner; `StudioShell` and the
  section pages render bodies only. The bar therefore sits at an identical
  offset in every section by construction, not by three call sites
  agreeing on padding.
- **The story book and the two editors stay early-return.** They replace
  the whole app rather than sitting in a section, so entering or leaving
  them still hard-reloads to keep hook order sound.

## Consequences

- Adding a section = a `SiteSection` variant, a route, and an arm in the
  render body; the section inherits navigation and the live runtime.
- Section transitions are instant and lossless — no reload, no unmount.
- The studio runtime boots for every visitor, including someone who opened
  `#/docs` directly. Accepted deliberately: live docs and an actionable
  boards catalog both need it. If cold start on those entry points ever
  becomes a real complaint, defer the heavy pieces behind first use rather
  than reintroducing a runtime-less mode.
- The chrome's studio mode threads `on_action` so the Studio tab preserves
  the direct lens-detach dispatch (the D29 device editor lives at `#/` and
  never fires `hashchange`).

## Alternatives Considered

- **Satellite pages with a back-link:** rejected at the spike gate —
  fragments the product and blocks boards/docs from acting on app state.
- **Logo as home link:** rejected — conflicts with a future landing page
  and hides the home affordance.
- **In-bar links to the tools:** rejected — tabs imply sections; the tools
  are editors.
- **Sections as early-return pages with a reload between them** (the first
  implementation of this ADR, reverted the same day at the visual gate):
  rejected — the reload was visibly janky, and a section with no actor
  behind it cannot host the live examples docs are being built around.

## Follow-ups

- Interactive docs architecture (live embedded nodes) — shipped, see `2026-08-06-interactive-docs-architecture.md`.
- Boards "I have one of these" setup entry — future work.
- Landing page; when it exists, the lockup may become its link.
