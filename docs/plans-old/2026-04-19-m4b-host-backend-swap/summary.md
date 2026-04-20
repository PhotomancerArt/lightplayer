# M4b — Host backend swap (Cranelift → Wasmtime) — summary

Plan dir: `docs/plans-old/2026-04-19-m4b-host-backend-swap/`
Roadmap: `docs/roadmaps/2026-04-16-lp-shader-textures/m4b-host-backend-swap.md`
Perf snapshot: `docs/design/native/perf-report/2026-04-19-m4b-wasmtime-swap.md`
Date: 2026-04-19

## What landed

- `lp-engine`'s host shader backend now runs through `lpvm-wasm`
  (wasmtime) instead of `lpvm-cranelift`. Backend selection is
  `cfg(target_arch = …)`-driven end-to-end:
  - `riscv32`  → `lpvm-native::rt_jit`
  - `wasm32`   → `lpvm-wasm::rt_browser`
  - catchall   → `lpvm-wasm::rt_wasmtime`
- `lp-engine` exposes a single unqualified `Graphics` type. The old
  feature-gated `CraneliftGraphics` / `NativeJitGraphics` aliases
  are gone, as are the `cranelift`, `native-jit`,
  `cranelift-optimizer`, and `cranelift-verifier` Cargo features
  on `lp-engine`, `lp-server`, `fw-emu`, `fw-esp32`, and the
  forwarding feature on `fw-core`.
- `lpvm-wasm`'s wasmtime runtime now pre-grows linear memory once
  at engine construction (default 64 MiB via
  `WasmOptions::host_memory_pages = 1024`) and `WasmtimeLpvmMemory`
  no longer calls `Memory::grow` from `alloc`. This closes the
  stale-pointer hazard that came from `LpvmBuffer::native_ptr`
  outliving a relocated linear memory.
- `WasmLpvmEngine` overrides `LpvmEngine::compile_with_config`, so
  `lpir::CompilerConfig` (e.g. Q32 op-mode toggles) reaches the
  WASM emitter on the host path, matching the cranelift / native
  backends' behaviour. New regression test:
  `lpvm-wasm/tests/compile_with_config.rs`.

## What did NOT land (deferred)

- Wasmtime perf tuning (epoch interruption, parallel compile,
  custom memory reservation strategies). Hooks are in
  `rt_wasmtime/engine.rs` with comments. Tracked as a separate
  later milestone.
- Removing `lpvm-cranelift` from the workspace. It still backs
  `lp-cli shader-debug` AOT and is referenced by `lpfx-cpu` until
  M4c lands.
- `lpfx-cpu` migration (M4c).
- Re-enabling `validate-x64` in `.github/workflows/pre-merge.yml`.
  `AGENTS.md` was updated to flip the M4b status from "deprecation
  path" to "completed" but the CI matrix change is intentionally
  deferred so this plan didn't churn CI at the same time as the
  backend swap.

## Files changed (high-level)

- New: `lp-core/lp-engine/src/gfx/host.rs`,
  `lp-core/lp-engine/src/gfx/wasm_guest.rs`,
  `lp-shader/lpvm-wasm/tests/compile_with_config.rs`,
  `docs/design/native/perf-report/2026-04-19-m4b-wasmtime-swap.md`.
- Deleted: `lp-core/lp-engine/src/gfx/cranelift.rs`.
- Modified `lp-engine` gfx module (`mod.rs`, `native_jit.rs`,
  `lib.rs`, integration tests) to use the unqualified `Graphics`
  type and target-arch dispatch.
- Modified `lp-server`, `lp-cli`, `fw-emu`, `fw-esp32`, `fw-core`
  to drop the obsolete backend Cargo features and use
  `Graphics::new()` directly.
- Modified `lpvm-wasm` `options.rs`,
  `rt_wasmtime/{engine,shared_runtime}.rs` for the memory budget
  and `compile_with_config` parity.
- Modified `AGENTS.md` (architecture-coverage note) and the M4b
  roadmap (validation block + status footer).

## Validation

Per the perf-report and the phase-4 plan doc. All passing on the
final run:

- `just ci` (fmt-check + clippy-host + clippy-rv32 + build-ci +
  cargo test + glsl-filetests, 14028/14028 + 1316 expected-fail).
- `cargo build --workspace` (host exclusion set).
- `cargo test --workspace` (host exclusion set).
- `cargo check -p fw-emu --target riscv32imac-unknown-none-elf
  --profile release-emu`.
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf
  --profile release-esp32 --features esp32c6,server`.
- `cargo check -p lp-engine --target wasm32-unknown-unknown`.
- `cargo test -p fw-tests --test scene_render_emu --test
  alloc_trace_emu` (RV32 emu end-to-end).
- 5× parallel multi-shader stress on the new wasmtime path: zero
  hits for the named JIT-state-leakage failure pattern
  (`"function must be compiled before it can be finalized"`).

## Known issues left in place

- `lpvm-cranelift`'s own test binary (`cargo test -p
  lpvm-cranelift --lib`) intermittently fails when run with the
  default parallel test harness — the same JIT-state-leakage
  flake that motivated M4b. Passes reliably with
  `--test-threads=1`. M4b moves `lp-engine` off the cranelift
  path on the host and so eliminates the user-visible exposure,
  but does not fix the flake inside `lpvm-cranelift` itself
  (out of scope, that crate is unmodified). `cargo test
  --workspace` should be run with `--no-fail-fast` until
  `lpvm-cranelift` is either retired (post-M4c) or its test
  parallelism issue is fixed independently.
- Pre-existing `unexpected_cfgs` warnings from `host_debug!` in
  `fw-emu/src/main.rs` (target=riscv32). Unrelated to this plan.

## Deviations from the plan as written

- Phase 3: sub-agent removed `cranelift-optimizer` /
  `cranelift-verifier` features from `lp-fw/fw-core/Cargo.toml`
  (not listed in the phase doc) because they forwarded into the
  now-deleted `lp-server` features and broke workspace
  resolution. Necessary correction.
- Phase 3: sub-agent gated `extern crate unwinding;` in
  `lp-fw/fw-emu/src/main.rs` behind `#[cfg(feature =
  "test_unwind")]`. Verified this still link-builds (the
  unwinding crate is unconditionally a dep, so its
  `#[panic_handler]` is still picked up by rustc); slightly
  out-of-scope cleanup but not a regression.
- Phase 4: sub-agent ran `cargo fmt --all` once to fix M4b-
  introduced formatting drift in `gfx/mod.rs`,
  `rt_wasmtime/{engine,shared_runtime}.rs`, and
  `lpvm-wasm/tests/compile_with_config.rs`. Mechanical, required
  for `just ci` to pass.
- Phase 4: stress-test command corrected from `--tests <a> <b>
  <c>` (rejected by cargo) to repeated `--test`. Same intent.
