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
