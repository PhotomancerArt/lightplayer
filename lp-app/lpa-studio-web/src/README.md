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

Component stories are colocated with the component family they describe, but
they are not wired by hand or listed in `mod.rs`. Add `*_stories.rs` files
beside the relevant component family and mark story entry functions with
`#[story]`; the generated story registry includes those files directly.

```text
src/ui_base/<component>_stories.rs
src/ui_core/<component>_stories.rs
src/ui_studio/<component>_stories.rs
src/ui_studio/<category>/<component>_stories.rs
src/ui_exploration/<component>_stories.rs
```

Examples:

```text
src/ui_base/popover_stories.rs             -> base/popover/<story>
src/ui_core/action_strip_stories.rs         -> core/action-strip/<story>
src/ui_studio/device/picker_stories.rs      -> studio/device/picker/<story>
src/ui_exploration/node_ui_stories.rs       -> exploration/node-ui/<story>
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
`src/ui_base/popover_stories.rs` becomes:

```text
base/popover/edge-placement
```

Use `snake_case` for Rust filenames and functions; the registry converts those
segments to `kebab-case` for story routes and baseline PNG names. The visible
story label is also derived from the function name, so `edge_placement` renders
as `Edge placement`. Use `#[story(label = "...")]` only when the derived label
would be misleading, and `description = "..."` only when the storybook chrome
needs extra context.

`build.rs` parses `#[story]` metadata with `syn` and generates the central
story registry. If a story is malformed, the build should fail with a concrete
diagnostic telling you which file, function, or route is wrong. Do not recreate
manual `StoryDescriptor` arrays or per-file `render_story` matches.

Broad fixture modules are allowed during exploration, but production component
stories should migrate toward real component-adjacent files. For example, a
temporary `ui_studio/studio_ux_stories.rs` file can hold app-wide shell
fixtures, but device-specific stories should eventually live under
`ui_studio/device/*_stories.rs`.

Story source-root guidance:

- `ui_base` generates the `base` story family for generic building blocks such
  as popovers, tabs, buttons, and icons.
- `ui_core` generates the `core` story family for data-driven controls that
  render generic `Ui*` values.
- `ui_studio` generates the `studio` story family for app/domain surfaces such
  as device, project, panes, and shell.
- `ui_exploration` generates the `exploration` story family for spikes and
  mockups that are intentionally not production
  component stories yet.

When a change touches Studio web source or story output, follow the repo
baseline workflow:

```bash
just studio-story-baselines-if-needed
```

Include updated files from `lp-app/lpa-studio-web/story-images/` with the same
commit when baselines change.
