# Milestone 2: lp-studio app core — Dioxus skeleton, virtual fs, widget library, showcase

## Goal

Stand up the lp-studio web app: Dioxus skeleton, localStorage-backed
virtual filesystem (seeded from bundled examples), reusable widget
library crate, and a roll-our-own widget showcase. **No rendering
yet** — this milestone is "the app boots, shows the widget gallery,
loads/edits/persists files via the virtual fs."

Runs in parallel with M1 (different code surfaces, no
cross-dependency).

## Suggested plan location

`docs/roadmaps/2026-04-23-lp-render-mvp/m2-lp-studio-app-core/`

Full plan: `00-notes.md`, `00-design.md`, numbered phase files.

## Scope

**In scope:**

- **New crate `lp-app/lp-studio/`** — Dioxus web app:
  - `Cargo.toml` with Dioxus + dioxus-web + wasm-bindgen.
  - `src/main.rs` — `dx serve` entrypoint.
  - `src/app.rs` — root component, top-level layout (header + nav
    + content area), Dioxus router with at least three routes:
    `/showcase`, `/files`, `/about`.
  - `src/fs/` module:
    - `local_storage_fs.rs` — `LocalStorageFs`: an `LpFs` impl
      that reads/writes through `LpFsMem` and persists the whole
      tree (or per-path deltas) to localStorage on mutation. On
      boot, restores from localStorage if present; otherwise
      seeds from `assets/examples/`.
    - `seed.rs` — bundle of `lp-domain/lp-domain/examples/v1/`
      compiled in via `include_dir!` (or `build.rs` walking the
      directory). Materializes into `LpFsMem`.
  - `src/pages/files.rs` — minimal file tree page: list
    directories and files under `/`, click-to-view raw text,
    "reset to example" button that clears localStorage and
    reseeds.
  - `src/pages/about.rs` — version + roadmap link, basically a
    smoke test that routing works.
- **New crate `lp-app/lp-studio-widgets/`** — Slot-driven widget
  library:
  - `Cargo.toml` with Dioxus.
  - `src/lib.rs` — re-exports.
  - `src/slider.rs` — `Slider` component. Props: `value: f32`,
    `min: f32`, `max: f32`, `step: Option<f32>`, `label: &str`,
    `on_change: EventHandler<f32>`. Variants implicit in props
    (range constraint = both min/max set; step = step set).
  - `src/dropdown.rs` — `Dropdown<T>` with `choices: Vec<(T,
    String)>`, `selected: T`, `on_change`.
  - `src/number_input.rs` — fallback for unconstrained scalars.
  - `src/checkbox.rs` — bool widget.
  - `src/lib.rs` exposes a `widget_for_slot(&Slot) -> Element`
    helper that picks the right widget based on `Constraint`
    (the **load-bearing primitive** the M3 Pattern editor uses).
- **Roll-our-own showcase** at
  `lp-app/lp-studio-widgets/examples/showcase.rs`:
  - Sibling Dioxus app (`dx serve --example showcase`).
  - One page per widget showing all useful states:
    - Slider: no constraint, range only, range + step,
      degenerate range, edge values, very wide range.
    - Dropdown: 2 choices, 5 choices, 20 choices, very long
      labels.
    - Number input: positive/negative, integer/float,
      huge/tiny values.
    - Checkbox: true/false default.
  - One overview page: every widget in a "normal" state, in a
    grid, for at-a-glance review.
  - Routing between pages via Dioxus router.
- **Build pipeline**:
  - `dx serve` works for both `lp-studio` and the showcase.
  - `dx build` (release) succeeds for both — confirms wasm
    artifacts compile clean.
  - Document in lp-studio README.
- **Smoke tests / acceptance**:
  - `cargo build -p lp-studio -p lp-studio-widgets` passes.
  - `dx build` for both succeeds.
  - Manual: `dx serve` boots, showcase renders all widgets,
    files page lists bundled examples, "reset" works,
    persistence survives refresh.

**Out of scope:**

- Anything that touches lpfx or rendering (M3).
- Pattern editor specifically (M3).
- Stack / Effect editor (M4).
- Bus debugger (M5).
- Rich widgets — color picker, log slider, dial,
  composition (M6).
- TOML side-by-side editor (M6).
- File System Access API (M7 stretch).
- Multi-project support.
- Authentication, server sync.

## Key decisions

- **lp-studio is the long-term app crate name** (per Q6
  resolution). Even though M1–M4 only exercise the visual
  subsystem, the name accommodates the full Lightplayer studio
  it grows into (composing Show / rig / fixture editors on top
  of the same framework).
- **Widget library is a separate crate** (`lp-studio-widgets`).
  Importable by both `lp-studio` and the showcase example, and
  by any future testing harness. Refactoring later when more
  consumers arrive is unnecessary if we extract day one.
- **Roll our own showcase, no Lookbook / dioxus-showcase** (per
  Q10). Keep it minimal — one page per widget with hard-coded
  variants, one overview page. No framework abstractions over
  Dioxus.
- **localStorage persistence from day one** (per Q7 resolution).
  Single active project, easy reset to example, one fs tree.
  Future expansion (multi-project, FSA-backed real fs) is
  additive on top.
- **No rendering in this milestone.** lp-studio in M2 is "the
  shell with widgets that don't drive anything yet." Wiring to
  lpfx happens in M3.
- **Keep widget API minimal.** Each widget takes raw props
  (`value`, `min`, `max`, `on_change`); the
  `widget_for_slot(&Slot)` adapter is the only place that
  knows about lp-domain. Widgets stay reusable in non-Slot
  contexts (e.g. system settings later).

## Deliverables

- `lp-app/lp-studio/` crate with the Dioxus shell.
- `lp-app/lp-studio-widgets/` crate with scalar widgets +
  showcase example.
- localStorage-backed virtual fs (`LocalStorageFs`).
- Bundled examples seed from `lp-domain/lp-domain/examples/v1/`.
- Roll-our-own showcase running via `dx serve --example
  showcase`.
- Files page in lp-studio listing bundled examples.
- "Reset to example" affordance.
- lp-studio README documenting `dx serve` / `dx build` flows.

## Acceptance smoke tests

```bash
cargo build -p lp-studio -p lp-studio-widgets
dx build --release  # in lp-studio/
dx build --release --example showcase  # in lp-studio-widgets/

# Manual:
cd lp-app/lp-studio && dx serve
# → app boots, files page lists rainbow.pattern.toml etc.,
#   reset button works, refresh preserves edits

cd lp-app/lp-studio-widgets && dx serve --example showcase
# → showcase boots, every widget visible across its variants
```

## Dependencies

- Dioxus toolchain installed (`dx` CLI).
- lpfs (`lp-base/lpfs/`) — `LpFsMem` already exists from
  lp-domain M1.
- No dependency on M1 (parallel milestone).
- lp-domain examples corpus exists at
  `lp-domain/lp-domain/examples/v1/`.

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: Two new crates, Dioxus toolchain integration,
filesystem persistence layer, widget API design, showcase
structure, build pipeline. Multiple small but real decisions
(widget prop shape, fs persistence strategy — whole-tree
snapshot vs per-path delta, showcase routing structure,
`include_dir!` vs `build.rs` for the example bundle).
Phaseable: app skeleton + routing + about page as one phase
(prove `dx serve` works), fs layer as another (prove
persistence works), widget library + showcase as a third
(prove widgets render in isolation), file tree page as the
join.

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?
