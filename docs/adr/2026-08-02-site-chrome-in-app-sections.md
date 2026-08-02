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
  `BoardsCatalogPage` at the `web_app.rs` route boundary.
- **Standalone sections own leaving themselves.** Standalone pages
  early-return before the studio's hooks, so no studio route listener
  exists there. `SiteChrome` (standalone mode) installs a `hashchange`
  listener that hard-reloads when the hash leaves its section, mirroring
  the studio-side listener's treatment of standalone routes. Listeners are
  added with `addEventListener` (never `onhashchange =`) so a section page
  and its chrome can both listen, and installation is guarded on the
  current route so story-book mounts never capture the book's navigation.

## Consequences

- Adding a section = a `SiteSection` variant, a route, and a wrapped
  early-return; the section inherits navigation for free.
- Section transitions are full page loads (the existing standalone-page
  contract), which keeps hook order sound at the cost of a reload between
  sections.
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

## Follow-ups

- Interactive docs architecture (live embedded nodes) — separate plan.
- Boards "I have one of these" setup entry — future work.
- Landing page; when it exists, the lockup may become its link.
