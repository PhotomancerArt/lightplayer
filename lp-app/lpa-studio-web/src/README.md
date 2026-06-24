# Studio Web Component Families

Studio web UI components are organized by dependency direction and domain
knowledge.

## `ui_base`

Base building blocks. These are generic controls and display primitives, similar
to components Studio might get from a design-system package.

Rules:

- Do not depend on `lpa-studio-ux`.
- Do not know about Studio devices, projects, nodes, or panes.
- Prefer stable, reusable props over rendering ux-layer view models directly.

Examples: icon, tabs, simple field rows.

## `ui_core`

Data-driven app controls. These render generic `Ui*` structs from
`lpa-studio-ux`.

Rules:

- May depend on `lpa-studio-ux` generic UI types such as `UiAction`,
  `UiProgress`, `UiActivity`, `UiPaneView`, and `UiStackView`.
- May compose `ui_base`.
- Should not own Studio workflows when `ui_studio` can compose them.

Examples: app actions, app progress, app activity, app stack, app pane.

## `ui_studio`

Studio-specific surfaces and workflows. These components understand LightPlayer
Studio concepts and compose page-level UI.

Rules:

- May depend on domain-specific ux views such as project, device, and node
  views.
- May compose `ui_core` and `ui_base`.
- Owns layout and workflow composition for Studio surfaces.

Examples: Studio shell, pane frame, project workspace, device log, node UI.

## Dependency Direction

```text
ui_base <- ui_core <- ui_studio
```

Imports should follow the arrows. If a component wants to import “up” the stack,
it probably belongs in the higher family.

## Stories

Component stories are colocated with the family or exploration they describe,
but they are not wired by hand. Add `*_story.rs` files under the story family
tree and mark story entry functions with `#[story]`.

```text
src/<family>/<component>_story.rs
src/<family>/<category>/<component>_story.rs
```

Examples:

```text
src/base/popover_story.rs                -> base/popover/<story>
src/studio/device/picker_story.rs        -> studio/device/picker/<story>
src/exploration/node_ui_story.rs         -> exploration/node-ui/<story>
```

Within a story file, define zero-argument functions returning `Element`:

```rust
use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

#[story(
    label = "Popover placement",
    description = "Icon popovers anchored near viewport edges."
)]
fn edge_placement() -> Element {
    rsx! { section { "..." } }
}
```

Story ids are inferred from the path plus function name. The example above in
`src/base/popover_story.rs` becomes:

```text
base/popover/edge-placement
```

Use `snake_case` for Rust filenames and functions; the registry converts those
segments to `kebab-case` for story routes and baseline PNG names.

`build.rs` parses `#[story]` metadata with `syn` and generates the central
story registry. If a story is malformed, the build should fail with a concrete
diagnostic telling you which file, function, or route is wrong. Do not recreate
manual `StoryDescriptor` arrays or per-file `render_story` matches.

Story family guidance:

- `base`: generic building blocks such as popovers, tabs, buttons, and icons.
- `core`: data-driven controls that render generic `Ui*` values.
- `studio`: app/domain surfaces such as device, project, panes, and shell.
- `exploration`: spikes and mockups that are intentionally not production
  component stories yet.

When a change touches Studio web source or story output, follow the repo
baseline workflow:

```bash
just studio-story-baselines-if-needed
```

Include updated files from `lp-app/lpa-studio-web/story-images/` with the same
commit when baselines change.
