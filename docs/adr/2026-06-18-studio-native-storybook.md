# ADR 2026-06-18: Studio-Native Component Storybook

## Status

Accepted.

## Context

LightPlayer Studio is a Rust-first Dioxus web application. We want the component
development ergonomics of Storybook: isolated examples, meaningful states near
the component source, direct links, and fast visual review. At the same time, we
do not want to introduce a JavaScript Storybook toolchain before the Studio UI
surface is large enough to justify it.

The current Studio components render product-shaped Studio state and simple UI
props. That makes them well suited to local fixture-driven stories.

## Decision

Studio component stories will be native Dioxus code in `lpa-studio-web`.

- Story files live next to components as sibling `*_stories.rs` modules.
- A small explicit Rust registry collects story descriptors and render functions.
- Stories render the real Studio components against fake, domain-shaped
  `StudioView` / `UxPaneView` fixtures.
- `just studio-dev` builds the web app with the `stories` feature so the local
  storybook is available at `/#/stories`.
- Production/static `studio-web-build` does not enable the storybook feature.
- `just studio-story-pngs` generates local PNGs into gitignored
  `lp-app/lpa-studio-web/story-images/.scratch/`.

## Consequences

The component workflow stays close to the Rust code and avoids a second UI
runtime. Stories can evolve with the same types the production app uses, which
keeps UI fixtures honest as the Studio domain model grows.

The tradeoff is that we do not get Storybook's ecosystem features for free:
add-ons, controls, automatic discovery, and hosted visual-regression workflows
would need to be built or adopted later.

PNG generation is intentionally local-only for now. Before PNGs become committed
baselines or CI gates, we need a stable rendering environment, a curated story
set, and rules for volatile content such as logs, animation, timestamps, and
browser/font differences.

## Update 2026-06-18

The PNG baseline policy was amended by
[ADR 2026-06-18: Studio Story PNG Baselines](./2026-06-18-studio-story-png-baselines.md).
The native Dioxus storybook decision remains accepted.

## Update 2026-06-22

Studio stories now render `StudioView` and `UxPaneView` fixtures from
`lpa-studio-ux`. The storybook remains native Dioxus code, but it should follow
the active view/action model rather than the deleted `lpa-studio-core`
`StudioState` model.
