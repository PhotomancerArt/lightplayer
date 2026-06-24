# Studio Web Component Families

Studio web UI components are organized by dependency direction and domain
knowledge.

## `base`

Base building blocks. These are generic controls and display primitives, similar
to components Studio might get from a design-system package.

Rules:

- Do not depend on `lpa-studio-core`.
- Do not know about Studio devices, projects, nodes, or panes.
- Prefer stable, reusable props over rendering app-core view models directly.

Examples: icon, tabs, simple field rows.

## `core`

Data-driven app controls. These render generic `Ui*` structs from
`lpa-studio-core`.

Rules:

- May depend on `lpa-studio-core` generic UI types such as `UiAction`,
  `UiProgress`, `UiActivity`, `UiPaneView`, and `UiStackView`.
- May compose `base`.
- Should not own Studio workflows when `app` can compose them.

Examples: app actions, app progress, app activity, app stack, app pane.

## `app`

Studio-specific surfaces and workflows. These components understand LightPlayer
Studio concepts and compose page-level UI.

Rules:

- May depend on domain-specific ux views such as project, device, and node
  views.
- May compose `core` and `base`.
- Owns layout and workflow composition for Studio surfaces.

Examples: Studio shell, pane frame, project workspace, device log, node UI.

## Dependency Direction

```text
base <- core <- app
```

Imports should follow the arrows. If a component wants to import “up” the stack,
it probably belongs in the higher family.

## Stories

Component stories are colocated with the component family they describe, but
they are not listed in the central story registry by hand. Add `*_stories.rs`
files beside the relevant component family, list them in the nearest `mod.rs`
behind `#[cfg(feature = "stories")]`, and mark story entry functions with
`#[story]`; the generated story registry discovers those files and calls the
normal Rust module path.

```text
src/base/<component>_stories.rs
src/core/<component>_stories.rs
src/core/<category>/<component>_stories.rs
src/app/<component>_stories.rs
src/app/<category>/<component>_stories.rs
src/exploration/<component>_stories.rs
```

Examples:

```text
src/base/popover_stories.rs             -> base/popover/<story>
src/core/actions/action_strip_stories.rs -> core/actions/action-strip/<story>
src/app/device/device_pane_stories.rs  -> studio/device/device-pane/<story>
src/exploration/node_ui_stories.rs       -> exploration/node-ui/<story>
```

Within a story file, define zero-argument functions returning `Element`:

```rust
use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

#[story]
fn edge_placement() -> Element {
    rsx! { section { "..." } }
}
```

Story ids are inferred from the path plus function name. The example above in
`src/base/popover_stories.rs` becomes:

```text
base/popover/edge-placement
```

Use `snake_case` for Rust filenames and functions; the registry converts those
segments to `kebab-case` for story routes and baseline PNG names. The visible
story label is also derived from the function name, so `edge_placement` renders
as `Edge placement`. Use `#[story(label = "...")]` only when the derived label
would be misleading, and `description = "..."` only when the storybook chrome
needs extra context.

The storybook creates one synthetic overview route per component, such as:

```text
base/popover/overview
```

Overview pages render every story for that component in one scrollable review
surface. They are storybook UI affordances, not generated story functions.
Individual story pages should keep their own chrome minimal: title, optional
description, and the source file path supplied by the generated descriptor.

`build.rs` parses `#[story]` metadata with `syn` and generates the central
story registry. If a story is malformed, the build should fail with a concrete
diagnostic telling you which file, function, or route is wrong. Do not recreate
manual `StoryDescriptor` arrays or per-file `render_story` matches.

Broad fixture modules are allowed during exploration, but story entrypoints
should live in real component-adjacent files. Shared story fixtures should not
end in `_stories.rs`; for example, `app/story_fixtures.rs` can support
stories under `app/device/*_stories.rs`,
`app/project/*_stories.rs`, and `app/layout/*_stories.rs`.

Story source-root guidance:

- `base` generates the `base` story family for generic building blocks such
  as popovers, tabs, buttons, and icons.
- `core` generates the `core` story family for data-driven controls that
  render generic `Ui*` values.
- `app` generates the `studio` story family for app/domain surfaces such
  as device, project, panes, and shell.
- `exploration` generates the `exploration` story family for spikes and
  mockups that are intentionally not production
  component stories yet.

When a change touches Studio web source or story output, follow the repo
baseline workflow:

```bash
just studio-story-baselines-if-needed
```

Include updated files from `lp-app/lpa-studio-web/story-images/` with the same
commit when baselines change.
