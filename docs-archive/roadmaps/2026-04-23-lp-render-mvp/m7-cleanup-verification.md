# Milestone 7: Cleanup + verification — delete superseded code, update docs, gate

## Goal

Wrap the roadmap: delete `lp-app/web-demo` (superseded by
lp-studio), delete superseded fixtures from lpfx (the old
`noise.fx` directory and similar parallel-domain artifacts that
M1 demolition left behind), update design docs to reflect the
new architecture, and run all gates. Optional stretch: File
System Access API impl as an alternate fs backend.

After M7, the roadmap is complete and the codebase is clean. The
deferred lp-domain M4–M6 (schema codegen, migration framework,
CI gates) can resume with confidence — the model has been
exercised end-to-end through six visual artifact kinds, three
node types, a real bus, and a real editor.

## Suggested plan location

`docs/roadmaps/2026-04-23-lp-render-mvp/m7-cleanup-verification/`

Small plan: a single `plan.md`.

## Scope

**In scope:**

- **Delete `lp-app/web-demo/`** entirely.
  - Update workspace `Cargo.toml` to remove the member.
  - Update any build scripts or CI configs referencing it.
  - Update READMEs that point at it.
- **Delete superseded lpfx fixtures**:
  - `lpfx/lpfx/tests/fixtures/noise.fx/` (the parallel-domain
    test fixture).
  - Any `manifest.toml` test data left over from M1's
    parallel-domain demolition.
  - Verify nothing else references them.
- **Audit and update design docs**:
  - `lpfx/concepts.md`: rewrite to describe the new role
    (visual subsystem implementation, lp-domain consumer).
    Replace any references to `FxModule` / `FxManifest`.
  - `lpfx/design.md`: update the architecture diagram and
    crate descriptions.
  - `lpfx/architecture.md` (if it exists): same treatment.
  - `docs/lp-architecture.md`: reflect lpfx's new role and the
    bus seam between lpfx and the future Show layer.
  - `docs/design/color.md` and
    `docs/design/lightplayer/quantity.md`: reconcile the color-
    family authoring model with the runtime rule that palettes and
    gradients materialize as height-one texture resources before
    shader binding.
  - `docs/roadmaps/2026-04-22-lp-domain/overview.md`: add a
    note that M4–M6 of that roadmap were intentionally
    deferred for `lp-render-mvp`, with a forward link.
- **Validation gates** — re-run all per-milestone smoke tests
  end-to-end:
  ```bash
  cargo test --workspace
  cargo build --workspace
  cargo build -p lpfx --no-default-features  # no_std still holds
  dx build --release  # in lp-studio/
  dx build --release --example showcase  # in lp-studio-widgets/
  ```
  Any flaky or failing tests get fixed here, not deferred.
- **Texture-resource validation audit**:
  - Height-one Pattern sampling test exists and passes.
  - Effect input sampler test exists and passes.
  - Palette/gradient recipe edit rebakes and invalidates the
    previous runtime resource instead of accumulating unbounded
    live textures.
  - Runtime validation errors are covered for wrong format and
    `HeightOne` resources with `height != 1`.
  - `params` struct ABI validation: dotted texture paths like
    `params.gradient` work correctly.
  - Example shader migration audit: no remaining flat `param_*`
    uniforms in lp-render MVP examples.
  - Palette/gradient params are represented as `params.*` texture
    fields, not top-level sampler uniforms.
- **Roadmap summary**: write `summary.md` with what shipped,
  what surprised us, what the next roadmap (likely lp-domain
  M4–M6 resumption) should know going in. Include whether
  texture-backed palettes/gradients changed `Kind::ColorPalette`,
  `Kind::Gradient`, `TextureSpec`, or resource lifetime policy.
  Document the adopted `params` struct ABI and the migration away
  from flat `param_*` uniforms.
- **Stretch — File System Access API impl**:
  - `lp-app/lp-studio/src/fs/fsa_fs.rs` — `LpFs` impl backed
    by the browser's File System Access API.
  - "Open folder" affordance: pick a real directory, read/write
    actual files (not just localStorage).
  - Falls back to `LocalStorageFs` in browsers that don't
    support FSA.
  - Strictly optional; if it doesn't fit in M7, it becomes its
    own follow-up.

**Out of scope:**

- New features.
- Performance optimization passes.
- The deferred lp-domain M4–M6 work (resumes after this
  roadmap).
- wgpu backend (separate roadmap).

## Key decisions

- **Web-demo dies completely.** No transitional period. lp-studio
  supersedes it functionally; web-demo's only value was as a
  proof of concept for the wasm + lpvm + canvas path, and that
  path is now baked into lp-studio.
- **Superseded fixtures get deleted, not archived.** Git history
  is the archive.
- **`cargo test --workspace` is the gate.** If anything is red
  at M7, it gets fixed here. No green-light shipping a roadmap
  with red tests.
- **`summary.md` is mandatory.** Captures what surprised us
  during validation; informs the resumed lp-domain M4–M6
  schema/migration work.
- **FSA is stretch, not core.** The prereq is a working
  localStorage-backed editor, which we have. Adding FSA is a
  Q-of-life improvement, not a blocker.

## Deliverables

- Deleted: `lp-app/web-demo/`, lpfx parallel-domain test
  fixtures.
- Updated workspace `Cargo.toml`.
- Rewritten `lpfx/concepts.md`, `lpfx/design.md`, etc.
- Updated `docs/lp-architecture.md`.
- Updated color/quantity docs for texture-backed palette/gradient
  runtime binding.
- Note in `docs/roadmaps/2026-04-22-lp-domain/overview.md`
  about the M4–M6 deferral and pointer to this roadmap.
- All workspace tests green.
- `summary.md` for this roadmap.
- (Stretch) FSA-backed fs impl + "open folder" UI.

## Acceptance smoke tests

```bash
# Hard gate:
cargo test --workspace
cargo build --workspace
cargo build -p lpfx --no-default-features

# Editor builds:
cd lp-app/lp-studio && dx build --release
cd lp-app/lp-studio-widgets && dx build --release --example showcase

# Web-demo gone:
! test -d lp-app/web-demo

# Parallel-domain types gone:
! grep -r 'FxModule\|FxManifest\|FxInputDef\|FxValue' lpfx/ lp-app/

# Texture resource tests present and passing:
cargo test -p lpfx --test render -- texture
cargo test -p lpfx --test render -- height_one

# Manual: full editor smoke (open pattern → tweak → bind to
# bus → save → refresh → restored).
```

## Dependencies

- M6 complete (semantic editor; lp-studio is feature-complete
  for this roadmap before web-demo can die).
- All previous milestones' tests passing.

## Execution strategy

**Option B — Small plan (`/plan-small`).**

Justification: Mostly mechanical (deletions, doc edits). FSA
stretch has its own design surface but is explicitly optional.
Two phases: deletions + doc updates + gates as one, FSA
stretch as the other (or break it out as a follow-up entirely).

> I suggest we use the `/plan-small` process for this milestone, after
> which I will automatically implement. Agree?
