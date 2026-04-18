# Phase 6 — Cleanup + workspace validation

## Scope

Final pass: doc/comment sweep, workspace-wide build, targeted test
runs, roadmap status sync. No new behavioural code lands here — this
phase is the M2.0 closeout.

## Prerequisites

- Phases 1–5 merged.

## Tasks

### 1. Doc / comment sweep

- [`px_shader.rs`](../../../lp-shader/lp-shader/src/px_shader.rs):
  remove the now-stale `# M0 / roadmap M2` comment block on
  `render_frame` (which still says "deferred to roadmap M2").
  Replace with a one-paragraph overview describing the new
  pipeline (apply uniforms → call synthesised `__render_texture`).
- [`engine.rs::compile_px`](../../../lp-shader/lp-shader/src/engine.rs):
  add a doc note that `compile_px` synthesises a format-specific
  `__render_texture_<format>` function, accessible via
  `meta().functions` filtered by `kind == LpsFnKind::Synthetic`.
- `LpvmInstance::call_render_texture` doc (added in Phase 2):
  ensure the doc accurately describes the cache lifecycle and the
  validation timing (first call only). Cross-reference the Phase 3
  synth output shape.
- `LpsFnKind` doc (added in Phase 1): expand to mention the two
  current synthetic functions (`__shader_init`,
  `__render_texture_<format>`) so future readers know what to
  expect.

### 2. Roadmap sync

- [`docs/roadmaps/2026-04-16-lp-shader-textures/overview.md`](../../roadmaps/2026-04-16-lp-shader-textures/overview.md):
  mark M2.0 as ✅ complete; link to this stage's plan dir.
- [`docs/roadmaps/2026-04-16-lp-shader-textures/m2-render-frame.md`](../../roadmaps/2026-04-16-lp-shader-textures/m2-render-frame.md):
  add an "Implemented in" footer pointing at this plan dir.
- Verify M3 (texture *reads*) and M4 (consumer migration) plan
  files reference the M2.0 deliverables correctly. No code
  changes; just a read-and-cross-link pass.

### 3. Sig docs (Q11 follow-through)

- [`lps-shared/src/sig.rs`](../../../lp-shader/lps-shared/src/sig.rs):
  ensure the `LpsModuleSig::functions` doc mentions that the list
  contains both user and synthetic functions, with the
  `LpsFnKind` discriminant for filtering. Suggested wording:
  ```rust
  /// One entry per function in the module — user-authored *and*
  /// toolchain-synthesised. Filter via [`LpsFnSig::kind`] to
  /// distinguish; e.g. `f.kind == LpsFnKind::UserDefined` for
  /// user code only.
  pub functions: Vec<LpsFnSig>,
  ```

### 4. Stale comments / TODOs sweep

Grep for and fix:

```bash
rg -n "M0 / roadmap M2|roadmap M2|deferred to roadmap" -- lp-shader docs
rg -n "TODO.*render_frame|TODO.*M2\.0" -- lp-shader docs
rg -n "lookup_render_texture|RenderTextureHandle" -- lp-shader docs
```

The `lookup_render_texture` / `RenderTextureHandle` greps should
return only historical mentions in `00-notes.md` (which is the
design log and intentionally retains the rejected alternatives);
nothing else should survive. If anything else does, delete it.

### 5. Synthetic-function visibility cross-check

Confirm via `cargo test`:

- `lp-cli/src/commands/shader_debug/collect.rs` (around line 40 —
  the `default_sig` site touched in Phase 1) doesn't accidentally
  filter out synthetic functions when listing module contents. If
  the lp-cli surfaces "module functions" anywhere, decide per
  consumer whether to filter `kind == UserDefined` (likely yes for
  user-facing listings) or show both (debug surfaces).
- No code path crashes on encountering an `LpsFnSig` with
  `kind == Synthetic` (e.g. attempting to `call_q32` it from
  outside `LpsPxShader`). If the public `LpvmInstance::call_q32`
  is invoked with `__render_texture_*` by mistake, the existing
  arity check should produce a clear error — verify by inspection.

### 6. Workspace build + test sweep

```bash
cargo check --workspace --all-features
cargo build --workspace --all-features

cargo test  -p lps-shared
cargo test  -p lps-frontend
cargo test  -p lpvm
cargo test  -p lpvm-cranelift                  # Phase 2 JIT smoke
cargo test  -p lpvm-native
cargo test  -p lpvm-emu
cargo test  -p lpvm-wasm                       # if testable in workspace target
cargo test  -p lp-shader                       # Phase 5 format tests (Wasmtime default)
```

If the workspace's normal CI test command differs (e.g. uses a
specific toolchain or feature set), run that instead.

### 7. Filetests sanity run

If `lps-filetests` exercises any shader fixtures end-to-end, run:

```bash
cargo test -p lps-filetests
```

These don't directly cover M2.0's `render_frame` path (they predate
texture rendering), but they exercise `call_q32` heavily — useful as
a regression check that Phase 2's trait extension didn't break the
existing call path on any backend.

### 8. lpfx unaffected verification

Sanity-check that `lpfx` (M4 consumer, deliberately out of scope
here) still builds:

```bash
cargo build -p lpfx --features default
```

The M2.0 work is purely additive to `lp-shader`; `lpfx` shouldn't
need any changes. If it breaks, it's because of an inadvertent
public-API regression in `lp-shader` or `lps-shared` — track down
and fix before declaring M2.0 done.

### 9. Final design-doc reconciliation

- [`00-design.md`](./00-design.md): if anything materially diverged
  during implementation (e.g. `module_globals_mutated` ended up
  smarter than the always-reset baseline; the cache shape on a
  backend grew beyond `Option<…>`; etc.), append an
  "Implementation notes" section at the bottom. Keep the body
  describing the *as-designed* approach so the design doc remains
  a faithful record of intent.
- [`00-notes.md`](./00-notes.md): mark all Q1–Q13 as ✅ resolved
  (most already are). Add a short closing note if any
  implementation surprises came up that should inform future
  milestones.

## Acceptance criteria

M2.0 is complete when, simultaneously:

1. `cargo build --workspace --all-features` succeeds.
2. All tests in tasks 6 + 7 pass.
3. `lp-cli` (any path that exercises `compile_px`) renders frames
   end-to-end without error once it is switched to the Wasmtime-backed
   `lp-shader` host path (M4 tracks consumer migration; until then
   legacy callers may still use the deprecated JIT stack).
4. The four format-correctness tests from Phase 5 pass under the
   default `lp-shader` configuration (**Wasmtime** / `lpvm-wasm`).
5. The Phase 2 lpvm-cranelift JIT smoke passes.
6. The Phase 3 inliner-sanity assertion passes (one `Call` to
   `render` in `__render_texture_<format>` body).
7. All other backends (`lpvm-native::rt_jit`, `lpvm-native::rt_emu`,
   `lpvm-emu`, `lpvm-wasm::rt_wasmtime`, `lpvm-wasm::rt_browser`)
   compile clean with their `call_render_texture` impls. Runtime
   correctness for these is deferred to M4 (`fw-emu` /
   `fw-wasm`-based integration tests once `lpfx` / `lp-engine` are
   threaded through).
8. Roadmap updated to mark M2.0 ✅.
