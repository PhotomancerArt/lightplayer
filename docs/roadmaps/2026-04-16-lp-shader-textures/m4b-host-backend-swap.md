# M4b — Host Backend Swap (Cranelift → Wasmtime)

## Goal

On the host (desktop / `fw-emu`) build, swap the LPVM backend used by
`lp-engine`'s `CraneliftGraphics` from `lpvm-cranelift` to `lpvm-wasm`'s
Wasmtime engine, surfaced through `lp-shader`. The shader compile path
becomes:

```
GLSL → lps_frontend → LPIR → lpvm-wasm (wasm bytes) → Wasmtime → guest code
```

After M4b, the production host path runs through Wasmtime. The
`lpvm-cranelift` crate **stays in the tree** for the moment per the
overview's notes — kept as a regression guard and for any legacy
consumer not yet migrated. Removing it is a later, separate task.

The firmware (RV32 / ESP32) path is **unaffected** by M4b — Wasmtime is
not a viable backend on bare-metal RV32. `native_jit.rs` continues to
use `lpvm-native`, just routed through `LpsPxShader` (per M4a).

## Why this swap

From [`notes.md`](./notes.md):

- Multi-`JITModule` use in one process has shown non-deterministic
  state leakage in Cranelift's JIT (flaky/order-dependent failures,
  including `"function must be compiled before it can be finalized"`).
  Wasmtime uses Cranelift internally with proper per-instance
  isolation.
- 32-bit guest pointers on the host match RV32, the emulator, and the
  browser, removing the 64-bit-host-pointer ABI corner that
  complicated `call_render_texture` in M2.
- Single host execution backend across `lp-shader` unit tests, future
  `lp-cli` authoring tooling, and `lp-engine` host runs.

## Why M4b after M4a

M4a reduces the gfx wrapper to:

```rust
pub struct CraneliftGraphics {
    engine: LpsEngine<CraneliftEngine>,
}
```

After M4a, swapping the backend is mechanical: change `CraneliftEngine`
to the Wasmtime-backed `LpvmEngine` impl. No pixel-loop or
Q32-conversion logic needs to move; it's already inside `LpsPxShader`.

Reviewing the swap separately also means any perf or correctness
regression introduced by Wasmtime is bisectable to a single commit.

## Deliverables

### `lp-engine/src/gfx/cranelift.rs` becomes Wasmtime-backed

Either rename the file (`gfx/wasmtime.rs`) or keep the filename and
treat "Cranelift" as historical. Internally:

- Replace `lpvm_cranelift::CraneliftEngine` with the Wasmtime-backed
  `LpvmEngine` impl from `lpvm-wasm` (likely `WasmtimeEngine` or
  similar — exact name pinned during implementation).
- `LpsEngine<WasmtimeEngine>` becomes the field type.
- Re-export name: rename `CraneliftGraphics` → `WasmtimeGraphics` (or
  `HostGraphics`). Keep a `CraneliftGraphics` type alias for one
  release for downstream compatibility, then remove.

The `LpGraphics::backend_name()` string changes from `"cranelift"` to
`"wasmtime"` — surface this in `lp-cli` `--target` listings if needed.

### Cargo features

`lp-engine`'s `cranelift` feature flag is now misnamed. Two options:

1. Rename to `host-jit` (or `wasmtime`) and update all
   `--features cranelift` callers (`fw-emu`, `lp-cli`, CI scripts).
2. Keep `cranelift` as the feature-flag name (deprecated alias) and
   internally select `lpvm-wasm`. Less churn but surfaces a stale
   name.

Recommendation: **rename** — clean break, low cost, this is internal
infra plumbing.

### `lpvm-wasm` exports an `LpvmEngine`

If `lpvm-wasm` doesn't already implement the `LpvmEngine` trait shape
that `LpsEngine<E>` requires, do that here. Verify
`call_render_texture` is implemented (M2 listed it in the deliverables
for all backends including `lpvm-wasm`).

### Wasmtime build / size considerations

Wasmtime is a heavier dep than Cranelift JIT alone. Validate:

- `fw-emu` binary size delta is acceptable (compare before/after).
- Cold-start compile time for a single shader stays in the order of
  100s of ms (M2 baseline was ~600ms on hardware via native-jit;
  desktop Wasmtime should be faster but confirm).
- Workspace `cargo check` time delta is acceptable for daily
  development.

If size/cold-start is meaningfully worse, document and decide whether
to gate Wasmtime behind a non-default feature.

### `lp-cli` target table

`lp-cli shader-debug` and `glsl-filetests.sh` may list
backends/targets. Rename or remove "cranelift" entries that referred
to the live host path. Keep a target for the Wasmtime path. Filetests
that ran via the cranelift backend should auto-route to the Wasmtime
backend — minimal test churn expected since both end up running the
same LPIR.

## Validation

```bash
cargo test -p lp-engine --features wasmtime  # or whatever the new feature is
cargo build --features wasmtime -p fw-emu
scripts/glsl-filetests.sh --concise            # no regressions
```

End-to-end on `fw-emu`:

- All existing fw-emu shader workloads render correctly.
- Shader compile time + first-frame latency for rainbow.shader is
  within ~2× of pre-swap baseline (Wasmtime adds AOT compile cost on
  first call).
- Multi-shader projects (load several shaders in one fw-emu run) — no
  flaky failures, no JIT-state-leakage symptoms (a known motivator).

Document size/perf delta vs. the pre-swap baseline in
`docs/design/native/perf-report/`.

## Risks

- **Wasmtime surface area.** Wasmtime is a real engine with its own
  config knobs (memory limits, fuel/epoch interruption, etc.). M4b
  should pick conservative defaults and document them; perf tuning is
  a follow-up.
- **`call_render_texture` Wasmtime impl.** M2 says the wasmtime
  backend implements this; verify it actually exists and is exercised
  by `lp-shader` tests before depending on it from `lp-engine`.
- **Feature-flag rename churn** in CI scripts and developer muscle
  memory. Mitigate with a clear `AGENTS.md` note and alias for one
  release.

## Dependencies

- **M4a** (pixel-loop migration) — strongly preferred predecessor; M4b
  is a backend swap on top of M4a's clean API surface. Possible to do
  M4b without M4a but loses the bisect-friendliness and forces
  duplicate pixel-loop changes.

## Out of scope

- Removing `lpvm-cranelift` from the workspace (separate task; the
  Phase 2 `render_texture_smoke.rs` regression test stays).
- Firmware (RV32) path — see M4a; `native_jit` keeps using
  `lpvm-native`.
- `lpfx-cpu` backend swap (M4c covers `lpfx-cpu`'s migration; whether
  it also swaps to Wasmtime is decided in M4c).
- Wasmtime perf tuning (separate perf-tracing milestone the user has
  flagged for later).
