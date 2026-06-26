# ADR: Studio Web Uses Tailwind-First Semantic Styling

## Status

Accepted

## Context

`lpa-studio-web` had grown a large `src/style.css` file that mixed theme
tokens, reusable component styling, app layout, storybook chrome, exploration
stories, media queries, and animations. That made the component library harder
to scan because the visual behavior of a Dioxus component often lived far away
from the markup that owned it.

The Studio UI is becoming a reusable app/component surface driven by semantic
`Ui*` data from `lpa-studio-core`. The styling model needs to support that same
data-driven mental model without duplicating broad selector families for every
component state.

## Decision

Studio web styling is Tailwind-first.

- Components should prefer Tailwind utility classes in Dioxus markup.
- Tailwind tokens should be semantic, shadcn-style names such as `background`,
  `card`, `border`, `muted-foreground`, `accent`, and status color families.
- Theme values remain centralized as Studio CSS variables and are exposed to
  Tailwind from `lp-app/lpa-studio-web/tailwind.css`.
- The existing `tw:` prefix remains during the migration so utility classes can
  coexist with historical `ux-*` classes without ambiguity.
- Simple static styles should stay as direct utility strings.
- Repeated stateful variants should use small Rust helper functions rather than
  broad CSS selector families.
- `src/style.css` should be limited to theme variables, base rules, keyframes,
  browser/measurement behavior, and explicitly transitional legacy surfaces.

## Consequences

Most reusable Studio components now carry their visual structure locally in the
Dioxus component that renders them. This makes component behavior easier to
review and keeps visual variants close to the `Ui*` state that drives them.

Story baselines remain the visual regression safety net for this migration.
Any change under `lp-app/lpa-studio-web` should continue to run
`just studio-story-baselines-if-needed` before commit.

The exploration node UI still has a substantial historical `ux-node-ui-*` CSS
surface. It is intentionally treated as transitional follow-up rather than
expanded into this migration.

