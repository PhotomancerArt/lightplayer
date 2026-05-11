# Design

## Scope of Work

Milestone 6 wraps up the lp-shader texture access roadmap. It validates the
full texture access path that M1-M5 built, updates the reference docs so
downstream lpfx/domain and future wgpu work have a stable contract, and removes
temporary scaffolding that remained after the implementation milestones.

In scope:

- Run and document validation for texture filetests, `lp-shader` API tests,
  relevant shared/frontend/runtime crates, and repo-policy RV32 checks.
- Update texture access documentation for:
  - `TextureBindingSpec` and helper APIs,
  - logical `Texture2D` values vs the guest descriptor ABI,
  - `texture-spec` / `texture-data` filetest syntax,
  - supported formats, filters, wraps, shape hints, and diagnostics,
  - deferred wgpu/WGSL/sampling features.
- Audit texture diagnostics and filetests for sampler-name context and stale
  unsupported/expect-fail markers.
- Remove temporary TODOs, debug hooks, duplicated helpers, or stale milestone
  notes created during M1-M5.
- Add concise follow-up notes for real wgpu comparison, WGSL source input,
  `clamp_to_border`, and mipmaps/manual LOD.

Out of scope:

- New sampling features or behavior changes.
- lpfx/domain integration beyond documentation and API-readiness notes.
- wgpu backend implementation or WGSL source support.
- External roadmap/schema changes.

## File Structure

```text
docs/
├── design/
│   └── lp-shader-texture-access.md                 # UPDATE: final shipped contract
└── roadmaps/2026-04-24-lp-shader-texture-access/
    ├── overview.md                                 # UPDATE: milestone/status notes if stale
    ├── decisions.md                                # UPDATE: only if final decisions are missing
    └── m6-integration-validation-cleanup/
        └── plan.md                                 # UPDATE: detailed phases + final notes

lp-shader/
├── README.md                                       # UPDATE: high-level texture-read overview/link
├── lps-filetests/
│   └── README.md                                   # UPDATE: texture-spec / texture-data syntax
└── lps-shared/
    └── README.md                                   # UPDATE: only if a concise shared-types note helps
```

## Conceptual Architecture

```text
M1-M5 shipped texture access
        │
        ▼
M6 validation pass
  ├─ run texture GLSL filetests across wasm + RV32 targets
  ├─ run public lp-shader API tests and repo policy checks
  └─ audit stale xfail/TODO/debug/scaffolding
        │
        ▼
M6 documentation pass
  ├─ final contract doc: TextureBindingSpec, Texture2D ABI, supported modes
  ├─ user-facing README: where texture reads live and how to run tests
  └─ filetest README: texture fixture directives and examples
        │
        ▼
roadmap wrap-up
  └─ follow-ups: wgpu comparison, WGSL input, clamp_to_border, mip/LOD
```

## Main Components

### Contract Documentation

`docs/design/lp-shader-texture-access.md` is the main shipped-state contract.
It should reconcile the original design with what landed:

- `TextureBindingSpec` is keyed by sampler uniform name and provides format,
  filter, wrap, and shape metadata outside GLSL source.
- Logical `Texture2D` / `sampler2D` is distinct from the four-lane guest
  descriptor ABI (`ptr`, `width`, `height`, `row_stride`).
- `texelFetch` supports `R16Unorm`, `Rgb16Unorm`, and `Rgba16Unorm`.
- Filtered `texture()` supports `R16Unorm` and `Rgba16Unorm` today; `Rgb16Unorm`
  remains unsupported for filtered sampling until a format builtin exists.
- `TextureShapeHint::HeightOne` keeps the GLSL surface as `sampler2D` +
  `vec2`, but lowering selects a 1D-style path that ignores `uv.y` and `wrap_y`.
- `lp-shader` owns binding/lowering/validation; lpfx/domain own routing and
  palette/gradient baking.

### README Documentation

`lp-shader/README.md` should briefly mention that shaders can read texture
uniforms with GLSL `texelFetch` and `texture`, and link to the design doc for
the full binding contract.

`lp-shader/lps-filetests/README.md` should document the texture directive
surface now used by the canonical GLSL filetests:

- `// texture-spec: <name> format=<...> filter=<...> wrap=<...> shape=<...>`
- `// texture-data: <name> <width>x<height> <format>`
- normalized float pixel groups and exact hex channel values;
- texture-specific validation/failure modes.

### Validation and Cleanup

M6 should not invent new features. It should run the real texture validation
commands, audit for stale temporary markers, and document either passing
results or concrete blockers. Any behavior changes should be limited to small
bug fixes found during validation.

# Phases

## Phase 1: Documentation Contract Update

[sub-agent: yes]

### Scope of Phase

Update the shipped texture-access documentation so it matches M1-M5.

In scope:

- Update `docs/design/lp-shader-texture-access.md`.
- Update `lp-shader/README.md` with a concise texture-read overview and link
  to the design doc.
- Update `lp-shader/lps-filetests/README.md` with texture fixture directives
  and examples.
- Optionally update `lp-shader/lps-shared/README.md` if a short note about
  shared texture vocabulary is useful.
- Update `docs/roadmaps/2026-04-24-lp-shader-texture-access/overview.md` or
  `decisions.md` only if they are stale or missing a final decision.

Out of scope:

- Code changes.
- New sampling behavior.
- New filetests.
- Product/domain/wgpu implementation docs beyond follow-up notes.

### Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

### Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

### Implementation Details

Relevant files:

- `docs/design/lp-shader-texture-access.md`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/overview.md`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/decisions.md`
- `lp-shader/README.md`
- `lp-shader/lps-filetests/README.md`
- `lp-shader/lps-shared/README.md`

Documentation should reflect these shipped facts:

- GLSL source uses standard `sampler2D`, `texelFetch`, and `texture`.
- Compile-time texture metadata is supplied outside shader source with
  `TextureBindingSpec`.
- Public convenience APIs now include:
  - `CompilePxDesc::with_texture_spec`;
  - `lp_shader::texture_binding::texture2d`;
  - `lp_shader::texture_binding::height_one`;
  - `LpsTextureBuf::to_texture2d_value`;
  - `LpsTextureBuf::to_named_texture_uniform`.
- Guest ABI remains a four-lane `LpsTexture2DDescriptor`: `ptr`, `width`,
  `height`, `row_stride`.
- Host validation also uses format and byte-length metadata from
  `LpsTexture2DValue`; these are not guest descriptor lanes.
- `texelFetch` supports `R16Unorm`, `Rgb16Unorm`, and `Rgba16Unorm`.
- Filtered `texture()` supports `R16Unorm` and `Rgba16Unorm`; unsupported
  format diagnostics should be described as compile-time/lowering errors.
- Filter modes: `Nearest`, `Linear`.
- Wrap modes: `ClampToEdge`, `Repeat`, `MirrorRepeat`.
- Shape hints: `General2D`, `HeightOne`.
- `HeightOne` is an optimization hint for width-by-one 2D textures, not a new
  resource type; GLSL remains `sampler2D`.
- Filetests live under `lp-shader/lps-filetests/filetests/texture/`.
- Real texture GLSL validation should use `scripts/filetests.sh`, not filtered
  Rust unit tests.
- Future work remains explicit: real wgpu comparison runner, WGSL source input,
  `clamp_to_border`, mipmaps/manual LOD, larger sidecar fixtures if needed.

Keep docs concise and factual. Do not claim wgpu parity or product/domain
integration that does not exist.

### Validate

Run:

```bash
cargo +nightly fmt --all -- --check
```

## Phase 2: Diagnostics And Scaffolding Audit

[sub-agent: yes]

### Scope of Phase

Audit texture-related tests, diagnostics, and implementation notes for stale
temporary markers or misleading text after M1-M5.

In scope:

- Search texture filetests for stale `@unimplemented`, `@broken`,
  `@unsupported`, or expect-fail markers.
- Search texture-related Rust and docs for temporary TODOs, `dbg!`,
  `println!`, debug-only hooks, commented-out code, and duplicated helpers.
- Read existing negative texture filetests and diagnostic strings for sampler
  name/context clarity.
- Make small cleanup fixes when the intent is obvious.
- Add or adjust one small diagnostic/filetest only if the audit finds a clear
  missing coverage gap.

Out of scope:

- New texture features.
- Broad diagnostic redesign.
- Renaming the texture directive syntax.
- Large refactors in frontend, builtins, filetests, or runtime validation.

### Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

### Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

### Implementation Details

Relevant files/directories:

- `lp-shader/lps-filetests/filetests/texture/`
- `lp-shader/lps-filetests/src/`
- `lp-shader/lps-frontend/src/`
- `lp-shader/lp-shader/src/runtime_texture_validation.rs`
- `lp-shader/lp-shader/src/tests.rs`
- `lp-shader/lps-builtins/src/builtins/texture/`
- `docs/design/lp-shader-texture-access.md`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/`

Audit commands to use as guidance:

```bash
rg "@unimplemented|@broken|@unsupported" lp-shader/lps-filetests/filetests/texture
rg "TODO|dbg!|println!|eprintln!|unimplemented!|todo!" \
  lp-shader/lps-filetests lp-shader/lps-frontend lp-shader/lp-shader/src \
  lp-shader/lps-builtins/src/builtins/texture docs/design/lp-shader-texture-access.md
rg "texture|sampler|Texture2D|TextureBindingSpec|HeightOne" \
  lp-shader/lps-filetests/filetests/texture lp-shader/lps-frontend/src \
  lp-shader/lp-shader/src/runtime_texture_validation.rs
```

Expected outcome:

- If no stale markers or cleanup issues exist, leave code unchanged and report
  what was audited.
- If an `@unsupported` marker is permanent and correct, leave it and document
  why in the report.
- If a diagnostic lacks sampler-name context and the fix is localized, improve
  it and add/update a focused test.
- Do not remove valid future-work notes from docs unless they are stale or
  misleading.

### Validate

Run:

```bash
cargo test -p lp-shader texture
cargo test -p lps-filetests texture
```

If no Rust/code/test files changed, it is acceptable to run only the audit
commands and report that validation was not necessary for this phase.

## Phase 3: Final Validation And Roadmap Wrap-up

[sub-agent: supervised]

### Scope of Phase

Run final validation, document the results in this plan, and add concise
roadmap wrap-up notes.

In scope:

- Run the final validation commands listed below.
- Append `# Notes` to this plan with final validation results and any blockers.
- Append `# Decisions for future reference` to this plan.
- If the documentation/audit phases found final follow-ups, record them in a
  concise future-work list.
- Leave this roadmap-backed plan in place.

Out of scope:

- New behavior.
- New broad tests after validation passes.
- Archiving this plan to `docs/plans-old/`.
- Pushing or committing; the main agent will commit after review.

### Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

### Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If validation fails for a non-obvious reason, stop and report rather than
  debugging deeply.
- Report back: what changed, what was validated, and any deviations.

### Implementation Details

Run validation from the repository root.

Append a `# Notes` section to this plan containing:

- final validation command list and pass/fail result;
- any blockers if a command could not run;
- any cleanup/audit result worth preserving.

Append a `# Decisions for future reference` section. Keep it terse and
high-signal. Likely decisions:

- M6 is validation/docs cleanup only; future wgpu parity is not claimed.
- The design doc remains the shipped contract; README docs link to it rather
  than duplicating the full contract.
- Texture GLSL validation uses `scripts/filetests.sh`; cargo tests alone are
  not the integration signal for the texture corpus.

Do not pad the decisions section with facts already obvious from the code.

### Validate

Run:

```bash
scripts/filetests.sh --target wasm.q32,rv32n.q32,rv32c.q32 texture/
cargo test -p lp-shader texture
cargo test -p lps-shared texture
cargo test -p lps-frontend texture
cargo test -p lps-filetests texture
just check
```

# Notes

- Confirmation answers: all suggested answers accepted.
- M6 is docs/validation/cleanup only. Do not add new sampling behavior unless
  validation exposes a small bug.
- `docs/design/lp-shader-texture-access.md` is the main shipped texture access
  contract, with small roadmap/decision notes only if needed.
- Texture GLSL validation should use
  `scripts/filetests.sh --target wasm.q32,rv32n.q32,rv32c.q32 texture/`, plus
  `cargo test -p lp-shader texture` and `just check`.
- wgpu/WGSL remains documented follow-up only.
- Existing texture diagnostic tests should be audited before adding coverage.
- The roadmap-backed `plan.md` remains in place after implementation.
- Additional docs to consider: `lp-shader/README.md` and
  `lp-shader/lps-filetests/README.md`.

### Phase 3 final validation (2026-04-28)

Commands run from repo root; all **passed** (exit 0).

| Command | Result |
|---------|--------|
| `scripts/filetests.sh --target wasm.q32,rv32n.q32,rv32c.q32 texture/` | **pass** — 120/120 tests, 31/31 files (~1.09s reported) |
| `cargo test -p lp-shader texture` | **pass** — 30 tests |
| `cargo test -p lps-shared texture` | **pass** — 22 tests |
| `cargo test -p lps-frontend texture` | **pass** — 19 unit + 0 integration (filtered) |
| `cargo test -p lps-filetests texture` | **pass** — 27 unit + 0 integration (filtered) |
| `just check` | **pass** — `cargo fmt --check`, host clippy `-D warnings`, RV32 clippy (`fw-esp32` esp32c6, `lp-riscv-emu-guest-test-app`) |

**Blockers:** none.

**Cleanup/audit preserved from earlier phases:** Phase 1 updated the shipped contract and READMEs; Phase 2 audited texture filetests/diagnostics for stale markers and localized fixes. Phase 3 did not change code.

# Decisions for future reference

- **M6 scope:** Validation, documentation, and cleanup only; no claim of wgpu parity or domain integration beyond API-readiness notes in docs.
- **Contract source of truth:** `docs/design/lp-shader-texture-access.md` is the shipped contract; user-facing READMEs link there instead of duplicating the full spec.
- **Texture GLSL integration signal:** Use `scripts/filetests.sh` with `wasm.q32`, `rv32n.q32`, and `rv32c.q32` on the `texture/` corpus; `cargo test … texture` filters are supplementary, not a substitute for the filetest run.
- **Roadmap artifact:** This `plan.md` stays under `docs/roadmaps/…/m6-integration-validation-cleanup/` (not archived to `docs/plans-old/`).

