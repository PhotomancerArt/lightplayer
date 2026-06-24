# ADR: Studio Core And Layer Vocabulary

- **Status:** Accepted
- **Date:** 2026-06-24
- **Deciders:** Photomancer
- **Updates:** [2026-06-21 Studio UX Layer](./2026-06-21-studio-ux-layer.md)

## Context

The first Studio app slices used `lpa-studio-ux` for the headless
resource-owning layer and `lpa-studio-web` for the Dioxus browser renderer.
That split worked technically, but the names blurred several roles:

- the headless crate owns real application policy, not just UX helper data;
- render data, controllers, snapshots, and operation payloads need separate
  vocabulary;
- web component source roots used `ui_base`, `ui_core`, and `ui_studio`, which
  repeated "UI" in a crate that is already the renderer;
- `Ui*` data consumed by `App*` components made the data/component boundary
  harder to explain.

Studio needs a durable mental model before the app grows much larger.

## Decision

Rename the headless Studio application crate from `lpa-studio-ux` to
`lpa-studio-core`.

```text
lpa-link / lpa-client / protocol services
        owned by
lpa-studio-core
        rendered by
lpa-studio-web, future CLI, desktop, tests, and agents
```

`lpa-studio-core` owns Studio application state, controller logic, app policy,
typed operations, action offerings, snapshots, live updates, and view data. It
is UI-independent, but its role is broader than "UX".

Keep `lpa-studio-web` as the browser/Dioxus renderer. The `studio-` prefix
continues to separate the actual Studio application crates from helper app
crates such as `lpa-link` and `lpa-client`.

Use source module paths for layer:

```text
base  -> primitive UI/data building blocks
core  -> reusable data-driven app/view/action substrate
app   -> the actual Studio product/application layer
```

In `lpa-studio-core`:

- `core/` holds generic view/action/node substrate.
- `app/` holds Studio ownership areas such as `studio`, `device`, `link`,
  `server`, and `project`.
- no empty `base/` module is created until there are truly base core concepts.

In `lpa-studio-web`:

- `base/` holds generic Dioxus primitives.
- `core/` renders generic app-core view/action data.
- `app/` holds Studio-specific surfaces and workflows.
- `exploration/` holds spikes and mockups.

Story routes remain product-facing. Source files under `src/app` still generate
`studio/*` story routes and baseline file names.

The public type rename is intentionally deferred. Current names such as
`StudioUx`, `UiAction`, `UiPaneView`, and `AppPane` remain for compatibility
while the crate/layer rename lands. The long-term direction is:

- stateful owners use `*Controller`;
- inert render data uses `*View`;
- command payloads use `*Op`;
- cloneable read models use `*Snapshot`;
- web renderers use plain component/domain nouns rather than `App*` wrappers
  where practical.

## Consequences

- The headless Studio crate name now describes application ownership instead of
  implying a narrow UX-helper layer.
- Imports read as `lpa_studio_core::{StudioUx, StudioView, UiAction}` until the
  later public type cleanup.
- Web source roots are shorter and align with dependency direction:
  `base <- core <- app`.
- Historical ADRs still mention `lpa-studio-ux`; this ADR is the current naming
  update instead of rewriting those historical decisions in place.
- A follow-up rename pass is still needed to align `Ui*`, `*Ux`, and `App*`
  type names with the new role vocabulary.

## Alternatives Considered

- Rename the crates to `lpa-core` and `lpa-web`.
  - Rejected for now because `studio-` usefully distinguishes the actual Studio
    app from lower-level helper/service crates.
- Use `lpa-studio-app` for the headless crate.
  - Rejected because the web crate is also an app, while `core` better conveys
    UI-independent application ownership.
- Keep `lpa-studio-ux`.
  - Rejected because the name made the crate sound narrower than its actual
    responsibility for service ownership, policy, dispatch, and view data.
- Complete all public type renames in the same pass.
  - Deferred to keep this architectural rename reviewable and avoid mixing
    package/module structure with larger API vocabulary churn.

## Follow-Ups

- Rename `Ui*` render-data types toward `*View` names.
- Rename `*Ux` stateful owners toward `*Controller` names.
- Rename `App*` Dioxus renderers toward plain component/domain nouns.
- Untangle app-specific body variants from generic view data where necessary so
  the `base/core/app` layering is enforceable rather than merely documented.
